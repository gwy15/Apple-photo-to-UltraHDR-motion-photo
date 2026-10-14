//! 任务发现 (task discovery)：把一个目录树里的照片/视频配成转换任务。
//!
//! 公共 seam 只有一个入口 [`discover`]：给根目录与选项，返回配好的
//! [`Discovery`]。配对规则（同 stem 图+视频、heic 优先、每 stem 一个任务）
//! 全部藏在实现里，调用方与测试都通过这一个 interface 观察行为。

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};

/// 同一个 stem 有多张图片时的挑选优先级：heic（HDR 源）优先，其余次之。
fn image_priority(path: &Path) -> u8 {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("heic") => 0,
        _ => 1,
    }
}

/// 一张配好的照片：主图 + 同名动态照片视频（静态照片为 None）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPhoto {
    pub image: PathBuf,
    pub video: Option<PathBuf>,
    pub output: PathBuf,
}

/// 发现过程中遇到的非致命问题，按 kind 断言而非匹配字符串。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveryIssue {
    pub kind: DiscoveryIssueKind,
    pub path: PathBuf,
}

impl DiscoveryIssue {
    fn new(kind: DiscoveryIssueKind, path: impl Into<PathBuf>) -> Self {
        DiscoveryIssue { kind, path: path.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryIssueKind {
    /// 同一 stem 有多张图，除选中那张外其余被忽略。
    DuplicateImage,
    /// 同一 stem 有多个视频，除选中那个外其余被忽略。
    DuplicateVideo,
    /// 符号链接不跟进、不处理。
    SymlinkSkipped,
    /// 非图片扩展名，不构成任务。
    UnknownFile,
    /// 有图片但不是 heic：没有 Apple HDR gainmap，跳过。
    NotHeic,
}

/// 发现阶段未通过 strict 校验的全部冲突。
#[derive(Debug)]
pub struct StrictErrors(pub Vec<DiscoveryIssue>);

impl std::fmt::Display for StrictErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "strict mode: {} conflict(s)", self.0.len())?;
        for issue in &self.0 {
            writeln!(f, "  {:?}: {}", issue.kind, issue.path.display())?;
        }
        Ok(())
    }
}

/// 发现选项：只含配对关注，转换参数（质量等）不过这里。
#[derive(Debug, Clone, Default)]
pub struct DiscoverOptions {
    /// 图片扩展名，默认 "heic,jpg,jpeg"。逗号分隔，大小写不敏感。
    pub image_extensions: Option<String>,
    /// 视频扩展名，默认 "mov,mp4"。
    pub video_extensions: Option<String>,
    /// 输出文件名后缀，避免输出重名。
    pub output_suffix: Option<String>,
    /// strict 模式：重名冲突不忽略而是收集全部后报错。
    pub strict: bool,
}

impl DiscoverOptions {
    fn extensions<'a>(&'a self, opt: &'a Option<String>, default: &'static str) -> HashSet<String> {
        opt.as_deref()
            .unwrap_or(default)
            .split(',')
            .map(|s| s.to_ascii_lowercase().to_string())
            .collect()
    }

    fn image_extensions(&self) -> HashSet<String> {
        self.extensions(&self.image_extensions, "heic,jpg,jpeg")
    }

    fn video_extensions(&self) -> HashSet<String> {
        self.extensions(&self.video_extensions, "mov,mp4")
    }
}

/// 发现结果：配好的任务 + 非致命问题清单。
#[derive(Debug, Default)]
pub struct Discovery {
    pub photos: Vec<DiscoveredPhoto>,
    pub issues: Vec<DiscoveryIssue>,
}

/// 遍历 `root` 目录树，把照片/视频配成转换任务。
/// 根目录不存在或不是目录是硬错误；单个文件的怪异记入 issues，遍历继续。
pub fn discover(root: &Path, options: &DiscoverOptions) -> Result<Discovery> {
    let mut visitor = Visitor::new(options);
    visitor.visit(root)?;
    visitor.finish()
}

