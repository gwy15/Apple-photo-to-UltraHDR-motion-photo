use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use tracing::*;
use tracing_indicatif::span_ext::IndicatifSpanExt;
use tracing_subscriber::prelude::*;

type Task = aa_photo_bridge::i2a::ConvertRequest;

#[derive(Parser)]
pub struct Args {
    /// Path to the directory containing images and videos to convert
    pub path: PathBuf,

    #[clap(short = 'e', long)]
    /// Path to the exiftool executable.
    pub exiftool: Option<PathBuf>,

    #[clap(short = 'j', long)]
    /// Run in parallel mode
    pub parallel: bool,

    #[clap(short = 'o', long, value_enum)]
    /// What to do with the original files.
    pub original: Original,

    #[clap(long)]
    /// Overwrite output file if already exist.
    pub overwrite_existing: bool,

    /// Add some suffix in output file to avoid filename collision. For example, 
    /// "--output-suffix _merge"
    #[clap(long)]
    pub output_suffix: Option<String>,

    #[clap(long)]
    /// Image extensions. Default: "heic,jpg,jpeg"
    image_extensions: Option<String>,

    #[clap(long)]
    /// Video extensions. Default: "mov,mp4"
    video_extensions: Option<String>,

    #[clap(short = 'q', long, default_value = "85")]
    /// Image quality. Default: 85
    pub image_quality: i32,

    #[clap(short = 'g', long, default_value = "85")]
    /// Gainmap quality. Default: 85
    pub gainmap_quality: i32,

    #[clap(long)]
    /// Strict mode: exit on multiple images / videos with same name.
    pub strict: bool,

    #[clap(short = 'v', long)]
    /// Print more detailed runtime information
    pub verbose: bool,
}
#[derive(clap::ValueEnum, Clone, Copy, PartialEq)]
pub enum Original {
    Keep,
    Delete,
}

/// 同一个 stem 有多张图片时的挑选优先级：heic（HDR 源）优先，其余次之。
fn image_priority(path: &Path) -> u8 {
    match path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("heic") => 0,
        _ => 1,
    }
}

impl Args {
    pub fn image_extensions(&self) -> HashSet<String> {
        self.image_extensions
            .as_deref()
            .unwrap_or("heic,jpg,jpeg")
            .split(',')
            .map(|s| s.to_ascii_lowercase().to_string())
            .collect()
    }
    pub fn video_extensions(&self) -> HashSet<String> {
        self.video_extensions
            .as_deref()
            .unwrap_or("mov,mp4")
            .split(',')
            .map(|s| s.to_ascii_lowercase().to_string())
            .collect()
    }

    fn visit(&self, path: &Path, tasks: &mut Vec<Task>) -> Result<()> {
        anyhow::ensure!(path.exists(), "Path does not exist: {:?}", path);
        anyhow::ensure!(path.is_dir(), "Not a directory: {:?}", path);

        let image_extensions = self.image_extensions();
        let video_extensions = self.video_extensions();
        // list
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let entry_type = entry.file_type()?;
            if entry_type.is_dir() {
                self.visit(&path, tasks)?;
                continue;
            }
            if entry_type.is_symlink() {
                warn!("Symbolic link found, ignoring: {path:?}");
            }
            self.visit_file(&path, &image_extensions, &video_extensions, tasks)?;
        }

