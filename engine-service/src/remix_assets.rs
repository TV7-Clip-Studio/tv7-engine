use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemeAsset {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub attribution: String,
    pub source_url: String,
    pub license: String,
    pub path: String,
    pub duration: f64,
}
fn lock() -> &'static Mutex<()> {
    static M: OnceLock<Mutex<()>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(()))
}
fn index(root: &Path) -> PathBuf {
    root.join(".remix-assets.json")
}
pub fn list(root: &Path) -> Result<Vec<MemeAsset>, String> {
    let _g = lock().lock().map_err(|e| e.to_string())?;
    read(root)
}
fn read(root: &Path) -> Result<Vec<MemeAsset>, String> {
    let path = index(root);
    if !path.exists() {
        return Ok(vec![]);
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn save(root: &Path, asset: MemeAsset) -> Result<(), String> {
    let _g = lock().lock().map_err(|e| e.to_string())?;
    let mut rows = read(root)?;
    if rows.len() >= 100 {
        return Err("Reaction library is full (100 clips)".into());
    }
    rows.push(asset);
    let p = index(root);
    let tmp = p.with_extension("pending.json");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&rows).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(tmp, p).map_err(|e| e.to_string())
}
pub fn owned_path(root: &Path, asset: &MemeAsset) -> Result<PathBuf, String> {
    let dir = fs::canonicalize(root.join("remix-assets")).map_err(|e| e.to_string())?;
    let p = fs::canonicalize(&asset.path)
        .map_err(|_| format!("Reaction media missing: {}", asset.title))?;
    if !p.starts_with(dir) || !p.is_file() {
        return Err("Reaction media must belong to the local library".into());
    }
    Ok(p)
}
pub fn validate_plan(
    root: &Path,
    plan: &crate::remix_plan::RemixPlan,
) -> Result<Vec<MemeAsset>, String> {
    let all = list(root)?;
    let mut used = vec![];
    for e in plan
        .events
        .iter()
        .filter(|e| e.enabled && e.tool == "cutaway")
    {
        let a = all
            .iter()
            .find(|a| a.id == e.value)
            .ok_or_else(|| format!("Unknown reaction clip: {}", e.value))?;
        owned_path(root, a)?;
        if e.asset_start + e.duration > a.duration + 0.001 {
            return Err(format!("Reaction trim exceeds {}", a.title));
        }
        if !used.iter().any(|v: &MemeAsset| v.id == a.id) {
            used.push(a.clone());
        }
    }
    Ok(used)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_assets_bad_trims_and_outside_media() {
        let root = std::env::temp_dir().join(format!("tv7-meme-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("remix-assets")).unwrap();
        let p = root.join("remix-assets/a.mp4");
        fs::write(&p, b"fixture").unwrap();
        let a = MemeAsset {
            id: "meme-test".into(),
            title: "Reaction".into(),
            tags: vec!["surprise".into()],
            attribution: "Creator".into(),
            license: "Creator owned".into(),
            source_url: "".into(),
            path: p.to_string_lossy().into_owned(),
            duration: 2.0,
        };
        save(&root, a.clone()).unwrap();
        let mut plan:crate::remix_plan::RemixPlan=serde_json::from_value(serde_json::json!({"version":1,"direction":"Reaction","events":[{"id":"e","at":3.0,"duration":1.5,"tool":"cutaway","value":"meme-test","amount":0,"reason":"Punchline"}]})).unwrap();
        assert!(plan.valid(12.0));
        assert!(validate_plan(&root, &plan).is_ok());
        plan.events[0].asset_start = 1.0;
        assert!(validate_plan(&root, &plan).is_err());
        plan.events[0].asset_start = 0.0;
        plan.events[0].value = "meme-unknown".into();
        assert!(validate_plan(&root, &plan).is_err());
        let mut outside = a;
        outside.path = root.join("outside.mp4").to_string_lossy().into_owned();
        fs::write(&outside.path, b"outside").unwrap();
        assert!(owned_path(&root, &outside).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
