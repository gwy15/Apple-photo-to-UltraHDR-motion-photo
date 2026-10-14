use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use tracing::*;
use tracing_indicatif::span_ext::IndicatifSpanExt;
use tracing_subscriber::prelude::*;

use aa_photo_bridge::discover::{self, DiscoverOptions};

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

impl Args {
    fn discover_options(&self) -> DiscoverOptions {
        DiscoverOptions {
            image_extensions: self.image_extensions.clone(),
            video_extensions: self.video_extensions.clone(),
            output_suffix: self.output_suffix.clone(),
            strict: self.strict,
        }
    }
}

fn run(task: aa_photo_bridge::i2a::ConvertRequest, original: Original) -> Result<()> {
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
    // 非致命问题由 discover 内部统一 warn（单一报告层），strict 冲突直接 Err。
    let discovery = discover::discover(&args.path, &args.discover_options()).context("Task discovery failed")?;
    let tasks: Vec<aa_photo_bridge::i2a::ConvertRequest> = discovery
        .photos
        .into_iter()
        .map(|p| aa_photo_bridge::i2a::ConvertRequest {
            image_path: p.image,
            video_path: p.video,
            output_path: p.output,
            exiftool_path: args.exiftool.clone(),
            image_quality: args.image_quality,
            gainmap_quality: args.gainmap_quality,
            overwrite_existing: args.overwrite_existing,
        })
        .collect();

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
    let run_task = |task: aa_photo_bridge::i2a::ConvertRequest| {
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
