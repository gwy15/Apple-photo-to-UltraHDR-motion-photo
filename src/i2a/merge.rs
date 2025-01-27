//! iOS to Android
//!

use std::path::Path;

use anyhow::{bail, Context, Result};

use super::ConvertRequest;

impl ConvertRequest {
    /// check if the request is valid
    pub(crate) fn check_valid(&self) -> Result<()> {
        if !self.image_path.exists() {
            bail!("Image does not exist");
        }
        if !self.image_path.is_file() {
            bail!("Image is not a file");
        }
        if !self.video_path.exists() {
            bail!("Video does not exist");
        }
        if !self.video_path.is_file() {
            bail!("Video is not a file");
        }
        if self.output_path.is_dir() {
            bail!("Output path is a directory");
        }
        let output_ext = self
            .output_path
            .extension()
            .context("output path has no extension")?;
        anyhow::ensure!(
            ["jpg", "jpeg"]
                .iter()
                .any(|e| output_ext.eq_ignore_ascii_case(e)),
            "Output path must have jpg extension"
        );
        if !self.io_same_file() && self.output_path.exists() {
            bail!("Output file already exists");
        }
        let parent = self
            .output_path
            .parent()
            .context("Invalid output path: no parent")?;
        if !parent.exists() {
            bail!("Output path parent does not exist. You must create it with proper permissions.");
        }
        Ok(())
    }

    pub(crate) fn copy_image(&self) -> anyhow::Result<()> {
        let mut output = std::fs::File::create(&self.output_path)?;
        let mut image = std::fs::File::open(&self.image_path)?;
        std::io::copy(&mut image, &mut output)?;
        Ok(())
    }
    pub(crate) fn append_video(&self) -> anyhow::Result<()> {
        let mut output = std::fs::File::options()
            .append(true)
            .truncate(false)
            .open(&self.output_path)?;
        let mut video = std::fs::File::open(&self.video_path)?;
        std::io::copy(&mut video, &mut output)?;
        Ok(())
    }

    pub(crate) fn sync_file_times(src: &Path, dst: &Path) -> Result<()> {
        #[cfg(target_os = "macos")]
        use std::os::macos::fs::FileTimesExt;
        #[cfg(target_os = "windows")]
        use std::os::windows::fs::FileTimesExt;

        let src_meta = src.metadata()?;
        let file_times = std::fs::FileTimes::new().set_modified(src_meta.modified()?);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let file_times = file_times.set_created(src_meta.created()?);

        let dst = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(dst)?;
        dst.set_times(file_times)?;
        Ok(())
    }

    pub(crate) fn update_exif(&self) -> anyhow::Result<()> {
        // release exiftool.config
        use std::io::Write;
        const EXIFTOOL_CONFIG: &str = include_str!("exiftool.config");
        let mut temp_config = tempfile::NamedTempFile::new()?;
        temp_config.write_all(EXIFTOOL_CONFIG.as_bytes())?;
        temp_config.flush()?;

        let mut cmd = self.exif_tool().command();
        let video_size = self.video_path.metadata()?.len();
        cmd.args(["-config", temp_config.path().to_str().unwrap()])
            .args([
                "-XMP-GCamera:MicroVideo=1",
                "-XMP-GCamera:MicroVideoVersion=1",
                "-XMP-GCamera:MicroVideoPresentationTimestampUs=1500000",
            ])
            .arg(format!("-XMP-GCamera:MicroVideoOffset={video_size}"))
            .arg("-XiaomiTag=1")
            .arg("-overwrite_original")
            .arg(&self.output_path);
        let output = cmd.output().context("Run exiftool failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Run exiftool failed: {}", stderr);
        }

        /* 小米的 tag 写不进去，放弃 exiv2 库
        let metadata =
            rexiv2::Metadata::new_from_path(&self.output_path).context("rexiv2 open failed")?;
        // set namespace
        rexiv2::register_xmp_namespace("http://ns.google.com/photos/1.0/camera/", "GCamera").ok();
        metadata
            .set_tag_numeric("Xmp.GCamera.MicroVideo", 1)
            .context("set Xmp.GCamera.MicroVideo failed")?;
        metadata.set_tag_numeric("Xmp.GCamera.MicroVideoVersion", 1)?;
        let offset = self.video_path.metadata()?.len();
        metadata.set_tag_numeric("Xmp.GCamera.MicroVideoOffset", offset as i32)?;
        // # in Apple Live Photos, the chosen photo is 1.5s after the start of the video, so 1500000 microseconds
        metadata.set_tag_numeric("Xmp.GCamera.MicroVideoPresentationTimestampUs", 1500000)?;
        // Xiaomi magic tag，这个 exiv2 写不进去
        metadata.set_tag_numeric("Exif.Photo.0x8897", 0x1)?;
        metadata
            .save_to_file(&self.output_path)
            .context("rexiv2 save failed")?;
        */

        Ok(())
    }
}
