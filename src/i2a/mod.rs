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
}

impl ConvertRequest {
    pub fn execute(self) -> anyhow::Result<()> {
        self.check_valid().context("request arguments invalid")?;

        // only heic is supported
        let converted = self.ensure_jpg()?;
        if !converted {
            self.copy_image()?;
        }

        self.append_video()?;
        self.update_exif()?;
        Self::sync_file_times(&self.video_path, &self.output_path)?;

        Ok(())
    }

    fn exif_tool(&self) -> crate::utils::ExifTool {
        match self.exiftool_path.as_ref() {
            Some(path) => crate::utils::ExifTool::with_path(path.clone()),
            None => crate::utils::ExifTool::new(),
        }
    }
}
