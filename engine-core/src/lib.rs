use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaMetadata {
    pub path: String,
    pub duration_seconds: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    InvalidPath(String),
    NotFound(PathBuf),
    NotFile(PathBuf),
    ProbeCommandFailed(String),
    ProbeOutputInvalid(String),
    Io(String),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(message) => write!(f, "invalid path: {message}"),
            Self::NotFound(path) => write!(f, "file not found: {}", path.display()),
            Self::NotFile(path) => write!(f, "path is not a file: {}", path.display()),
            Self::ProbeCommandFailed(message) => write!(f, "probe command failed: {message}"),
            Self::ProbeOutputInvalid(message) => write!(f, "probe output invalid: {message}"),
            Self::Io(message) => write!(f, "io error: {message}"),
        }
    }
}

impl Error for ProbeError {}

pub trait MediaProbe: Send + Sync {
    fn probe_path(&self, path: &Path) -> Result<MediaMetadata, ProbeError>;
}

#[derive(Debug, Clone)]
pub struct FsMediaProbe {
    ffprobe_bin: String,
}

impl Default for FsMediaProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl FsMediaProbe {
    pub fn new() -> Self {
        Self {
            ffprobe_bin: "ffprobe".to_string(),
        }
    }

    pub fn with_ffprobe_bin(ffprobe_bin: impl Into<String>) -> Self {
        Self {
            ffprobe_bin: ffprobe_bin.into(),
        }
    }

    fn validate_path(&self, path: &Path) -> Result<(), ProbeError> {
        if !path.is_absolute() {
            return Err(ProbeError::InvalidPath(format!(
                "expected absolute path, got {}",
                path.display()
            )));
        }

        if !path.exists() {
            return Err(ProbeError::NotFound(path.to_path_buf()));
        }

        if !path.is_file() {
            return Err(ProbeError::NotFile(path.to_path_buf()));
        }

        Ok(())
    }

    fn probe_with_ffprobe(&self, path: &Path) -> Result<MediaMetadata, ProbeError> {
        let output = Command::new(&self.ffprobe_bin)
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("format=duration,format_name")
            .arg("-show_entries")
            .arg("stream=width,height")
            .arg("-of")
            .arg("json")
            .arg(path)
            .output()
            .map_err(|error| ProbeError::ProbeCommandFailed(error.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(ProbeError::ProbeCommandFailed(stderr));
        }

        let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
            .map_err(|error| ProbeError::ProbeOutputInvalid(error.to_string()))?;

        let size_bytes = fs::metadata(path)
            .map_err(|error| ProbeError::Io(error.to_string()))?
            .len();

        let stream = parsed.streams.first();
        let format = parsed
            .format
            .as_ref()
            .and_then(|value| value.format_name.as_deref())
            .and_then(|value| value.split(',').next())
            .unwrap_or("unknown")
            .to_string();

        let duration_seconds = parsed
            .format
            .as_ref()
            .and_then(|value| value.duration.as_deref())
            .map(str::parse::<f64>)
            .transpose()
            .map_err(|error| ProbeError::ProbeOutputInvalid(error.to_string()))?;

        Ok(MediaMetadata {
            path: path.to_string_lossy().to_string(),
            duration_seconds,
            width: stream.and_then(|value| value.width),
            height: stream.and_then(|value| value.height),
            format,
            size_bytes,
        })
    }

    fn probe_with_filesystem_metadata(&self, path: &Path) -> Result<MediaMetadata, ProbeError> {
        let metadata = fs::metadata(path).map_err(|error| ProbeError::Io(error.to_string()))?;
        let format = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(MediaMetadata {
            path: path.to_string_lossy().to_string(),
            duration_seconds: None,
            width: None,
            height: None,
            format,
            size_bytes: metadata.len(),
        })
    }
}

impl MediaProbe for FsMediaProbe {
    fn probe_path(&self, path: &Path) -> Result<MediaMetadata, ProbeError> {
        self.validate_path(path)?;
        match self.probe_with_ffprobe(path) {
            Ok(metadata) => Ok(metadata),
            Err(ProbeError::ProbeCommandFailed(_)) => self.probe_with_filesystem_metadata(path),
            Err(other) => Err(other),
        }
    }
}

pub fn probe_media(path: &str) -> Result<MediaMetadata, ProbeError> {
    FsMediaProbe::new().probe_path(Path::new(path))
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    format_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_file_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("tv7-engine-test-{nanos}.txt"))
    }

    #[test]
    fn rejects_relative_path() {
        let probe = FsMediaProbe::new();
        let error = probe
            .probe_path(Path::new("relative/path.mp4"))
            .expect_err("relative paths should fail");

        assert!(matches!(error, ProbeError::InvalidPath(_)));
    }

    #[test]
    fn reports_missing_file() {
        let probe = FsMediaProbe::new();
        let missing = std::env::temp_dir().join("tv7-engine-missing-file.mp4");
        let error = probe
            .probe_path(&missing)
            .expect_err("missing file should fail");

        assert!(matches!(error, ProbeError::NotFound(_)));
    }

    #[test]
    fn falls_back_to_filesystem_when_probe_binary_is_unavailable() {
        let probe = FsMediaProbe::with_ffprobe_bin("this-binary-does-not-exist");
        let file_path = unique_temp_file_path();
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");

        let result = probe
            .probe_path(&file_path)
            .expect("filesystem fallback should succeed");

        assert_eq!(result.path, file_path.to_string_lossy().to_string());
        assert_eq!(result.duration_seconds, None);
        assert_eq!(result.width, None);
        assert_eq!(result.height, None);
        assert_eq!(result.format, "txt");
        assert_eq!(result.size_bytes, 12);

        fs::remove_file(file_path).expect("fixture file should be removed");
    }
}
