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

    pub overwrite_existing: bool,

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
        let ans = self.image_extension()?.eq_ignore_ascii_case("heic");
        Ok(ans)
    }

    pub fn convert(&self) -> anyhow::Result<()> {
        debug!(
            "Running convert request {} + {} => {}",
            self.image_path.display(),
            self.video_path.display(),
            self.output_path.display(),
        );

        self.check_valid().context("request arguments invalid")?;
        let t = std::time::Instant::now();

        // 1. convert image
        let mut guard = self.make_hdr()?;

        // 2. append video
        self.make_motion()?;

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

    fn make_hdr(&self) -> anyhow::Result<utils::Guard<impl FnOnce() + '_>> {
        let t = std::time::Instant::now();
        match self.is_input_heic()? {
            true => self.convert_heic_to_jpg()?,
            false => self.copy_image()?,
        }
        let guard = utils::Guard::new(|| {
            // in case rest failed, remove generated output
            std::fs::remove_file(&self.output_path).ok();
        });
        debug!("jpg ensured (with HDR effect), time={:?}", t.elapsed());
        Ok(guard)
    }

    #[instrument(skip_all)]
    fn make_motion(&self) -> anyhow::Result<()> {
        if self.output_is_motion_photo()? {
            warn!("Output is already a motion photo, skip append video");
            self.sync_file_times(&self.image_path, &self.output_path)?;
            return Ok(());
        }

        // convert mov to mp4 (and ensure audio codec is supported)
        let Some(audio_codec) = video::VideoUtils::get_audio_codec(&self.video_path)? else {
            debug!("no audio in video, append");
            self.append_video(&self.video_path)?;
            self.update_motion_photo_exif(&self.video_path)?;
            self.sync_file_times(&self.image_path, &self.output_path)?;
            return Ok(());
        };
        debug!(%audio_codec, "input video");
        if audio_codec == "aac" || audio_codec == "ac3" {
            self.append_video(&self.video_path)?;
            self.update_motion_photo_exif(&self.video_path)?;
            self.sync_file_times(&self.image_path, &self.output_path)?;
            return Ok(());
        }

        let video_name = self.video_path.file_stem().context("parse video path filename failed")?;
        let tmp_video_name = format!("{}-aac-converting.mp4", video_name.to_string_lossy());
        let tmp_video = self.video_path.with_file_name(tmp_video_name);
        anyhow::ensure!(!tmp_video.exists(), "tempfile exists");
        let _guard = utils::Guard::new(|| {
            std::fs::remove_file(&tmp_video).ok();
        });

        info_span!("transcode_aac_audio")
            .in_scope(|| {
                video::VideoAudioEncodeRequest {
                    input: &self.video_path,
                    output: &tmp_video,
                    bit_rate: 128 << 10,
                    encoder: "aac",
                }
                .execute()
            })
            .context("convert video audio to aac failed")?;

        self.append_video(&tmp_video)?;
        self.update_motion_photo_exif(&tmp_video)?;
        self.sync_file_times(&self.image_path, &self.output_path)?;
        Ok(())
    }
}
