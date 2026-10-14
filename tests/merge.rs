use std::path::PathBuf;

#[test]
fn main() {
    tracing_subscriber::fmt::fmt().with_max_level(tracing::Level::DEBUG).init();

    let output = PathBuf::from("./testoutput/MVIMG_3853.jpg");
    if output.exists() {
        std::fs::remove_file(&output).unwrap();
    }
    std::fs::create_dir_all("./testoutput").unwrap();

    aa_photo_bridge::i2a::ConvertRequest {
        image_path: "./tests/IMG_3853.HEIC".into(),
        video_path: Some("./tests/IMG_3853.MOV".into()),
        output_path: output,
        exiftool_path: Some("exiftool".into()),
        overwrite_existing: false,
        image_quality: 85,
        gainmap_quality: 85,
    }
    .convert()
    .unwrap();
}
