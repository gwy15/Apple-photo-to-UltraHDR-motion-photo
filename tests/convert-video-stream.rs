#[test]
fn main() {
    tracing_subscriber::fmt::init();
    // motion::i2a::video::VideoAudioEncodeRequest::mute_ffmpeg_log();
    motion::i2a::video::VideoAudioEncodeRequest {
        input: "./tests/IMG_3853.MOV".as_ref(),
        // input: "./tests/IMG_3281.MOV".as_ref(),
        output: "./testoutput/output.mp4".as_ref(),
        encoder: "aac",
        bit_rate: 128_000,
    }
    .execute()
    .unwrap();
}
