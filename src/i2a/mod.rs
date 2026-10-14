use anyhow::Context;
use std::path::PathBuf;

mod convert;
mod merge;
mod utils;
pub mod video;

#[derive(Debug)]
pub struct ConvertRequest {
    pub image_path: PathBuf,
    /// 同名视频。None 表示这是一张静态照片，没有动态照片视频要追加。
    pub video_path: Option<PathBuf>,
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
        let video = self
            .video_path
            .as_deref()
            .map_or_else(|| "<static photo>".to_string(), |p| p.display().to_string());
        debug!(
            "Running convert request {} + {} => {}",
            self.image_path.display(),
            video,
            self.output_path.display(),
        );

        self.check_valid().context("request arguments invalid")?;
        let t = std::time::Instant::now();

        // 1. convert image（基础图；元数据统一在末尾用一次 exiftool 写入）
        let mut guard = self.make_hdr()?;

        // 2. append video，记录实际追加的视频大小
        let appended_video_size = self.make_motion()?;

        // 3. 一次性写完全部元数据（原先 copy_meta + update_motion_photo_exif 会让
        //    exiftool 用 -overwrite_original 把整个输出再重写 1~2 遍）
        self.write_output_metadata(&self.image_path, appended_video_size)?;
        self.sync_file_times(&self.image_path, &self.output_path)?;

        #[rustfmt::skip]
        let output_size = self.output_path.metadata().context("Output is gone")?.len() as f32 / 1024.0 / 1024.0;
        info!(
            "convert success in {:.2?}: {} + {} => {} (size={output_size:.2} MiB)",
            t.elapsed(),
            self.image_path.display(),
            video,
            self.output_path.display(),
        );

        guard.cancel();
        Ok(())
    }

    pub fn delete_original(&self) -> anyhow::Result<()> {
        if !self.io_same_file() {
            std::fs::remove_file(&self.image_path).context("delete original image failed")?;
        }
        if let Some(video_path) = self.video_path.as_deref() {
            std::fs::remove_file(video_path).context("delete original video failed")?;
        }
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

    /// 追加动态照片视频。返回实际追加到输出末尾的视频字节数（静态照片返回 None）。
    /// 元数据写入与文件时间对齐统一由 convert() 在末尾处理。
    #[instrument(skip_all)]
    fn make_motion(&self) -> anyhow::Result<Option<u64>> {
        // 静态照片：没有视频要追加。
        let Some(video_path) = self.video_path.as_deref() else {
            debug!("no video, treat as static photo");
            return Ok(None);
        };

        // heic 输出的基础图是刚编码出来的，不可能已经是 motion photo；
        // 只有非 heic 输入走拷贝路径时才有可能，检查留给那种情况。
        if !self.is_input_heic()? && self.output_is_motion_photo()? {
            warn!("Output is already a motion photo, skip append video");
            return Ok(None);
        }

        // convert mov to mp4 (and ensure audio codec is supported)
        let Some(audio_codec) = video::VideoUtils::get_audio_codec(video_path)? else {
            debug!("no audio in video, append");
            self.append_video(video_path)?;
            return Ok(Some(video_path.metadata()?.len()));
        };
        debug!(%audio_codec, "input video");
        if audio_codec == "aac" || audio_codec == "ac3" {
            self.append_video(video_path)?;
            return Ok(Some(video_path.metadata()?.len()));
        }

        let video_name = video_path.file_stem().context("parse video path filename failed")?;
        let tmp_video_name = format!("{}-aac-converting.mp4", video_name.to_string_lossy());
        let tmp_video = video_path.with_file_name(tmp_video_name);
        anyhow::ensure!(!tmp_video.exists(), "tempfile exists");
        let _guard = utils::Guard::new(|| {
            std::fs::remove_file(&tmp_video).ok();
        });

        info_span!("transcode_aac_audio")
            .in_scope(|| {
                video::VideoAudioEncodeRequest {
                    input: video_path,
                    output: &tmp_video,
                    bit_rate: 128 << 10,
                    encoder: "aac",
                }
                .execute()
            })
            .context("convert video audio to aac failed")?;

        self.append_video(&tmp_video)?;
        let appended = tmp_video.metadata()?.len();
        Ok(Some(appended))
    }
}
