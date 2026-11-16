use serde::{Deserialize, Serialize};

pub const TOOLS: &str = "hook: scroll-stopping clickbait opening at 0.0..0.3s, value banner/freeze_banner/flash, short punchy text required (<=60 chars), duration 0.6..1.8s, amount 40..100; zoom: camera punch-in; impact: punch zoom; freeze: visual freeze, dialogue continues; glitch: RGB split; shockwave: distortion pulse; whip: lateral motion; spotlight: dark edge focus; sound: value impact/digital/sweep/pop/rimshot/record_stop; sticker: value food/football/magic/car/ghost/money; meme: value double_take/awkward/sus/stonks (original graphic reaction cards, no external footage); callout: original short text; background: value tunnel/kaleidoscope. Percent amount 0..100; durations 0.15..2 seconds; text <=60 chars. Every event needs a short grounded reason. Sound <=30%.";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemixEvent {
    pub id: String,
    pub at: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub asset_start: f64,
    pub duration: f64,
    pub tool: String,
    pub amount: u8,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub text: String,
    pub reason: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemixPlan {
    pub version: u8,
    pub direction: String,
    pub events: Vec<RemixEvent>,
}
impl RemixPlan {
    pub fn valid(&self, duration: f64) -> bool {
        if self.version != 1
            || self.direction.trim().is_empty()
            || self.direction.chars().count() > 400
            || self.events.len() > 16
        {
            return false;
        }
        let hooks: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.enabled && e.tool == "hook")
            .collect();
        if hooks.len() > 1 {
            return false;
        }
        let cuts: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.enabled && e.tool == "cutaway")
            .collect();
        if cuts.len() > 2
            || cuts.iter().map(|e| e.duration).sum::<f64>() > duration * 0.25
            || cuts
                .iter()
                .enumerate()
                .any(|(i, a)| cuts.iter().skip(i + 1).any(|b| (a.at - b.at).abs() < 0.001))
        {
            return false;
        }
        let mut ids = std::collections::HashSet::new();
        let mut spans: Vec<(f64, f64)> = vec![];
        for e in &self.events {
            let is_hook = e.tool == "hook";
            let min_at = if is_hook { 0.0 } else { 1.0 };
            let max_dur = if e.tool == "cutaway" {
                3.0
            } else if is_hook {
                1.8
            } else {
                2.0
            };
            let min_dur = if is_hook { 0.6 } else { 0.15 };
            if e.id.is_empty()
                || e.id.len() > 64
                || !ids.insert(&e.id)
                || !e.at.is_finite()
                || !e.duration.is_finite()
                || e.at < min_at
                || (is_hook && e.at > 0.3)
                || !(min_dur..=max_dur).contains(&e.duration)
                || !e.asset_start.is_finite()
                || e.asset_start < 0.0
                || e.asset_start > 8.0
                || (is_hook && e.asset_start > 0.0)
                || e.at + e.duration > duration - 1.0
                || e.amount > 100
                || e.text.chars().count() > 60
                || e.reason.trim().is_empty()
                || e.reason.chars().count() > 240
            {
                return false;
            }
            let choice = match e.tool.as_str() {
                "hook" => {
                    e.amount >= 40
                        && !e.text.trim().is_empty()
                        && matches!(e.value.as_str(), "banner" | "freeze_banner" | "flash")
                }
                "zoom" | "impact" | "freeze" | "glitch" | "shockwave" | "whip" | "spotlight" => {
                    e.value.is_empty()
                }
                "sound" => {
                    e.amount <= 30
                        && matches!(
                            e.value.as_str(),
                            "impact" | "digital" | "sweep" | "pop" | "rimshot" | "record_stop"
                        )
                }
                "sticker" => matches!(
                    e.value.as_str(),
                    "food" | "football" | "magic" | "car" | "ghost" | "money"
                ),
                "cutaway" => {
                    e.value.starts_with("meme-")
                        && e.value.len() <= 80
                        && e.value
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-')
                }
                "meme" => matches!(
                    e.value.as_str(),
                    "double_take" | "awkward" | "sus" | "stonks"
                ),
                "callout" => !e.text.trim().is_empty() && e.value.is_empty(),
                "background" => matches!(e.value.as_str(), "tunnel" | "kaleidoscope"),
                _ => false,
            };
            if !choice {
                return false;
            }
            if e.enabled && !is_hook {
                spans.push((e.at, e.at + e.duration));
            }
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut total = 0.0;
        let mut merged: Vec<(f64, f64)> = vec![];
        for (a, b) in spans {
            if let Some(last) = merged.last_mut() {
                if a <= last.1 {
                    last.1 = last.1.max(b);
                    continue;
                }
                if a - last.1 < 2.0 - 0.001 {
                    return false;
                }
            }
            merged.push((a, b));
        }
        for (a, b) in merged {
            total += b - a;
        }
        total <= duration * 0.35
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_timing_assets_and_clean_space() {
        let mut p:RemixPlan=serde_json::from_value(serde_json::json!({"version":1,"direction":"Dry comedy","events":[{"id":"h","at":0.0,"duration":1.1,"tool":"hook","amount":80,"value":"freeze_banner","text":"¿ASADO A LAS 9:30?","reason":"Opening hook"},{"id":"a","at":3.0,"duration":0.5,"tool":"sound","amount":20,"value":"rimshot","reason":"Punchline"}]})).unwrap();
        assert!(p.valid(12.0));
        p.events[0].at = 0.5;
        assert!(!p.valid(12.0));
        p.events[0].at = 0.0;
        p.events[0].text = "".into();
        assert!(!p.valid(12.0));
        p.events[0].text = "¿ASADO A LAS 9:30?".into();
        p.events[1].value = "https://bad".into();
        assert!(!p.valid(12.0));
        p.events[1].value = "rimshot".into();
        p.events[1].at = f64::NAN;
        assert!(!p.valid(12.0));
        p.events[1].at = 3.0;
        p.events[1].amount = 50;
        assert!(!p.valid(12.0));
        p.events[1].amount = 20;
        let mut e = p.events[1].clone();
        e.id = "b".into();
        e.at = 4.0;
        p.events.push(e);
        assert!(!p.valid(12.0));
        p.events[2].at = 6.0;
        assert!(p.valid(12.0));
        assert!(!p.valid(6.0));
    }
}
