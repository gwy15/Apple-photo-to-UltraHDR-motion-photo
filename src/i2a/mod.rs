use anyhow::Context;
use std::path::PathBuf;

mod convert;
mod merge;
mod utils;
pub mod video;

#[derive(Debug)]
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
    /// Input and output is same file
    pub fn io_same_file(&self) -> bool {
        self.image_path.as_os_str().eq_ignore_ascii_case(self.output_path.as_os_str())
    }

    fn is_input_heic(&self) -> anyhow::Result<bool> {
        let ans = self.image_extension()?.to_ascii_lowercase() == "heic";
        Ok(ans)
    }

    pub fn convert(&self) -> anyhow::Result<()> {
        debug!(
            "Running convert request {} + {} => {}",
            self.image_path.display(),
            self.video_path.display(),
            self.output_path.display(),
        );
        let t = std::time::Instant::now();
        self.check_valid().context("request arguments invalid")?;

        // 1. convert image
        match self.is_input_heic()? {
            true => self.convert_heic_to_jpg()?,
            false => self.copy_image()?,
        }
        let mut guard = utils::Guard::new(|| {
            // in case rest failed, remove generated output
            std::fs::remove_file(&self.output_path).ok();
        });
        debug!("jpg ensured (with HDR effect), time={:?}", t.elapsed());

        // 2. append video
        match self.output_is_motion_photo()? {
            true => warn!("Output is already a motion photo, skip append video"),
            false => {
                // TODO: convert mov to mp4 (and ensure audio codec is supported)
                self.append_video()?;
                self.update_motion_photo_exif()?;
            }
        }
        Self::sync_file_times(&self.image_path, &self.output_path)?;

        #[rustfmt::skip]
        let output_size = self.output_path.metadata().context("Output is gone")?.len() as f32 / 1024.0 / 1024.0;
        info!(
            "convert success in {:.2?}: {} + {} => {} (size={output_size:.2} MiB)",
            t.elapsed(),
            self.image_path.display(),
            self.video_path.display(),
            self.output_path.display(),
        );

        guard.cancel();
        Ok(())
    }

    pub fn delete_original(&self) -> anyhow::Result<()> {
        if !self.io_same_file() {
            std::fs::remove_file(&self.image_path).context("delete original image failed")?;
        }
        std::fs::remove_file(&self.video_path).context("delete original video failed")?;
        Ok(())
    }

    fn exif_tool(&self) -> crate::utils::ExifTool {
        match self.exiftool_path.as_ref() {
            Some(path) => crate::utils::ExifTool::with_path(path.clone()),
            None => crate::utils::ExifTool::new(),
        }
    }
}