        Ok(())
    }

    fn visit_file(
        &self,
        path: &Path,
        image_allow_ext: &HashSet<String>,
        video_allow_ext: &HashSet<String>,
        tasks: &mut Vec<Task>,
    ) -> Result<()> {
        let find_ext = |ext: &str| -> Option<PathBuf> {
            let lower = path.with_extension(ext);
            let upper = path.with_extension(ext.to_uppercase());
            if lower.exists() {
                Some(lower)
            } else if upper.exists() {
                Some(upper)
            } else {
                None
            }
        };

        anyhow::ensure!(path.is_file(), "Unknown type: {}", path.display());
        let mut found_image = image_allow_ext.iter().filter_map(|ext| find_ext(ext)).collect::<Vec<_>>();
        if found_image.is_empty() {
            return Ok(());
        }
        // 同一个 stem 可能同时存在 .HEIC（带 Apple HDR gainmap 的源）和 .JPG（SDR 导出），
        // iCloud 里很常见。排序后取第一个以保证结果确定——原来直接 pop() 一个 HashSet 的
        // 迭代结果，选中哪张是随机的。
        found_image.sort_by(|a, b| image_priority(a).cmp(&image_priority(b)).then_with(|| a.as_os_str().cmp(b.as_os_str())));
        if found_image.len() > 1 {
            warn!("Multiple images found with the same name, using {}:", found_image[0].display());
            for extra in &found_image[1..] {
                warn!("    - ignored {}", extra.display());
            }
            if self.strict {
                std::process::exit(1);
            }
        }
        let image_path = found_image.remove(0);

        // 只有 heic 带 Apple HDR gainmap；jpg/png 是 SDR，走转换也只是原样拷贝一份。
        if !image_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("heic")) {
            warn!("Not a heic (no Apple HDR gainmap), skipping {}", image_path.display());
            return Ok(());
        }

        let mut found_video = video_allow_ext.iter().filter_map(|ext| find_ext(ext)).collect::<Vec<_>>();
        found_video.sort();
        if found_video.len() > 1 {
            warn!("Multiple videos found with the same name, using {}:", found_video[0].display());
            for extra in &found_video[1..] {
                warn!("    - ignored {}", extra.display());
            }
            if self.strict {
                std::process::exit(1);
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
        let should_build = if video_path.is_some() { triggered_by_video } else { triggered_by_image };
        if !should_build {
            return Ok(());
        }

        let mut output_path = image_path.with_extension("jpg");
        if let Some(suffix) = self.output_suffix.as_deref() {
            let stem = output_path.file_stem().unwrap().to_str().unwrap();
            let extension = output_path.extension().unwrap().to_str().unwrap();
            let suffixed_filename = format!("{stem}{suffix}.{extension}");
            output_path.set_file_name(suffixed_filename);
        }
        tasks.push(Task {
            image_path,
            video_path,
            output_path,
            exiftool_path: self.exiftool.clone(),
            image_quality: self.image_quality,
            gainmap_quality: self.gainmap_quality,
            overwrite_existing: self.overwrite_existing,
        });
        Ok(())
    }
}

fn run(task: Task, original: Original) -> Result<()> {
    task.convert().with_context(|| format!("Task failed for {task:#?}"))?;
    if original == Original::Delete {
        task.delete_original()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    let indicatif_layer = tracing_indicatif::IndicatifLayer::new();
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(indicatif_layer.get_stderr_writer().with_max_level(level));

    tracing_subscriber::registry().with(fmt_layer).with(indicatif_layer).init();

    // 1. collect all tasks
    let mut tasks = Vec::new();
    args.visit(&args.path, &mut tasks)?;
    // 防御：同一个输出文件不该出现两次，-j 并行下会互相踩。
    tasks.sort_by(|a, b| a.output_path.cmp(&b.output_path));
    let collected = tasks.len();
    tasks.dedup_by(|a, b| a.output_path == b.output_path);
    if tasks.len() != collected {
        warn!("Ignored {} duplicate task(s)", collected - tasks.len());
    }

    // 2. run all tasks
    info!("Running {} tasks", tasks.len());
    let bar_style = indicatif::ProgressStyle::with_template("{pos:>5}/{len:5} ({percent:>2}%) {wide_bar} {elapsed:5}/{eta:<5}")?;
    let span = tracing::info_span!("Running tasks");
    span.pb_set_style(&bar_style);
    span.pb_set_length(tasks.len() as u64);
    let guard = span.enter();
    let t = std::time::Instant::now();

    // 单个文件失败不该中止整批：几百张照片里有一张坏文件是常态。
    // 记下失败数，全部跑完后汇总，并以非 0 退出码反映出来。
    let failures = AtomicUsize::new(0);
    let run_task = |task: Task| {
        if let Err(err) = run(task, args.original) {
            error!("{err:#}");
            failures.fetch_add(1, Ordering::Relaxed);
        }
        span.pb_inc(1);
    };

    if args.parallel {
        tasks.into_par_iter().for_each(run_task);
    } else {
        tasks.into_iter().for_each(run_task);
    }

    drop(guard);
    let failures = failures.load(Ordering::Relaxed);
    info!("All tasks completed in {:?}, {failures} failed", t.elapsed());

    anyhow::ensure!(failures == 0, "{failures} task(s) failed");
    Ok(())
}
