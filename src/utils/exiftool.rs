use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ExifTool {
    pub path: Option<PathBuf>,
}

impl ExifTool {
    pub fn new() -> Self {
        Self { path: None }
    }
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }
    pub fn command(&self) -> std::process::Command {
        std::process::Command::new(
            self.path
                .as_ref()
                .and_then(|e| e.as_os_str().to_str())
                .unwrap_or("exiftool"),
        )
    }
    pub fn get_value(&self, file: impl AsRef<Path>, key: &str) -> anyhow::Result<Option<String>> {
        let output = self
            .command()
            .arg(format!("-{key}"))
            .args(["-s", "-s", "-s"])
            .arg(file.as_ref().as_os_str())
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exiftool failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            return Ok(None);
        }
        Ok(Some(value))
    }
    pub fn copy_meta(&self, src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
        let output = self
            .command()
            .arg("-TagsFromFile")
            .arg(src.as_ref().as_os_str())
            .arg("-Orientation=")
            .arg("-overwrite_original")
            .arg(dst.as_ref().as_os_str())
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "exiftool failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }
}
