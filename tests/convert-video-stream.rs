#[test]
fn main() {
    tracing_subscriber::fmt::init();
    // motion::i2a::video::VideoAudioEncodeRequest::mute_ffmpeg_log();
    motion::i2a::video::VideoAudioEncodeRequest {
        input: "./tests/IMG_3853.MOV".as_ref(),
        // input: "./tests/IMG_3281.MOV".as_ref(),
        output: "./tests/output.mp4".as_ref(),
        encoder: "ac3",
        bit_rate: 96_000,
    }
    .execute()
    .unwrap();
}
