//! 任务发现 (task discovery) 的集成测试。
//! 全部走公共 seam：`discover(root, &DiscoverOptions)`。

use aa_photo_bridge::discover::{discover, DiscoverOptions, DiscoveryIssueKind};
use std::{collections::HashSet, fs, path::PathBuf};

/// 每个测试一个临时目录 fixture。
struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Self {
        Fixture(tempfile::tempdir().unwrap())
    }
    fn root(&self) -> PathBuf {
        self.0.path().to_path_buf()
    }
    /// 在根目录创建一个空文件，返回其路径。
    fn touch(&self, name: &str) -> PathBuf {
        let p = self.root().join(name);
        fs::write(&p, b"x").unwrap();
        p
    }
}

fn opts() -> DiscoverOptions {
    DiscoverOptions::default()
}

#[test]
fn heic_with_paired_mov_discovers_one_task() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0001.HEIC");
    let mov = f.touch("IMG_0001.MOV");

    let d = discover(&f.root(), &opts()).unwrap();

    assert!(d.issues.is_empty(), "unexpected issues: {:?}", d.issues);
    assert_eq!(d.photos.len(), 1);
    let p = &d.photos[0];
    assert_eq!(p.image, heic);
    assert_eq!(p.video.as_deref(), Some(mov.as_path()));
    assert_eq!(p.output, f.root().join("IMG_0001.jpg"));
}

#[test]
fn heic_wins_over_jpg_with_same_stem() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0002.HEIC");
    let jpg = f.touch("IMG_0002.JPG");

    let d = discover(&f.root(), &opts()).unwrap();

    assert_eq!(d.photos.len(), 1);
    assert_eq!(d.photos[0].image, heic);
    // jpg 被忽略要记 issue
    assert_eq!(d.issues.len(), 1);
    assert_eq!(d.issues[0].kind, DiscoveryIssueKind::DuplicateImage);
    assert_eq!(d.issues[0].path, jpg);
}

#[test]
fn static_heic_without_video_discovers_one_task() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0003.HEIC");

    let d = discover(&f.root(), &opts()).unwrap();

    assert!(d.issues.is_empty(), "unexpected issues: {:?}", d.issues);
    assert_eq!(d.photos.len(), 1);
    assert_eq!(d.photos[0].image, heic);
    assert_eq!(d.photos[0].video, None);
    assert_eq!(d.photos[0].output, f.root().join("IMG_0003.jpg"));
}

#[test]
fn jpg_only_stem_is_skipped_as_not_heic() {
    let f = Fixture::new();
    let jpg = f.touch("IMG_0004.jpg");

    let d = discover(&f.root(), &opts()).unwrap();

    assert!(d.photos.is_empty());
    assert_eq!(d.issues.len(), 1);
    assert_eq!(d.issues[0].kind, DiscoveryIssueKind::NotHeic);
    assert_eq!(d.issues[0].path, jpg);
}

#[test]
fn paired_stem_builds_exactly_one_task_whichever_file_is_visited() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0005.HEIC");
    let mov = f.touch("IMG_0005.MOV");

    let d = discover(&f.root(), &opts()).unwrap();

    // 有视频时由「迭代到视频」触发；图片入口不该再建一次任务。
    assert_eq!(d.photos.len(), 1, "photos: {:?}", d.photos);
    assert_eq!(d.photos[0].image, heic);
    assert_eq!(d.photos[0].video.as_deref(), Some(mov.as_path()));
}

#[test]
fn heic_and_mov_in_subdirectory_are_discovered() {
    let f = Fixture::new();
    let sub = f.root().join("sub");
    fs::create_dir(&sub).unwrap();
    let heic = sub.join("IMG_0006.heic");
    let mov = sub.join("IMG_0006.mov");
    fs::write(&heic, b"x").unwrap();
    fs::write(&mov, b"x").unwrap();

    let d = discover(&f.root(), &opts()).unwrap();

    assert!(d.issues.is_empty(), "unexpected issues: {:?}", d.issues);
    assert_eq!(d.photos.len(), 1);
    assert_eq!(d.photos[0].image, heic);
    assert_eq!(d.photos[0].output, sub.join("IMG_0006.jpg"));
}

#[test]
fn unknown_extension_file_is_ignored_silently() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0007.HEIC");
    f.touch("notes.txt");

    let d = discover(&f.root(), &opts()).unwrap();

    // 扩展名不认识的普通文件是目录里的常态（.txt、.DS_Store…），直接忽略。
    assert_eq!(d.photos.len(), 1);
    assert_eq!(d.photos[0].image, heic);
    assert!(d.issues.is_empty(), "unexpected issues: {:?}", d.issues);
}

