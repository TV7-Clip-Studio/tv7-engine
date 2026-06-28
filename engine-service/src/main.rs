use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use engine_core::{
    ClipRenderer, DiagnosticStatus, DiagnosticsContext, DiagnosticsProvider, EngineDiagnostics,
    EngineError, FfmpegMediaEngine, MediaMetadata, MediaProbe, ProviderSource, RenderClipRequest,
    RenderClipResult, RuntimeSetupProvider, SetupCapabilityState, SetupContext, SetupManifest,
    SetupStatus, ThumbnailGenerator, ThumbnailRequest, ThumbnailResult, TranscriptDownloadRequest,
    TranscriptGenerateRequest, TranscriptProvider, UrlIngestRequest, UrlIngestor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

type SharedMediaProbe = Arc<dyn MediaProbe>;
type SharedThumbnailGenerator = Arc<dyn ThumbnailGenerator>;
type SharedClipRenderer = Arc<dyn ClipRenderer>;
type SharedDiagnosticsProvider = Arc<dyn DiagnosticsProvider>;
type SharedRuntimeSetupProvider = Arc<dyn RuntimeSetupProvider>;
type SharedUrlIngestor = Arc<dyn UrlIngestor>;
type SharedTranscriptProvider = Arc<dyn TranscriptProvider>;
type SharedCloudAiExecutor = Arc<dyn CloudAiExecutor>;
type CloudAiExecuteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CloudAiExecuteResponse, CloudAiError>> + Send + 'a>>;

#[derive(Clone)]
struct AppState {
    media_probe: SharedMediaProbe,
    thumbnail_generator: SharedThumbnailGenerator,
    clip_renderer: SharedClipRenderer,
    diagnostics_provider: SharedDiagnosticsProvider,
    setup_provider: SharedRuntimeSetupProvider,
    url_ingestor: SharedUrlIngestor,
    transcript_provider: SharedTranscriptProvider,
    jobs: JobStore,
    diagnostics_context: DiagnosticsContext,
    setup_context: SetupContext,
    cloud_ai_tokens: CloudAiTokenStore,
    cloud_ai_executor: SharedCloudAiExecutor,
}

impl AppState {
    fn effective_setup_context(&self) -> SetupContext {
        let mut context = self.setup_context.clone();
        context.cloud_ai_available =
            self.setup_context.cloud_ai_available && self.cloud_ai_tokens.has_ready_token();
        context
    }
}

#[derive(Debug, Clone)]
struct AppConfig {
    host: IpAddr,
    port: u16,
    log_level: String,
    work_dir: PathBuf,
    asset_dir: PathBuf,
    ffmpeg_bin: String,
    ffprobe_bin: String,
    yt_dlp_bin: String,
    whisper_bin: Option<String>,
    whisper_model: Option<PathBuf>,
    ollama_bin: String,
    ollama_model: Option<String>,
    cloud_ai_available: bool,
    backend_base_url: Option<String>,
}

#[derive(Debug)]
struct AppConfigValues {
    host: Option<String>,
    port: Option<String>,
    log_level: Option<String>,
    work_dir: PathBuf,
    asset_dir: Option<PathBuf>,
    ffmpeg_bin: Option<String>,
    ffprobe_bin: Option<String>,
    yt_dlp_bin: Option<String>,
    whisper_bin: Option<String>,
    whisper_model: Option<PathBuf>,
    ollama_bin: Option<String>,
    ollama_model: Option<String>,
    cloud_ai_available: Option<String>,
    backend_base_url: Option<String>,
}

impl AppConfig {
    fn from_env() -> Result<Self> {
        let work_dir = match env::var("TV7_ENGINE_WORK_DIR") {
            Ok(value) => PathBuf::from(value),
            Err(_) => env::current_dir().context("failed to resolve current directory")?,
        };

        Self::from_values(AppConfigValues {
            host: env::var("TV7_ENGINE_HOST").ok(),
            port: env::var("TV7_ENGINE_PORT").ok(),
            log_level: env::var("TV7_ENGINE_LOG_LEVEL").ok(),
            work_dir,
            asset_dir: env::var("TV7_ENGINE_ASSET_DIR").ok().map(PathBuf::from),
            ffmpeg_bin: env::var("TV7_ENGINE_FFMPEG_BIN").ok(),
            ffprobe_bin: env::var("TV7_ENGINE_FFPROBE_BIN").ok(),
            yt_dlp_bin: env::var("TV7_ENGINE_YT_DLP_BIN").ok(),
            whisper_bin: env::var("TV7_ENGINE_WHISPER_BIN").ok(),
            whisper_model: env::var("TV7_ENGINE_WHISPER_MODEL").ok().map(PathBuf::from),
            ollama_bin: env::var("TV7_ENGINE_OLLAMA_BIN").ok(),
            ollama_model: env::var("TV7_ENGINE_OLLAMA_MODEL").ok(),
            cloud_ai_available: env::var("TV7_ENGINE_CLOUD_AI_AVAILABLE").ok(),
            backend_base_url: env::var("TV7_ENGINE_BACKEND_BASE_URL").ok(),
        })
    }

    fn from_values(values: AppConfigValues) -> Result<Self> {
        let host = values
            .host
            .as_deref()
            .map(str::parse::<IpAddr>)
            .transpose()
            .context("TV7_ENGINE_HOST must be a valid ip address")?
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

        if !host.is_loopback() {
            anyhow::bail!("TV7_ENGINE_HOST must be loopback, got {host}");
        }

        let port = values
            .port
            .as_deref()
            .map(str::parse::<u16>)
            .transpose()
            .context("TV7_ENGINE_PORT must be a valid u16 port")?
            .unwrap_or(7070);

        let log_level = values.log_level.unwrap_or_else(|| "info".to_string());
        let ffmpeg_bin = values.ffmpeg_bin.unwrap_or_else(|| "ffmpeg".to_string());
        let ffprobe_bin = values.ffprobe_bin.unwrap_or_else(|| "ffprobe".to_string());
        let yt_dlp_bin = values.yt_dlp_bin.unwrap_or_else(|| "yt-dlp".to_string());
        let ollama_bin = values.ollama_bin.unwrap_or_else(|| "ollama".to_string());
        let cloud_ai_available = values
            .cloud_ai_available
            .as_deref()
            .map(parse_bool)
            .transpose()
            .context("TV7_ENGINE_CLOUD_AI_AVAILABLE must be true or false")?
            .unwrap_or(true);
        let backend_base_url = values
            .backend_base_url
            .map(|value| value.trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());
        let asset_dir = values
            .asset_dir
            .unwrap_or_else(|| values.work_dir.join(".tv7-engine-assets"));

        if !values.work_dir.is_absolute() {
            anyhow::bail!(
                "TV7_ENGINE_WORK_DIR must be an absolute path, got {}",
                values.work_dir.display()
            );
        }

        if !values.work_dir.is_dir() {
            anyhow::bail!(
                "TV7_ENGINE_WORK_DIR must point to an existing directory, got {}",
                values.work_dir.display()
            );
        }

        if !asset_dir.is_absolute() {
            anyhow::bail!(
                "TV7_ENGINE_ASSET_DIR must be an absolute path, got {}",
                asset_dir.display()
            );
        }

        if let Some(path) = values.whisper_model.as_deref() {
            if !path.is_absolute() {
                anyhow::bail!(
                    "TV7_ENGINE_WHISPER_MODEL must be an absolute path, got {}",
                    path.display()
                );
            }
        }

        Ok(Self {
            host,
            port,
            log_level,
            work_dir: values.work_dir,
            asset_dir,
            ffmpeg_bin,
            ffprobe_bin,
            yt_dlp_bin,
            whisper_bin: values.whisper_bin,
            whisper_model: values.whisper_model,
            ollama_bin,
            ollama_model: values.ollama_model,
            cloud_ai_available,
            backend_base_url,
        })
    }

    fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Deserialize)]
struct ProbeRequest {
    path: String,
}

#[derive(Debug, Serialize)]
struct ProbeResponse {
    metadata: MediaMetadata,
}

#[derive(Debug, Serialize)]
struct ThumbnailResponse {
    result: ThumbnailResult,
}

#[derive(Debug, Serialize)]
struct RenderClipResponse {
    result: RenderClipResult,
}

#[derive(Debug, Serialize)]
struct JobCreateResponse {
    job: EngineJob,
}

