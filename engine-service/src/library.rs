use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryClip {
    #[serde(default)]
    pub remix_settings: Option<crate::remix::RemixSettings>,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub cue_count: u32,
    #[serde(default)]
    pub proposal_source: Option<String>,
    pub caption_text: String,
    pub burn_subtitles: bool,
    #[serde(default)]
    pub subtitle_style: Option<engine_core::SubtitleStyle>,
    #[serde(default)]
    pub subtitle_language: Option<String>,
    #[serde(default)]
    pub original_transcript_path: Option<String>,
    #[serde(default)]
    pub word_transcript_path: Option<String>,
    pub format: String,
    pub output_path: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub post_copy: Option<PostCopy>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostCopy {
    #[serde(default)]
    pub mood: String,
    #[serde(default)]
    pub hook: String,
    pub youtube_title: String,
    pub youtube_description: String,
    pub instagram_caption: String,
    pub tiktok_caption: String,
    #[serde(default)]
    pub hashtags: Vec<String>,
    pub source: String,
    #[serde(default)]
    pub model: Option<String>,
}

impl PostCopy {
    fn valid(&self) -> bool {
        self.mood.chars().count() <= 120
            && self.hook.chars().count() <= 120
            && self.youtube_title.chars().count() <= 100
            && self.youtube_description.chars().count() <= 4500
            && self.instagram_caption.chars().count() <= 2000
            && self.tiktok_caption.chars().count() <= 2000
            && self.hashtags.len() <= 8
            && self.hashtags.iter().all(|tag| {
                tag.chars().count() <= 40
                    && tag.starts_with('#')
                    && tag
                        .chars()
                        .skip(1)
                        .all(|ch| ch.is_alphanumeric() || ch == '_')
            })
            && matches!(self.source.as_str(), "template" | "ai" | "edited")
            && self.model.as_ref().is_none_or(|model| model.len() <= 120)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryVideo {
    pub media_path: String,
    pub title: String,
    pub source_url: Option<String>,
    pub duration_seconds: Option<f64>,
    pub size_bytes: Option<u64>,
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub clips: Vec<LibraryClip>,
    #[serde(default)]
    pub recovered: bool,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub modified_unix_ms: Option<u64>,
}

#[derive(Default, Serialize, Deserialize)]
struct FingerprintCache {
    entries: HashMap<String, FingerprintEntry>,
}

#[derive(Serialize, Deserialize)]
struct FingerprintEntry {
    size: u64,
    modified_unix_ns: u128,
    hash: String,
}

#[derive(Default, Serialize, Deserialize)]
struct LibraryIndex {
    videos: Vec<LibraryVideo>,
}

#[derive(Serialize)]
pub struct UnlinkedClip {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Serialize)]
pub struct ConsolidationResult {
    pub projects_updated: usize,
    pub duplicate_sources_removed: usize,
    pub clips_preserved: usize,
}

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn index_path(root: &Path) -> PathBuf {
    root.join(".tv7-library.json")
}

fn backup_path(root: &Path) -> PathBuf {
    root.join(".tv7-library.previous.json")
}

fn fingerprint_cache_path(root: &Path) -> PathBuf {
    root.join(".tv7-library-fingerprints.json")
}

fn add_fingerprints(root: &Path, videos: &mut [LibraryVideo]) -> Result<(), String> {
    let cache_path = fingerprint_cache_path(root);
    let mut cache: FingerprintCache = fs::read(&cache_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut changed = false;
    for video in videos {
        let path = Path::new(&video.media_path);
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .unwrap_or_default();
        let modified_ns = modified.as_nanos();
        video.modified_unix_ms = Some(modified.as_millis().min(u64::MAX as u128) as u64);
        if let Some(entry) = cache.entries.get(&video.media_path) {
            if entry.size == metadata.len() && entry.modified_unix_ns == modified_ns {
                video.content_hash = Some(entry.hash.clone());
                continue;
            }
        }
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut hasher = blake3::Hasher::new();
        let mut chunk = [0_u8; 1024 * 1024];
        loop {
            let count = file.read(&mut chunk).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&chunk[..count]);
        }
        let hash = hasher.finalize().to_hex().to_string();
        cache.entries.insert(
            video.media_path.clone(),
            FingerprintEntry {
                size: metadata.len(),
                modified_unix_ns: modified_ns,
                hash: hash.clone(),
            },
        );
        video.content_hash = Some(hash);
        changed = true;
    }
    if changed {
        let temp = root.join(format!(
            ".tv7-library-fingerprints-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        fs::write(
            &temp,
            serde_json::to_vec(&cache).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if cache_path.exists() {
            fs::remove_file(&cache_path).map_err(|e| e.to_string())?;
        }
        fs::rename(temp, cache_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn read_index(root: &Path) -> Result<LibraryIndex, String> {
    let path = if index_path(root).exists() {
        index_path(root)
    } else {
        backup_path(root)
    };
    if !path.exists() {
        return Ok(LibraryIndex::default());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Local library index is damaged: {e}"))
}

fn owned_file(root: &Path, value: &str, extensions: &[&str]) -> Result<PathBuf, String> {
    let file = fs::canonicalize(value).map_err(|_| format!("Media file is missing: {value}"))?;
    if !file.starts_with(root)
        || !file.is_file()
        || !file
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| extensions.iter().any(|ext| s.eq_ignore_ascii_case(ext)))
    {
        return Err("File must be an Engine-owned media asset".to_string());
    }
    Ok(file)
}

pub fn list(root: &Path) -> Result<Vec<LibraryVideo>, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    list_unlocked(&root)
}

fn list_unlocked(root: &Path) -> Result<Vec<LibraryVideo>, String> {
    let mut videos = read_index(root)?.videos;
    videos.retain(|video| {
        owned_file(
            root,
            &video.media_path,
            &["mp4", "mov", "mkv", "webm", "m4v"],
        )
        .is_ok()
    });
    for video in &mut videos {
        video.clips.retain(|clip| {
            clip.output_path
                .as_deref()
                .is_none_or(|p| owned_file(root, p, &["mp4"]).is_ok())
        });
    }
    let known: HashSet<String> = videos.iter().map(|v| v.media_path.clone()).collect();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("job_") || name.starts_with("upload-"))
            || !path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                ["mp4", "mov", "mkv", "webm", "m4v"].contains(&s.to_ascii_lowercase().as_str())
            })
        {
            continue;
        }
        let path = match fs::canonicalize(path) {
            Ok(path) if path.is_file() => path,
            _ => continue,
        };
        let media_path = path.to_string_lossy().to_string();
        if known.contains(&media_path) {
            continue;
        }
        videos.push(LibraryVideo {
            media_path,
            title: name,
            source_url: None,
            duration_seconds: None,
            size_bytes: fs::metadata(&path).ok().map(|m| m.len()),
            transcript_path: None,
            clips: vec![],
            recovered: true,
            content_hash: None,
            modified_unix_ms: None,
        });
    }
    add_fingerprints(root, &mut videos)?;
    videos.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(videos)
}

pub fn save(root: &Path, mut video: LibraryVideo) -> Result<LibraryVideo, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    video.media_path = owned_file(
        &root,
        &video.media_path,
        &["mp4", "mov", "mkv", "webm", "m4v"],
    )?
    .to_string_lossy()
    .to_string();
    if video.title.trim().is_empty()
        || video.title.len() > 500
        || video.clips.len() > 100
        || video
            .duration_seconds
            .is_some_and(|d| !d.is_finite() || d < 0.0)
    {
        return Err("Invalid video details".into());
    }
    if let Some(path) = &video.transcript_path {
        owned_file(&root, path, &["vtt"])?;
    }
    for clip in &mut video.clips {
        if let Some(plan) = clip.remix_settings.as_ref().and_then(|s| s.plan.as_ref()) {
            crate::remix_assets::validate_plan(&root, plan)?;
        }
        if clip.id.is_empty()
            || clip.id.len() > 200
            || clip.caption_text.len() > 20_000
            || clip.title.as_ref().is_some_and(|title| {
                title.chars().count() > 160 || title.chars().any(char::is_control)
            })
            || clip.post_copy.as_ref().is_some_and(|copy| !copy.valid())
            || clip.remix_settings.as_ref().is_some_and(|settings| {
                !settings.valid()
                    || settings.plan.as_ref().is_some_and(|plan| {
                        !plan.valid(
                            clip.end.parse::<f64>().unwrap_or(f64::NAN)
                                - clip.start.parse::<f64>().unwrap_or(f64::NAN),
                        )
                    })
            })
        {
            return Err("Invalid clip details".into());
        }
        clip.title = clip.title.take().and_then(|title| {
            let trimmed = title.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        if let Some(path) = &clip.output_path {
            clip.output_path = Some(
                owned_file(&root, path, &["mp4"])?
                    .to_string_lossy()
                    .to_string(),
            );
        }
        if let Some(path) = &clip.word_transcript_path {
            clip.word_transcript_path = Some(
                owned_file(&root, path, &["vtt"])?
                    .to_string_lossy()
                    .to_string(),
            );
        }
        if let Some(path) = &clip.transcript_path {
            clip.transcript_path = Some(
                owned_file(&root, path, &["vtt"])?
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    video.recovered = false;
    video.content_hash = None;
    video.modified_unix_ms = None;
    let mut index = read_index(&root)?;
    index
        .videos
        .retain(|item| item.media_path != video.media_path);
    index.videos.push(video.clone());
    write_index(&root, &index)?;
    Ok(video)
}

fn write_index(root: &Path, index: &LibraryIndex) -> Result<(), String> {
    let temp = root.join(format!(".tv7-library-{}.tmp", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let current = index_path(root);
    let backup = backup_path(root);
    if current.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(&current, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(error) = fs::rename(&temp, &current) {
        if backup.exists() {
            let _ = fs::rename(&backup, &current);
        }
        return Err(error.to_string());
    }
    if backup.exists() {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub fn consolidate_exact_duplicates(root: &Path) -> Result<ConsolidationResult, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let videos = list_unlocked(&root)?;
    let original_videos = videos.clone();
    let mut groups: HashMap<String, Vec<LibraryVideo>> = HashMap::new();
    let mut singles = Vec::new();
    for video in videos {
        if let Some(hash) = video.content_hash.clone() {
            groups.entry(hash).or_default().push(video);
        } else {
            singles.push(video);
        }
    }
    let mut result = ConsolidationResult {
        projects_updated: 0,
        duplicate_sources_removed: 0,
        clips_preserved: 0,
    };
    let mut duplicates = Vec::new();
    for (hash, mut copies) in groups {
        if copies.len() == 1 {
            singles.push(copies.remove(0));
            continue;
        }
        copies.sort_by(|a, b| {
            let rank = |v: &LibraryVideo| {
                (
                    v.clips
                        .iter()
                        .filter(|clip| clip.output_path.is_some())
                        .count(),
                    v.clips.len(),
                    usize::from(!v.recovered),
                    usize::from(!v.title.starts_with("job_") && !v.title.starts_with("upload-")),
                )
            };
            rank(b)
                .cmp(&rank(a))
                .then_with(|| a.media_path.cmp(&b.media_path))
        });
        let mut master = copies.remove(0);
        for clip in &mut master.clips {
            if clip.transcript_path.is_none() {
                clip.transcript_path = master.transcript_path.clone();
            }
        }
        let mut clip_ids: HashSet<String> =
            master.clips.iter().map(|clip| clip.id.clone()).collect();
        for copy in &copies {
            if master.source_url.is_none() {
                master.source_url = copy.source_url.clone();
            }
            if master.transcript_path.is_none() {
                master.transcript_path = copy.transcript_path.clone();
            }
            for clip in &copy.clips {
                let mut clip = clip.clone();
                if clip.transcript_path.is_none() {
                    clip.transcript_path = copy.transcript_path.clone();
                }
                if !clip_ids.insert(clip.id.clone()) {
                    clip.id = uuid::Uuid::new_v4().to_string();
                    clip_ids.insert(clip.id.clone());
                }
                master.clips.push(clip);
            }
            duplicates.push((copy.media_path.clone(), hash.clone()));
        }
        result.clips_preserved += master.clips.len();
        result.projects_updated += 1;
        singles.push(master);
    }
    if result.projects_updated == 0 {
        return Ok(result);
    }
    let migration_backup = root.join(".tv7-library.pre-consolidation.json");
    if !migration_backup.exists() {
        fs::write(
            &migration_backup,
            serde_json::to_vec_pretty(&LibraryIndex {
                videos: original_videos,
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    write_index(&root, &LibraryIndex { videos: singles })?;
    for (path, expected_hash) in duplicates {
        let path = match owned_file(&root, &path, &["mp4", "mov", "mkv", "webm", "m4v"]) {
            Ok(path) => path,
            Err(_) => continue,
        };
        let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
        if hasher.finalize().to_hex().as_str() == expected_hash && fs::remove_file(path).is_ok() {
            result.duplicate_sources_removed += 1;
        }
    }
    Ok(result)
}

pub fn unlinked_clips(root: &Path) -> Result<Vec<UnlinkedClip>, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let linked: HashSet<String> = read_index(&root)?
        .videos
        .iter()
        .filter(|video| {
            owned_file(
                &root,
                &video.media_path,
                &["mp4", "mov", "mkv", "webm", "m4v"],
            )
            .is_ok()
        })
        .flat_map(|video| video.clips.iter())
        .filter_map(|clip| clip.output_path.as_deref())
        .filter_map(|path| fs::canonicalize(path).ok())
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    let mut clips = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("clip-") || !name.ends_with(".mp4") {
            continue;
        }
        let path = match fs::canonicalize(entry.path()) {
            Ok(path) if path.is_file() => path,
            _ => continue,
        };
        let path = path.to_string_lossy().to_string();
        if linked.contains(&path) {
            continue;
        }
        clips.push(UnlinkedClip {
            path,
            name,
            size_bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
        });
    }
    clips.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(clips)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_survives_reload_and_rejects_external_files() {
        let root = std::env::temp_dir().join(format!("tv7-library-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("upload-test.mp4");
        let output = root.join("clip-test.mp4");
        fs::write(&source, b"source").unwrap();
        fs::write(&output, b"clip").unwrap();
        let video = LibraryVideo {
            media_path: source.to_string_lossy().to_string(),
            title: "Test".into(),
            source_url: None,
            duration_seconds: Some(10.0),
            size_bytes: Some(6),
            transcript_path: None,
            clips: vec![LibraryClip {
                id: "one".into(),
                title: Some("A short title".into()),
                start: "0".into(),
                end: "2".into(),
                text: "original".into(),
                cue_count: 1,
                proposal_source: Some("local_timing".into()),
                caption_text: "hello".into(),
                subtitle_style: None,
                subtitle_language: None,
                original_transcript_path: None,
                word_transcript_path: None,
                burn_subtitles: true,
                format: "source".into(),
                output_path: Some(output.to_string_lossy().to_string()),
                transcript_path: None,
                post_copy: None,
                remix_settings: None,
            }],
            recovered: true,
            content_hash: None,
            modified_unix_ms: None,
        };
        assert!(list(&root).unwrap()[0].recovered);
        save(&root, video).unwrap();
        let saved = list(&root).unwrap();
        assert_eq!(saved[0].clips[0].title.as_deref(), Some("A short title"));
        let mut accented = saved[0].clone();
        accented.clips[0].title = Some("ñ".repeat(160));
        save(&root, accented).unwrap();
        assert_eq!(
            list(&root).unwrap()[0].clips[0]
                .title
                .as_ref()
                .unwrap()
                .chars()
                .count(),
            160
        );
        assert_eq!(saved[0].clips.len(), 1);
        assert_eq!(saved[0].clips[0].text, "original");
        assert_eq!(saved[0].clips[0].caption_text, "hello");
        let mut with_copy = saved[0].clone();
        with_copy.clips[0].post_copy = Some(PostCopy {
            mood: "playful".into(),
            hook: "A surprising demand".into(),
            youtube_title: "A useful moment".into(),
            youtube_description: "Description".into(),
            instagram_caption: "Instagram text".into(),
            tiktok_caption: "TikTok text".into(),
            hashtags: vec!["#example".into()],
            source: "edited".into(),
            model: None,
        });
        save(&root, with_copy).unwrap();
        assert_eq!(
            list(&root).unwrap()[0].clips[0]
                .post_copy
                .as_ref()
                .unwrap()
                .instagram_caption,
            "Instagram text"
        );
        assert_eq!(
            list(&root).unwrap()[0].clips[0]
                .post_copy
                .as_ref()
                .unwrap()
                .mood,
            "playful"
        );
        let duplicate = root.join("upload-duplicate.mp4");
        fs::copy(&source, &duplicate).unwrap();
        let grouped = list(&root).unwrap();
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].content_hash, grouped[1].content_hash);
        fs::write(&duplicate, b"different source").unwrap();
        let changed = list(&root).unwrap();
        assert_ne!(changed[0].content_hash, changed[1].content_hash);
        assert!(unlinked_clips(&root).unwrap().is_empty());
        fs::rename(index_path(&root), backup_path(&root)).unwrap();
        assert_eq!(list(&root).unwrap()[0].clips.len(), 1);
        let outside = LibraryVideo {
            media_path: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            title: "outside".into(),
            source_url: None,
            duration_seconds: None,
            size_bytes: None,
            transcript_path: None,
            clips: vec![],
            recovered: false,
            content_hash: None,
            modified_unix_ms: None,
        };
        assert!(save(&root, outside).is_err());
        fs::remove_file(source).unwrap();
        assert_eq!(unlinked_clips(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_clip_without_title_remains_readable() {
        let clip: LibraryClip = serde_json::from_value(serde_json::json!({
            "id": "legacy", "start": "0", "end": "2", "text": "caption",
            "cue_count": 1, "proposal_source": null, "caption_text": "caption",
            "burn_subtitles": true, "format": "source", "output_path": null
        }))
        .unwrap();
        assert!(clip.title.is_none());
    }

    #[test]
    fn consolidation_preserves_drafts_and_removes_only_exact_sources() {
        let root =
            std::env::temp_dir().join(format!("tv7-consolidate-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let first = root.join("upload-first.mp4");
        let second = root.join("upload-second.mp4");
        let different = root.join("upload-different.mp4");
        let transcript = root.join("captions.vtt");
        fs::write(&first, b"same source bytes").unwrap();
        fs::write(&second, b"same source bytes").unwrap();
        fs::write(&different, b"other source bytes").unwrap();
        fs::write(&transcript, b"WEBVTT\n").unwrap();
        for (path, id, vtt) in [(&first, "a", None), (&second, "b", Some(&transcript))] {
            save(
                &root,
                LibraryVideo {
                    media_path: path.to_string_lossy().to_string(),
                    title: "A title".into(),
                    source_url: None,
                    duration_seconds: Some(10.0),
                    size_bytes: None,
                    transcript_path: vtt.map(|p| p.to_string_lossy().to_string()),
                    clips: vec![LibraryClip {
                        id: id.into(),
                        title: None,
                        start: "0".into(),
                        end: "2".into(),
                        text: id.into(),
                        cue_count: 1,
                        proposal_source: None,
                        caption_text: id.into(),
                        subtitle_style: None,
                        subtitle_language: None,
                        original_transcript_path: None,
                        word_transcript_path: None,
                        burn_subtitles: true,
                        format: "source".into(),
                        output_path: None,
                        transcript_path: None,
                        post_copy: None,
                        remix_settings: None,
                    }],
                    recovered: false,
                    content_hash: None,
                    modified_unix_ms: None,
                },
            )
            .unwrap();
        }
        let result = consolidate_exact_duplicates(&root).unwrap();
        assert_eq!(result.projects_updated, 1);
        assert_eq!(result.duplicate_sources_removed, 1);
        assert!(different.exists());
        let catalog = list(&root).unwrap();
        assert_eq!(catalog.len(), 2);
        let merged = catalog.iter().find(|video| video.clips.len() == 2).unwrap();
        assert_eq!(
            merged
                .clips
                .iter()
                .map(|clip| clip.id.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["a", "b"])
        );
        assert!(
            merged
                .clips
                .iter()
                .any(|clip| clip.transcript_path.as_deref() == Some(transcript.to_str().unwrap()))
        );
        assert!(root.join(".tv7-library.pre-consolidation.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
