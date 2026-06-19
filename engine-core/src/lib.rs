use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::net::Ipv6Addr;
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
pub struct TranscriptGenerateRequest {
    pub media_path: String,
    pub output_path: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptGenerateResult {
    pub transcript_path: String,
    pub format: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupContext {
    pub platform: String,
    pub asset_dir: PathBuf,
    pub whisper_bin: Option<String>,
    pub whisper_model: Option<PathBuf>,
    pub ollama_bin: String,
    pub ollama_model: Option<String>,
    pub cloud_ai_available: bool,
}

impl SetupContext {
    pub fn new(
        platform: impl Into<String>,
        asset_dir: impl Into<PathBuf>,
        whisper_bin: Option<String>,
        whisper_model: Option<PathBuf>,
        ollama_bin: impl Into<String>,
        ollama_model: Option<String>,
        cloud_ai_available: bool,
    ) -> Self {
        Self {
            platform: platform.into(),
            asset_dir: asset_dir.into(),
            whisper_bin,
            whisper_model,
            ollama_bin: ollama_bin.into(),
            ollama_model,
            cloud_ai_available,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupManifest {
    pub schema_version: u32,
    pub platform: String,
    pub setup_modes: Vec<String>,
    pub ai_provider_order: Vec<ProviderSource>,
    pub assets: Vec<SetupManifestAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupManifestAsset {
    pub id: String,
    pub kind: RuntimeAssetKind,
    pub name: String,
    pub required_for: Vec<String>,
    pub installable: bool,
    pub bundled: bool,
    pub source: ProviderSource,
    pub version: Option<String>,
    pub download: Option<SetupDownload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupDownload {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupStatus {
    pub schema_version: u32,
    pub setup_complete: bool,
    pub active_ai_provider: Option<ProviderSource>,
    pub assets: Vec<SetupAssetStatus>,
    pub ai_providers: Vec<AiProviderCandidate>,
    pub capabilities: Vec<SetupCapabilityStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupAssetStatus {
    pub id: String,
    pub kind: RuntimeAssetKind,
    pub name: String,
    pub status: SetupItemStatus,
    pub source: ProviderSource,
    pub path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiProviderCandidate {
    pub source: ProviderSource,
    pub status: ProviderCandidateStatus,
    pub required_assets: Vec<String>,
    pub requires_internet: bool,
    pub requires_user_confirmation: bool,
    pub cost_policy: String,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupCapabilityStatus {
    pub name: String,
    pub status: SetupCapabilityState,
    pub provider: Option<ProviderSource>,
    pub missing: Vec<String>,
    pub installable: bool,
    pub user_action_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAssetKind {
    MediaTool,
    TranscriptionRuntime,
    TranscriptionModel,
    LlmRuntime,
    LlmModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSource {
    UserPath,
    Tv7Managed,
    Tv7Cloud,
    Bundled,
    Ollama,
    Unconfigured,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupItemStatus {
    Available,
    Missing,
    Installable,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCandidateStatus {
    Ready,
    MissingAssets,
    Installable,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupCapabilityState {
    Ready,
    NotReady,
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

pub trait RuntimeSetupProvider: Send + Sync {
    fn setup_manifest(&self, context: &SetupContext) -> SetupManifest;
    fn setup_status(&self, context: &SetupContext) -> SetupStatus;
}

pub trait UrlIngestor: Send + Sync {
    fn ingest_url(&self, request: &UrlIngestRequest) -> Result<UrlIngestResult, EngineError>;
}

pub trait TranscriptProvider: Send + Sync {
    fn download_transcript(
        &self,
        request: &TranscriptDownloadRequest,
    ) -> Result<TranscriptDownloadResult, EngineError>;

    fn generate_transcript(
        &self,
        request: &TranscriptGenerateRequest,
    ) -> Result<TranscriptGenerateResult, EngineError>;
}

#[derive(Clone)]
pub struct FfmpegMediaEngine {
    ffmpeg_bin: String,
    ffprobe_bin: String,
    yt_dlp_bin: String,
    whisper_bin: Option<String>,
    whisper_model: Option<PathBuf>,
    executor: Arc<dyn CommandExecutor>,
}

impl fmt::Debug for FfmpegMediaEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FfmpegMediaEngine")
            .field("ffmpeg_bin", &self.ffmpeg_bin)
            .field("ffprobe_bin", &self.ffprobe_bin)
            .field("yt_dlp_bin", &self.yt_dlp_bin)
            .field("whisper_bin", &self.whisper_bin)
            .field("whisper_model", &self.whisper_model)
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
            whisper_bin: None,
            whisper_model: None,
            executor: Arc::new(ProcessCommandExecutor),
        }
    }

    pub fn with_bins(ffmpeg_bin: impl Into<String>, ffprobe_bin: impl Into<String>) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: "yt-dlp".to_string(),
            whisper_bin: None,
            whisper_model: None,
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
            whisper_bin: None,
            whisper_model: None,
            executor: Arc::new(ProcessCommandExecutor),
        }
    }

    pub fn with_all_bins_and_transcription(
        ffmpeg_bin: impl Into<String>,
        ffprobe_bin: impl Into<String>,
        yt_dlp_bin: impl Into<String>,
        whisper_bin: Option<String>,
        whisper_model: Option<PathBuf>,
    ) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: yt_dlp_bin.into(),
            whisper_bin,
            whisper_model,
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
            whisper_bin: None,
            whisper_model: None,
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
            whisper_bin: None,
            whisper_model: None,
            executor,
        }
    }

    pub fn with_all_bins_transcription_and_executor(
        ffmpeg_bin: impl Into<String>,
        ffprobe_bin: impl Into<String>,
        yt_dlp_bin: impl Into<String>,
        whisper_bin: Option<String>,
        whisper_model: Option<PathBuf>,
        executor: Arc<dyn CommandExecutor>,
    ) -> Self {
        Self {
            ffmpeg_bin: ffmpeg_bin.into(),
            ffprobe_bin: ffprobe_bin.into(),
            yt_dlp_bin: yt_dlp_bin.into(),
            whisper_bin,
            whisper_model,
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
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| EngineError::InvalidUrl("URL must include a scheme".to_string()))?;
        match scheme {
            "http" | "https" => {}
            scheme => {
                return Err(EngineError::InvalidUrl(format!(
                    "expected http or https URL, got {scheme}"
                )));
            }
        }

        let authority = rest
            .split(['/', '?', '#'])
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| EngineError::InvalidUrl("URL must include a host".to_string()))?;
        let host_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = host_port
            .strip_prefix('[')
            .and_then(|value| value.split_once(']').map(|(host, _)| host))
            .unwrap_or_else(|| host_port.split(':').next().unwrap_or(host_port));
        if host.is_empty() {
            return Err(EngineError::InvalidUrl(
                "URL must include a host".to_string(),
            ));
        }

        if is_disallowed_host(host) {
            return Err(EngineError::InvalidUrl(
                "local and private network URLs are not allowed".to_string(),
            ));
        }

        Ok(())
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

fn is_disallowed_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
            std::net::IpAddr::V6(ip) => is_disallowed_ipv6(ip),
        }
    } else {
        false
    }
}

fn is_disallowed_ipv6(ip: Ipv6Addr) -> bool {
    let first_segment = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || (first_segment & 0xfe00) == 0xfc00
        || (first_segment & 0xffc0) == 0xfe80
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
        let output_base = request
            .output_path
            .strip_suffix(".vtt")
            .unwrap_or(&request.output_path)
            .to_string();
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

    fn generate_transcript(
        &self,
        request: &TranscriptGenerateRequest,
    ) -> Result<TranscriptGenerateResult, EngineError> {
        let media_path = Path::new(&request.media_path);
        self.validate_source_path(media_path)?;
        let output_path = Path::new(&request.output_path);
        self.validate_output_path(media_path, output_path)?;
        if output_path.extension().and_then(|ext| ext.to_str()) != Some("vtt") {
            return Err(EngineError::InvalidPath(format!(
                "expected output_path to end with .vtt, got {}",
                output_path.display()
            )));
        }

        let whisper_bin =
            self.whisper_bin
                .as_deref()
                .ok_or_else(|| EngineError::CommandFailed {
                    command: "whisper".to_string(),
                    message: "transcription runtime is not configured".to_string(),
                })?;
        let whisper_model =
            self.whisper_model
                .as_deref()
                .ok_or_else(|| EngineError::CommandFailed {
                    command: whisper_bin.to_string(),
                    message: "transcription model is not configured".to_string(),
                })?;

        self.validate_source_path(whisper_model)?;

        let language = request.language.clone().unwrap_or_else(|| "en".to_string());
        let output_base = request
            .output_path
            .strip_suffix(".vtt")
            .unwrap_or(&request.output_path)
            .to_string();
        let args = vec![
            "-m".to_string(),
            whisper_model.to_string_lossy().to_string(),
            "-f".to_string(),
            request.media_path.clone(),
            "-l".to_string(),
            language.clone(),
            "-ovtt".to_string(),
            "-of".to_string(),
            output_base,
        ];
        self.run_checked(whisper_bin, &args)?;

        if !output_path.is_file() {
            return Err(EngineError::OutputInvalid(format!(
                "transcription completed without creating {}",
                output_path.display()
            )));
        }

        Ok(TranscriptGenerateResult {
            transcript_path: request.output_path.clone(),
            format: "vtt".to_string(),
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

impl RuntimeSetupProvider for FfmpegMediaEngine {
    fn setup_manifest(&self, context: &SetupContext) -> SetupManifest {
        SetupManifest {
            schema_version: 1,
            platform: context.platform.clone(),
            setup_modes: vec!["automatic".to_string(), "disabled".to_string()],
            ai_provider_order: vec![
                ProviderSource::Ollama,
                ProviderSource::Tv7Managed,
                ProviderSource::Tv7Cloud,
            ],
            assets: vec![
                SetupManifestAsset {
                    id: "whisper-runtime.user-path".to_string(),
                    kind: RuntimeAssetKind::TranscriptionRuntime,
                    name: "User-installed Whisper runtime".to_string(),
                    required_for: vec!["transcription".to_string(), "offline_creation".to_string()],
                    installable: false,
                    bundled: false,
                    source: ProviderSource::UserPath,
                    version: None,
                    download: None,
                },
                SetupManifestAsset {
                    id: format!("whisper-model.tv7-managed.{}", context.platform),
                    kind: RuntimeAssetKind::TranscriptionModel,
                    name: "TV7-managed Whisper model".to_string(),
                    required_for: vec!["transcription".to_string(), "offline_creation".to_string()],
                    installable: true,
                    bundled: false,
                    source: ProviderSource::Tv7Managed,
                    version: None,
                    download: None,
                },
                SetupManifestAsset {
                    id: "ollama.user-runtime".to_string(),
                    kind: RuntimeAssetKind::LlmRuntime,
                    name: "User-installed Ollama runtime".to_string(),
                    required_for: vec!["ai".to_string(), "offline_creation".to_string()],
                    installable: false,
                    bundled: false,
                    source: ProviderSource::Ollama,
                    version: None,
                    download: None,
                },
                SetupManifestAsset {
                    id: "ollama.user-model".to_string(),
                    kind: RuntimeAssetKind::LlmModel,
                    name: "User-installed Ollama model".to_string(),
                    required_for: vec!["ai".to_string(), "offline_creation".to_string()],
                    installable: false,
                    bundled: false,
                    source: ProviderSource::Ollama,
                    version: context.ollama_model.clone(),
                    download: None,
                },
                SetupManifestAsset {
                    id: format!("local-llm.tv7-managed.{}", context.platform),
                    kind: RuntimeAssetKind::LlmModel,
                    name: "TV7-managed local LLM".to_string(),
                    required_for: vec!["ai".to_string(), "offline_creation".to_string()],
                    installable: true,
                    bundled: false,
                    source: ProviderSource::Tv7Managed,
                    version: None,
                    download: None,
                },
            ],
        }
    }

    fn setup_status(&self, context: &SetupContext) -> SetupStatus {
        let whisper_runtime = self.detect_whisper_runtime(context);
        let whisper_model = detect_path_asset(
            "whisper-model.user-path",
            RuntimeAssetKind::TranscriptionModel,
            "User-provided Whisper model",
            ProviderSource::UserPath,
            context.whisper_model.as_deref(),
        );
        let ollama_runtime = self.detect_ollama_runtime(context);
        let ollama_model = self.detect_ollama_model(context, &ollama_runtime);

        let whisper_runtime_ready = whisper_runtime.status == SetupItemStatus::Available;
        let whisper_model_ready = whisper_model.status == SetupItemStatus::Available;
        let ollama_runtime_ready = ollama_runtime.status == SetupItemStatus::Available;
        let ollama_model_ready = ollama_model.status == SetupItemStatus::Available;
        let transcription_ready = whisper_runtime_ready && whisper_model_ready;
        let local_ai_ready = ollama_runtime_ready && ollama_model_ready;
        let ai_ready = local_ai_ready || context.cloud_ai_available;
        let offline_ready = transcription_ready && local_ai_ready;

        SetupStatus {
            schema_version: 1,
            setup_complete: offline_ready,
            active_ai_provider: active_ai_provider(local_ai_ready, context.cloud_ai_available),
            assets: vec![
                whisper_runtime,
                whisper_model,
                SetupAssetStatus {
                    id: format!("whisper-model.tv7-managed.{}", context.platform),
                    kind: RuntimeAssetKind::TranscriptionModel,
                    name: "TV7-managed Whisper model".to_string(),
                    status: SetupItemStatus::Installable,
                    source: ProviderSource::Tv7Managed,
                    path: Some(
                        context
                            .asset_dir
                            .join("models")
                            .join("whisper")
                            .to_string_lossy()
                            .to_string(),
                    ),
                    version: None,
                    error: Some("download requires explicit user confirmation".to_string()),
                },
                ollama_runtime,
                ollama_model,
                SetupAssetStatus {
                    id: format!("local-llm.tv7-managed.{}", context.platform),
                    kind: RuntimeAssetKind::LlmModel,
                    name: "TV7-managed local LLM".to_string(),
                    status: SetupItemStatus::Installable,
                    source: ProviderSource::Tv7Managed,
                    path: Some(
                        context
                            .asset_dir
                            .join("models")
                            .join("llm")
                            .to_string_lossy()
                            .to_string(),
                    ),
                    version: None,
                    error: Some("download requires explicit user confirmation".to_string()),
                },
            ],
            ai_providers: ai_provider_candidates(
                local_ai_ready,
                context.cloud_ai_available,
                context.ollama_model.as_deref(),
                &context.platform,
            ),
            capabilities: vec![
                SetupCapabilityStatus {
                    name: "transcription".to_string(),
                    status: setup_status_for(transcription_ready),
                    provider: if transcription_ready {
                        Some(ProviderSource::UserPath)
                    } else {
                        None
                    },
                    missing: missing_for_transcription(whisper_runtime_ready, whisper_model_ready),
                    installable: true,
                    user_action_required: !transcription_ready,
                },
                SetupCapabilityStatus {
                    name: "ai".to_string(),
                    status: setup_status_for(ai_ready),
                    provider: active_ai_provider(local_ai_ready, context.cloud_ai_available),
                    missing: missing_for_ai(
                        ollama_runtime_ready,
                        ollama_model_ready,
                        context.ollama_model.as_deref(),
                        context.cloud_ai_available,
                    ),
                    installable: true,
                    user_action_required: !ai_ready,
                },
                SetupCapabilityStatus {
                    name: "offline_creation".to_string(),
                    status: setup_status_for(offline_ready),
                    provider: if offline_ready {
                        Some(ProviderSource::UserPath)
                    } else {
                        None
                    },
                    missing: missing_for_offline_creation(transcription_ready, local_ai_ready),
                    installable: true,
                    user_action_required: !offline_ready,
                },
            ],
        }
    }
}

impl FfmpegMediaEngine {
    fn detect_whisper_runtime(&self, context: &SetupContext) -> SetupAssetStatus {
        let candidates = context
            .whisper_bin
            .clone()
            .map(|bin| vec![bin])
            .unwrap_or_else(|| vec!["whisper-cli".to_string(), "whisper".to_string()]);

        for candidate in candidates {
            if let Some(version) = self.detect_tool_version(&candidate, &["--help"]) {
                return SetupAssetStatus {
                    id: "whisper-runtime.user-path".to_string(),
                    kind: RuntimeAssetKind::TranscriptionRuntime,
                    name: "User-installed Whisper runtime".to_string(),
                    status: SetupItemStatus::Available,
                    source: ProviderSource::UserPath,
                    path: Some(candidate),
                    version,
                    error: None,
                };
            }
        }

        SetupAssetStatus {
            id: "whisper-runtime.user-path".to_string(),
            kind: RuntimeAssetKind::TranscriptionRuntime,
            name: "User-installed Whisper runtime".to_string(),
            status: SetupItemStatus::Missing,
            source: ProviderSource::UserPath,
            path: context.whisper_bin.clone(),
            version: None,
            error: Some("no configured or PATH-resolved Whisper runtime found".to_string()),
        }
    }

    fn detect_ollama_runtime(&self, context: &SetupContext) -> SetupAssetStatus {
        match self.detect_tool_version(&context.ollama_bin, &["--version"]) {
            Some(version) => SetupAssetStatus {
                id: "ollama.user-runtime".to_string(),
                kind: RuntimeAssetKind::LlmRuntime,
                name: "User-installed Ollama runtime".to_string(),
                status: SetupItemStatus::Available,
                source: ProviderSource::Ollama,
                path: Some(context.ollama_bin.clone()),
                version,
                error: None,
            },
            None => SetupAssetStatus {
                id: "ollama.user-runtime".to_string(),
                kind: RuntimeAssetKind::LlmRuntime,
                name: "User-installed Ollama runtime".to_string(),
                status: SetupItemStatus::Missing,
                source: ProviderSource::Ollama,
                path: Some(context.ollama_bin.clone()),
                version: None,
                error: Some("Ollama is not available on the configured path".to_string()),
            },
        }
    }

    fn detect_ollama_model(
        &self,
        context: &SetupContext,
        runtime: &SetupAssetStatus,
    ) -> SetupAssetStatus {
        let Some(model) = context.ollama_model.as_deref() else {
            return SetupAssetStatus {
                id: "ollama.user-model".to_string(),
                kind: RuntimeAssetKind::LlmModel,
                name: "User-installed Ollama model".to_string(),
                status: SetupItemStatus::Missing,
                source: ProviderSource::Ollama,
                path: None,
                version: None,
                error: Some("TV7_ENGINE_OLLAMA_MODEL is not configured".to_string()),
            };
        };

        if runtime.status != SetupItemStatus::Available {
            return SetupAssetStatus {
                id: "ollama.user-model".to_string(),
                kind: RuntimeAssetKind::LlmModel,
                name: "User-installed Ollama model".to_string(),
                status: SetupItemStatus::Missing,
                source: ProviderSource::Ollama,
                path: None,
                version: Some(model.to_string()),
                error: Some("Ollama runtime is unavailable".to_string()),
            };
        }

        let args = vec!["list".to_string()];
        match self.executor.run(&context.ollama_bin, &args) {
            Ok(output) if output.status_success => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let found = stdout.lines().any(|line| {
                    line.split_whitespace()
                        .next()
                        .map(|name| name == model || name.starts_with(&format!("{model}:")))
                        .unwrap_or(false)
                });
                SetupAssetStatus {
                    id: "ollama.user-model".to_string(),
                    kind: RuntimeAssetKind::LlmModel,
                    name: "User-installed Ollama model".to_string(),
                    status: if found {
                        SetupItemStatus::Available
                    } else {
                        SetupItemStatus::Missing
                    },
                    source: ProviderSource::Ollama,
                    path: None,
                    version: Some(model.to_string()),
                    error: if found {
                        None
                    } else {
                        Some("configured Ollama model is not installed".to_string())
                    },
                }
            }
            Ok(output) => SetupAssetStatus {
                id: "ollama.user-model".to_string(),
                kind: RuntimeAssetKind::LlmModel,
                name: "User-installed Ollama model".to_string(),
                status: SetupItemStatus::Missing,
                source: ProviderSource::Ollama,
                path: None,
                version: Some(model.to_string()),
                error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            },
            Err(error) => SetupAssetStatus {
                id: "ollama.user-model".to_string(),
                kind: RuntimeAssetKind::LlmModel,
                name: "User-installed Ollama model".to_string(),
                status: SetupItemStatus::Missing,
                source: ProviderSource::Ollama,
                path: None,
                version: Some(model.to_string()),
                error: Some(error.to_string()),
            },
        }
    }

    fn detect_tool_version(&self, executable: &str, args: &[&str]) -> Option<Option<String>> {
        let args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        match self.executor.run(executable, &args) {
            Ok(output) if output.status_success => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                Some(
                    stdout
                        .lines()
                        .chain(stderr.lines())
                        .next()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_string),
                )
            }
            _ => None,
        }
    }
}

fn detect_path_asset(
    id: &str,
    kind: RuntimeAssetKind,
    name: &str,
    source: ProviderSource,
    path: Option<&Path>,
) -> SetupAssetStatus {
    match path {
        Some(path) if path.is_file() => SetupAssetStatus {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            status: SetupItemStatus::Available,
            source,
            path: Some(path.to_string_lossy().to_string()),
            version: None,
            error: None,
        },
        Some(path) => SetupAssetStatus {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            status: SetupItemStatus::Missing,
            source,
            path: Some(path.to_string_lossy().to_string()),
            version: None,
            error: Some("configured path does not point to a file".to_string()),
        },
        None => SetupAssetStatus {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            status: SetupItemStatus::Missing,
            source,
            path: None,
            version: None,
            error: Some("not configured".to_string()),
        },
    }
}

fn setup_status_for(ready: bool) -> SetupCapabilityState {
    if ready {
        SetupCapabilityState::Ready
    } else {
        SetupCapabilityState::NotReady
    }
}

fn missing_for_transcription(runtime_ready: bool, model_ready: bool) -> Vec<String> {
    let mut missing = Vec::new();
    if !runtime_ready {
        missing.push("transcription_runtime".to_string());
    }
    if !model_ready {
        missing.push("transcription_model".to_string());
    }
    missing
}

fn active_ai_provider(local_ai_ready: bool, cloud_ai_available: bool) -> Option<ProviderSource> {
    if local_ai_ready {
        Some(ProviderSource::Ollama)
    } else if cloud_ai_available {
        Some(ProviderSource::Tv7Cloud)
    } else {
        None
    }
}

fn ai_provider_candidates(
    local_ai_ready: bool,
    cloud_ai_available: bool,
    ollama_model: Option<&str>,
    platform: &str,
) -> Vec<AiProviderCandidate> {
    vec![
        AiProviderCandidate {
            source: ProviderSource::Ollama,
            status: if local_ai_ready {
                ProviderCandidateStatus::Ready
            } else {
                ProviderCandidateStatus::MissingAssets
            },
            required_assets: missing_local_ai_assets(local_ai_ready, ollama_model),
            requires_internet: false,
            requires_user_confirmation: !local_ai_ready,
            cost_policy: "user_local".to_string(),
            action: if local_ai_ready {
                None
            } else {
                Some("Install Ollama and configure TV7_ENGINE_OLLAMA_MODEL, or use another AI provider.".to_string())
            },
        },
        AiProviderCandidate {
            source: ProviderSource::Tv7Managed,
            status: ProviderCandidateStatus::Installable,
            required_assets: vec![format!("local-llm.tv7-managed.{platform}")],
            requires_internet: true,
            requires_user_confirmation: true,
            cost_policy: "one_time_download".to_string(),
            action: Some(
                "Download or import a TV7-managed local model after explicit user confirmation."
                    .to_string(),
            ),
        },
        AiProviderCandidate {
            source: ProviderSource::Tv7Cloud,
            status: if cloud_ai_available {
                ProviderCandidateStatus::Ready
            } else {
                ProviderCandidateStatus::Unavailable
            },
            required_assets: Vec::new(),
            requires_internet: true,
            requires_user_confirmation: false,
            cost_policy: "tv7_managed_usage".to_string(),
            action: if cloud_ai_available {
                None
            } else {
                Some("TV7 cloud AI is unavailable for this account or environment.".to_string())
            },
        },
    ]
}

fn missing_for_ai(
    runtime_ready: bool,
    model_ready: bool,
    ollama_model: Option<&str>,
    cloud_ai_available: bool,
) -> Vec<String> {
    if cloud_ai_available {
        return Vec::new();
    }
    missing_local_ai_assets(runtime_ready && model_ready, ollama_model)
}

fn missing_local_ai_assets(local_ai_ready: bool, ollama_model: Option<&str>) -> Vec<String> {
    if local_ai_ready {
        return Vec::new();
    }
    let mut missing = Vec::new();
    missing.push("llm_runtime".to_string());
    if ollama_model.is_none() {
        missing.push("llm_model".to_string());
    }
    missing
}

fn missing_for_offline_creation(transcription_ready: bool, local_ai_ready: bool) -> Vec<String> {
    let mut missing = Vec::new();
    if !transcription_ready {
        missing.push("transcription".to_string());
    }
    if !local_ai_ready {
        missing.push("local_ai".to_string());
    }
    missing
}

fn list_files(directory: &Path) -> Result<Vec<PathBuf>, EngineError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| EngineError::Io(error.to_string()))? {
        let path = entry
            .map_err(|error| EngineError::Io(error.to_string()))?
            .path();
        if path.is_file() {
            files.push(path);
        }
    }
    Ok(files)
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

    #[derive(Debug)]
    struct TranscriptExecutor {
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl TranscriptExecutor {
        fn new() -> Self {
            Self {
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

    impl CommandExecutor for TranscriptExecutor {
        fn run(&self, executable: &str, args: &[String]) -> Result<ToolOutput, EngineError> {
            self.calls
                .lock()
                .expect("mock lock should not be poisoned")
                .push((executable.to_string(), args.to_vec()));
            let output_base = args
                .windows(2)
                .find(|pair| pair[0] == "-of")
                .map(|pair| pair[1].clone())
                .expect("transcript command should include -of");
            fs::write(format!("{output_base}.vtt"), "WEBVTT\n\n")
                .map_err(|error| EngineError::Io(error.to_string()))?;
            Ok(ToolOutput {
                status_success: true,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    fn success_output(stdout: &[u8]) -> ToolOutput {
        ToolOutput {
            status_success: true,
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        }
    }

    fn failed_command(command: &str) -> Result<ToolOutput, EngineError> {
        Err(EngineError::CommandFailed {
            command: command.to_string(),
            message: "missing".to_string(),
        })
    }

    static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn unique_temp_file_path(extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let process_id = std::process::id();
        std::env::temp_dir().join(format!(
            "tv7-engine-test-{process_id}-{nanos}-{counter}.{extension}"
        ))
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
    fn rejects_output_path_with_missing_parent() {
        let file_path = unique_temp_file_path("mp4");
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");
        let missing_output = std::env::temp_dir()
            .join("tv7-engine-missing-output-parent")
            .join("thumb.jpg");
        let engine = FfmpegMediaEngine::new();

        let error = engine
            .generate_thumbnail(&ThumbnailRequest {
                source_path: file_path.to_string_lossy().to_string(),
                output_path: missing_output.to_string_lossy().to_string(),
                timestamp_seconds: Some(1.0),
            })
            .expect_err("missing output parent should fail");

        assert!(matches!(error, EngineError::OutputParentMissing(_)));
        fs::remove_file(file_path).expect("fixture file should be removed");
    }

    #[test]
    fn rejects_source_and_output_same_path() {
        let file_path = unique_temp_file_path("mp4");
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");
        let path = file_path.to_string_lossy().to_string();
        let engine = FfmpegMediaEngine::new();

        let error = engine
            .render_clip(&RenderClipRequest {
                source_path: path.clone(),
                output_path: path,
                start_seconds: 0.0,
                end_seconds: 1.0,
            })
            .expect_err("same source and output should fail");

        assert!(matches!(error, EngineError::SourceOutputSame(_)));
        fs::remove_file(file_path).expect("fixture file should be removed");
    }

    #[test]
    fn maps_nonzero_command_exit_to_command_failed() {
        let file_path = unique_temp_file_path("mp4");
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");
        let executor = Arc::new(MockExecutor::new(vec![Ok(ToolOutput {
            status_success: false,
            stdout: Vec::new(),
            stderr: b"ffprobe failed".to_vec(),
        })]));
        let engine = FfmpegMediaEngine::with_executor("ffmpeg", "ffprobe", executor);

        let error = engine
            .probe_path(&file_path)
            .expect_err("nonzero command should fail");

        assert!(matches!(
            error,
            EngineError::CommandFailed { command, message }
                if command == "ffprobe" && message == "ffprobe failed"
        ));
        fs::remove_file(file_path).expect("fixture file should be removed");
    }

    #[test]
    fn maps_malformed_ffprobe_json_to_output_invalid() {
        let file_path = unique_temp_file_path("mp4");
        fs::write(&file_path, b"sample-bytes").expect("fixture file should be created");
        let executor = Arc::new(MockExecutor::new(vec![Ok(success_output(b"not json"))]));
        let engine = FfmpegMediaEngine::with_executor("ffmpeg", "ffprobe", executor);

        let error = engine
            .probe_path(&file_path)
            .expect_err("malformed ffprobe json should fail");

        assert!(matches!(error, EngineError::OutputInvalid(_)));
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
        let engine = FfmpegMediaEngine::with_all_bins_and_executor(
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
        assert_eq!(
            calls[0],
            ("ffprobe".to_string(), vec!["-version".to_string()])
        );
        assert_eq!(
            calls[1],
            ("ffmpeg".to_string(), vec!["-version".to_string()])
        );
        assert_eq!(
            calls[2],
            ("yt-dlp".to_string(), vec!["--version".to_string()])
        );
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
    fn setup_status_prefers_user_installed_whisper_and_ollama_assets() {
        let model_path = unique_temp_file_path("bin");
        fs::write(&model_path, b"model").expect("model fixture should be created");
        let executor = Arc::new(MockExecutor::new(vec![
            Ok(success_output(b"whisper-cli help")),
            Ok(success_output(b"ollama version 0.1.0")),
            Ok(success_output(b"NAME ID SIZE\n gemma4:latest abc 4GB\n")),
        ]));
        let engine =
            FfmpegMediaEngine::with_all_bins_and_executor("ffmpeg", "ffprobe", "yt-dlp", executor);

        let status = engine.setup_status(&SetupContext::new(
            "test-platform",
            std::env::temp_dir().join("tv7-assets"),
            Some("whisper-cli".to_string()),
            Some(model_path.clone()),
            "ollama",
            Some("gemma4".to_string()),
            true,
        ));

        let transcription = status
            .capabilities
            .iter()
            .find(|capability| capability.name == "transcription")
            .expect("transcription capability should exist");
        assert_eq!(transcription.status, SetupCapabilityState::Ready);
        assert_eq!(transcription.provider, Some(ProviderSource::UserPath));

        let ai = status
            .capabilities
            .iter()
            .find(|capability| capability.name == "ai")
            .expect("ai capability should exist");
        assert_eq!(ai.status, SetupCapabilityState::Ready);
        assert_eq!(ai.provider, Some(ProviderSource::Ollama));
        assert_eq!(status.active_ai_provider, Some(ProviderSource::Ollama));

        fs::remove_file(model_path).expect("model fixture should be removed");
    }

    #[test]
    fn setup_status_uses_tv7_cloud_ai_without_marking_offline_creation_ready() {
        let executor = Arc::new(MockExecutor::new(vec![
            failed_command("whisper-cli"),
            failed_command("whisper"),
            failed_command("ollama"),
        ]));
        let engine =
            FfmpegMediaEngine::with_all_bins_and_executor("ffmpeg", "ffprobe", "yt-dlp", executor);

        let status = engine.setup_status(&SetupContext::new(
            "test-platform",
            std::env::temp_dir().join("tv7-assets"),
            None,
            None,
            "ollama",
            None,
            true,
        ));

        let ai = status
            .capabilities
            .iter()
            .find(|capability| capability.name == "ai")
            .expect("ai capability should exist");
        assert_eq!(ai.status, SetupCapabilityState::Ready);
        assert_eq!(ai.provider, Some(ProviderSource::Tv7Cloud));

        let offline = status
            .capabilities
            .iter()
            .find(|capability| capability.name == "offline_creation")
            .expect("offline_creation capability should exist");
        assert_eq!(offline.status, SetupCapabilityState::NotReady);
        assert!(offline.missing.contains(&"transcription".to_string()));
        assert!(offline.missing.contains(&"local_ai".to_string()));
    }

    #[test]
    fn setup_status_reports_installable_fallbacks_without_downloads() {
        let executor = Arc::new(MockExecutor::new(vec![
            failed_command("whisper-cli"),
            failed_command("whisper"),
            failed_command("ollama"),
        ]));
        let engine =
            FfmpegMediaEngine::with_all_bins_and_executor("ffmpeg", "ffprobe", "yt-dlp", executor);

        let status = engine.setup_status(&SetupContext::new(
            "test-platform",
            std::env::temp_dir().join("tv7-assets"),
            None,
            None,
            "ollama",
            None,
            false,
        ));

        let installable_assets = status
            .assets
            .iter()
            .filter(|asset| asset.status == SetupItemStatus::Installable)
            .map(|asset| asset.source.clone())
            .collect::<Vec<_>>();
        assert!(installable_assets.contains(&ProviderSource::Tv7Managed));

        let offline = status
            .capabilities
            .iter()
            .find(|capability| capability.name == "offline_creation")
            .expect("offline_creation capability should exist");
        assert_eq!(offline.status, SetupCapabilityState::NotReady);
        assert!(offline.user_action_required);
        assert!(offline.installable);
    }

    #[test]
    fn rejects_non_http_and_local_ingest_urls() {
        let engine = FfmpegMediaEngine::new();
        let output_dir = std::env::temp_dir().to_string_lossy().to_string();

        for url in [
            "file:///tmp/video.mp4",
            "ytsearch:test",
            "http://localhost/video",
            "https://127.0.0.1/video",
            "https://10.0.0.1/video",
            "https://[fc00::1]/video",
            "https://[fd12:3456:789a::1]/video",
            "https://[fe80::1]/video",
        ] {
            let error = engine
                .ingest_url(&UrlIngestRequest {
                    url: url.to_string(),
                    output_dir: output_dir.clone(),
                    output_template: None,
                })
                .expect_err("unsafe url should be rejected before command execution");
            assert!(matches!(error, EngineError::InvalidUrl(_)));
        }
    }

    #[test]
    fn transcript_output_base_removes_only_one_vtt_suffix() {
        let path = "/tmp/video.vtt.vtt".to_string();
        let output_base = path.strip_suffix(".vtt").unwrap_or(&path).to_string();
        assert_eq!(output_base, "/tmp/video.vtt");
    }

    #[test]
    fn generate_transcript_runs_configured_whisper_and_returns_vtt_path() {
        let temp_root = std::env::temp_dir().join(format!(
            "tv7-engine-transcript-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_root).expect("temp root should be created");
        let media_path = temp_root.join("video.mp4");
        let model_path = temp_root.join("ggml-base.bin");
        let output_path = temp_root.join("video.vtt");
        fs::write(&media_path, b"fake video").expect("media file should be created");
        fs::write(&model_path, b"fake model").expect("model file should be created");
        let executor = Arc::new(TranscriptExecutor::new());
        let engine = FfmpegMediaEngine::with_all_bins_transcription_and_executor(
            "ffmpeg",
            "ffprobe",
            "yt-dlp",
            Some("whisper-cli".to_string()),
            Some(model_path.clone()),
            executor.clone(),
        );

        let result = engine
            .generate_transcript(&TranscriptGenerateRequest {
                media_path: media_path.to_string_lossy().to_string(),
                output_path: output_path.to_string_lossy().to_string(),
                language: Some("en".to_string()),
            })
            .expect("transcript generation should succeed");

        assert_eq!(result.transcript_path, output_path.to_string_lossy());
        assert_eq!(result.format, "vtt");
        assert_eq!(result.language, "en");
        let calls = executor.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "whisper-cli");
        assert!(calls[0].1.contains(&"-ovtt".to_string()));
        assert!(
            calls[0]
                .1
                .contains(&model_path.to_string_lossy().to_string())
        );
        fs::remove_dir_all(temp_root).expect("temp root should be removed");
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
