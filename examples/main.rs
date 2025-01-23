fn main() {
    tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    motion::i2a::ConvertRequest {
        image_path: "./testdata/IMG_3853.HEIC".into(),
        video_path: "./testdata/IMG_3853.mov".into(),
        output_path: "./testoutput/MVIMG_3853.jpg".into(),
        exiftool_path: None,
    }
    .execute()
    .unwrap();
}