#[derive(Debug, Serialize)]
struct JobLogsResponse {
    job_id: String,
    logs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SetupRequiredBody {
    error: &'static str,
    message: String,
    missing: Vec<String>,
    setup_capability: &'static str,
}

#[derive(Debug, Deserialize)]
struct CloudAiTokenRequest {
    token: String,
    #[serde(default = "default_bearer_token_type")]
    token_type: String,
    #[serde(default = "default_cloud_ai_scope")]
    scope: String,
    expires_in_seconds: u64,
    credits_remaining: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CloudAiTokenStatusResponse {
    status: CloudAiTokenStatus,
}

#[derive(Debug, Serialize)]
struct CloudAiDebugResponse {
    provider: &'static str,
    status: CloudAiDebugState,
    error: Option<String>,
    token: CloudAiTokenStatus,
}

#[derive(Debug, Clone, Deserialize)]
struct TranscriptIntelligenceRequest {
    transcript: Vec<TranscriptCue>,
    clip_duration: ClipDurationPreset,
    timeline_target: TimelineTarget,
    existing_clips: Option<Vec<ExistingClip>>,
    max_suggestions: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TranscriptCue {
    start_seconds: f64,
    end_seconds: f64,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ExistingClip {
    start_seconds: f64,
    end_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ClipDurationPreset {
    Short,
    Medium,
    Long,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TimelineTarget {
    UnclippedGapsOnly,
    EntireVideo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TranscriptIntelligenceResponse {
    provider: ProviderExecutionSource,
    model: String,
    clips: Vec<AiClipSuggestion>,
    usage: Option<CloudAiUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct AiClipSuggestion {
    title: String,
    start_seconds: f64,
    end_seconds: f64,
    reason: String,
    score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ProviderExecutionSource {
    Ollama,
    Tv7Managed,
    Tv7Cloud,
}

#[derive(Debug, Deserialize)]
struct AiClipSuggestionEnvelope {
    clips: Vec<AiClipSuggestion>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CloudAiDebugState {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CloudAiTokenState {
    Configured,
    Missing,
    Expired,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct CloudAiTokenStatus {
    state: CloudAiTokenState,
    scope: Option<String>,
    token_type: Option<String>,
    expires_in_seconds: Option<u64>,
    credits_remaining: Option<i64>,
}

#[derive(Debug, Clone)]
struct CloudAiToken {
    token: String,
    token_type: String,
    scope: String,
    expires_at: Instant,
    credits_remaining: Option<i64>,
}

#[derive(Clone, Default)]
struct CloudAiTokenStore {
    token: Arc<Mutex<Option<CloudAiToken>>>,
}

impl CloudAiTokenStore {
    fn set(&self, request: CloudAiTokenRequest) -> Result<CloudAiTokenStatus, ApiError> {
        if request.token.trim().is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "AI_AUTH_REQUIRED",
                "cloud AI delegation token is required",
            ));
        }
        if request.token_type != "bearer" {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "AI_AUTH_REQUIRED",
                "cloud AI delegation token_type must be bearer",
            ));
        }
        if request.scope != "engine:cloud_ai" {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "AI_AUTH_REQUIRED",
                "cloud AI delegation token scope must be engine:cloud_ai",
            ));
        }
        if request.expires_in_seconds == 0 {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "AI_AUTH_REQUIRED",
                "cloud AI delegation token must have a positive expiry",
            ));
        }

        let token = CloudAiToken {
            token: request.token,
            token_type: request.token_type,
            scope: request.scope,
            expires_at: Instant::now() + Duration::from_secs(request.expires_in_seconds),
            credits_remaining: request.credits_remaining,
        };
        let status = token.status();
        *self
            .token
            .lock()
            .expect("cloud token mutex should not be poisoned") = Some(token);
        Ok(status)
    }

    fn clear(&self) -> CloudAiTokenStatus {
        *self
            .token
            .lock()
            .expect("cloud token mutex should not be poisoned") = None;
        CloudAiTokenStatus::missing()
    }

    fn status(&self) -> CloudAiTokenStatus {
        let token = self
            .token
            .lock()
            .expect("cloud token mutex should not be poisoned")
            .clone();
        match token {
            Some(token) => token.status(),
            None => CloudAiTokenStatus::missing(),
        }
    }

    fn bearer_token(&self) -> Option<String> {
        let token = self
            .token
            .lock()
            .expect("cloud token mutex should not be poisoned")
            .clone()?;
        if token.is_expired() {
            None
        } else {
            Some(token.token)
        }
    }

    fn has_ready_token(&self) -> bool {
        self.bearer_token().is_some()
    }
}

impl CloudAiToken {
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    fn status(&self) -> CloudAiTokenStatus {
        let now = Instant::now();
        CloudAiTokenStatus {
            state: if now >= self.expires_at {
                CloudAiTokenState::Expired
            } else {
                CloudAiTokenState::Configured
            },
            scope: Some(self.scope.clone()),
            token_type: Some(self.token_type.clone()),
            expires_in_seconds: Some(
                self.expires_at
                    .checked_duration_since(now)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            credits_remaining: self.credits_remaining,
        }
    }
}

impl CloudAiTokenStatus {
    fn missing() -> Self {
        Self {
            state: CloudAiTokenState::Missing,
            scope: None,
            token_type: None,
            expires_in_seconds: None,
            credits_remaining: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CloudAiMessage {
    role: CloudAiMessageRole,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CloudAiMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CloudAiExecuteRequest {
    messages: Vec<CloudAiMessage>,
    model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CloudAiExecuteResponse {
    provider: String,
    model: String,
    content: String,
    usage: CloudAiUsage,
    raw_finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CloudAiUsage {
    credits_charged: i64,
    credits_remaining: i64,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    latency_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CloudAiErrorCode {
    AuthRequired,
    LicenseRequired,
    QuotaExceeded,
    ProviderUnavailable,
    NetworkUnavailable,
    ContextTooLarge,
}

impl CloudAiErrorCode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::AuthRequired => "AI_AUTH_REQUIRED",
            Self::LicenseRequired => "AI_LICENSE_REQUIRED",
            Self::QuotaExceeded => "AI_QUOTA_EXCEEDED",
            Self::ProviderUnavailable => "AI_PROVIDER_UNAVAILABLE",
            Self::NetworkUnavailable => "AI_NETWORK_UNAVAILABLE",
            Self::ContextTooLarge => "AI_CONTEXT_TOO_LARGE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CloudAiError {
    code: CloudAiErrorCode,
    message: String,
}

trait CloudAiExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        token: &'a str,
        request: &'a CloudAiExecuteRequest,
    ) -> CloudAiExecuteFuture<'a>;
}

#[derive(Clone)]
struct BackendCloudAiClient {
    base_url: Option<String>,
    client: reqwest::Client,
}

impl BackendCloudAiClient {
    fn new(base_url: Option<String>) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
}

impl CloudAiExecutor for BackendCloudAiClient {
    fn execute<'a>(
        &'a self,
        token: &'a str,
        request: &'a CloudAiExecuteRequest,
    ) -> CloudAiExecuteFuture<'a> {
        Box::pin(async move {
            let Some(base_url) = self.base_url.as_deref() else {
                return Err(CloudAiError {
                    code: CloudAiErrorCode::NetworkUnavailable,
                    message: "Backend base URL is not configured".to_string(),
                });
            };
            let url = format!("{base_url}/api/v1/cloud-ai/engine/execute");
            let response = self
                .client
                .post(url)
                .bearer_auth(token)
                .json(request)
                .send()
                .await
                .map_err(|error| CloudAiError {
                    code: CloudAiErrorCode::NetworkUnavailable,
                    message: error.to_string(),
                })?;
            let status = response.status();
            if status.is_success() {
                return response.json().await.map_err(|error| CloudAiError {
                    code: CloudAiErrorCode::ProviderUnavailable,
                    message: error.to_string(),
                });
            }

            Err(map_backend_cloud_ai_error(status, response).await)
        })
    }
}

async fn map_backend_cloud_ai_error(
    status: reqwest::StatusCode,
    response: reqwest::Response,
) -> CloudAiError {
    let backend_code = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| {
            body.get("code")
                .or_else(|| body.get("error"))
                .or_else(|| body.pointer("/detail/code"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    let code = backend_code
        .as_deref()
        .map(normalize_backend_ai_error_code)
        .unwrap_or_else(|| match status.as_u16() {
            401 => CloudAiErrorCode::AuthRequired,
            402 | 429 => CloudAiErrorCode::QuotaExceeded,
            403 => CloudAiErrorCode::LicenseRequired,
            413 | 422 => CloudAiErrorCode::ContextTooLarge,
            502..=504 => CloudAiErrorCode::ProviderUnavailable,
            _ => CloudAiErrorCode::ProviderUnavailable,
        });
    CloudAiError {
        code,
        message: backend_code
            .unwrap_or_else(|| format!("Backend cloud AI request failed with HTTP {status}")),
    }
}

fn normalize_backend_ai_error_code(code: &str) -> CloudAiErrorCode {
    match code {
        "AI_AUTH_REQUIRED" => CloudAiErrorCode::AuthRequired,
        "AI_LICENSE_REQUIRED" => CloudAiErrorCode::LicenseRequired,
        "AI_QUOTA_EXCEEDED" => CloudAiErrorCode::QuotaExceeded,
        "AI_NETWORK_UNAVAILABLE" => CloudAiErrorCode::NetworkUnavailable,
        "AI_CONTEXT_TOO_LARGE" => CloudAiErrorCode::ContextTooLarge,
        "AI_PROVIDER_UNAVAILABLE" => CloudAiErrorCode::ProviderUnavailable,
        _ => CloudAiErrorCode::ProviderUnavailable,
    }
}

fn validate_transcript_intelligence_request(
    request: &TranscriptIntelligenceRequest,
) -> Result<(), ApiError> {
    if request.transcript.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_TRANSCRIPT",
            "transcript must contain at least one cue",
        ));
    }
    for cue in &request.transcript {
        if cue.text.trim().is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_TRANSCRIPT",
                "transcript cue text must not be empty",
            ));
        }
        if cue.start_seconds < 0.0 || cue.end_seconds <= cue.start_seconds {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_TRANSCRIPT",
                "transcript cue times must be non-negative and increasing",
            ));
        }
    }
    if matches!(request.max_suggestions, Some(0)) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_AI_OPTIONS",
            "max_suggestions must be positive",
        ));
    }
    Ok(())
}

async fn execute_cloud_transcript_intelligence(
    state: &AppState,
    request: &TranscriptIntelligenceRequest,
) -> Result<TranscriptIntelligenceResponse, CloudAiError> {
    let token = state
        .cloud_ai_tokens
        .bearer_token()
        .ok_or_else(|| CloudAiError {
            code: CloudAiErrorCode::AuthRequired,
            message: "missing delegated cloud AI token".to_string(),
        })?;
    let cloud_request = CloudAiExecuteRequest {
        messages: build_transcript_intelligence_messages(request),
        model: None,
        temperature: Some(0.2),
        max_tokens: Some(1400),
    };
    let cloud_response = state
        .cloud_ai_executor
        .execute(&token, &cloud_request)
        .await?;
    let envelope: AiClipSuggestionEnvelope = serde_json::from_str(&cloud_response.content)
        .map_err(|error| CloudAiError {
            code: CloudAiErrorCode::ProviderUnavailable,
            message: format!("cloud AI returned invalid transcript intelligence JSON: {error}"),
        })?;
    validate_ai_clip_suggestions(&envelope.clips)?;
    Ok(TranscriptIntelligenceResponse {
        provider: ProviderExecutionSource::Tv7Cloud,
        model: cloud_response.model,
        clips: envelope.clips,
        usage: Some(cloud_response.usage),
    })
}

