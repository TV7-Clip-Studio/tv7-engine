use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use engine_core::{FsMediaProbe, MediaMetadata, MediaProbe, ProbeError};
use serde::{Deserialize, Serialize};
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

type SharedMediaProbe = Arc<dyn MediaProbe>;

#[derive(Clone)]
struct AppState {
    media_probe: SharedMediaProbe,
}

#[derive(Debug, Clone)]
struct AppConfig {
    host: IpAddr,
    port: u16,
    log_level: String,
    work_dir: PathBuf,
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

impl From<ProbeError> for ApiError {
    fn from(value: ProbeError) -> Self {
        match value {
            ProbeError::InvalidPath(message) => {
                Self::new(StatusCode::BAD_REQUEST, "INVALID_PATH", message)
            }
            ProbeError::NotFound(path) => Self::new(
                StatusCode::NOT_FOUND,
                "FILE_NOT_FOUND",
                format!("file does not exist: {}", path.display()),
            ),
            ProbeError::NotFile(path) => Self::new(
                StatusCode::BAD_REQUEST,
                "NOT_A_FILE",
                format!("path is not a file: {}", path.display()),
            ),
            ProbeError::ProbeCommandFailed(message) => {
                Self::new(StatusCode::UNPROCESSABLE_ENTITY, "PROBE_FAILED", message)
            }
            ProbeError::ProbeOutputInvalid(message) => Self::new(
                StatusCode::BAD_GATEWAY,
                "PROBE_OUTPUT_INVALID",
                format!("failed to parse probe output: {message}"),
            ),
            ProbeError::Io(message) => {
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
        .route("/v1/media/probe", post(probe_media))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    debug!("health endpoint called");
    Json(HealthResponse { status: "ok" })
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

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env()?;
    initialize_logging(&config.log_level);

    let state = AppState {
        media_probe: Arc::new(FsMediaProbe::new()),
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.socket_addr())
        .await
        .with_context(|| format!("failed to bind on {}", config.socket_addr()))?;

    info!(
        address = %config.socket_addr(),
        work_dir = %config.work_dir.display(),
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
    use tower::ServiceExt;

    #[derive(Debug)]
    struct MockMediaProbe {
        response: Result<MediaMetadata, ProbeError>,
    }

    impl MediaProbe for MockMediaProbe {
        fn probe_path(&self, _path: &Path) -> Result<MediaMetadata, ProbeError> {
            self.response.clone()
        }
    }

    fn router_with_probe(response: Result<MediaMetadata, ProbeError>) -> Router {
        build_router(AppState {
            media_probe: Arc::new(MockMediaProbe { response }),
        })
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let app = router_with_probe(Err(ProbeError::Io("unused".to_string())));

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
        let app = router_with_probe(Err(ProbeError::InvalidPath(
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
}
