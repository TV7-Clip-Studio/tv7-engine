use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use engine_core::{
    ClipRenderer, DiagnosticStatus, DiagnosticsContext, DiagnosticsProvider, EngineDiagnostics,
    EngineError,
    FfmpegMediaEngine, MediaMetadata, MediaProbe, RenderClipRequest, RenderClipResult,
    ThumbnailGenerator, ThumbnailRequest, ThumbnailResult, TranscriptDownloadRequest,
    TranscriptProvider, UrlIngestRequest, UrlIngestor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

type SharedMediaProbe = Arc<dyn MediaProbe>;
type SharedThumbnailGenerator = Arc<dyn ThumbnailGenerator>;
type SharedClipRenderer = Arc<dyn ClipRenderer>;
type SharedDiagnosticsProvider = Arc<dyn DiagnosticsProvider>;
type SharedUrlIngestor = Arc<dyn UrlIngestor>;
type SharedTranscriptProvider = Arc<dyn TranscriptProvider>;

#[derive(Clone)]
struct AppState {
    media_probe: SharedMediaProbe,
    thumbnail_generator: SharedThumbnailGenerator,
    clip_renderer: SharedClipRenderer,
    diagnostics_provider: SharedDiagnosticsProvider,
    url_ingestor: SharedUrlIngestor,
    transcript_provider: SharedTranscriptProvider,
    jobs: JobStore,
    diagnostics_context: DiagnosticsContext,
}

#[derive(Debug, Clone)]
struct AppConfig {
    host: IpAddr,
    port: u16,
    log_level: String,
    work_dir: PathBuf,
    ffmpeg_bin: String,
    ffprobe_bin: String,
    yt_dlp_bin: String,
}

impl AppConfig {
    fn from_env() -> Result<Self> {
        let host = env::var("TV7_ENGINE_HOST")
            .ok()
            .map(|value| value.parse::<IpAddr>())
            .transpose()
            .context("TV7_ENGINE_HOST must be a valid ip address")?
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

        if !host.is_loopback() {
            anyhow::bail!("TV7_ENGINE_HOST must be loopback, got {host}");
        }

        let port = env::var("TV7_ENGINE_PORT")
            .ok()
            .map(|value| value.parse::<u16>())
            .transpose()
            .context("TV7_ENGINE_PORT must be a valid u16 port")?
            .unwrap_or(7070);

        let log_level = env::var("TV7_ENGINE_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let ffmpeg_bin = env::var("TV7_ENGINE_FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".to_string());
        let ffprobe_bin =
            env::var("TV7_ENGINE_FFPROBE_BIN").unwrap_or_else(|_| "ffprobe".to_string());
        let yt_dlp_bin = env::var("TV7_ENGINE_YT_DLP_BIN").unwrap_or_else(|_| "yt-dlp".to_string());
        let work_dir = match env::var("TV7_ENGINE_WORK_DIR") {
            Ok(value) => PathBuf::from(value),
            Err(_) => env::current_dir().context("failed to resolve current directory")?,
        };

        if !work_dir.is_absolute() {
            anyhow::bail!(
                "TV7_ENGINE_WORK_DIR must be an absolute path, got {}",
                work_dir.display()
            );
        }

        if !work_dir.is_dir() {
            anyhow::bail!(
                "TV7_ENGINE_WORK_DIR must point to an existing directory, got {}",
                work_dir.display()
            );
        }

        Ok(Self {
            host,
            port,
            log_level,
            work_dir,
            ffmpeg_bin,
            ffprobe_bin,
            yt_dlp_bin,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum JobKind {
    UrlIngest,
    TranscriptDownload,
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

impl JobStore {
    fn create(&self, kind: JobKind) -> EngineJob {
        let id = format!("job_{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let job = EngineJob {
            id: id.clone(),
            kind,
            status: JobStatus::Queued,
            progress: 0.0,
            logs: vec!["queued".to_string()],
            result: None,
            error: None,
            cancel_requested: false,
        };
        self.jobs
            .lock()
            .expect("job store mutex should not be poisoned")
            .insert(id, job.clone());
        job
    }

    fn get(&self, id: &str) -> Option<EngineJob> {
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
        .route("/v1/media/probe", post(probe_media))
        .route("/v1/media/thumbnail", post(generate_thumbnail))
        .route("/v1/media/render-clip", post(render_clip))
        .route("/v1/ingest/url", post(start_url_ingest))
        .route("/v1/transcripts/download", post(start_transcript_download))
        .route("/v1/jobs/:id", get(get_job))
        .route("/v1/jobs/:id/logs", get(get_job_logs))
        .route("/v1/jobs/:id/cancel", post(cancel_job))
        .with_state(state)
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
    tokio::task::spawn_blocking(move || {
        if jobs.is_cancel_requested(&job_id) {
            return;
        }
        jobs.update(&job_id, |job| {
            job.status = JobStatus::Running;
            job.progress = 0.1;
            job.logs.push("started url ingest".to_string());
        });
        match ingestor.ingest_url(&payload) {
            Ok(result) => complete_job(&jobs, &job_id, result),
            Err(error) => fail_job(&jobs, &job_id, error),
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
    tokio::task::spawn_blocking(move || {
        if jobs.is_cancel_requested(&job_id) {
            return;
        }
        jobs.update(&job_id, |job| {
            job.status = JobStatus::Running;
            job.progress = 0.1;
            job.logs.push("started transcript download".to_string());
        });
        match transcript_provider.download_transcript(&payload) {
            Ok(result) => complete_job(&jobs, &job_id, result),
            Err(error) => fail_job(&jobs, &job_id, error),
        }
    });

    Ok((StatusCode::ACCEPTED, Json(JobCreateResponse { job })))
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

fn complete_job<T>(jobs: &JobStore, job_id: &str, result: T)
where
    T: Serialize,
{
    let value = serde_json::to_value(result).unwrap_or_else(|error| {
        serde_json::json!({
            "serialization_error": error.to_string()
        })
    });
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

    let media_engine = Arc::new(FfmpegMediaEngine::with_all_bins(
        config.ffmpeg_bin.clone(),
        config.ffprobe_bin.clone(),
        config.yt_dlp_bin.clone(),
    ));
    let state = AppState {
        media_probe: media_engine.clone(),
        thumbnail_generator: media_engine.clone(),
        clip_renderer: media_engine.clone(),
        url_ingestor: media_engine.clone(),
        transcript_provider: media_engine.clone(),
        diagnostics_provider: media_engine,
        jobs: JobStore::default(),
        diagnostics_context: DiagnosticsContext::new(
            env!("CARGO_PKG_VERSION"),
            config.host.to_string(),
            config.port,
            config.work_dir.clone(),
        ),
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
        "tv7 engine service listening"
    );
    axum::serve(listener, app)
        .await
        .context("service stopped")?;
    Ok(())
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
        CapabilityDiagnostic, DiagnosticStatus, ToolDiagnostic, TranscriptDownloadResult,
        UrlIngestResult, WorkDirDiagnostic,
    };
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

    impl TranscriptProvider for MockMediaProbe {
        fn download_transcript(
            &self,
            request: &TranscriptDownloadRequest,
        ) -> Result<TranscriptDownloadResult, EngineError> {
            std::thread::sleep(std::time::Duration::from_millis(25));
            Ok(TranscriptDownloadResult {
                transcript_path: Some(request.output_path.clone()),
                language: request.language.clone().unwrap_or_else(|| "en".to_string()),
            })
        }
    }

    fn router_with_probe(response: Result<MediaMetadata, EngineError>) -> Router {
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
            diagnostics_provider: mock,
            jobs: JobStore::default(),
            diagnostics_context: DiagnosticsContext::new(
                env!("CARGO_PKG_VERSION"),
                "127.0.0.1",
                7070,
                std::env::temp_dir(),
            ),
        })
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
        let app = router_with_probe(Err(EngineError::Io("unused".to_string())));

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/transcripts/download")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"url":"https://example.com/video","output_path":"/tmp/video.vtt"}"#,
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
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");
        assert!(body.contains("cancelled"));
        assert!(body.contains("cancel_requested"));
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
