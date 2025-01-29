use anyhow::{Context, Result};
use std::{
    ffi::CString,
    path::{Path, PathBuf},
};

pub struct VideoAudioEncodeRequest<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
}

impl VideoAudioEncodeRequest<'_> {
    pub fn execute(&self) -> Result<()> {
        // 1.a open input
        let mut input_options = None;
        let input = self.input.to_str().context("input path to_str failed")?;
        let input = CString::new(input)?;
        let mut input_context =
            rsmpeg::avformat::AVFormatContextInput::open(&input, None, &mut input_options)?;

        // 1.b open output
        let output = self.output.to_str().context("output path to_str failed")?;
        let output = CString::new(output)?;
        let mut output_context = rsmpeg::avformat::AVFormatContextOutput::create(&output, None)?;

        // 2.a configurations: video
        let (input_video_idx, output_video_idx) = {
            let (input_video_idx, _) = input_context
                .find_best_stream(rsmpeg::ffi::AVMEDIA_TYPE_VIDEO)
                .context("Find video stream failed")?
                .context("No video stream found")?;
            let input_video = &input_context.streams()[input_video_idx];
            let mut output_video = output_context.new_stream();
            output_video.codecpar_mut().copy(&input_video.codecpar());
            (input_video_idx, output_video.index as usize)
        };
        debug!(%input_video_idx, %output_video_idx, "video configured");

        // 2.b configurations: audio
        let (input_audio_idx, output_audio_idx) = {
            let (input_audio_idx, _) = input_context
                .find_best_stream(rsmpeg::ffi::AVMEDIA_TYPE_AUDIO)
                .context("Find audio stream failed")?
                .context("No audio stream found")?;
            let input_audio = &input_context.streams()[input_audio_idx];
            let input_audio_codecpar = &input_audio.codecpar();
            let mut output_audio = output_context.new_stream();
            let output_audio_encoder =
                rsmpeg::avcodec::AVCodec::find_encoder(rsmpeg::ffi::AV_CODEC_ID_AAC)
                    .context("No AAC encoder builtin.")?;
            let mut output_audio_ctx = rsmpeg::avcodec::AVCodecContext::new(&output_audio_encoder);
            output_audio_ctx.set_sample_rate(input_audio_codecpar.sample_rate);
            output_audio_ctx.set_bit_rate(128000);
            output_audio_ctx.set_ch_layout(**input_audio_codecpar.ch_layout());
            let sample_fmt = output_audio_encoder
                .sample_fmts()
                .context("no aac sample fmt")?;
            output_audio_ctx.set_sample_fmt(sample_fmt[0]);
            output_audio_ctx.set_framerate(input_audio_codecpar.framerate);

            // output_audio_ctx.set_frame
            rsmpeg::avcodec::AVCodecParameters::from_context(
                &mut output_audio.codecpar_mut(),
                &mut output_audio_ctx,
            );

            (input_audio_idx, output_audio.index as usize)
        };
        debug!(input_audio_idx, output_audio_idx, "audio configured");

        // 3. open and write header
        let mut output_options = None;
        output_context
            .write_header(&mut output_options)
            .context("output context write header failed")?;

        // 4. copy packets
        while let Some(mut packet) = input_context.read_packet()? {
            if packet.stream_index == input_video_idx as i32 {
                packet.rescale_ts(
                    input_context.streams()[input_video_idx].time_base,
                    output_context.streams()[output_video_idx].time_base,
                );
                packet.set_stream_index(output_video_idx as i32);
                output_context.write_frame(&mut packet)?;
            } else if packet.stream_index == input_audio_idx as i32 {
                packet.rescale_ts(
                    input_context.streams()[input_audio_idx].time_base,
                    output_context.streams()[output_audio_idx].time_base,
                );
                packet.set_stream_index(output_audio_idx as i32);
                output_context.write_frame(&mut packet)?;
            } else {
                continue;
            }
        }
        output_context
            .write_trailer()
            .context("write tailer failed")?;

        Ok(())
    }
}
