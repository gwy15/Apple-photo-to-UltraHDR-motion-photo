fn main() {
    tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    motion::i2a::ConvertRequest {
        image_path: "./testdata/IMG_3850.HEIC".into(),
        video_path: "./testdata/IMG_3850.mov".into(),
        output_path: "./testoutput/MVIMG_3850.jpg".into(),
        exiftool_path: Some(r"c:\My Program Files\exiftool\exiftool.exe".into()),
        image_quality: 85,
        gainmap_quality: 85,
    }
    .convert()
    .unwrap();
}
