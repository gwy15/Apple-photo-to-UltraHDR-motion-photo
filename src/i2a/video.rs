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
        let mut input_context = rsmpeg::avformat::AVFormatContextInput::open(&input, None, &mut input_options)?;

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
        let (input_audio_idx, output_audio_idx, mut input_audio_ctx, mut output_audio_ctx, resampler) = {
            let (input_audio_idx, input_audio_codec) = input_context
                .find_best_stream(rsmpeg::ffi::AVMEDIA_TYPE_AUDIO)
                .context("Find audio stream failed")?
                .context("No audio stream found")?;
            let input_audio = &input_context.streams()[input_audio_idx];
            let input_audio_dec = rsmpeg::avcodec::AVCodec::find_decoder(input_audio_codec.id).context("input audio decoder not found")?;
            let mut input_audio_ctx = rsmpeg::avcodec::AVCodecContext::new(&input_audio_dec);
            input_audio_ctx.apply_codecpar(&input_audio.codecpar())?;
            input_audio_ctx.set_ch_layout(*rsmpeg::avutil::AVChannelLayout::from_nb_channels(1));
            debug!("input audio layout: {:?}", input_audio_ctx.ch_layout().describe()?);
            debug!(
                "input audio codecpar: sample_rate={}, bit_rate={}",
                input_audio_ctx.sample_rate, input_audio_ctx.bit_rate
            );
            input_audio_ctx.open(None).context("input video codec context open failed")?;

            let mut output_audio = output_context.new_stream();
            let output_audio_encoder =
                rsmpeg::avcodec::AVCodec::find_encoder(rsmpeg::ffi::AV_CODEC_ID_AAC).context("No AAC encoder builtin.")?;
            let mut output_audio_ctx = rsmpeg::avcodec::AVCodecContext::new(&output_audio_encoder);
            output_audio_ctx.set_sample_rate(input_audio_ctx.sample_rate);
            output_audio_ctx.set_bit_rate(128000); // 128kbps
            output_audio_ctx.set_ch_layout(*rsmpeg::avutil::AVChannelLayout::from_nb_channels(1));
            output_audio_ctx.set_sample_fmt(rsmpeg::ffi::AV_SAMPLE_FMT_FLTP);
            output_audio_ctx.open(None).context("output audio codec context open failed")?;
            debug!("output audio layout: {:?}", output_audio_ctx.ch_layout().describe()?);

            // set output audio codecpar
            output_audio.codecpar_mut().from_context(&mut output_audio_ctx);

            // resample
            let mut resampler = rsmpeg::swresample::SwrContext::new(
                &output_audio_ctx.ch_layout(),
                output_audio_ctx.sample_fmt,
                output_audio_ctx.sample_rate,
                &input_audio_ctx.ch_layout(),
                input_audio_ctx.sample_fmt,
                input_audio_ctx.sample_rate,
            )?;
            dbg!(
                output_audio_ctx.ch_layout().nb_channels,
                output_audio_ctx.ch_layout().order,
                output_audio_ctx.sample_fmt,
                output_audio_ctx.sample_rate,
                input_audio_ctx.ch_layout().nb_channels,
                input_audio_ctx.ch_layout().order,
                input_audio_ctx.sample_fmt,
                input_audio_ctx.sample_rate,
            );
            resampler.init()?;

            (
                input_audio_idx,
                output_audio.index as usize,
                input_audio_ctx,
                output_audio_ctx,
                resampler,
            )
        };
        debug!(input_audio_idx, output_audio_idx, "audio configured");

        // 3. open and write header
        let mut output_options = None;
        output_context
            .write_header(&mut output_options)
            .context("output context write header failed")?;

        // 4. copy packets
        let mut resampled_frame = rsmpeg::avutil::AVFrame::new();
        resampled_frame.set_ch_layout(**output_audio_ctx.ch_layout());
        resampled_frame.set_format(output_audio_ctx.sample_fmt);
        resampled_frame.set_sample_rate(output_audio_ctx.sample_rate);
        resampled_frame.set_nb_samples(output_audio_ctx.frame_size);

        while let Some(mut packet) = input_context.read_packet()? {
            if packet.stream_index == input_video_idx as i32 {
                packet.rescale_ts(
                    input_context.streams()[input_video_idx].time_base,
                    output_context.streams()[output_video_idx].time_base,
                );
                packet.set_stream_index(output_video_idx as i32);
                output_context.write_frame(&mut packet)?;
                continue;
            }
            if packet.stream_index == input_audio_idx as i32 {
                input_audio_ctx
                    .send_packet(Some(&packet))
                    .context("Send packet to input audio codec context failed")?;
                debug!("audio packet sent to input audio codec context");
                while let Ok(mut frame) = input_audio_ctx.receive_frame() {
                    debug!("audio frame size = {}", frame.pkt_size);
                    unsafe { (*frame.as_mut_ptr()).ch_layout.order = rsmpeg::ffi::AV_CHANNEL_ORDER_NATIVE };

                    // debug!(" audio codec ctx received frame");
                    // debug!(" audio frame ch_layout.nb_channels={}", frame.ch_layout.nb_channels);
                    // debug!(" audio frame ch_layout.order={}", frame.ch_layout.order);
                    // // *(&mut frame.ch_layout.order) = rsmpeg::ffi::AV_CHANNEL_ORDER_NATIVE;
                    // debug!(" audio frame sample_fmt={}", frame.format);
                    // debug!(" audio frame sample_rate={}", frame.sample_rate);
                    // debug!(" audio frame nb_samples={}", frame.nb_samples);
                    // resampler.convert_frame(None, &mut resampled_frame)?;
                    resampler
                        .convert_frame(Some(&frame), &mut resampled_frame)
                        .context("convert frame failed")?;
                    debug!(" audio frame converted, resampled_frame size = {}", resampled_frame.pkt_size);
                    output_audio_ctx
                        .send_frame(Some(&resampled_frame)).ok();
                    // TODO: frame_size (1024) was not respected for a non-last frame
                        // .context("send_frame failed middle")?;
                    debug!(" converted frame sent to output audio codec context");
                    while let Ok(mut packet) = output_audio_ctx.receive_packet() {
                        packet.set_stream_index(output_audio_idx as i32);
                        packet.rescale_ts(output_audio_ctx.time_base, output_context.streams()[output_audio_idx].time_base);
                        output_context.write_frame(&mut packet).context("write frame failed")?;
                        debug!("  audio packet written, size={}", packet.size);
                    }
                }
                continue;
            }
        }
        while let Ok(frame) = input_audio_ctx.receive_frame() {
            resampler
                .convert_frame(Some(&frame), &mut resampled_frame)
                .context("convert frame last failed")?;
            output_audio_ctx
                .send_frame(Some(&resampled_frame))
                .context("send_frame last failed")?;
            while let Ok(mut packet) = output_audio_ctx.receive_packet() {
                packet.set_stream_index(output_audio_idx as i32);
                output_context.write_frame(&mut packet).context("write frame last failed")?;
            }
        }

        // 5. write trailer
        output_context.write_trailer().context("write tailer failed")?;

        Ok(())
    }
}