#[test]
fn symlink_is_skipped_and_recorded() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0008.HEIC");
    let target = f.touch("IMG_0009.HEIC");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, f.root().join("link.HEIC")).unwrap();

    let d = discover(&f.root(), &opts()).unwrap();

    assert_eq!(d.photos.len(), 2, "symlink 的目标文件仍应作为普通文件处理: {:?}", d.photos);
    assert!(d.photos.iter().any(|p| p.image == heic));
    assert!(d.issues.iter().any(|i| i.kind == DiscoveryIssueKind::SymlinkSkipped));
}

#[test]
fn custom_output_suffix_lands_in_output_name() {
    let f = Fixture::new();
    f.touch("IMG_0010.HEIC");

    let d = discover(
        &f.root(),
        &DiscoverOptions {
            output_suffix: Some("_merge".into()),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(d.photos[0].output, f.root().join("IMG_0010_merge.jpg"));
}

#[test]
fn nonexistent_root_is_a_hard_error() {
    let f = Fixture::new();
    let missing = f.root().join("nope");

    let err = discover(&missing, &opts()).unwrap_err();

    assert!(err.to_string().contains("does not exist"), "{err:#}");
}

#[test]
fn strict_mode_reports_all_duplicate_conflicts() {
    let f = Fixture::new();
    // 两个 stem 各有重名冲突。
    f.touch("IMG_0020.HEIC");
    f.touch("IMG_0020.JPG");
    f.touch("IMG_0021.HEIC");
    f.touch("IMG_0021.JPG");
    f.touch("IMG_0022.HEIC"); // 无冲突，strict 不该拦它

    let opts = DiscoverOptions {
        strict: true,
        ..Default::default()
    };
    let err = discover(&f.root(), &opts).unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("IMG_0020"), "{msg}");
    assert!(msg.contains("IMG_0021"), "{msg}");
    assert!(!msg.contains("IMG_0022"), "{msg}");
}

#[test]
fn strict_mode_also_reports_duplicate_videos() {
    let f = Fixture::new();
    f.touch("IMG_0030.HEIC");
    f.touch("IMG_0030.MOV");
    f.touch("IMG_0030.mp4");

    let opts = DiscoverOptions {
        strict: true,
        ..Default::default()
    };
    let err = discover(&f.root(), &opts).unwrap_err();

    let msg = format!("{err:#}");
    assert!(msg.contains("DuplicateVideo"), "{msg}");
    assert!(msg.contains("IMG_0030.mp4"), "{msg}");
}

#[test]
fn duplicate_outputs_are_deduped() {
    let f = Fixture::new();
    // 同一 stem 两张图：heic 选中，jpg 被忽略，但两个入口的输出名相同。
    // 触发规则已保证每 stem 一个任务，这里验证的是发现结果层面无重复输出。
    let heic = f.touch("IMG_0031.HEIC");
    let mov = f.touch("IMG_0031.MOV");
    f.touch("IMG_0031.JPG");

    let d = discover(&f.root(), &opts()).unwrap();

    let outputs: Vec<_> = d.photos.iter().map(|p| &p.output).collect();
    assert_eq!(d.photos.len(), 1, "photos: {:?}", d.photos);
    assert_eq!(outputs.len(), outputs.iter().collect::<HashSet<_>>().len());
    assert_eq!(d.photos[0].image, heic);
    assert_eq!(d.photos[0].video.as_deref(), Some(mov.as_path()));
}

#[test]
fn symlinked_pair_is_not_selected_by_find_ext() {
    let f = Fixture::new();
    let heic = f.touch("IMG_0032.HEIC");
    // 视频是指向目录外文件的 symlink：walk 跳过它，配对也不该把它选进来。
    let external = f.root().join("external.MOV");
    fs::write(&external, b"x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external, f.root().join("IMG_0032.MOV")).unwrap();

    let d = discover(&f.root(), &opts()).unwrap();

    // 视频 entry 被 walk 跳过 + find_ext 不跟随链接 → 退化为静态照片任务，而不是消失。
    assert_eq!(d.photos.len(), 1, "photos: {:?}", d.photos);
    assert_eq!(d.photos[0].image, heic);
    assert_eq!(d.photos[0].video, None);
    assert!(d.issues.iter().any(|i| i.kind == DiscoveryIssueKind::SymlinkSkipped));
}
