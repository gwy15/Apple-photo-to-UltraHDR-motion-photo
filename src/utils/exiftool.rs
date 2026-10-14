use anyhow::Context;
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
        Self { path: Some(path.into()) }
    }
    pub fn command(&self) -> std::process::Command {
        std::process::Command::new(self.path.as_ref().and_then(|e| e.as_os_str().to_str()).unwrap_or("exiftool"))
    }
    pub fn get_value(&self, file: impl AsRef<Path>, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.get_values(file, &[key])?.into_iter().next().flatten())
    }
    /// 一次 exiftool 调用读取多个标签。字段顺序由 -p 格式串显式固定，缺失的标签返回 None。
    /// 每少读一个标签就省一次 ~200ms 的进程启动。
    pub fn get_values(&self, file: impl AsRef<Path>, keys: &[&str]) -> anyhow::Result<Vec<Option<String>>> {
        // 注意：exiftool 12.76 不会把 -p 格式串里的字面量 "\t" 解释成制表符，
        // 所以这里直接嵌入真实的 tab 字符，输出原样带回，split 时正好对上。
        let format = keys.iter().map(|k| format!("${k}")).collect::<Vec<_>>().join("\t");
        let output = self
            .command()
            .arg("-p")
            .arg(format)
            .arg("-m") // ignore minor error
            .arg(file.as_ref().as_os_str())
            .output()
            .context("Run exiftool command failed. Is exiftool path corrent?")?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("exiftool failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim_end_matches('\n');
        let mut values = line
            .split('\t')
            .map(|v| {
                let v = v.trim();
                (!v.is_empty()).then(|| v.to_string())
            })
            .collect::<Vec<_>>();
        values.resize(keys.len(), None);
        Ok(values)
    }
}
