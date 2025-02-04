use anyhow::{Context, Result};
use rsmpeg::{
    avcodec::AVCodecContext,
    avformat::{AVFormatContextInput, AVFormatContextOutput},
    avutil::AVAudioFifo,
    swresample::SwrContext,
};
use std::{ffi::CString, path::Path};

pub struct VideoAudioEncodeRequest<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub bit_rate: i64,
    pub encoder: &'static str,
}

impl VideoAudioEncodeRequest<'_> {
    pub fn mute_ffmpeg_log() {
        unsafe { rsmpeg::ffi::av_log_set_level(rsmpeg::ffi::AV_LOG_ERROR as i32) };
    }

    pub fn execute(&self) -> Result<()> {
        // 1.a open input
        let mut i_fmt_ctx = self.input_format_context()?;

        // 1.b open output
        let mut o_fmt_ctx = self.output_format_context()?;

        // 2.a configurations: video
        let (input_video_idx, output_video_idx) = Self::get_video_stream_index(&i_fmt_ctx, &mut o_fmt_ctx)?;
        debug!(%input_video_idx, %output_video_idx, "video configured");

        // 2.b configurations: audio
        let mut audio = self.get_audio_configure(&i_fmt_ctx, &mut o_fmt_ctx)?;
        debug!(
            input_audio_idx = audio.input_stream_index,
            output_audio_idx = audio.output_stream_index,
            "audio configured"
        );

        // 3. open and write header
        let mut output_options = None;
        o_fmt_ctx
            .write_header(&mut output_options)
            .context("output context write header failed")?;

        // 4. copy packets
        let mut fifo = AVAudioFifo::new(
            audio.output_codec_context.sample_fmt,
            audio.output_codec_context.ch_layout().nb_channels,
            1,
        );
        let frame_size = audio.output_codec_context.frame_size as usize;
        let mut pts: i64 = 0;
        debug!(
            "ia_frame_size = {}, oa_frame_size = {}",
            audio.input_codec_context.frame_size, audio.output_codec_context.frame_size
        );

        while let Some(mut packet) = i_fmt_ctx.read_packet()? {
            if packet.stream_index == input_video_idx as i32 {
                packet.rescale_ts(
                    i_fmt_ctx.streams()[input_video_idx].time_base,
                    o_fmt_ctx.streams()[output_video_idx].time_base,
                );
                packet.set_stream_index(output_video_idx as i32);
                o_fmt_ctx.write_frame(&mut packet)?;
                continue;
            }
            if packet.stream_index == audio.input_stream_index as i32 {
                audio
                    .input_codec_context
                    .send_packet(Some(&packet))
                    .context("Send packet to input audio codec context failed")?;
                while let Ok(frame) = audio.input_codec_context.receive_frame() {
                    let mut converted_frame = audio.new_frame();
                    audio
                        .resampler
                        .convert_frame(Some(&frame), &mut converted_frame)
                        .context("convert frame failed")?;
                    converted_frame.set_pts(pts);
                    pts += converted_frame.nb_samples as i64;

                    unsafe { fifo.write(converted_frame.data.as_ptr(), converted_frame.nb_samples) }?;

                    while fifo.size() as usize >= frame_size {
                        let mut new_frame = audio.new_frame();
                        new_frame.set_nb_samples(frame_size as i32);
                        new_frame.set_pts(pts - fifo.size() as i64);
                        new_frame.alloc_buffer()?;
                        unsafe { fifo.read(new_frame.data.as_ptr(), new_frame.nb_samples) }?;

                        audio
                            .output_codec_context
                            .send_frame(Some(&new_frame))
                            .context("send converted frame to output codec context failed")?;
                        audio.flush_output(&i_fmt_ctx, &mut o_fmt_ctx)?;
                    }
                }
            }
        }

        // 处理 FIFO 中剩余的数据
        while fifo.size() > 0 {
            debug!("flushing tail packets, size={}", fifo.size());
            let samples = std::cmp::min(fifo.size(), frame_size as i32);
            let mut new_frame = audio.new_frame();
            new_frame.set_nb_samples(samples as i32);
            new_frame.set_pts(pts - samples as i64);
            new_frame.alloc_buffer()?;
            unsafe { fifo.read(new_frame.data.as_ptr(), new_frame.nb_samples) }?;

            audio
                .output_codec_context
                .send_frame(Some(&new_frame))
                .context("send remaining frame to output codec context failed")?;
            audio.flush_output(&i_fmt_ctx, &mut o_fmt_ctx)?;
        }

        // 清空输出编解码器缓冲区
        audio.output_codec_context.send_frame(None)?;
        audio.flush_output(&i_fmt_ctx, &mut o_fmt_ctx)?;

        // 5. write trailer
        o_fmt_ctx.write_trailer().context("write trailer failed")?;

        Ok(())
    }

    fn input_format_context(&self) -> Result<AVFormatContextInput> {
        let input = self.input.to_str().context("input path to_str failed")?;
        let input = CString::new(input)?;
        let mut input_options = None;
        let format_context = rsmpeg::avformat::AVFormatContextInput::open(&input, None, &mut input_options)?;
        Ok(format_context)
    }

    fn output_format_context(&self) -> Result<AVFormatContextOutput> {
        let output = self.output.to_str().context("output path to_str failed")?;
        let output = CString::new(output)?;
        let format_context = rsmpeg::avformat::AVFormatContextOutput::create(&output, None)?;
        Ok(format_context)
    }

    fn get_video_stream_index(i_fmt_ctx: &AVFormatContextInput, o_fmt_ctx: &mut AVFormatContextOutput) -> Result<(usize, usize)> {
        let (input_video_idx, _) = i_fmt_ctx
            .find_best_stream(rsmpeg::ffi::AVMEDIA_TYPE_VIDEO)
            .context("Find video stream failed")?
            .context("No video stream found")?;
        let input_video = &i_fmt_ctx.streams()[input_video_idx];
        let mut output_video = o_fmt_ctx.new_stream();
        output_video.codecpar_mut().copy(&input_video.codecpar());
        Ok((input_video_idx, output_video.index as usize))
    }

    fn get_audio_configure(&self, i_fmt_ctx: &AVFormatContextInput, o_fmt_ctx: &mut AVFormatContextOutput) -> Result<AudioConfigure> {
        // 1. get input audio index and codec
        let (i_idx, i_codec) = i_fmt_ctx
            .find_best_stream(rsmpeg::ffi::AVMEDIA_TYPE_AUDIO)
            .context("Find audio stream failed")?
            .context("No audio stream found")?;
        let i_stream = &i_fmt_ctx.streams()[i_idx];
        // 2. create input codec context
        let mut i_codec_ctx = rsmpeg::avcodec::AVCodecContext::new(&i_codec);
        i_codec_ctx.apply_codecpar(&i_stream.codecpar())?;
        i_codec_ctx.set_ch_layout(*rsmpeg::avutil::AVChannelLayout::from_nb_channels(1)); // overwrite channel layout
        i_codec_ctx.set_pkt_timebase(i_stream.time_base);
        i_codec_ctx.set_time_base(i_stream.time_base);
        i_codec_ctx.open(None).context("input audio codec context open failed")?;
        debug!(%i_codec_ctx.sample_rate, %i_codec_ctx.bit_rate, %i_codec_ctx.frame_size, "input audio codec");

        // 3. create output audio stream
        let global_header = (o_fmt_ctx.flags & rsmpeg::ffi::AVFMT_GLOBALHEADER as i32) != 0;
        let mut o_stream = o_fmt_ctx.new_stream();
        let output_stream_index = o_stream.index as usize;
        // 4. create output audio codec context
        let name = CString::new(self.encoder)?;
        let o_codec = rsmpeg::avcodec::AVCodec::find_encoder_by_name(&name).context("No encoder found.")?;
        debug!("Using encoder {:?}", o_codec.name());
        let mut o_codec_ctx = rsmpeg::avcodec::AVCodecContext::new(&o_codec);
        o_codec_ctx.set_sample_rate(i_codec_ctx.sample_rate);
        o_codec_ctx.set_bit_rate(self.bit_rate);
        o_codec_ctx.set_ch_layout(*rsmpeg::avutil::AVChannelLayout::from_nb_channels(1));
        o_codec_ctx.set_sample_fmt(o_codec.sample_fmts().unwrap()[0]);
        // See https://github.com/larksuite/rsmpeg/issues/198
        // o_codec_ctx.set_frame_size(1024);
        // o_codec_ctx.set_codec_type(rsmpeg::ffi::AVMEDIA_TYPE_AUDIO);
        // o_codec_ctx.set_profile(rsmpeg::ffi::FF_PROFILE_AAC_LOW as i32);
        o_codec_ctx.set_pkt_timebase(rsmpeg::avutil::AVRational {
            num: 1,
            den: i_codec_ctx.sample_rate,
        });
        o_codec_ctx.set_time_base(rsmpeg::avutil::AVRational {
            num: 1,
            den: o_codec_ctx.sample_rate,
        });
        if global_header {
            o_codec_ctx.set_flags(o_codec_ctx.flags | rsmpeg::ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }
        o_stream.set_time_base(o_codec_ctx.time_base);

        o_codec_ctx.open(None).context("output audio codec context open failed")?;
        o_stream.codecpar_mut().from_context(&o_codec_ctx);
        debug!(%o_codec_ctx.sample_rate, %o_codec_ctx.bit_rate, %o_codec_ctx.frame_size, "output audio codec");

        // 5. create resampler
        let mut resampler = rsmpeg::swresample::SwrContext::new(
            &o_codec_ctx.ch_layout(),
            o_codec_ctx.sample_fmt,
            o_codec_ctx.sample_rate,
            &i_codec_ctx.ch_layout(),
            i_codec_ctx.sample_fmt,
            i_codec_ctx.sample_rate,
        )?;
        resampler.init()?;

        Ok(AudioConfigure {
            input_stream_index: i_idx,
            output_stream_index,
            input_codec_context: i_codec_ctx,
            output_codec_context: o_codec_ctx,
            resampler,
        })
    }
}

struct AudioConfigure {
    pub input_stream_index: usize,
    pub output_stream_index: usize,
    pub input_codec_context: AVCodecContext,
    pub output_codec_context: AVCodecContext,
    pub resampler: SwrContext,
}

impl AudioConfigure {
    pub fn new_frame(&self) -> rsmpeg::avutil::AVFrame {
        let mut frame = rsmpeg::avutil::AVFrame::new();
        frame.set_ch_layout(**self.output_codec_context.ch_layout());
        frame.set_format(self.output_codec_context.sample_fmt);
        frame.set_sample_rate(self.output_codec_context.sample_rate);
        frame
    }

    pub fn flush_output(&mut self, i_fmt_ctx: &AVFormatContextInput, o_fmt_ctx: &mut AVFormatContextOutput) -> Result<()> {
        while let Ok(mut packet) = self.output_codec_context.receive_packet() {
            packet.set_stream_index(self.output_stream_index as i32);
            let from = i_fmt_ctx.streams()[self.input_stream_index].time_base;
            let to = o_fmt_ctx.streams()[self.output_stream_index].time_base;
            packet.rescale_ts(from, to);
            o_fmt_ctx.write_frame(&mut packet).context("write frame failed")?;
        }
        Ok(())
    }
}
