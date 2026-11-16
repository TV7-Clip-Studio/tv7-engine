use serde::{Deserialize, Serialize};
/// Percentages are bounded before rendering or saving a clip recipe.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemixSettings {
    pub treatment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<crate::remix_plan::RemixPlan>,
    #[serde(rename = "artPack", default = "default_art_pack")]
    pub art_pack: String,
    #[serde(rename = "artIntensity", default = "default_percent")]
    pub art_intensity: u8,
    #[serde(rename = "stickerSize", default = "default_percent")]
    pub sticker_size: u8,
    #[serde(rename = "stickerDensity", default = "default_sticker_density")]
    pub sticker_density: String,
    #[serde(default)]
    pub palette: Option<String>,
    pub effect: String,
    pub intensity: u8,
    pub frequency: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u16,
    #[serde(rename = "breathingRoom", default = "default_breathing_room")]
    pub breathing_room: u8,
    pub background: String,
    #[serde(rename = "backgroundIntensity")]
    pub background_intensity: u8,
    pub sound: String,
    #[serde(rename = "soundVolume")]
    pub sound_volume: u8,
    #[serde(rename = "dialogueVolume")]
    pub dialogue_volume: u8,
}
fn default_art_pack() -> String {
    "none".into()
}
fn default_percent() -> u8 {
    50
}
fn default_sticker_density() -> String {
    "sparse".into()
}
fn default_breathing_room() -> u8 {
    3
}
impl RemixSettings {
    pub fn valid(&self) -> bool {
        self.plan.as_ref().is_none_or(|p| p.valid(180.0))
            && matches!(self.treatment.as_str(), "comedy" | "editorial" | "retro")
            && self.palette.as_ref().is_none_or(|p| {
                matches!(p.as_str(), "yellow" | "cyan" | "pink" | "lime" | "orange")
            })
            && matches!(
                self.art_pack.as_str(),
                "none" | "comic" | "collage" | "neon" | "psychedelic" | "minimal"
            )
            && matches!(self.sticker_density.as_str(), "off" | "sparse" | "normal")
            && matches!(
                self.effect.as_str(),
                "none" | "impact" | "glitch" | "shockwave" | "freeze" | "whip"
            )
            && matches!(self.frequency.as_str(), "sparse" | "medium" | "frequent")
            && matches!(self.duration_ms, 150 | 300 | 500)
            && matches!(self.breathing_room, 2..=4)
            && matches!(self.background.as_str(), "blur" | "tunnel" | "kaleidoscope")
            && matches!(self.sound.as_str(), "none" | "impact" | "digital" | "sweep")
            && [
                self.intensity,
                self.background_intensity,
                self.sound_volume,
                self.dialogue_volume,
                self.art_intensity,
                self.sticker_size,
            ]
            .iter()
            .all(|v| *v <= 100)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_recipe_round_trips_and_rejects_unsafe_controls() {
        let value = serde_json::json!({"treatment":"retro","effect":"glitch","intensity":45,"frequency":"medium","durationMs":300,"background":"tunnel","backgroundIntensity":35,"sound":"digital","soundVolume":15,"dialogueVolume":100});
        let mut settings: RemixSettings = serde_json::from_value(value.clone()).unwrap();
        assert!(settings.valid());
        assert_eq!(settings.breathing_room, 3);
        let mut expected = value;
        expected["breathingRoom"] = serde_json::json!(3);
        expected["palette"] = serde_json::Value::Null;
        expected["artPack"] = serde_json::json!("none");
        expected["artIntensity"] = serde_json::json!(50);
        expected["stickerSize"] = serde_json::json!(50);
        expected["stickerDensity"] = serde_json::json!("sparse");
        assert_eq!(serde_json::to_value(&settings).unwrap(), expected);
        settings.palette = Some("purple".into());
        assert!(!settings.valid());
        settings.palette = Some("pink".into());
        assert!(settings.valid());
        settings.art_intensity = 101;
        assert!(!settings.valid());
        settings.art_intensity = 50;
        settings.art_pack = "unknown".into();
        assert!(!settings.valid());
        settings.art_pack = "comic".into();
        settings.breathing_room = 1;
        assert!(!settings.valid());
        settings.breathing_room = 3;
        settings.sound_volume = 101;
        assert!(!settings.valid());
        settings.sound_volume = 20;
        settings.effect = "arbitrary".into();
        assert!(!settings.valid());
        settings.effect = "impact".into();
        settings.duration_ms = 10000;
        assert!(!settings.valid());
    }
}