fn build_transcript_intelligence_messages(
    request: &TranscriptIntelligenceRequest,
) -> Vec<CloudAiMessage> {
    let max_suggestions = request.max_suggestions.unwrap_or(6).min(12);
    let cue_text = request
        .transcript
        .iter()
        .take(500)
        .map(|cue| {
            format!(
                "[{:.2}-{:.2}] {}",
                cue.start_seconds,
                cue.end_seconds,
                cue.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let existing_clips = request
        .existing_clips
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|clip| format!("[{:.2}-{:.2}]", clip.start_seconds, clip.end_seconds))
        .collect::<Vec<_>>()
        .join(", ");

    vec![
        CloudAiMessage {
            role: CloudAiMessageRole::System,
            content: "You are TV7 Engine. Return only valid JSON with a top-level clips array. Each clip must include title, start_seconds, end_seconds, reason, and score from 0 to 1. Do not include markdown.".to_string(),
        },
        CloudAiMessage {
            role: CloudAiMessageRole::User,
            content: format!(
                "Find up to {max_suggestions} highlight clips for social video creation.\nDuration preset: {:?}\nTimeline target: {:?}\nExisting clips: {}\nTranscript cues:\n{}",
                request.clip_duration,
                request.timeline_target,
                if existing_clips.is_empty() { "none".to_string() } else { existing_clips },
                cue_text
            ),
        },
    ]
}

fn validate_ai_clip_suggestions(clips: &[AiClipSuggestion]) -> Result<(), CloudAiError> {
    for clip in clips {
        if clip.title.trim().is_empty()
            || clip.reason.trim().is_empty()
            || clip.start_seconds < 0.0
            || clip.end_seconds <= clip.start_seconds
            || !(0.0..=1.0).contains(&clip.score)
        {
            return Err(CloudAiError {
                code: CloudAiErrorCode::ProviderUnavailable,
                message: "cloud AI returned invalid clip suggestion fields".to_string(),
            });
        }
    }
    Ok(())
}

fn default_bearer_token_type() -> String {
    "bearer".to_string()
}

fn default_cloud_ai_scope() -> String {
    "engine:cloud_ai".to_string()
}

#[derive(Debug, Clone, Serialize)]
struct EngineJob {
    id: String,
    kind: JobKind,
    status: JobStatus,
    progress: f32,
    logs: Vec<String>,
    result: Option<Value>,
    error: Option<String>,
    cancel_requested: bool,
    #[serde(skip_serializing)]
    updated_at: SystemTime,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum JobKind {
    UrlIngest,
    TranscriptDownload,
    TranscriptGenerate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Default)]
struct JobStore {
    next_id: Arc<AtomicU64>,
    jobs: Arc<Mutex<HashMap<String, EngineJob>>>,
}

const JOB_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_JOBS: usize = 1_000;

impl JobStore {
    fn create(&self, kind: JobKind) -> EngineJob {
        self.cleanup_expired();
        let id = format!("job_{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let job = EngineJob {
            id: id.clone(),
            kind,
            status: JobStatus::Queued,
            progress: 0.0,
            logs: vec!["queued".to_string()],
            result: None,
            error: None,
            cancel_requested: false,
            updated_at: SystemTime::now(),
        };
        self.jobs
            .lock()
            .expect("job store mutex should not be poisoned")
            .insert(id, job.clone());
        self.enforce_job_limit(MAX_JOBS);
        job
    }

    fn get(&self, id: &str) -> Option<EngineJob> {
        self.cleanup_expired();
        self.jobs
            .lock()
            .expect("job store mutex should not be poisoned")
            .get(id)
            .cloned()
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut EngineJob)) {
        if let Some(job) = self
            .jobs
            .lock()
            .expect("job store mutex should not be poisoned")
            .get_mut(id)
        {
            job.updated_at = SystemTime::now();
            update(job);
        }
    }

    fn cancel(&self, id: &str) -> Option<EngineJob> {
        let mut jobs = self
            .jobs
            .lock()
            .expect("job store mutex should not be poisoned");
        let job = jobs.get_mut(id)?;
        match job.status {
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled => {}
            JobStatus::Queued | JobStatus::Running => {
                job.cancel_requested = true;
                job.status = JobStatus::Cancelled;
                job.progress = 1.0;
                job.logs.push("cancel requested".to_string());
                job.error = Some("job cancelled".to_string());
            }
        }
        Some(job.clone())
    }

    fn is_cancel_requested(&self, id: &str) -> bool {
        self.get(id)
            .map(|job| job.cancel_requested || matches!(job.status, JobStatus::Cancelled))
            .unwrap_or(true)
    }

    fn cleanup_expired(&self) {
        let now = SystemTime::now();
        self.jobs
            .lock()
            .expect("job store mutex should not be poisoned")
            .retain(|_, job| {
                let terminal = matches!(
                    job.status,
                    JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
                );
                let expired = now
                    .duration_since(job.updated_at)
                    .map(|age| age > JOB_TTL)
                    .unwrap_or(false);
                !(terminal && expired)
            });
    }

    fn enforce_job_limit(&self, limit: usize) {
        let mut jobs = self
            .jobs
            .lock()
            .expect("job store mutex should not be poisoned");
        if jobs.len() <= limit {
            return;
        }

        if limit == 0 {
            jobs.clear();
            return;
        }

        let mut candidates: Vec<_> = jobs
            .iter()
            .map(|(id, job)| {
                let terminal = matches!(
                    job.status,
                    JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
                );
                let updated_at = job
                    .updated_at
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default();
                (id.clone(), terminal, updated_at)
            })
            .collect();

        candidates.sort_by_key(|(_, terminal, updated_at)| (!*terminal, *updated_at));

        let remove_count = jobs.len() - limit;
        for (id, _, _) in candidates.into_iter().take(remove_count) {
            jobs.remove(&id);
        }
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl From<CloudAiError> for ApiError {
    fn from(value: CloudAiError) -> Self {
        let status = match value.code {
            CloudAiErrorCode::AuthRequired => StatusCode::UNAUTHORIZED,
            CloudAiErrorCode::LicenseRequired => StatusCode::FORBIDDEN,
            CloudAiErrorCode::QuotaExceeded => StatusCode::PAYMENT_REQUIRED,
            CloudAiErrorCode::ProviderUnavailable => StatusCode::BAD_GATEWAY,
            CloudAiErrorCode::NetworkUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            CloudAiErrorCode::ContextTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        };
        Self::new(status, value.code.as_str(), value.message)
    }
}

impl From<EngineError> for ApiError {
    fn from(value: EngineError) -> Self {
        match value {
            EngineError::InvalidPath(message) => {
                Self::new(StatusCode::BAD_REQUEST, "INVALID_PATH", message)
            }
            EngineError::NotFound(path) => Self::new(
                StatusCode::NOT_FOUND,
                "FILE_NOT_FOUND",
                format!("file does not exist: {}", path.display()),
            ),
            EngineError::NotFile(path) => Self::new(
                StatusCode::BAD_REQUEST,
                "NOT_A_FILE",
                format!("path is not a file: {}", path.display()),
            ),
            EngineError::OutputParentMissing(path) => Self::new(
                StatusCode::BAD_REQUEST,
                "OUTPUT_PARENT_MISSING",
                format!("output parent directory does not exist: {}", path.display()),
            ),
            EngineError::SourceOutputSame(path) => Self::new(
                StatusCode::BAD_REQUEST,
                "SOURCE_OUTPUT_SAME",
                format!("source and output paths must differ: {}", path.display()),
            ),
            EngineError::InvalidTimeRange(message) => {
                Self::new(StatusCode::BAD_REQUEST, "INVALID_TIME_RANGE", message)
            }
            EngineError::InvalidUrl(message) => {
                Self::new(StatusCode::BAD_REQUEST, "INVALID_URL", message)
            }
            EngineError::CommandFailed { command, message } => Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "COMMAND_FAILED",
                format!("{command} failed: {message}"),
            ),
            EngineError::OutputInvalid(message) => Self::new(
                StatusCode::BAD_GATEWAY,
                "OUTPUT_INVALID",
                format!("failed to parse command output: {message}"),
            ),
            EngineError::Io(message) => {
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "IO_ERROR", message)
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/diagnostics", get(diagnostics))
        .route("/v1/setup/manifest", get(setup_manifest))
        .route("/v1/setup/status", get(setup_status))
        .route(
            "/v1/ai/cloud-token",
            post(set_cloud_ai_token).delete(clear_cloud_ai_token),
        )
        .route("/v1/ai/cloud-debug", post(debug_cloud_ai))
        .route(
            "/v1/ai/transcript-intelligence",
            post(transcript_intelligence),
        )
        .route("/v1/media/probe", post(probe_media))
        .route("/v1/media/thumbnail", post(generate_thumbnail))
        .route("/v1/media/render-clip", post(render_clip))
        .route("/v1/ingest/url", post(start_url_ingest))
        .route("/v1/transcripts/download", post(start_transcript_download))
        .route("/v1/transcripts/generate", post(start_transcript_generate))
        .route("/v1/jobs/:id", get(get_job))
        .route("/v1/jobs/:id/logs", get(get_job_logs))
        .route("/v1/jobs/:id/cancel", post(cancel_job))
        .with_state(state)
}

async fn setup_manifest(State(state): State<AppState>) -> Json<SetupManifest> {
    debug!("setup manifest endpoint called");
    Json(state.setup_provider.setup_manifest(&state.setup_context))
}

async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    debug!("setup status endpoint called");
    Ok(Json(resolve_setup_status(&state).await?))
}

async fn resolve_setup_status(state: &AppState) -> Result<SetupStatus, ApiError> {
    let setup_provider = state.setup_provider.clone();
    let setup_context = state.effective_setup_context();
    tokio::task::spawn_blocking(move || setup_provider.setup_status(&setup_context))
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SETUP_STATUS_FAILED",
                format!("setup status task failed: {error}"),
            )
        })
}

async fn set_cloud_ai_token(
    State(state): State<AppState>,
    Json(payload): Json<CloudAiTokenRequest>,
) -> Result<Json<CloudAiTokenStatusResponse>, ApiError> {
    let status = state.cloud_ai_tokens.set(payload)?;
    Ok(Json(CloudAiTokenStatusResponse { status }))
}

async fn clear_cloud_ai_token(State(state): State<AppState>) -> Json<CloudAiTokenStatusResponse> {
    let status = state.cloud_ai_tokens.clear();
    Json(CloudAiTokenStatusResponse { status })
}

