use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorProfile {
    pub id: String,
    pub name: String,
    pub channel_url: String,
    pub default_clip_count: u8,
    pub days_between_posts: u8,
    pub instagram_handle: String,
    pub tiktok_handle: String,
    #[serde(default)]
    pub subtitle_style: Option<engine_core::SubtitleStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub profiles: Vec<CreatorProfile>,
    pub active_profile_id: String,
    #[serde(default)]
    pub project_assignments: HashMap<String, String>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            profiles: vec![CreatorProfile {
                id: "personal".into(),
                name: "My creator profile".into(),
                channel_url: String::new(),
                default_clip_count: 5,
                days_between_posts: 2,
                instagram_handle: String::new(),
                tiktok_handle: String::new(),
                subtitle_style: None,
            }],
            active_profile_id: "personal".into(),
            project_assignments: HashMap::new(),
        }
    }
}

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn read(root: &Path) -> Result<Workspace, String> {
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    let path = root.join(".tv7-workspace.json");
    let backup = root.join(".tv7-workspace.previous.json");
    let path = if path.exists() { path } else { backup };
    if !path.exists() {
        return Ok(Workspace::default());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Local workspace is damaged: {e}"))
}

pub fn save(root: &Path, workspace: Workspace) -> Result<Workspace, String> {
    if workspace.profiles.is_empty() || workspace.profiles.len() > 50 {
        return Err("Expected 1–50 creator profiles".into());
    }
    let ids: std::collections::HashSet<_> =
        workspace.profiles.iter().map(|p| p.id.as_str()).collect();
    if ids.len() != workspace.profiles.len() || !ids.contains(workspace.active_profile_id.as_str())
    {
        return Err("Profile IDs must be unique and active profile must exist".into());
    }
    for profile in &workspace.profiles {
        if let Some(style) = &profile.subtitle_style {
            style.validate().map_err(|error| error.to_string())?;
        }
        if profile.id.is_empty()
            || profile.id.len() > 80
            || profile.name.trim().is_empty()
            || profile.name.chars().count() > 120
            || !(1..=12).contains(&profile.default_clip_count)
            || !(1..=30).contains(&profile.days_between_posts)
            || profile.channel_url.len() > 2048
            || profile.instagram_handle.len() > 255
            || profile.tiktok_handle.len() > 255
        {
            return Err("Invalid creator profile".into());
        }
    }
    if workspace.project_assignments.len() > 10000
        || workspace
            .project_assignments
            .iter()
            .any(|(key, value)| key.len() > 2048 || !ids.contains(value.as_str()))
    {
        return Err("Invalid project assignments".into());
    }
    let _guard = lock().lock().map_err(|e| e.to_string())?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join(".tv7-workspace.json");
    let backup = root.join(".tv7-workspace.previous.json");
    let temp = root.join(format!(".tv7-workspace-{}.tmp", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(&workspace).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(&path, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(error) = fs::rename(&temp, &path) {
        if backup.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(error.to_string());
    }
    if backup.exists() {
        let _ = fs::remove_file(backup);
    }
    Ok(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_reject_unknown_assignment() {
        let root =
            std::env::temp_dir().join(format!("tv7-workspace-test-{}", uuid::Uuid::new_v4()));
        let mut value = Workspace::default();
        value
            .project_assignments
            .insert("source:abc".into(), "personal".into());
        save(&root, value).unwrap();
        assert_eq!(
            read(&root).unwrap().project_assignments["source:abc"],
            "personal"
        );
        let mut updated = read(&root).unwrap();
        updated.profiles[0].name = "Updated creator".into();
        updated.profiles[0].subtitle_style = Some(engine_core::SubtitleStyle {
            preset: "boxed".into(),
            bottom_margin: 55,
            ..Default::default()
        });
        save(&root, updated).unwrap();
        assert_eq!(read(&root).unwrap().profiles[0].name, "Updated creator");
        assert_eq!(
            read(&root).unwrap().profiles[0]
                .subtitle_style
                .as_ref()
                .unwrap()
                .preset,
            "boxed"
        );
        let mut invalid_style = read(&root).unwrap();
        invalid_style.profiles[0]
            .subtitle_style
            .as_mut()
            .unwrap()
            .font_size = 500;
        assert!(save(&root, invalid_style).is_err());
        let mut invalid = Workspace::default();
        invalid
            .project_assignments
            .insert("source:abc".into(), "missing".into());
        assert!(save(&root, invalid).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
