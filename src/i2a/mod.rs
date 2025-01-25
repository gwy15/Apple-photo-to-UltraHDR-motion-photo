use anyhow::Context;
use std::path::PathBuf;

mod convert;
mod merge;

pub struct ConvertRequest {
    pub image_path: PathBuf,
    pub video_path: PathBuf,
    pub output_path: PathBuf,
    // 用 exiftool 实现 exif 写入
    pub exiftool_path: Option<PathBuf>,

    /// [0, 100]
    pub image_quality: i32,
    /// [0, 100]
    pub gainmap_quality: i32,
}

impl ConvertRequest {
    pub fn execute(self) -> anyhow::Result<()> {
        let t = std::time::Instant::now();
        self.check_valid().context("request arguments invalid")?;

        // only heic is supported
        let converted = self.ensure_jpg()?;
        if !converted {
            self.copy_image()?;
        }
        debug!("jpg ensured (with HDR effect), time={:?}", t.elapsed());

        self.append_video()?;
        self.update_exif()?;
        Self::sync_file_times(&self.video_path, &self.output_path)?;

        let output_size = self
            .output_path
            .metadata()
            .context("Output is gone??")?
            .len() as f32
            / 1024.0
            / 1024.0;
        info!(
            "convert success in {:?}: {} + {} => {} (size={output_size:.2} MiB)",
            t.elapsed(),
            self.image_path.display(),
            self.video_path.display(),
            self.output_path.display(),
        );

        Ok(())
    }

    fn exif_tool(&self) -> crate::utils::ExifTool {
        match self.exiftool_path.as_ref() {
            Some(path) => crate::utils::ExifTool::with_path(path.clone()),
            None => crate::utils::ExifTool::new(),
        }
    }
}