async fn debug_cloud_ai(
    State(state): State<AppState>,
) -> Result<Json<CloudAiDebugResponse>, ApiError> {
    let token_status = state.cloud_ai_tokens.status();
    let Some(token) = state.cloud_ai_tokens.bearer_token() else {
        return Ok(Json(CloudAiDebugResponse {
            provider: "tv7_cloud",
            status: CloudAiDebugState::NotReady,
            error: Some("AI_AUTH_REQUIRED".to_string()),
            token: token_status,
        }));
    };
    let request = CloudAiExecuteRequest {
        messages: vec![
            CloudAiMessage {
                role: CloudAiMessageRole::System,
                content: "Return exactly: ok".to_string(),
            },
            CloudAiMessage {
                role: CloudAiMessageRole::User,
                content: "Health check".to_string(),
            },
        ],
        model: None,
        temperature: Some(0.0),
        max_tokens: Some(8),
    };
    match state.cloud_ai_executor.execute(&token, &request).await {
        Ok(_) => Ok(Json(CloudAiDebugResponse {
            provider: "tv7_cloud",
            status: CloudAiDebugState::Ready,
            error: None,
            token: token_status,
        })),
        Err(error) => Ok(Json(CloudAiDebugResponse {
            provider: "tv7_cloud",
            status: CloudAiDebugState::NotReady,
            error: Some(error.code.as_str().to_string()),
            token: token_status,
        })),
    }
}

async fn transcript_intelligence(
    State(state): State<AppState>,
    Json(payload): Json<TranscriptIntelligenceRequest>,
) -> Result<Json<TranscriptIntelligenceResponse>, ApiError> {
    validate_transcript_intelligence_request(&payload)?;
    let setup_status = resolve_setup_status(&state).await?;
    let provider = setup_status
        .capabilities
        .iter()
        .find(|capability| capability.name == "ai")
        .filter(|capability| capability.status == SetupCapabilityState::Ready)
        .and_then(|capability| capability.provider.clone())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "AI_AUTH_REQUIRED",
                "AI is not ready. Configure local AI or pass a delegated TV7 cloud AI token.",
            )
        })?;

    match provider {
        ProviderSource::Tv7Cloud => execute_cloud_transcript_intelligence(&state, &payload)
            .await
            .map(Json)
            .map_err(ApiError::from),
        ProviderSource::Ollama | ProviderSource::Tv7Managed => Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "AI_PROVIDER_UNAVAILABLE",
            "local AI execution is not implemented for transcript intelligence yet",
        )),
        ProviderSource::UserPath => Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI_PROVIDER_UNAVAILABLE",
            "user-path AI provider is not executable for transcript intelligence",
        )),
        ProviderSource::Bundled => Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI_PROVIDER_UNAVAILABLE",
            "bundled AI provider is not executable for transcript intelligence",
        )),
        ProviderSource::Unconfigured => Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI_PROVIDER_UNAVAILABLE",
            "AI provider is unconfigured for transcript intelligence",
        )),
    }
}

async fn health() -> Json<HealthResponse> {
    debug!("health endpoint called");
    Json(HealthResponse { status: "ok" })
}

async fn diagnostics(State(state): State<AppState>) -> Json<EngineDiagnostics> {
    debug!("diagnostics endpoint called");
    let diagnostics = state
        .diagnostics_provider
        .diagnostics(&state.diagnostics_context);

    if diagnostics.status == DiagnosticStatus::Degraded {
        warn!(
            host = %diagnostics.runtime.host,
            port = diagnostics.runtime.port,
            work_dir = %diagnostics.work_dir.path,
            "engine diagnostics degraded"
        );

        for capability in diagnostics
            .capabilities
            .iter()
            .filter(|capability| capability.status == DiagnosticStatus::Degraded)
        {
            warn!(
                capability = %capability.name,
                required_tools = ?capability.required_tools,
                "diagnostics capability degraded"
            );
        }

        for tool in diagnostics
            .tools
            .iter()
            .filter(|tool| tool.available != Some(true))
        {
            warn!(
                tool = %tool.name,
                required_for = ?tool.required_for,
                error = %tool.error.as_deref().unwrap_or("unknown"),
                "diagnostics tool unavailable"
            );
        }
    }

    Json(diagnostics)
}

async fn probe_media(
    State(state): State<AppState>,
    Json(payload): Json<ProbeRequest>,
) -> Result<Json<ProbeResponse>, ApiError> {
    let metadata = state
        .media_probe
        .probe_path(Path::new(&payload.path))
        .map_err(ApiError::from)?;

    Ok(Json(ProbeResponse { metadata }))
}

async fn generate_thumbnail(
    State(state): State<AppState>,
    Json(payload): Json<ThumbnailRequest>,
) -> Result<Json<ThumbnailResponse>, ApiError> {
    let result = state
        .thumbnail_generator
        .generate_thumbnail(&payload)
        .map_err(ApiError::from)?;

    Ok(Json(ThumbnailResponse { result }))
}

async fn render_clip(
    State(state): State<AppState>,
    Json(payload): Json<RenderClipRequest>,
) -> Result<Json<RenderClipResponse>, ApiError> {
    let result = state
        .clip_renderer
        .render_clip(&payload)
        .map_err(ApiError::from)?;

    Ok(Json(RenderClipResponse { result }))
}

async fn start_url_ingest(
    State(state): State<AppState>,
    Json(payload): Json<UrlIngestRequest>,
) -> Result<(StatusCode, Json<JobCreateResponse>), ApiError> {
    let job = state.jobs.create(JobKind::UrlIngest);
    let job_id = job.id.clone();
    let jobs = state.jobs.clone();
    let ingestor = state.url_ingestor.clone();
    let task_jobs = jobs.clone();
    spawn_blocking_job(jobs, job_id.clone(), move || {
        if task_jobs.is_cancel_requested(&job_id) {
            return;
        }
        task_jobs.update(&job_id, |job| {
            job.status = JobStatus::Running;
            job.progress = 0.1;
            job.logs.push("started url ingest".to_string());
        });
        match ingestor.ingest_url(&payload) {
            Ok(result) => complete_job(&task_jobs, &job_id, result),
            Err(error) => fail_job(&task_jobs, &job_id, error),
        }
    });

    Ok((StatusCode::ACCEPTED, Json(JobCreateResponse { job })))
}

async fn start_transcript_download(
    State(state): State<AppState>,
    Json(payload): Json<TranscriptDownloadRequest>,
) -> Result<(StatusCode, Json<JobCreateResponse>), ApiError> {
    let job = state.jobs.create(JobKind::TranscriptDownload);
    let job_id = job.id.clone();
    let jobs = state.jobs.clone();
    let transcript_provider = state.transcript_provider.clone();
    let task_jobs = jobs.clone();
    spawn_blocking_job(jobs, job_id.clone(), move || {
        if task_jobs.is_cancel_requested(&job_id) {
            return;
        }
        task_jobs.update(&job_id, |job| {
            job.status = JobStatus::Running;
            job.progress = 0.1;
            job.logs.push("started transcript download".to_string());
        });
        match transcript_provider.download_transcript(&payload) {
            Ok(result) => complete_job(&task_jobs, &job_id, result),
            Err(error) => fail_job(&task_jobs, &job_id, error),
        }
    });

    Ok((StatusCode::ACCEPTED, Json(JobCreateResponse { job })))
}

async fn start_transcript_generate(
    State(state): State<AppState>,
    Json(payload): Json<TranscriptGenerateRequest>,
) -> Result<Response, ApiError> {
    let setup_status = resolve_setup_status(&state).await?;
    let transcription = setup_status
        .capabilities
        .iter()
        .find(|capability| capability.name == "transcription");
    if !matches!(
        transcription.map(|capability| &capability.status),
        Some(SetupCapabilityState::Ready)
    ) {
        let missing = transcription
            .map(|capability| capability.missing.clone())
            .filter(|missing| !missing.is_empty())
            .unwrap_or_else(|| {
                vec![
                    "transcription_runtime".to_string(),
                    "transcription_model".to_string(),
                ]
            });
        return Ok((
            StatusCode::CONFLICT,
            Json(SetupRequiredBody {
                error: "TRANSCRIPTION_SETUP_REQUIRED",
                message: "Local transcription requires a configured runtime and model.".to_string(),
                missing,
                setup_capability: "transcription",
            }),
        )
            .into_response());
    }

    let job = state.jobs.create(JobKind::TranscriptGenerate);
    let job_id = job.id.clone();
    let jobs = state.jobs.clone();
    let transcript_provider = state.transcript_provider.clone();
    let task_jobs = jobs.clone();
    spawn_blocking_job(jobs, job_id.clone(), move || {
        if task_jobs.is_cancel_requested(&job_id) {
            return;
        }
        task_jobs.update(&job_id, |job| {
            job.status = JobStatus::Running;
            job.progress = 0.1;
            job.logs.push("started transcript generation".to_string());
        });
        match transcript_provider.generate_transcript(&payload) {
            Ok(result) => complete_job(&task_jobs, &job_id, result),
            Err(error) => fail_job(&task_jobs, &job_id, error),
        }
    });

    Ok((StatusCode::ACCEPTED, Json(JobCreateResponse { job })).into_response())
}

async fn get_job(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<EngineJob>, ApiError> {
    state
        .jobs
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "JOB_NOT_FOUND", "job not found"))
}

async fn get_job_logs(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<JobLogsResponse>, ApiError> {
    let job = state
        .jobs
        .get(&id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "JOB_NOT_FOUND", "job not found"))?;

    Ok(Json(JobLogsResponse {
        job_id: id,
        logs: job.logs,
    }))
}

async fn cancel_job(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<EngineJob>, ApiError> {
    state
        .jobs
        .cancel(&id)
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "JOB_NOT_FOUND", "job not found"))
}

fn spawn_blocking_job(job_store: JobStore, job_id: String, run: impl FnOnce() + Send + 'static) {
    tokio::task::spawn_blocking(move || {
        if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(run)) {
            fail_job_panic(&job_store, &job_id, panic_payload_message(payload));
        }
    });
}

fn complete_job<T>(jobs: &JobStore, job_id: &str, result: T)
where
    T: Serialize,
{
    let value = match serde_json::to_value(result) {
        Ok(value) => value,
        Err(error) => {
            jobs.update(job_id, |job| {
                job.status = JobStatus::Failed;
                job.progress = 1.0;
                job.logs.push("failed to serialize job result".to_string());
                job.error = Some(error.to_string());
            });
            return;
        }
    };
    jobs.update(job_id, |job| {
        if job.cancel_requested {
            job.status = JobStatus::Cancelled;
            job.progress = 1.0;
            job.logs
                .push("completed after cancellation request".to_string());
            return;
        }
        job.status = JobStatus::Succeeded;
        job.progress = 1.0;
        job.logs.push("completed".to_string());
        job.result = Some(value);
    });
}

