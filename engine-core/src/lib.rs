use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaMetadata {
    pub path: String,
    pub duration_seconds: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThumbnailRequest {
    pub source_path: String,
    pub output_path: String,
    pub timestamp_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThumbnailResult {
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderClipRequest {
    pub source_path: String,
    pub output_path: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderClipResult {
    pub output_path: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UrlIngestRequest {
    pub url: String,
    pub output_dir: String,
    pub output_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UrlIngestResult {
    pub url: String,
    pub media_path: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptDownloadRequest {
    pub url: String,
    pub output_path: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptDownloadResult {
    pub transcript_path: Option<String>,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineDiagnostics {
    pub status: DiagnosticStatus,
    pub runtime: RuntimeDiagnostic,
    pub capabilities: Vec<CapabilityDiagnostic>,
    pub tools: Vec<ToolDiagnostic>,
    pub work_dir: WorkDirDiagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    Ready,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityDiagnostic {
    pub name: String,
    pub status: DiagnosticStatus,
    pub required_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDiagnostic {
    pub name: String,
    pub available: Option<bool>,
    pub version: Option<String>,
    pub required_for: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkDirDiagnostic {
    pub path: String,
    pub ready: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeDiagnostic {
    pub engine_version: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsContext {
    pub engine_version: String,
    pub host: String,
    pub port: u16,
    pub work_dir: PathBuf,
}

impl DiagnosticsContext {
    pub fn new(
        engine_version: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        work_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            engine_version: engine_version.into(),
            host: host.into(),
            port,
            work_dir: work_dir.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    InvalidPath(String),
    NotFound(PathBuf),
    NotFile(PathBuf),
    OutputParentMissing(PathBuf),
    SourceOutputSame(PathBuf),
    InvalidTimeRange(String),
    InvalidUrl(String),
    CommandFailed { command: String, message: String },
    OutputInvalid(String),
    Io(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(message) => write!(f, "invalid path: {message}"),
            Self::NotFound(path) => write!(f, "file not found: {}", path.display()),
            Self::NotFile(path) => write!(f, "path is not a file: {}", path.display()),
            Self::OutputParentMissing(path) => {
                write!(
                    f,
                    "output parent directory does not exist: {}",
                    path.display()
                )
            }
            Self::SourceOutputSame(path) => {
                write!(f, "source and output paths must differ: {}", path.display())
            }
            Self::InvalidTimeRange(message) => write!(f, "invalid time range: {message}"),
            Self::InvalidUrl(message) => write!(f, "invalid url: {message}"),
            Self::CommandFailed { command, message } => {
                write!(f, "{command} command failed: {message}")
            }
            Self::OutputInvalid(message) => write!(f, "command output invalid: {message}"),
            Self::Io(message) => write!(f, "io error: {message}"),
        }
    }
}

impl Error for EngineError {}

pub type ProbeError = EngineError;

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub status_success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CommandExecutor: Send + Sync {
    fn run(&self, executable: &str, args: &[String]) -> Result<ToolOutput, EngineError>;
}

#[derive(Debug, Default)]
pub struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
    fn run(&self, executable: &str, args: &[String]) -> Result<ToolOutput, EngineError> {
        let output = Command::new(executable)
            .args(args)
            .output()
            .map_err(|error| EngineError::CommandFailed {
                command: executable.to_string(),
                message: error.to_string(),
            })?;

        Ok(ToolOutput {
            status_success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

pub trait MediaProbe: Send + Sync {
    fn probe_path(&self, path: &Path) -> Result<MediaMetadata, EngineError>;
}

pub trait ThumbnailGenerator: Send + Sync {
    fn generate_thumbnail(
        &self,
        request: &ThumbnailRequest,
    ) -> Result<ThumbnailResult, EngineError>;
}

pub trait ClipRenderer: Send + Sync {
    fn render_clip(&self, request: &RenderClipRequest) -> Result<RenderClipResult, EngineError>;
}

pub trait DiagnosticsProvider: Send + Sync {
    fn diagnostics(&self, context: &DiagnosticsContext) -> EngineDiagnostics;
}

pub trait UrlIngestor: Send + Sync {
    fn ingest_url(&self, request: &UrlIngestRequest) -> Result<UrlIngestResult, EngineError>;
}

pub trait TranscriptProvider: Send + Sync {
    fn download_transcript(
        &self,
        request: &TranscriptDownloadRequest,
    ) -> Result<TranscriptDownloadResult, EngineError>;
}

#[derive(Clone)]
pub struct FfmpegMediaEngine {
    ffmpeg_bin: String,
    ffprobe_bin: String,
    yt_dlp_bin: String,
    executor: Arc<dyn CommandExecutor>,
}

impl fmt::Debug for FfmpegMediaEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FfmpegMediaEngine")
            .field("ffmpeg_bin", &self.ffmpeg_bin)
            .field("ffprobe_bin", &self.ffprobe_bin)
            .field("yt_dlp_bin", &self.yt_dlp_bin)
            .finish_non_exhaustive()
    }
}

impl Default for FfmpegMediaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FfmpegMediaEngine {
    pub fn new() -> Self {
        Self {
            ffmpeg_bin: "ffmpeg".to_string(),
            ffprobe_bin: "ffprobe".to_string(),
            yt_dlp_bin: "yt-dlp".to_string(),
            executor: Arc::new(ProcessCommandExecutor),
        }
    }

    pub fn with_bins(ffmpeg_bin: impl Into<String>, ffprobe_bin: impl Into<String>) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: "yt-dlp".to_string(),
            executor: Arc::new(ProcessCommandExecutor),
        }
    }

    pub fn with_all_bins(
        ffmpeg_bin: impl Into<String>,
        ffprobe_bin: impl Into<String>,
        yt_dlp_bin: impl Into<String>,
    ) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: yt_dlp_bin.into(),
            executor: Arc::new(ProcessCommandExecutor),
        }
    }

    pub fn with_executor(
        ffmpeg_bin: impl Into<String>,
        ffprobe_bin: impl Into<String>,
        executor: Arc<dyn CommandExecutor>,
    ) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: "yt-dlp".to_string(),
            executor,
        }
    }

    pub fn with_all_bins_and_executor(
        ffmpeg_bin: impl Into<String>,
        ffprobe_bin: impl Into<String>,
        yt_dlp_bin: impl Into<String>,
        executor: Arc<dyn CommandExecutor>,
    ) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: yt_dlp_bin.into(),
            executor,
        }
    }

    fn validate_source_path(&self, path: &Path) -> Result<(), EngineError> {
        if !path.is_absolute() {
            return Err(EngineError::InvalidPath(format!(
                "expected absolute path, got {}",
                path.display()
            )));
        }

        if !path.exists() {
            return Err(EngineError::NotFound(path.to_path_buf()));
        }

        if !path.is_file() {
            return Err(EngineError::NotFile(path.to_path_buf()));
        }

        Ok(())
    }

    fn validate_output_path(&self, source: &Path, output: &Path) -> Result<(), EngineError> {
        if !output.is_absolute() {
            return Err(EngineError::InvalidPath(format!(
                "expected absolute output path, got {}",
                output.display()
            )));
        }

        if source == output {
            return Err(EngineError::SourceOutputSame(output.to_path_buf()));
        }

        let parent = output
            .parent()
            .ok_or_else(|| EngineError::OutputParentMissing(PathBuf::from("<missing-parent>")))?;
        if !parent.is_dir() {
            return Err(EngineError::OutputParentMissing(parent.to_path_buf()));
        }

        Ok(())
    }

    fn validate_output_dir(&self, output_dir: &Path) -> Result<(), EngineError> {
        if !output_dir.is_absolute() {
            return Err(EngineError::InvalidPath(format!(
                "expected absolute output_dir, got {}",
                output_dir.display()
            )));
        }

        if !output_dir.is_dir() {
            return Err(EngineError::OutputParentMissing(output_dir.to_path_buf()));
        }

        Ok(())
    }

    fn validate_url(&self, url: &str) -> Result<(), EngineError> {
        let lower = url.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            Ok(())
        } else {
            Err(EngineError::InvalidUrl(
                "expected http or https URL".to_string(),
            ))
        }
    }

    fn run_checked(&self, executable: &str, args: &[String]) -> Result<ToolOutput, EngineError> {
        let output = self.executor.run(executable, args)?;
        if !output.status_success {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(EngineError::CommandFailed {
                command: executable.to_string(),
                message: stderr,
            });
        }
        Ok(output)
    }

    fn tool_diagnostic(&self, name: &str, required_for: Vec<String>) -> ToolDiagnostic {
        let args = tool_version_args(name);
        match self.executor.run(name, &args) {
            Ok(output) if output.status_success => {
                let version = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string);
                ToolDiagnostic {
                    name: name.to_string(),
                    available: Some(true),
                    version,
                    required_for,
                    error: None,
                }
            }
            Ok(output) => ToolDiagnostic {
                name: name.to_string(),
                available: Some(false),
                version: None,
                required_for,
                error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            },
            Err(error) => ToolDiagnostic {
                name: name.to_string(),
                available: Some(false),
                version: None,
                required_for,
                error: Some(error.to_string()),
            },
        }
    }
}

fn tool_version_args(name: &str) -> Vec<String> {
    let executable_name = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name)
        .to_ascii_lowercase();
    if executable_name == "yt-dlp" || executable_name == "yt_dlp" {
        vec!["--version".to_string()]
    } else {
        vec!["-version".to_string()]
    }
}

impl UrlIngestor for FfmpegMediaEngine {
    fn ingest_url(&self, request: &UrlIngestRequest) -> Result<UrlIngestResult, EngineError> {
        self.validate_url(&request.url)?;
        let output_dir = Path::new(&request.output_dir);
        self.validate_output_dir(output_dir)?;

        let metadata_args = vec![
            "--dump-json".to_string(),
            "--skip-download".to_string(),
            request.url.clone(),
        ];
        let metadata_output = self.run_checked(&self.yt_dlp_bin, &metadata_args)?;
        let metadata: serde_json::Value = serde_json::from_slice(&metadata_output.stdout)
            .map_err(|error| EngineError::OutputInvalid(error.to_string()))?;

        let before = list_files(output_dir)?;
        let template = request
            .output_template
            .clone()
            .unwrap_or_else(|| "%(title).200B.%(ext)s".to_string());
        let output_pattern = output_dir.join(template);
        let download_args = vec![
            "-o".to_string(),
            output_pattern.to_string_lossy().to_string(),
            request.url.clone(),
        ];
        self.run_checked(&self.yt_dlp_bin, &download_args)?;
        let after = list_files(output_dir)?;
        let media_path = after
            .into_iter()
            .filter(|path| !before.contains(path))
            .max_by_key(|path| {
                fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
            })
            .map(|path| path.to_string_lossy().to_string());

        Ok(UrlIngestResult {
            url: request.url.clone(),
            media_path,
            metadata,
        })
    }
}

impl TranscriptProvider for FfmpegMediaEngine {
    fn download_transcript(
        &self,
        request: &TranscriptDownloadRequest,
    ) -> Result<TranscriptDownloadResult, EngineError> {
        self.validate_url(&request.url)?;
        let output_path = Path::new(&request.output_path);
        if !output_path.is_absolute() {
            return Err(EngineError::InvalidPath(format!(
                "expected absolute output_path, got {}",
                output_path.display()
            )));
        }
        let parent = output_path
            .parent()
            .ok_or_else(|| EngineError::OutputParentMissing(PathBuf::from("<missing-parent>")))?;
        if !parent.is_dir() {
            return Err(EngineError::OutputParentMissing(parent.to_path_buf()));
        }

        let language = request.language.clone().unwrap_or_else(|| "en".to_string());
        let before = list_files(parent)?;
        let output_base = request.output_path.trim_end_matches(".vtt").to_string();
        let args = vec![
            "--write-auto-sub".to_string(),
            "--sub-lang".to_string(),
            language.clone(),
            "--sub-format".to_string(),
            "vtt".to_string(),
            "--skip-download".to_string(),
            "-o".to_string(),
            output_base,
            request.url.clone(),
        ];
        self.run_checked(&self.yt_dlp_bin, &args)?;
        let after = list_files(parent)?;
        let transcript_path = after
            .into_iter()
            .filter(|path| !before.contains(path))
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("vtt"))
            .max_by_key(|path| {
                fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
            })
            .map(|path| path.to_string_lossy().to_string());

        Ok(TranscriptDownloadResult {
            transcript_path,
            language,
        })
    }
}

impl MediaProbe for FfmpegMediaEngine {
    fn probe_path(&self, path: &Path) -> Result<MediaMetadata, EngineError> {
        self.validate_source_path(path)?;
        let args = vec![
            "-v".to_string(),
            "error".to_string(),
            "-show_entries".to_string(),
            "format=duration,format_name".to_string(),
            "-show_entries".to_string(),
            "stream=width,height".to_string(),
            "-of".to_string(),
            "json".to_string(),
            path.to_string_lossy().to_string(),
        ];
        let output = self.run_checked(&self.ffprobe_bin, &args)?;

        let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
            .map_err(|error| EngineError::OutputInvalid(error.to_string()))?;

        let size_bytes = fs::metadata(path)
            .map_err(|error| EngineError::Io(error.to_string()))?
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
            .map_err(|error| EngineError::OutputInvalid(error.to_string()))?;

        Ok(MediaMetadata {
            path: path.to_string_lossy().to_string(),
            duration_seconds,
            width: stream.and_then(|value| value.width),
            height: stream.and_then(|value| value.height),
            format,
            size_bytes,
        })
    }
}

impl ThumbnailGenerator for FfmpegMediaEngine {
    fn generate_thumbnail(
        &self,
        request: &ThumbnailRequest,
    ) -> Result<ThumbnailResult, EngineError> {
        let source = Path::new(&request.source_path);
        let output = Path::new(&request.output_path);
        self.validate_source_path(source)?;
        self.validate_output_path(source, output)?;

        let timestamp = request.timestamp_seconds.unwrap_or(1.0);
        if !timestamp.is_finite() || timestamp < 0.0 {
            return Err(EngineError::InvalidTimeRange(
                "timestamp_seconds must be finite and non-negative".to_string(),
            ));
        }

        let args = vec![
            "-y".to_string(),
            "-i".to_string(),
            request.source_path.clone(),
            "-ss".to_string(),
            timestamp.to_string(),
            "-vframes".to_string(),
            "1".to_string(),
            request.output_path.clone(),
        ];
        self.run_checked(&self.ffmpeg_bin, &args)?;

        Ok(ThumbnailResult {
            output_path: request.output_path.clone(),
        })
    }
}

impl ClipRenderer for FfmpegMediaEngine {
    fn render_clip(&self, request: &RenderClipRequest) -> Result<RenderClipResult, EngineError> {
        let source = Path::new(&request.source_path);
        let output = Path::new(&request.output_path);
        self.validate_source_path(source)?;
        self.validate_output_path(source, output)?;

        if !request.start_seconds.is_finite()
            || !request.end_seconds.is_finite()
            || request.start_seconds < 0.0
            || request.end_seconds <= request.start_seconds
        {
            return Err(EngineError::InvalidTimeRange(
                "expected 0 <= start_seconds < end_seconds".to_string(),
            ));
        }

        let args = vec![
            "-y".to_string(),
            "-i".to_string(),
            request.source_path.clone(),
            "-ss".to_string(),
            request.start_seconds.to_string(),
            "-to".to_string(),
            request.end_seconds.to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            request.output_path.clone(),
        ];
        self.run_checked(&self.ffmpeg_bin, &args)?;

        Ok(RenderClipResult {
            output_path: request.output_path.clone(),
            duration_seconds: request.end_seconds - request.start_seconds,
        })
    }
}

impl DiagnosticsProvider for FfmpegMediaEngine {
    fn diagnostics(&self, context: &DiagnosticsContext) -> EngineDiagnostics {
        let work_dir_diag = diagnose_work_dir(&context.work_dir);
        let ffprobe = self.tool_diagnostic(&self.ffprobe_bin, vec!["probe".to_string()]);
        let ffmpeg = self.tool_diagnostic(
            &self.ffmpeg_bin,
            vec!["thumbnail".to_string(), "render_clip".to_string()],
        );
        let yt_dlp = self.tool_diagnostic(
            &self.yt_dlp_bin,
            vec!["url_ingest".to_string(), "transcript".to_string()],
        );

        let probe_ready = ffprobe.available == Some(true);
        let thumbnail_ready = ffmpeg.available == Some(true);
        let render_ready = ffmpeg.available == Some(true);
        let url_ingest_ready = yt_dlp.available == Some(true);
        let all_ready = work_dir_diag.ready
            && probe_ready
            && thumbnail_ready
            && render_ready
            && url_ingest_ready;

        EngineDiagnostics {
            status: if all_ready {
                DiagnosticStatus::Ready
            } else {
                DiagnosticStatus::Degraded
            },
            runtime: RuntimeDiagnostic {
                engine_version: context.engine_version.clone(),
                host: context.host.clone(),
                port: context.port,
            },
            capabilities: vec![
                CapabilityDiagnostic {
                    name: "probe".to_string(),
                    status: status_for(probe_ready),
                    required_tools: vec![self.ffprobe_bin.clone()],
                },
                CapabilityDiagnostic {
                    name: "thumbnail".to_string(),
                    status: status_for(thumbnail_ready),
                    required_tools: vec![self.ffmpeg_bin.clone()],
                },
                CapabilityDiagnostic {
                    name: "render_clip".to_string(),
                    status: status_for(render_ready),
                    required_tools: vec![self.ffmpeg_bin.clone()],
                },
                CapabilityDiagnostic {
                    name: "url_ingest".to_string(),
                    status: status_for(url_ingest_ready),
                    required_tools: vec![self.yt_dlp_bin.clone()],
                },
                CapabilityDiagnostic {
                    name: "transcript".to_string(),
                    status: status_for(url_ingest_ready),
                    required_tools: vec![self.yt_dlp_bin.clone()],
                },
            ],
            tools: vec![ffprobe, ffmpeg, yt_dlp],
            work_dir: work_dir_diag,
        }
    }
}

fn list_files(directory: &Path) -> Result<Vec<PathBuf>, EngineError> {
    fs::read_dir(directory)
        .map_err(|error| EngineError::Io(error.to_string()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| EngineError::Io(error.to_string()))
        })
        .filter(|path| path.as_ref().map(|path| path.is_file()).unwrap_or(true))
        .collect()
}

fn status_for(ready: bool) -> DiagnosticStatus {
    if ready {
        DiagnosticStatus::Ready
    } else {
        DiagnosticStatus::Degraded
    }
}

fn diagnose_work_dir(work_dir: &Path) -> WorkDirDiagnostic {
    let mut diagnostic = WorkDirDiagnostic {
        path: work_dir.to_string_lossy().to_string(),
        ready: false,
        error: None,
    };

    if !work_dir.is_absolute() {
        diagnostic.error = Some("work_dir must be absolute".to_string());
        return diagnostic;
    }

    if !work_dir.is_dir() {
        diagnostic.error = Some("work_dir must be an existing directory".to_string());
        return diagnostic;
    }

    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let probe_file = work_dir.join(format!(
        ".tv7-engine-write-check-{}-{unique_id}",
        std::process::id()
    ));
    match fs::write(&probe_file, b"ok").and_then(|_| fs::remove_file(&probe_file)) {
        Ok(()) => {
            diagnostic.ready = true;
        }
        Err(error) => {
            diagnostic.error = Some(format!("work_dir is not writable: {error}"));
        }
    }

    diagnostic
}

pub fn probe_media(path: &str) -> Result<MediaMetadata, EngineError> {
    FfmpegMediaEngine::new().probe_path(Path::new(path))
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
    use std::collections::VecDeque;
    use std::process::Command;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug)]
    struct MockExecutor {
        outputs: Mutex<VecDeque<Result<ToolOutput, EngineError>>>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl MockExecutor {
        fn new(outputs: Vec<Result<ToolOutput, EngineError>>) -> Self {
            Self {
                outputs: Mutex::new(VecDeque::from(outputs)),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls
                .lock()
                .expect("mock lock should not be poisoned")
                .clone()
        }
    }

    impl CommandExecutor for MockExecutor {
        fn run(&self, executable: &str, args: &[String]) -> Result<ToolOutput, EngineError> {
            self.calls
                .lock()
                .expect("mock lock should not be poisoned")
                .push((executable.to_string(), args.to_vec()));
            self.outputs
                .lock()
                .expect("mock lock should not be poisoned")
                .pop_front()
                .expect("mock output should exist")
        }
    }

    fn success_output(stdout: &[u8]) -> ToolOutput {
        ToolOutput {
            status_success: true,
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        }
    }

    fn unique_temp_file_path(extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("tv7-engine-test-{nanos}.{extension}"))
    }

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn generate_tiny_video(path: &Path) {
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=10",
                "-t",
                "2",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(path)
            .output()
            .expect("ffmpeg should run for generated test fixture");

        assert!(
            output.status.success(),
            "ffmpeg fixture generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn rejects_relative_path() {
        let engine = FfmpegMediaEngine::new();
        let error = engine
            .probe_path(Path::new("relative/path.mp4"))
            .expect_err("relative paths should fail");

        assert!(matches!(error, EngineError::InvalidPath(_)));
    }

    #[test]
    fn reports_missing_file() {
        let engine = FfmpegMediaEngine::new();
        let missing = std::env::temp_dir().join("tv7-engine-missing-file.mp4");
        let error = engine
            .probe_path(&missing)
            .expect_err("missing file should fail");

        assert!(matches!(error, EngineError::NotFound(_)));
    }

    #[test]
    fn parses_ffprobe_output() {
        let file_path = unique_temp_file_path("mp4");
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");
        let executor = Arc::new(MockExecutor::new(vec![Ok(success_output(
            br#"{"streams":[{"width":1920,"height":1080}],"format":{"duration":"12.5","format_name":"mov,mp4,m4a"}}"#,
        ))]));
        let engine = FfmpegMediaEngine::with_executor("ffmpeg", "ffprobe", executor);

        let result = engine
            .probe_path(&file_path)
            .expect("probe should parse mock output");

        assert_eq!(result.path, file_path.to_string_lossy().to_string());
        assert_eq!(result.duration_seconds, Some(12.5));
        assert_eq!(result.width, Some(1920));
        assert_eq!(result.height, Some(1080));
        assert_eq!(result.format, "mov");
        assert_eq!(result.size_bytes, 12);

        fs::remove_file(file_path).expect("fixture file should be removed");
    }

    #[test]
    fn rejects_invalid_render_range() {
        let file_path = unique_temp_file_path("mp4");
        let output_path = unique_temp_file_path("mp4");
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");
        let engine = FfmpegMediaEngine::new();

        let error = engine
            .render_clip(&RenderClipRequest {
                source_path: file_path.to_string_lossy().to_string(),
                output_path: output_path.to_string_lossy().to_string(),
                start_seconds: 10.0,
                end_seconds: 5.0,
            })
            .expect_err("invalid range should fail");

        assert!(matches!(error, EngineError::InvalidTimeRange(_)));
        fs::remove_file(file_path).expect("fixture file should be removed");
    }

    #[test]
    fn diagnostics_require_yt_dlp_for_url_ingest_milestone() {
        let temp_dir = std::env::temp_dir();
        let executor = Arc::new(MockExecutor::new(vec![
            Ok(success_output(b"ffprobe version 1")),
            Ok(success_output(b"ffmpeg version 1")),
            Ok(success_output(b"yt-dlp 2026.01.01")),
        ]));
        let engine =
            FfmpegMediaEngine::with_all_bins_and_executor(
                "ffmpeg",
                "ffprobe",
                "yt-dlp",
                executor.clone(),
            );

        let diagnostics = engine.diagnostics(&DiagnosticsContext::new(
            env!("CARGO_PKG_VERSION"),
            "127.0.0.1",
            7070,
            temp_dir,
        ));

        assert_eq!(diagnostics.runtime.host, "127.0.0.1");
        assert_eq!(diagnostics.runtime.port, 7070);
        let yt_dlp = diagnostics
            .tools
            .iter()
            .find(|tool| tool.name == "yt-dlp")
            .expect("yt-dlp diagnostic should exist");
        assert_eq!(yt_dlp.available, Some(true));

        let calls = executor.calls();
        assert_eq!(calls[0], ("ffprobe".to_string(), vec!["-version".to_string()]));
        assert_eq!(calls[1], ("ffmpeg".to_string(), vec!["-version".to_string()]));
        assert_eq!(calls[2], ("yt-dlp".to_string(), vec!["--version".to_string()]));
    }

    #[test]
    fn diagnostics_reports_degraded_when_required_tool_is_missing() {
        let executor = Arc::new(MockExecutor::new(vec![
            Err(EngineError::CommandFailed {
                command: "ffprobe".to_string(),
                message: "missing".to_string(),
            }),
            Ok(success_output(b"ffmpeg version 1")),
            Ok(success_output(b"yt-dlp 2026.01.01")),
        ]));
        let engine =
            FfmpegMediaEngine::with_all_bins_and_executor("ffmpeg", "ffprobe", "yt-dlp", executor);

        let diagnostics = engine.diagnostics(&DiagnosticsContext::new(
            env!("CARGO_PKG_VERSION"),
            "127.0.0.1",
            7070,
            std::env::temp_dir(),
        ));

        assert_eq!(diagnostics.status, DiagnosticStatus::Degraded);
        let ffprobe = diagnostics
            .tools
            .iter()
            .find(|tool| tool.name == "ffprobe")
            .expect("ffprobe diagnostic should exist");
        assert_eq!(ffprobe.available, Some(false));
    }

    #[test]
    fn generated_video_can_be_thumbnailed_rendered_and_probed() {
        if !ffmpeg_available() {
            eprintln!("skipping generated media test because ffmpeg is unavailable");
            return;
        }

        let source_path = unique_temp_file_path("mp4");
        let thumb_path = unique_temp_file_path("jpg");
        let clip_path = unique_temp_file_path("mp4");
        generate_tiny_video(&source_path);
        let engine = FfmpegMediaEngine::new();

        let source_metadata = engine
            .probe_path(&source_path)
            .expect("generated source should be probeable");
        assert_eq!(source_metadata.width, Some(160));
        assert_eq!(source_metadata.height, Some(90));

        let thumbnail = engine
            .generate_thumbnail(&ThumbnailRequest {
                source_path: source_path.to_string_lossy().to_string(),
                output_path: thumb_path.to_string_lossy().to_string(),
                timestamp_seconds: Some(0.25),
            })
            .expect("thumbnail should be generated");
        assert_eq!(
            thumbnail.output_path,
            thumb_path.to_string_lossy().to_string()
        );
        assert!(thumb_path.exists());
        assert!(fs::metadata(&thumb_path).expect("thumb metadata").len() > 0);

        let rendered = engine
            .render_clip(&RenderClipRequest {
                source_path: source_path.to_string_lossy().to_string(),
                output_path: clip_path.to_string_lossy().to_string(),
                start_seconds: 0.25,
                end_seconds: 1.25,
            })
            .expect("clip should be rendered");
        assert_eq!(
            rendered.output_path,
            clip_path.to_string_lossy().to_string()
        );
        assert_eq!(rendered.duration_seconds, 1.0);
        assert!(clip_path.exists());
        assert!(fs::metadata(&clip_path).expect("clip metadata").len() > 0);

        let clip_metadata = engine
            .probe_path(&clip_path)
            .expect("rendered clip should be probeable");
        assert_eq!(clip_metadata.width, Some(160));
        assert_eq!(clip_metadata.height, Some(90));

        fs::remove_file(source_path).expect("source fixture should be removed");
        fs::remove_file(thumb_path).expect("thumbnail fixture should be removed");
        fs::remove_file(clip_path).expect("clip fixture should be removed");
    }
}
