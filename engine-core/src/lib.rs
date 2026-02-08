#[derive(Debug)]
pub struct MediaMetadata {
    pub path: String,
    pub duration_seconds: f64,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

pub fn probe_media(path: &str) -> Result<MediaMetadata, String> {
    // Stub implementation for POC
    if path.contains("invalid") {
        return Err("File not found or invalid media".to_string());
    }
    
    Ok(MediaMetadata {
        path: path.to_string(),
        duration_seconds: 120.5,
        width: 1920,
        height: 1080,
        format: "mp4".to_string(),
    })
}