fn fail_job_panic(jobs: &JobStore, job_id: &str, message: String) {
    jobs.update(job_id, |job| {
        if job.cancel_requested {
            job.status = JobStatus::Cancelled;
            job.progress = 1.0;
            job.logs
                .push("panicked after cancellation request".to_string());
            return;
        }
        job.status = JobStatus::Failed;
        job.progress = 1.0;
        job.logs.push("failed after task panic".to_string());
        job.error = Some(format!("job task panicked: {message}"));
    });
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn fail_job(jobs: &JobStore, job_id: &str, error: EngineError) {
    jobs.update(job_id, |job| {
        if job.cancel_requested {
            job.status = JobStatus::Cancelled;
            job.progress = 1.0;
            job.logs
                .push("failed after cancellation request".to_string());
            return;
        }
        job.status = JobStatus::Failed;
        job.logs.push("failed".to_string());
        job.error = Some(error.to_string());
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env()?;
    initialize_logging(&config.log_level);

    let media_engine = Arc::new(FfmpegMediaEngine::with_all_bins_and_transcription(
        config.ffmpeg_bin.clone(),
        config.ffprobe_bin.clone(),
        config.yt_dlp_bin.clone(),
        config.whisper_bin.clone(),
        config.whisper_model.clone(),
    ));
    let state = AppState {
        media_probe: media_engine.clone(),
        thumbnail_generator: media_engine.clone(),
        clip_renderer: media_engine.clone(),
        url_ingestor: media_engine.clone(),
        transcript_provider: media_engine.clone(),
        diagnostics_provider: media_engine.clone(),
        setup_provider: media_engine,
        jobs: JobStore::default(),
        diagnostics_context: DiagnosticsContext::new(
            env!("CARGO_PKG_VERSION"),
            config.host.to_string(),
            config.port,
            config.work_dir.clone(),
        ),
        setup_context: SetupContext::new(
            platform_id(),
            config.asset_dir.clone(),
            config.whisper_bin.clone(),
            config.whisper_model.clone(),
            config.ollama_bin.clone(),
            config.ollama_model.clone(),
            config.cloud_ai_available,
        ),
        cloud_ai_tokens: CloudAiTokenStore::default(),
        cloud_ai_executor: Arc::new(BackendCloudAiClient::new(config.backend_base_url.clone())),
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.socket_addr())
        .await
        .with_context(|| format!("failed to bind on {}", config.socket_addr()))?;

    info!(
        address = %config.socket_addr(),
        work_dir = %config.work_dir.display(),
        ffmpeg_bin = %config.ffmpeg_bin,
        ffprobe_bin = %config.ffprobe_bin,
        yt_dlp_bin = %config.yt_dlp_bin,
        asset_dir = %config.asset_dir.display(),
        "tv7 engine service listening"
    );
    axum::serve(listener, app)
        .await
        .context("service stopped")?;
    Ok(())
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "1" | "true" | "TRUE" | "True" => Ok(true),
        "0" | "false" | "FALSE" | "False" => Ok(false),
        _ => anyhow::bail!("expected true or false, got {value}"),
    }
}

fn platform_id() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn initialize_logging(default_log_level: &str) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_log_level));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use engine_core::{
        AiProviderCandidate, CapabilityDiagnostic, DiagnosticStatus, ProviderCandidateStatus,
        ProviderSource, SetupCapabilityState, SetupCapabilityStatus, ToolDiagnostic,
        TranscriptDownloadResult, UrlIngestResult, WorkDirDiagnostic,
    };
    use serde::ser::{Error as SerError, SerializeStruct};
    use std::collections::BTreeSet;
    use std::sync::Mutex as StdMutex;
    use tower::ServiceExt;

    #[derive(Debug)]
    struct MockMediaProbe {
        probe_response: Result<MediaMetadata, EngineError>,
        thumbnail_response: Result<ThumbnailResult, EngineError>,
        render_response: Result<RenderClipResult, EngineError>,
    }

    impl MediaProbe for MockMediaProbe {
        fn probe_path(&self, _path: &Path) -> Result<MediaMetadata, EngineError> {
            self.probe_response.clone()
        }
    }

    impl ThumbnailGenerator for MockMediaProbe {
        fn generate_thumbnail(
            &self,
            _request: &ThumbnailRequest,
        ) -> Result<ThumbnailResult, EngineError> {
            self.thumbnail_response.clone()
        }
    }

    impl ClipRenderer for MockMediaProbe {
        fn render_clip(
            &self,
            _request: &RenderClipRequest,
        ) -> Result<RenderClipResult, EngineError> {
            self.render_response.clone()
        }
    }

    impl DiagnosticsProvider for MockMediaProbe {
        fn diagnostics(&self, context: &DiagnosticsContext) -> EngineDiagnostics {
            EngineDiagnostics {
                status: DiagnosticStatus::Ready,
                runtime: engine_core::RuntimeDiagnostic {
                    engine_version: context.engine_version.clone(),
                    host: context.host.clone(),
                    port: context.port,
                },
                capabilities: vec![CapabilityDiagnostic {
                    name: "probe".to_string(),
                    status: DiagnosticStatus::Ready,
                    required_tools: vec!["ffprobe".to_string()],
                }],
                tools: vec![ToolDiagnostic {
                    name: "ffprobe".to_string(),
                    available: Some(true),
                    version: Some("ffprobe version test".to_string()),
                    required_for: vec!["probe".to_string()],
                    error: None,
                }],
                work_dir: WorkDirDiagnostic {
                    path: context.work_dir.to_string_lossy().to_string(),
                    ready: true,
                    error: None,
                },
            }
        }
    }

    impl UrlIngestor for MockMediaProbe {
        fn ingest_url(&self, request: &UrlIngestRequest) -> Result<UrlIngestResult, EngineError> {
            Ok(UrlIngestResult {
                url: request.url.clone(),
                media_path: Some("/tmp/video.mp4".to_string()),
                metadata: serde_json::json!({"title": "Test Video"}),
            })
        }
    }

    impl RuntimeSetupProvider for MockMediaProbe {
        fn setup_manifest(&self, context: &SetupContext) -> SetupManifest {
            SetupManifest {
                schema_version: 1,
                platform: context.platform.clone(),
                setup_modes: vec!["automatic".to_string(), "disabled".to_string()],
                ai_provider_order: Vec::new(),
                assets: Vec::new(),
            }
        }

        fn setup_status(&self, _context: &SetupContext) -> SetupStatus {
            let transcription_ready =
                _context.whisper_bin.is_some() && _context.whisper_model.is_some();
            SetupStatus {
                schema_version: 1,
                setup_complete: false,
                active_ai_provider: if _context.cloud_ai_available {
                    Some(ProviderSource::Tv7Cloud)
                } else {
                    None
                },
                assets: Vec::new(),
                ai_providers: vec![AiProviderCandidate {
                    source: ProviderSource::Tv7Cloud,
                    status: if _context.cloud_ai_available {
                        ProviderCandidateStatus::Ready
                    } else {
                        ProviderCandidateStatus::Unavailable
                    },
                    required_assets: Vec::new(),
                    requires_internet: true,
                    requires_user_confirmation: false,
                    cost_policy: "tv7_managed_usage".to_string(),
                    action: None,
                }],
                capabilities: vec![
                    SetupCapabilityStatus {
                        name: "transcription".to_string(),
                        status: if transcription_ready {
                            SetupCapabilityState::Ready
                        } else {
                            SetupCapabilityState::NotReady
                        },
                        provider: if transcription_ready {
                            Some(ProviderSource::UserPath)
                        } else {
                            None
                        },
                        missing: if transcription_ready {
                            Vec::new()
                        } else {
                            vec![
                                "transcription_runtime".to_string(),
                                "transcription_model".to_string(),
                            ]
                        },
                        installable: true,
                        user_action_required: !transcription_ready,
                    },
                    SetupCapabilityStatus {
                        name: "ai".to_string(),
                        status: if _context.cloud_ai_available {
                            SetupCapabilityState::Ready
                        } else {
                            SetupCapabilityState::NotReady
                        },
                        provider: if _context.cloud_ai_available {
                            Some(ProviderSource::Tv7Cloud)
                        } else {
                            None
                        },
                        missing: if _context.cloud_ai_available {
                            Vec::new()
                        } else {
                            vec!["cloud_ai_delegation_token".to_string()]
                        },
                        installable: true,
                        user_action_required: !_context.cloud_ai_available,
                    },
                ],
            }
        }
    }

    #[derive(Debug)]
    struct PanicUrlIngestor;

    impl UrlIngestor for PanicUrlIngestor {
        fn ingest_url(&self, _request: &UrlIngestRequest) -> Result<UrlIngestResult, EngineError> {
            panic!("ingest panic")
        }
    }

    #[derive(Debug)]
    struct SlowTranscriptProvider;

    #[derive(Debug, Default)]
    struct RecordingCloudAiExecutor {
        calls: StdMutex<Vec<(String, CloudAiExecuteRequest)>>,
        response: StdMutex<Option<Result<CloudAiExecuteResponse, CloudAiError>>>,
    }

    impl RecordingCloudAiExecutor {
        fn with_response(response: Result<CloudAiExecuteResponse, CloudAiError>) -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
                response: StdMutex::new(Some(response)),
            }
        }

        fn calls(&self) -> Vec<(String, CloudAiExecuteRequest)> {
            self.calls
                .lock()
                .expect("calls mutex should not be poisoned")
                .clone()
        }
    }

    impl CloudAiExecutor for RecordingCloudAiExecutor {
        fn execute<'a>(
            &'a self,
            token: &'a str,
            request: &'a CloudAiExecuteRequest,
        ) -> CloudAiExecuteFuture<'a> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("calls mutex should not be poisoned")
                    .push((token.to_string(), request.clone()));
                self.response
                    .lock()
                    .expect("response mutex should not be poisoned")
                    .clone()
                    .unwrap_or_else(|| {
                        Ok(CloudAiExecuteResponse {
                            provider: "deepseek".to_string(),
                            model: "deepseek-chat".to_string(),
                            content: r#"{"clips":[{"title":"Strong opening","start_seconds":1.0,"end_seconds":31.0,"reason":"Clear hook and useful standalone segment.","score":0.91}]}"#.to_string(),
                            usage: CloudAiUsage {
                                credits_charged: 1,
                                credits_remaining: 9,
                                prompt_tokens: Some(1),
                                completion_tokens: Some(1),
                                total_tokens: Some(2),
                                latency_ms: Some(10),
                            },
                            raw_finish_reason: Some("stop".to_string()),
                        })
                    })
            })
        }
    }

    impl TranscriptProvider for SlowTranscriptProvider {
        fn download_transcript(
            &self,
            request: &TranscriptDownloadRequest,
        ) -> Result<TranscriptDownloadResult, EngineError> {
            std::thread::sleep(Duration::from_millis(100));
            Ok(TranscriptDownloadResult {
                transcript_path: Some(request.output_path.clone()),
                language: request.language.clone().unwrap_or_else(|| "en".to_string()),
            })
        }

        fn generate_transcript(
            &self,
            request: &TranscriptGenerateRequest,
        ) -> Result<engine_core::TranscriptGenerateResult, EngineError> {
            std::thread::sleep(Duration::from_millis(100));
            Ok(engine_core::TranscriptGenerateResult {
                transcript_path: request.output_path.clone(),
                format: "vtt".to_string(),
                language: request.language.clone().unwrap_or_else(|| "en".to_string()),
            })
        }
    }

    impl TranscriptProvider for MockMediaProbe {
        fn download_transcript(
            &self,
            request: &TranscriptDownloadRequest,
        ) -> Result<TranscriptDownloadResult, EngineError> {
            Ok(TranscriptDownloadResult {
                transcript_path: Some(request.output_path.clone()),
                language: request.language.clone().unwrap_or_else(|| "en".to_string()),
            })
        }

        fn generate_transcript(
            &self,
            request: &TranscriptGenerateRequest,
        ) -> Result<engine_core::TranscriptGenerateResult, EngineError> {
            Ok(engine_core::TranscriptGenerateResult {
                transcript_path: request.output_path.clone(),
                format: "vtt".to_string(),
                language: request.language.clone().unwrap_or_else(|| "en".to_string()),
            })
        }
    }

    fn router_with_probe(response: Result<MediaMetadata, EngineError>) -> Router {
        router_with_probe_and_setup_context(response, test_setup_context())
    }

    fn router_with_probe_and_setup_context(
        response: Result<MediaMetadata, EngineError>,
        setup_context: SetupContext,
    ) -> Router {
        let mock = Arc::new(MockMediaProbe {
            probe_response: response,
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: mock.clone(),
            transcript_provider: mock.clone(),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context,
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: Arc::new(RecordingCloudAiExecutor::default()),
        })
    }

    fn engine_openapi() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../docs/core/contracts/openapi/tv7-engine/openapi.json"
        ))
        .expect("engine openapi contract should be valid json")
    }

    fn success_statuses_for(
        openapi: &serde_json::Value,
        path: &str,
        method: &str,
    ) -> BTreeSet<u16> {
        openapi["paths"][path][method]["responses"]
            .as_object()
            .expect("operation responses should be an object")
            .keys()
            .filter_map(|status| status.parse::<u16>().ok())
            .filter(|status| (200..300).contains(status))
            .collect()
    }

    async fn request_json(
        app: Router,
        method: &str,
        uri: &str,
        body: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().uri(uri).method(method);
        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        let request_body = body
            .map(|body| Body::from(body.to_string()))
            .unwrap_or_else(Body::empty);
        let response = app
            .oneshot(builder.body(request_body).expect("request should be valid"))
            .await
            .expect("request should complete");
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let json = serde_json::from_slice(&body).expect("response body should be json");
        (status, json)
    }

    fn test_setup_context() -> SetupContext {
        SetupContext::new(
            "test-platform",
            std::env::temp_dir().join("tv7-engine-test-assets"),
            None,
            None,
            "ollama",
            None,
            true,
        )
    }

    fn ready_transcription_setup_context() -> SetupContext {
        SetupContext::new(
            "test-platform",
            std::env::temp_dir().join("tv7-engine-test-assets"),
            Some("whisper-cli".to_string()),
            Some(std::env::temp_dir().join("model.bin")),
            "ollama",
            None,
            true,
        )
    }

    fn test_config_values(work_dir: PathBuf) -> AppConfigValues {
        AppConfigValues {
            host: Some("127.0.0.1".to_string()),
            port: Some("7070".to_string()),
            log_level: None,
            work_dir,
            asset_dir: None,
            ffmpeg_bin: None,
            ffprobe_bin: None,
            yt_dlp_bin: None,
            whisper_bin: None,
            whisper_model: None,
            ollama_bin: None,
            ollama_model: None,
            cloud_ai_available: None,
            backend_base_url: None,
        }
    }

    #[test]
    fn openapi_contract_declares_every_runtime_route() {
        let openapi = engine_openapi();
        let paths = openapi["paths"]
            .as_object()
            .expect("paths should be an object");
        let actual_routes = BTreeSet::from([
            "/health",
            "/v1/diagnostics",
            "/v1/ai/cloud-debug",
            "/v1/ai/cloud-token",
            "/v1/ai/transcript-intelligence",
            "/v1/setup/manifest",
            "/v1/setup/status",
            "/v1/media/probe",
            "/v1/media/thumbnail",
            "/v1/media/render-clip",
            "/v1/ingest/url",
            "/v1/transcripts/download",
            "/v1/transcripts/generate",
            "/v1/jobs/{id}",
            "/v1/jobs/{id}/logs",
            "/v1/jobs/{id}/cancel",
        ]);
        let contract_routes = paths.keys().map(String::as_str).collect::<BTreeSet<_>>();

        assert_eq!(contract_routes, actual_routes);
    }

    #[tokio::test]
    async fn runtime_success_statuses_match_openapi_contract() {
        let openapi = engine_openapi();
        let app = router_with_probe(Ok(MediaMetadata {
            path: "/tmp/file.mp4".to_string(),
            duration_seconds: Some(10.0),
            width: Some(1920),
            height: Some(1080),
            format: "mp4".to_string(),
            size_bytes: 1024,
        }));

        let cases = [
            ("get", "/health", "GET", "/health", None),
            ("get", "/v1/diagnostics", "GET", "/v1/diagnostics", None),
            (
                "post",
                "/v1/ai/cloud-token",
                "POST",
                "/v1/ai/cloud-token",
                Some(
                    r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300,"credits_remaining":10}"#,
                ),
            ),
            (
                "post",
                "/v1/ai/cloud-debug",
                "POST",
                "/v1/ai/cloud-debug",
                None,
            ),
            (
                "post",
                "/v1/ai/transcript-intelligence",
                "POST",
                "/v1/ai/transcript-intelligence",
                Some(
                    r#"{"transcript":[{"start_seconds":1.0,"end_seconds":5.0,"text":"This is a useful hook."}],"clip_duration":"medium","timeline_target":"entire_video","max_suggestions":3}"#,
                ),
            ),
            (
                "get",
                "/v1/setup/manifest",
                "GET",
                "/v1/setup/manifest",
                None,
            ),
            ("get", "/v1/setup/status", "GET", "/v1/setup/status", None),
            (
                "post",
                "/v1/media/probe",
                "POST",
                "/v1/media/probe",
                Some(r#"{"path":"/tmp/file.mp4"}"#),
            ),
            (
                "post",
                "/v1/media/thumbnail",
                "POST",
                "/v1/media/thumbnail",
                Some(
                    r#"{"source_path":"/tmp/video.mp4","output_path":"/tmp/thumb.jpg","timestamp_seconds":1.0}"#,
                ),
            ),
            (
                "post",
                "/v1/media/render-clip",
                "POST",
                "/v1/media/render-clip",
                Some(
                    r#"{"source_path":"/tmp/video.mp4","output_path":"/tmp/clip.mp4","start_seconds":1.0,"end_seconds":11.0}"#,
                ),
            ),
            (
                "post",
                "/v1/ingest/url",
                "POST",
                "/v1/ingest/url",
                Some(r#"{"url":"https://example.com/video","output_dir":"/tmp"}"#),
            ),
            (
                "post",
                "/v1/transcripts/download",
                "POST",
                "/v1/transcripts/download",
                Some(
                    r#"{"url":"https://example.com/video","output_path":"/tmp/video.vtt","language":"en"}"#,
                ),
            ),
            (
                "post",
                "/v1/transcripts/generate",
                "POST",
                "/v1/transcripts/generate",
                Some(
                    r#"{"media_path":"/tmp/video.mp4","output_path":"/tmp/video.vtt","language":"en"}"#,
                ),
            ),
        ];

        for (contract_method, contract_path, request_method, request_uri, request_body) in cases {
            let request_app = if contract_path == "/v1/transcripts/generate" {
                router_with_probe_and_setup_context(
                    Ok(MediaMetadata {
                        path: "/tmp/file.mp4".to_string(),
                        duration_seconds: Some(10.0),
                        width: Some(1920),
                        height: Some(1080),
                        format: "mp4".to_string(),
                        size_bytes: 1024,
                    }),
                    ready_transcription_setup_context(),
                )
            } else {
                app.clone()
            };
            let (status, body) =
                request_json(request_app, request_method, request_uri, request_body).await;
            assert!(
                success_statuses_for(&openapi, contract_path, contract_method)
                    .contains(&status.as_u16()),
                "{request_method} {request_uri} returned {status}, body {body}"
            );
        }

        for (contract_method, contract_path, request_method, request_uri) in [
            (
                "delete",
                "/v1/ai/cloud-token",
                "DELETE",
                "/v1/ai/cloud-token",
            ),
            ("get", "/v1/jobs/{id}", "GET", "/v1/jobs/job_1"),
            ("get", "/v1/jobs/{id}/logs", "GET", "/v1/jobs/job_1/logs"),
            (
                "post",
                "/v1/jobs/{id}/cancel",
                "POST",
                "/v1/jobs/job_1/cancel",
            ),
        ] {
            let (status, body) = request_json(app.clone(), request_method, request_uri, None).await;
            assert!(
                success_statuses_for(&openapi, contract_path, contract_method)
                    .contains(&status.as_u16()),
                "{request_method} {request_uri} returned {status}, body {body}"
            );
        }
    }

    #[tokio::test]
    async fn runtime_success_payloads_include_openapi_required_keys() {
        let app = router_with_probe(Ok(MediaMetadata {
            path: "/tmp/file.mp4".to_string(),
            duration_seconds: Some(10.0),
            width: Some(1920),
            height: Some(1080),
            format: "mp4".to_string(),
            size_bytes: 1024,
        }));

        let (_, health) = request_json(app.clone(), "GET", "/health", None).await;
        assert!(health.get("status").is_some());

        let (_, diagnostics) = request_json(app.clone(), "GET", "/v1/diagnostics", None).await;
        for key in ["status", "runtime", "capabilities", "tools", "work_dir"] {
            assert!(diagnostics.get(key).is_some(), "missing diagnostics.{key}");
        }

        let (_, cloud_token) = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300,"credits_remaining":10}"#,
            ),
        )
        .await;
        assert!(cloud_token["status"].get("state").is_some());
        assert!(cloud_token["status"].get("expires_in_seconds").is_some());
        assert!(cloud_token["status"].get("token").is_none());

        let (_, cloud_debug) = request_json(app.clone(), "POST", "/v1/ai/cloud-debug", None).await;
        for key in ["provider", "status", "error", "token"] {
            assert!(cloud_debug.get(key).is_some(), "missing cloud_debug.{key}");
        }

        let (_, intelligence) = request_json(
            app.clone(),
            "POST",
            "/v1/ai/transcript-intelligence",
            Some(
                r#"{"transcript":[{"start_seconds":1.0,"end_seconds":5.0,"text":"This is a useful hook."}],"clip_duration":"medium","timeline_target":"entire_video","max_suggestions":3}"#,
            ),
        )
        .await;
        for key in ["provider", "model", "clips", "usage"] {
            assert!(
                intelligence.get(key).is_some(),
                "missing transcript_intelligence.{key}"
            );
        }

        let (_, setup_manifest) =
            request_json(app.clone(), "GET", "/v1/setup/manifest", None).await;
        for key in [
            "schema_version",
            "platform",
            "setup_modes",
            "ai_provider_order",
            "assets",
        ] {
            assert!(
                setup_manifest.get(key).is_some(),
                "missing setup_manifest.{key}"
            );
        }

        let (_, setup_status) = request_json(app.clone(), "GET", "/v1/setup/status", None).await;
        for key in [
            "schema_version",
            "setup_complete",
            "active_ai_provider",
            "assets",
            "ai_providers",
            "capabilities",
        ] {
            assert!(
                setup_status.get(key).is_some(),
                "missing setup_status.{key}"
            );
        }

        let (_, probe) = request_json(
            app.clone(),
            "POST",
            "/v1/media/probe",
            Some(r#"{"path":"/tmp/file.mp4"}"#),
        )
        .await;
        for key in [
            "path",
            "duration_seconds",
            "width",
            "height",
            "format",
            "size_bytes",
        ] {
            assert!(
                probe["metadata"].get(key).is_some(),
                "missing metadata.{key}"
            );
        }

        let (_, thumbnail) = request_json(
            app.clone(),
            "POST",
            "/v1/media/thumbnail",
            Some(
                r#"{"source_path":"/tmp/video.mp4","output_path":"/tmp/thumb.jpg","timestamp_seconds":1.0}"#,
            ),
        )
        .await;
        assert!(thumbnail["result"].get("output_path").is_some());

        let (_, render) = request_json(
            app.clone(),
            "POST",
            "/v1/media/render-clip",
            Some(
                r#"{"source_path":"/tmp/video.mp4","output_path":"/tmp/clip.mp4","start_seconds":1.0,"end_seconds":11.0}"#,
            ),
        )
        .await;
        for key in ["output_path", "duration_seconds"] {
            assert!(render["result"].get(key).is_some(), "missing result.{key}");
        }

        let (_, job_create) = request_json(
            app.clone(),
            "POST",
            "/v1/ingest/url",
            Some(r#"{"url":"https://example.com/video","output_dir":"/tmp"}"#),
        )
        .await;
        for key in [
            "id",
            "kind",
            "status",
            "progress",
            "logs",
            "cancel_requested",
        ] {
            assert!(job_create["job"].get(key).is_some(), "missing job.{key}");
        }

        let (_, logs) = request_json(app, "GET", "/v1/jobs/job_1/logs", None).await;
        for key in ["job_id", "logs"] {
            assert!(logs.get(key).is_some(), "missing logs.{key}");
        }
    }

    #[test]
    fn config_rejects_non_loopback_host() {
        let mut values = test_config_values(std::env::temp_dir());
        values.host = Some("0.0.0.0".to_string());
        let error = AppConfig::from_values(values).expect_err("non-loopback host should fail");

        assert!(
            error.to_string().contains("must be loopback"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn config_rejects_relative_work_dir() {
        let error = AppConfig::from_values(test_config_values(PathBuf::from("relative-work-dir")))
            .expect_err("relative work dir should fail");

        assert!(
            error.to_string().contains("must be an absolute path"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn config_rejects_relative_whisper_model_path() {
        let mut values = test_config_values(std::env::temp_dir());
        values.whisper_model = Some(PathBuf::from("models/whisper.bin"));
        let error =
            AppConfig::from_values(values).expect_err("relative whisper model path should fail");

        assert!(
            error
                .to_string()
                .contains("TV7_ENGINE_WHISPER_MODEL must be an absolute path"),
            "unexpected error: {error:#}"
        );
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("health request should complete");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn probe_endpoint_returns_metadata() {
        let metadata = MediaMetadata {
            path: "/tmp/file.mp4".to_string(),
            duration_seconds: Some(10.0),
            width: Some(1920),
            height: Some(1080),
            format: "mp4".to_string(),
            size_bytes: 1024,
        };

        let app = router_with_probe(Ok(metadata));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/media/probe")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"path":"/Users/joaquinastelarra/video.mp4"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("probe request should complete");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn probe_endpoint_maps_domain_errors() {
        let app = router_with_probe(Err(EngineError::InvalidPath(
            "expected absolute path".to_string(),
        )));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/media/probe")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"path":"relative.mp4"}"#))
                    .expect("request should be valid"),
            )
            .await
            .expect("probe request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("error body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("INVALID_PATH"));
    }

    #[tokio::test]
    async fn diagnostics_endpoint_returns_ready_payload() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/diagnostics")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("diagnostics request should complete");

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("diagnostics body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("ffprobe"));
        assert!(body.contains("127.0.0.1"));
    }

    #[tokio::test]
    async fn cloud_token_endpoint_keeps_token_out_of_responses() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let (status, body) = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"secret-delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300,"credits_remaining":42}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"]["state"], "configured");
        assert_eq!(body["status"]["scope"], "engine:cloud_ai");
        assert_eq!(body["status"]["credits_remaining"], 42);
        assert!(
            !body.to_string().contains("secret-delegated-token"),
            "token leaked in response: {body}"
        );

        let (_, setup_status) = request_json(app, "GET", "/v1/setup/status", None).await;
        assert_eq!(setup_status["active_ai_provider"], "tv7_cloud");
        assert!(
            !setup_status.to_string().contains("secret-delegated-token"),
            "token leaked in setup status: {setup_status}"
        );
    }

    #[tokio::test]
    async fn cloud_token_clear_makes_cloud_ai_unavailable() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let _ = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300}"#,
            ),
        )
        .await;

        let (_, ready_status) = request_json(app.clone(), "GET", "/v1/setup/status", None).await;
        assert_eq!(ready_status["active_ai_provider"], "tv7_cloud");

        let (clear_status, clear_body) =
            request_json(app.clone(), "DELETE", "/v1/ai/cloud-token", None).await;
        assert_eq!(clear_status, StatusCode::OK);
        assert_eq!(clear_body["status"]["state"], "missing");

        let (_, not_ready_status) = request_json(app, "GET", "/v1/setup/status", None).await;
        assert_eq!(
            not_ready_status["active_ai_provider"],
            serde_json::Value::Null
        );
        let ai_capability = not_ready_status["capabilities"]
            .as_array()
            .expect("capabilities should be an array")
            .iter()
            .find(|capability| capability["name"] == "ai")
            .expect("ai capability should be present");
        assert_eq!(ai_capability["status"], "not_ready");
        assert_eq!(ai_capability["missing"][0], "cloud_ai_delegation_token");
    }

    #[tokio::test]
    async fn cloud_debug_uses_delegated_bearer_token() {
        let mock = Arc::new(MockMediaProbe {
            probe_response: Err(EngineError::Io("unused".to_string())),
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        let cloud = Arc::new(RecordingCloudAiExecutor::default());
        let app = build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: mock.clone(),
            transcript_provider: mock.clone(),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context: test_setup_context(),
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: cloud.clone(),
        });

        let _ = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300}"#,
            ),
        )
        .await;

        let (status, body) = request_json(app, "POST", "/v1/ai/cloud-debug", None).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
        let calls = cloud.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "delegated-token");
        assert_eq!(calls[0].1.messages[0].role, CloudAiMessageRole::System);
    }

    #[tokio::test]
    async fn cloud_debug_normalizes_backend_errors() {
        let mock = Arc::new(MockMediaProbe {
            probe_response: Err(EngineError::Io("unused".to_string())),
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        let cloud = Arc::new(RecordingCloudAiExecutor::with_response(Err(CloudAiError {
            code: CloudAiErrorCode::QuotaExceeded,
            message: "quota exhausted".to_string(),
        })));
        let app = build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: mock.clone(),
            transcript_provider: mock.clone(),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context: test_setup_context(),
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: cloud,
        });

        let _ = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300}"#,
            ),
        )
        .await;

        let (status, body) = request_json(app, "POST", "/v1/ai/cloud-debug", None).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "not_ready");
        assert_eq!(body["error"], "AI_QUOTA_EXCEEDED");
        assert!(!body.to_string().contains("delegated-token"));
    }

    #[tokio::test]
    async fn transcript_intelligence_dispatches_to_cloud_ai() {
        let mock = Arc::new(MockMediaProbe {
            probe_response: Err(EngineError::Io("unused".to_string())),
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        let cloud = Arc::new(RecordingCloudAiExecutor::default());
        let app = build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: mock.clone(),
            transcript_provider: mock.clone(),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context: test_setup_context(),
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: cloud.clone(),
        });

        let _ = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300}"#,
            ),
        )
        .await;
        let (status, body) = request_json(
            app,
            "POST",
            "/v1/ai/transcript-intelligence",
            Some(
                r#"{"transcript":[{"start_seconds":1.0,"end_seconds":7.0,"text":"Here is the main insight."}],"clip_duration":"medium","timeline_target":"entire_video","max_suggestions":2}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"], "tv7_cloud");
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["clips"][0]["title"], "Strong opening");
        assert!(!body.to_string().contains("delegated-token"));

        let calls = cloud.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "delegated-token");
        assert!(calls[0].1.messages[1].content.contains("main insight"));
    }

    #[tokio::test]
    async fn transcript_intelligence_requires_ready_ai_provider() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let (status, body) = request_json(
            app,
            "POST",
            "/v1/ai/transcript-intelligence",
            Some(
                r#"{"transcript":[{"start_seconds":1.0,"end_seconds":7.0,"text":"Here is the main insight."}],"clip_duration":"medium","timeline_target":"entire_video"}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "AI_AUTH_REQUIRED");
    }

    #[tokio::test]
    async fn transcript_intelligence_rejects_invalid_cloud_json() {
        let mock = Arc::new(MockMediaProbe {
            probe_response: Err(EngineError::Io("unused".to_string())),
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        let cloud = Arc::new(RecordingCloudAiExecutor::with_response(Ok(
            CloudAiExecuteResponse {
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                content: "not json".to_string(),
                usage: CloudAiUsage {
                    credits_charged: 1,
                    credits_remaining: 9,
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                    latency_ms: None,
                },
                raw_finish_reason: None,
            },
        )));
        let app = build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: mock.clone(),
            transcript_provider: mock.clone(),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context: test_setup_context(),
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: cloud,
        });

        let _ = request_json(
            app.clone(),
            "POST",
            "/v1/ai/cloud-token",
            Some(
                r#"{"token":"delegated-token","token_type":"bearer","scope":"engine:cloud_ai","expires_in_seconds":300}"#,
            ),
        )
        .await;
        let (status, body) = request_json(
            app,
            "POST",
            "/v1/ai/transcript-intelligence",
            Some(
                r#"{"transcript":[{"start_seconds":1.0,"end_seconds":7.0,"text":"Here is the main insight."}],"clip_duration":"medium","timeline_target":"entire_video"}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "AI_PROVIDER_UNAVAILABLE");
    }

    #[tokio::test]
    async fn thumbnail_endpoint_returns_output_path() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/media/thumbnail")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"source_path":"/tmp/video.mp4","output_path":"/tmp/thumb.jpg","timestamp_seconds":1.0}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("thumbnail request should complete");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn render_clip_endpoint_returns_output_path() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/media/render-clip")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"source_path":"/tmp/video.mp4","output_path":"/tmp/clip.mp4","start_seconds":1.0,"end_seconds":11.0}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("render request should complete");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn url_ingest_endpoint_creates_job() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/ingest/url")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"url":"https://example.com/video","output_dir":"/tmp","output_template":"video.%(ext)s"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("ingest request should complete");

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("job body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("url_ingest"));
        assert!(body.contains("job_"));
    }

    #[tokio::test]
    async fn transcript_endpoint_creates_job() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/transcripts/download")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"url":"https://example.com/video","output_path":"/tmp/video.vtt","language":"en"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("transcript request should complete");

        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn transcript_generate_requires_ready_local_transcription_setup() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let (status, body) = request_json(
            app,
            "POST",
            "/v1/transcripts/generate",
            Some(
                r#"{"media_path":"/tmp/video.mp4","output_path":"/tmp/video.vtt","language":"en"}"#,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "TRANSCRIPTION_SETUP_REQUIRED");
        assert_eq!(body["setup_capability"], "transcription");
        assert!(
            body["missing"]
                .as_array()
                .expect("missing should be an array")
                .contains(&serde_json::json!("transcription_runtime"))
        );
        assert!(
            body["missing"]
                .as_array()
                .expect("missing should be an array")
                .contains(&serde_json::json!("transcription_model"))
        );
    }

    #[tokio::test]
    async fn transcript_generate_endpoint_creates_job_when_setup_ready() {
        let app = router_with_probe_and_setup_context(
            Err(EngineError::Io("unused".to_string())),
            ready_transcription_setup_context(),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/transcripts/generate")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"media_path":"/tmp/video.mp4","output_path":"/tmp/video.vtt","language":"en"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("transcript generate request should complete");

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("job body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("transcript_generate"));
        assert!(body.contains("job_"));
    }

    #[tokio::test]
    async fn job_logs_endpoint_returns_logs() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/ingest/url")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"url":"https://example.com/video","output_dir":"/tmp"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("ingest request should complete");
        assert_eq!(create_response.status(), StatusCode::ACCEPTED);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/job_1/logs")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("logs request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("logs body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("queued"));
    }

    #[tokio::test]
    async fn cancel_job_endpoint_marks_job_cancelled() {
        let mock = Arc::new(MockMediaProbe {
            probe_response: Err(EngineError::Io("unused".to_string())),
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        let app = build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: mock.clone(),
            transcript_provider: Arc::new(SlowTranscriptProvider),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context: test_setup_context(),
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: Arc::new(RecordingCloudAiExecutor::default()),
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/transcripts/download")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"url":"https://example.com/video","output_path":"/tmp/video.vtt","language":"en"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("transcript request should complete");
        assert_eq!(create_response.status(), StatusCode::ACCEPTED);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/job_1/cancel")
                    .method("POST")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("cancel request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("cancel body should be readable");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("body should be json");
        assert_eq!(body["status"], "cancelled");
        assert_eq!(body["cancel_requested"], true);
    }

    #[tokio::test]
    async fn panicked_blocking_job_is_marked_failed() {
        let mock = Arc::new(MockMediaProbe {
            probe_response: Err(EngineError::Io("unused".to_string())),
            thumbnail_response: Ok(ThumbnailResult {
                output_path: "/tmp/thumb.jpg".to_string(),
            }),
            render_response: Ok(RenderClipResult {
                output_path: "/tmp/clip.mp4".to_string(),
                duration_seconds: 10.0,
            }),
        });
        let app = build_router(AppState {
            media_probe: mock.clone(),
            thumbnail_generator: mock.clone(),
            clip_renderer: mock.clone(),
            url_ingestor: Arc::new(PanicUrlIngestor),
            transcript_provider: mock.clone(),
            diagnostics_provider: mock.clone(),
            setup_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
            setup_context: test_setup_context(),
            cloud_ai_tokens: CloudAiTokenStore::default(),
            cloud_ai_executor: Arc::new(RecordingCloudAiExecutor::default()),
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/ingest/url")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"url":"https://example.com/video","output_dir":"/tmp"}"#,
                    ))
                    .expect("request should be valid"),
            )
            .await
            .expect("ingest request should complete");
        assert_eq!(create_response.status(), StatusCode::ACCEPTED);

        let mut job_body = serde_json::Value::Null;
        for _ in 0..20 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/v1/jobs/job_1")
                        .method("GET")
                        .body(Body::empty())
                        .expect("request should be valid"),
                )
                .await
                .expect("job request should complete");
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("job body should be readable");
            job_body = serde_json::from_slice(&body).expect("job body should be json");
            if job_body["status"] == "failed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(job_body["status"], "failed");
        assert!(
            job_body["error"]
                .as_str()
                .expect("failed job should include an error")
                .contains("job task panicked")
        );
    }

    #[test]
    fn job_store_marks_queued_job_cancelled() {
        let jobs = JobStore::default();
        let created = jobs.create(JobKind::TranscriptDownload);

        let cancelled = jobs
            .cancel(&created.id)
            .expect("created job should be cancellable");

        assert!(matches!(cancelled.status, JobStatus::Cancelled));
        assert!(cancelled.cancel_requested);
        assert_eq!(cancelled.error.as_deref(), Some("job cancelled"));
    }

    #[test]
    fn job_store_limit_removes_terminal_jobs_first() {
        let jobs = JobStore::default();
        let terminal = jobs.create(JobKind::UrlIngest);
        let running = jobs.create(JobKind::UrlIngest);
        let queued_a = jobs.create(JobKind::TranscriptDownload);
        let queued_b = jobs.create(JobKind::TranscriptDownload);

        jobs.update(&terminal.id, |job| {
            job.status = JobStatus::Succeeded;
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        });
        jobs.update(&running.id, |job| {
            job.status = JobStatus::Running;
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(2);
        });
        jobs.update(&queued_a.id, |job| {
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(3);
        });
        jobs.update(&queued_b.id, |job| {
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(4);
        });

        jobs.enforce_job_limit(3);

        assert!(jobs.get(&terminal.id).is_none());
        assert!(jobs.get(&running.id).is_some());
        assert!(jobs.get(&queued_a.id).is_some());
        assert!(jobs.get(&queued_b.id).is_some());
    }

    #[test]
    fn job_store_limit_bounds_non_terminal_jobs() {
        let jobs = JobStore::default();
        let oldest = jobs.create(JobKind::UrlIngest);
        let middle = jobs.create(JobKind::UrlIngest);
        let newest = jobs.create(JobKind::TranscriptDownload);

        jobs.update(&oldest.id, |job| {
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        });
        jobs.update(&middle.id, |job| {
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(2);
        });
        jobs.update(&newest.id, |job| {
            job.updated_at = SystemTime::UNIX_EPOCH + Duration::from_secs(3);
        });

        jobs.enforce_job_limit(2);

        assert!(jobs.get(&oldest.id).is_none());
        assert!(jobs.get(&middle.id).is_some());
        assert!(jobs.get(&newest.id).is_some());
    }

    struct BrokenSerialize;

    impl Serialize for BrokenSerialize {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let mut state = serializer.serialize_struct("BrokenSerialize", 1)?;
            state.serialize_field("broken", &AlwaysFails)?;
            state.end()
        }
    }

    struct AlwaysFails;

    impl Serialize for AlwaysFails {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(S::Error::custom("serialization failed"))
        }
    }

    #[test]
    fn complete_job_marks_serialization_errors_failed() {
        let jobs = JobStore::default();
        let created = jobs.create(JobKind::UrlIngest);

        complete_job(&jobs, &created.id, BrokenSerialize);

        let job = jobs.get(&created.id).expect("job should still exist");
        assert!(matches!(job.status, JobStatus::Failed));
        assert_eq!(job.progress, 1.0);
        assert!(
            job.error
                .as_deref()
                .unwrap_or_default()
                .contains("serialization failed")
        );
    }

    #[tokio::test]
    async fn missing_job_returns_404() {
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/missing")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("job request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
