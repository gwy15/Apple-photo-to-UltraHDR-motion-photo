use anyhow::{Context, Result};
use clap::Parser;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tracing::*;
use tracing_indicatif::span_ext::IndicatifSpanExt;
use tracing_subscriber::prelude::*;

type Task = motion::i2a::ConvertRequest;

#[derive(Parser)]
pub struct Args {
    /// Path to the directory containing images and videos to convert
    pub path: PathBuf,

    #[clap(short = 'e', long)]
    /// Path to the exiftool executable.
    pub exiftool: Option<PathBuf>,

    #[clap(short = 'o', long, value_enum)]
    /// What to do with the original files.
    pub original: Original,

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

    #[clap(short = 'v', long)]
    /// Print more detailed runtime information
    pub verbose: bool,
}
#[derive(clap::ValueEnum, Clone, PartialEq)]
pub enum Original {
    Keep,
    Delete,
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
        let mut found_image = image_allow_ext
            .iter()
            .filter_map(|ext| find_ext(ext).or_else(|| find_ext(&ext.to_uppercase())))
            .collect::<Vec<_>>();
        if found_image.is_empty() {
            return Ok(());
        }
        if found_image.len() > 1 {
            warn!("Multiple images found with the same name found:");
            for path in found_image {
                warn!("    - {}", path.display());
            }
            return Ok(());
        }
        let image_path = found_image.pop().unwrap();
        if image_path
            .as_os_str()
            .eq_ignore_ascii_case(path.as_os_str())
        {
            return Ok(());
        }
        // anyhow::ensure!(image_path == path, "{image_path:#?} != {path:#?}");

        let mut found_video = video_allow_ext
            .iter()
            .filter_map(|ext| find_ext(ext).or_else(|| find_ext(&ext.to_uppercase())))
            .collect::<Vec<_>>();
        if found_video.is_empty() {
            return Ok(());
        }
        if found_video.len() > 1 {
            warn!("Multiple videos found with the same name found:");
            for path in found_video {
                warn!("    - {}", path.display());
            }
            return Ok(());
        }
        let video_path = found_video.pop().unwrap();

        let output_path = path.with_extension("jpg");
        tasks.push(Task {
            image_path,
            video_path,
            output_path,
            exiftool_path: self.exiftool.clone(),
            image_quality: self.image_quality,
            gainmap_quality: self.gainmap_quality,
        });
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    let indicatif_layer = tracing_indicatif::IndicatifLayer::new();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(indicatif_layer.get_stderr_writer().with_max_level(level));

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(indicatif_layer)
        .init();

    // 1. collect all tasks
    let mut tasks = Vec::new();
    args.visit(&args.path, &mut tasks)?;

    // 2. run all tasks
    info!("Running {} tasks", tasks.len());
    let bar_style = indicatif::ProgressStyle::with_template(
        "{pos:>5}/{len:5} ({percent:>2}%) {wide_bar} {elapsed:5}/{eta:<5}",
    )?;
    let span = tracing::info_span!("Running tasks");
    span.pb_set_style(&bar_style);
    span.pb_set_length(tasks.len() as u64);
    let guard = span.enter();
    let t = std::time::Instant::now();
    for task in tasks {
        task.convert()
            .with_context(|| format!("Task failed for {task:#?}"))?;

        if args.original == Original::Delete {
            task.delete_original()?;
        }

        Span::current().pb_inc(1);
    }
    drop(guard);
    info!("All tasks completed in {:?}", t.elapsed());

    Ok(())
}
