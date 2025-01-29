#[test]
fn main() {
    tracing_subscriber::fmt::init();
    motion::i2a::video::VideoAudioEncodeRequest {
        input: "./tests/IMG_3853.MOV".as_ref(),
        output: "./tests/output.mp4".as_ref(),
    }
    .execute()
    .unwrap();
}