struct Visitor<'a> {
    options: &'a DiscoverOptions,
    image_exts: HashSet<String>,
    video_exts: HashSet<String>,
    photos: Vec<DiscoveredPhoto>,
    issues: Vec<DiscoveryIssue>,
    strict_conflicts: Vec<DiscoveryIssue>,
    /// stem → 被忽略的重名图，等触发路径真正建任务时再落 issues。
    pending_duplicate_images: HashMap<PathBuf, Vec<PathBuf>>,
}

impl<'a> Visitor<'a> {
    fn new(options: &'a DiscoverOptions) -> Self {
        Visitor {
            options,
            image_exts: options.image_extensions(),
            video_exts: options.video_extensions(),
            photos: Vec::new(),
            issues: Vec::new(),
            strict_conflicts: Vec::new(),
            pending_duplicate_images: HashMap::new(),
        }
    }

    fn visit(&mut self, path: &Path) -> Result<()> {
        anyhow::ensure!(path.exists(), "Path does not exist: {:?}", path);
        anyhow::ensure!(path.is_dir(), "Not a directory: {:?}", path);

        let entries = std::fs::read_dir(path)?;
        let mut entries: Vec<_> = entries
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry),
                // 单个目录项读不动不中止整棵树的遍历，记为 issue。
                Err(err) => {
                    warn!("Skipping unreadable directory entry in {}: {err}", path.display());
                    self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::UnknownFile, &path));
                    None
                }
            })
            .collect();
        // 先排序保证跨目录的输出顺序确定（测试与日志可复现）。
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                warn!("Skipping unreadable entry type: {}", path.display());
                self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::UnknownFile, path));
                continue;
            };
            if file_type.is_dir() {
                self.visit(&path)?;
                continue;
            }
            if file_type.is_symlink() {
                warn!("Symbolic link found, ignoring: {}", path.display());
                self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::SymlinkSkipped, path));
                continue;
            }
            self.visit_file(&path);
        }
        Ok(())
    }

    fn visit_file(&mut self, path: &Path) {
        if let Err(err) = self.try_visit_file(path) {
            // 单个文件的怪异（类型不明、读不动等）不中止整棵目录树的遍历，
            // 记入 issues 让调用方汇总——几百张照片里混进一个坏文件是常态。
            warn!("Skipping {}: {err:#}", path.display());
            self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::UnknownFile, path));
        }
    }

    fn try_visit_file(&mut self, path: &Path) -> Result<()> {
        // 不跟随 symlink：配对时把链接当不存在（walk 层已把 symlink 记为 issue 跳过），
        // 否则「视频是指向别处的链接」会让视频入口不触发、图片入口因 video_path 存在
        // 也不触发，整个 stem 静默消失。
        let find_ext = |ext: &str| -> Option<PathBuf> {
            let lower = path.with_extension(ext);
            let upper = path.with_extension(ext.to_uppercase());
            // symlink_metadata 不跟随最后一段链接。
            let is_real_file = |p: &Path| p.symlink_metadata().map(|m| m.is_file()).unwrap_or(false);
            if is_real_file(&lower) {
                Some(lower)
            } else if is_real_file(&upper) {
                Some(upper)
            } else {
                None
            }
        };

        anyhow::ensure!(path.is_file(), "Unknown type: {}", path.display());
        let mut found_image = self.image_exts.iter().filter_map(|ext| find_ext(ext)).collect::<Vec<_>>();
        if found_image.is_empty() {
            return Ok(());
        }
        // 同一个 stem 可能同时存在 .HEIC（带 Apple HDR gainmap 的源）和 .JPG（SDR 导出），
        // iCloud 里很常见。排序后取第一个以保证结果确定——原来直接 pop() 一个 HashSet 的
        // 迭代结果，选中哪张是随机的。
        found_image.sort_by(|a, b| {
            image_priority(a)
                .cmp(&image_priority(b))
                .then_with(|| a.as_os_str().cmp(b.as_os_str()))
        });
        if found_image.len() > 1 {
            warn!("Multiple images found with the same name, using {}:", found_image[0].display());
            for extra in &found_image[1..] {
                if self.options.strict {
                    self.strict_conflicts
                        .push(DiscoveryIssue::new(DiscoveryIssueKind::DuplicateImage, extra));
                } else {
                    // 先挂到 stem 上，等触发路径真正建任务时再落 issues，避免重复计入。
                    let stem = found_image[0].with_extension("");
                    self.pending_duplicate_images.entry(stem).or_default().push(extra.clone());
                }
            }
        }
        let image_path = found_image.remove(0);

        // 只有 heic 带 Apple HDR gainmap；jpg/png 是 SDR，走转换也只是原样拷贝一份。
        if !image_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("heic")) {
            warn!("Not a heic (no Apple HDR gainmap), skipping {}", image_path.display());
            self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::NotHeic, &image_path));
            // 该 stem 不会建任务，pending 的重名图也一并落下，避免静默丢失。
            self.record_duplicate_image_issues(&image_path);
            return Ok(());
        }
        let mut found_video = self.video_exts.iter().filter_map(|ext| find_ext(ext)).collect::<Vec<_>>();
        found_video.sort();
        if found_video.len() > 1 {
            warn!("Multiple videos found with the same name, using {}:", found_video[0].display());
            for extra in &found_video[1..] {
                if self.options.strict {
                    self.strict_conflicts
                        .push(DiscoveryIssue::new(DiscoveryIssueKind::DuplicateVideo, extra));
                } else {
                    warn!("    - ignored {}", extra.display());
                    self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::DuplicateVideo, extra));
                }
            }
        }
        let video_path = found_video.into_iter().next();

        // 每个 stem 只建一个任务：有视频时由「迭代到视频」那次触发，静态照片（无视频）时
        // 由「迭代到图片」那次触发。否则同一个 stem 会从多个入口各建一次任务，-j 并行下
        // 会同时写同一个输出文件。
        let triggered_by_video = video_path
            .as_ref()
            .is_some_and(|video| video.as_os_str().eq_ignore_ascii_case(path.as_os_str()));
        let triggered_by_image = image_path.as_os_str().eq_ignore_ascii_case(path.as_os_str());
        let should_build = if video_path.is_some() {
            triggered_by_video
        } else {
            triggered_by_image
        };
        if !should_build {
            return Ok(());
        }
        self.record_duplicate_image_issues(&image_path);

        self.push_photo(image_path, video_path);
        Ok(())
    }

    /// 只有真正建了任务，重名图的 issue 才有意义：非触发路径会重复遍历同一个 stem，
    /// 在那里记 issue 会重复计入。`selected` 是这个 stem 选中的主图。
    fn record_duplicate_image_issues(&mut self, selected: &Path) {
        let stem = selected.with_extension("");
        for extra in self.pending_duplicate_images.remove(&stem).into_iter().flatten() {
            if extra.as_os_str() == selected.as_os_str() {
                continue;
            }
            warn!("    - ignored {}", extra.display());
            self.issues.push(DiscoveryIssue::new(DiscoveryIssueKind::DuplicateImage, extra));
        }
    }

    fn push_photo(&mut self, image_path: PathBuf, video_path: Option<PathBuf>) {
        let mut output_path = image_path.with_extension("jpg");
        if let Some(suffix) = self.options.output_suffix.as_deref() {
            let stem = output_path.file_stem().unwrap().to_str().unwrap();
            let extension = output_path.extension().unwrap().to_str().unwrap();
            let suffixed_filename = format!("{stem}{suffix}.{extension}");
            output_path.set_file_name(suffixed_filename);
        }
        self.photos.push(DiscoveredPhoto {
            image: image_path,
            video: video_path,
            output: output_path,
        });
    }

    /// 遍历结束后收尾：去重 + strict 汇总。
    fn finish(self) -> Result<Discovery> {
        if !self.strict_conflicts.is_empty() {
            bail!("{}", StrictErrors(self.strict_conflicts));
        }
        let mut photos = self.photos;
        // 防御：同一个输出文件不该出现两次，-j 并行下会互相踩。
        photos.sort_by(|a, b| a.output.cmp(&b.output));
        let collected = photos.len();
        photos.dedup_by(|a, b| a.output == b.output);
        if photos.len() != collected {
            warn!("Ignored {} duplicate task(s)", collected - photos.len());
        }
        let mut issues = self.issues;
        // 稳定排序让测试可以按 kind 断言单个元素，也让日志顺序确定。
        issues.sort();
        Ok(Discovery { photos, issues })
    }
}
