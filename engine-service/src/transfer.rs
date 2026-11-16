//! Byte transfer only: Backend owns authorization, destinations and scheduling.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
pub(crate) struct Metadata {
    size_bytes: u64,
    checksum_sha256: String,
    content_type: &'static str,
}
fn error(status: StatusCode, code: &'static str, message: &str) -> ApiError {
    ApiError::new(status, code, message)
}
fn owned_path(state: &AppState, id: &str) -> Result<PathBuf, ApiError> {
    state
        .exports
        .get(id)
        .and_then(|p| owned_mp4_path(state, &p.to_string_lossy()))
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "ASSET_NOT_FOUND",
                "Register an Engine-owned rendered clip first",
            )
        })
}
pub(crate) async fn metadata(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Metadata>, ApiError> {
    let path = owned_path(&state, &id)?;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| error(StatusCode::NOT_FOUND, "ASSET_NOT_FOUND", "Cannot read clip"))?;
    let size = file
        .metadata()
        .await
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "INVALID_EXPORT",
                "Cannot inspect clip",
            )
        })?
        .len();
    if !(1..=MAX_EXPORT_BYTES).contains(&size) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "INVALID_EXPORT",
            "Invalid clip size",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).await.map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "INVALID_EXPORT",
                "Cannot read clip",
            )
        })?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(Json(Metadata {
        size_bytes: size,
        checksum_sha256: format!("{:x}", hash.finalize()),
        content_type: "video/mp4",
    }))
}
#[derive(Deserialize)]
pub(crate) struct Transfer {
    media_id: String,
    upload_url: String,
    upload_headers: HashMap<String, String>,
    size_bytes: u64,
    checksum_sha256: String,
}
fn valid_url(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == Some("www.googleapis.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && url.path() == "/upload/youtube/v3/videos"
        && url
            .query_pairs()
            .any(|(key, value)| key == "upload_id" && !value.is_empty())
}
fn bearer(headers: &HashMap<String, String>) -> Option<&str> {
    if headers.iter().any(|(key, value)| {
        !(key.eq_ignore_ascii_case("authorization")
            || key.eq_ignore_ascii_case("content-type") && value == "video/mp4")
    }) {
        return None;
    }
    let value = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("authorization"))?
        .1;
    value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty() && !token.chars().any(char::is_whitespace))
}
fn acknowledged_offset(range: Option<&str>, size: u64) -> Option<u64> {
    match range {
        None => Some(0),
        Some(range) => range
            .strip_prefix("bytes=0-")?
            .parse::<u64>()
            .ok()?
            .checked_add(1)
            .filter(|offset| *offset <= size),
    }
}
pub(crate) async fn youtube(
    State(state): State<AppState>,
    Json(payload): Json<Transfer>,
) -> Result<Json<Value>, ApiError> {
    if !valid_url(&payload.upload_url) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "INVALID_TRANSFER_URL",
            "Expected a YouTube resumable upload session",
        ));
    }
    let auth = bearer(&payload.upload_headers).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "INVALID_TRANSFER_AUTH",
            "Expected short-lived authorization and video/mp4 headers",
        )
    })?;
    let path = owned_path(&state, &payload.media_id)?;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| error(StatusCode::NOT_FOUND, "ASSET_NOT_FOUND", "Cannot read clip"))?;
    let size = file
        .metadata()
        .await
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "INVALID_EXPORT",
                "Cannot inspect clip",
            )
        })?
        .len();
    if !(1..=MAX_EXPORT_BYTES).contains(&size) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "INVALID_EXPORT",
            "Invalid clip size",
        ));
    }
    if size != payload.size_bytes {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "TRANSFER_ASSET_CHANGED",
            "Clip size differs from the authorized upload",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await.map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "INVALID_EXPORT",
                "Cannot read clip",
            )
        })?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != payload.checksum_sha256 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "TRANSFER_ASSET_CHANGED",
            "Clip checksum differs from the authorized upload",
        ));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "TRANSFER_SETUP_FAILED",
                "Cannot initialize transfer",
            )
        })?;
    // Query the same session before sending bytes. Never create a second upload on retry.
    let probe = client
        .put(&payload.upload_url)
        .bearer_auth(auth)
        .header("Content-Length", "0")
        .header("Content-Range", format!("bytes */{size}"))
        .send()
        .await
        .map_err(|_| {
            error(
                StatusCode::BAD_GATEWAY,
                "TRANSFER_STATUS_UNKNOWN",
                "Could not query upload. Verify the existing session before retrying",
            )
        })?;
    if probe.status().is_success() {
        return receipt(probe).await;
    }
    if probe.status().as_u16() != 308 {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "TRANSFER_SESSION_REJECTED",
            "YouTube rejected the upload session; refresh authorization or verify its status",
        ));
    }
    let offset = acknowledged_offset(
        probe.headers().get("Range").and_then(|h| h.to_str().ok()),
        size,
    )
    .filter(|offset| *offset < size)
    .ok_or_else(|| {
        error(
            StatusCode::BAD_GATEWAY,
            "TRANSFER_STATUS_UNKNOWN",
            "Invalid provider offset; verify the upload before retrying",
        )
    })?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "INVALID_EXPORT",
                "Cannot seek clip",
            )
        })?;
    let response = client
        .put(&payload.upload_url)
        .bearer_auth(auth)
        .header("Content-Type", "video/mp4")
        .header("Content-Length", (size - offset).to_string())
        .header(
            "Content-Range",
            format!("bytes {offset}-{}/{size}", size - 1),
        )
        .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
        .send()
        .await
        .map_err(|_| {
            error(
                StatusCode::BAD_GATEWAY,
                "TRANSFER_STATUS_UNKNOWN",
                "Upload result unknown. Verify the existing session before retrying",
            )
        })?;
    if !response.status().is_success() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "TRANSFER_INCOMPLETE",
            "Upload incomplete. Resume the existing session; do not create a new upload",
        ));
    }
    receipt(response).await
}
async fn receipt(response: reqwest::Response) -> Result<Json<Value>, ApiError> {
    let data: Value = response.json().await.map_err(|_| {
        error(
            StatusCode::BAD_GATEWAY,
            "TRANSFER_STATUS_UNKNOWN",
            "Invalid provider receipt; verify the existing upload",
        )
    })?;
    let id = data
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_youtube_video_id(id))
        .ok_or_else(|| {
            error(
                StatusCode::BAD_GATEWAY,
                "TRANSFER_STATUS_UNKNOWN",
                "Missing provider video ID; verify the existing upload",
            )
        })?;
    Ok(Json(serde_json::json!({"remote_video_id": id})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfer_capabilities_are_restricted() {
        assert!(valid_url(
            "https://www.googleapis.com/upload/youtube/v3/videos?upload_id=test"
        ));
        for url in [
            "http://www.googleapis.com/upload/youtube/v3/videos?upload_id=x",
            "https://evil.com/upload/youtube/v3/videos?upload_id=x",
            "https://www.googleapis.com/upload/youtube/v3/videos",
            "https://user@www.googleapis.com/upload/youtube/v3/videos?upload_id=x",
            "https://www.googleapis.com/upload/youtube/v3/videos?upload_id=x#secret",
        ] {
            assert!(!valid_url(url));
        }
        assert_eq!(acknowledged_offset(Some("bytes=0-9"), 20), Some(10));
        assert_eq!(acknowledged_offset(Some("bytes=0-21"), 20), None);
        assert_eq!(acknowledged_offset(Some("bytes=1-9"), 20), None);
        let mut headers = HashMap::from([
            ("Authorization".into(), "Bearer token".into()),
            ("Content-Type".into(), "video/mp4".into()),
        ]);
        assert_eq!(bearer(&headers), Some("token"));
        headers.insert("Host".into(), "evil.com".into());
        assert!(bearer(&headers).is_none());
    }
}
