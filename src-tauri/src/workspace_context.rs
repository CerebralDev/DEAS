//! Bounded reference context and non-executing authoring proposals.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub area: String,
    pub captured_at: u64,
    pub values: BTreeMap<String, Value>,
}
const KEYS: &[&str] = &[
    "positive",
    "negative",
    "pipeline",
    "checkpoint",
    "family",
    "steps",
    "cfg",
    "sampler",
    "scheduler",
    "width",
    "height",
    "seed",
    "loras",
    "brief",
    "resource",
    "triggers",
    "description",
    "jobStatus",
];
pub fn clean(text: &str) -> String {
    let filtered = text
        .lines()
        .map(|line| {
            let l = line.to_ascii_lowercase();
            if [
                "authorization:",
                "authorization=",
                "api_key",
                "api-key",
                "apikey",
                "token=",
                "token:",
            ]
            .iter()
            .any(|k| l.contains(k))
            {
                "[redacted]".to_string()
            } else {
                // Paths may contain spaces. Redact the rest of their line rather
                // than leaking the remaining directory/file components as words.
                let drive = line.find(":\\").or_else(|| line.find(":/"));
                let unc = line.find("\\\\");
                if let Some(at) = drive
                    .map(|n| line[..n].char_indices().next_back().map_or(0, |(i, _)| i))
                    .or(unc)
                {
                    format!("{}[redacted]", &line[..at])
                } else if line
                    .split_whitespace()
                    .any(|word| word.trim_start_matches(['\"', '\'', '(']).starts_with('/'))
                {
                    "[redacted]".to_string()
                } else {
                    line.to_string()
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    filtered
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let token = part.trim();
            let low = token.to_ascii_lowercase();
            let unsafe_text = token.contains(":\\")
                || token.contains(":/")
                || token.starts_with("\\\\")
                || token.starts_with('/')
                || low.contains("api_key")
                || low.contains("apikey")
                || low.contains("authorization")
                || low.starts_with("sk-")
                || low.contains("token=");
            if unsafe_text {
                format!("[redacted]{}", &part[token.len()..])
            } else {
                part.to_string()
            }
        })
        .collect::<String>()
        .chars()
        .take(6000)
        .collect()
}
pub fn validate_context(raw: &str) -> Result<String, String> {
    if raw.len() > 32000 {
        return Err("Workspace context exceeds its limit".into());
    }
    // Older clients used plain text; retain that boundary with redaction and bounds.
    if !raw.trim_start().starts_with('{') {
        return Ok(clean(raw));
    }
    let mut s: Snapshot = serde_json::from_str(raw).map_err(|_| "Invalid workspace context")?;
    if s.version != 1
        || ![
            "Studio",
            "Forge",
            "Library",
            "Monitoring",
            "Backend",
            "Analysis",
            "Video",
            "Logs",
            "ComfyUI",
        ]
        .contains(&s.area.as_str())
        || s.values.len() > KEYS.len()
    {
        return Err("Unsupported workspace context".into());
    }
    for (key, value) in &mut s.values {
        if !KEYS.contains(&key.as_str()) {
            return Err("Unapproved workspace context field".into());
        }
        match value {
            Value::String(text) => *text = clean(text),
            Value::Number(_) | Value::Bool(_) | Value::Null => {}
            _ => return Err("Nested context and workflow graphs are not allowed".into()),
        }
    }
    serde_json::to_string(&s).map_err(|_| "Cannot encode workspace context".into())
}

#[tauri::command]
pub fn native_workspace_validate_suggestion(
    proposed: BTreeMap<String, Value>,
    pipeline: String,
) -> Result<BTreeMap<String, Value>, String> {
    if proposed.is_empty() || proposed.len() > 4 {
        return Err("Empty or oversized suggestion".into());
    }
    for (key, value) in &proposed {
        let valid = match key.as_str() {
            "positive" | "negative" => {
                value
                    .as_str()
                    .is_some_and(|v| v.chars().count() <= 32000 && !v.contains('\0'))
                    && !["t2m", "cover"].contains(&pipeline.as_str())
            }
            "steps" => {
                ["t2i", "i2i"].contains(&pipeline.as_str())
                    && value.as_u64().is_some_and(|n| (1..=10000).contains(&n))
            }
            "cfg" => {
                ["t2i", "i2i"].contains(&pipeline.as_str())
                    && value
                        .as_f64()
                        .is_some_and(|n| n.is_finite() && (0.0..=100.0).contains(&n))
            }
            _ => false,
        };
        if !valid {
            return Err(format!(
                "Unsupported suggestion field for current pipeline: {key}"
            ));
        }
    }
    Ok(proposed)
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_and_rejects_graphs() {
        let raw = r#"{"version":1,"area":"Studio","capturedAt":1,"values":{"positive":"Cat C:\\private\\file sk-secret", "steps":8}}"#;
        let s = validate_context(raw).unwrap();
        assert!(!s.contains("private"));
        assert!(!s.contains("sk-secret"));
        assert!(s.contains("Cat"));
        assert_eq!(clean("é:/private name.png"), "[redacted]");
        assert_eq!(
            clean(r"Cat C:\Users\Jane Doe\private image.png"),
            "Cat [redacted]"
        );
        assert_eq!(clean("Photo '/home/Jane Doe/private.png'"), "[redacted]");
        assert!(validate_context(
            r#"{"version":1,"area":"Studio","capturedAt":1,"values":{"workflow":{}}}"#
        )
        .is_err());
    }
    #[test]
    fn proposals_never_accept_execution_or_video_schedule_changes() {
        assert!(native_workspace_validate_suggestion(
            BTreeMap::from([("render".into(), Value::Bool(true))]),
            "t2i".into()
        )
        .is_err());
        assert!(native_workspace_validate_suggestion(
            BTreeMap::from([("steps".into(), Value::from(8))]),
            "i2v".into()
        )
        .is_err());
        assert!(native_workspace_validate_suggestion(
            BTreeMap::from([("steps".into(), Value::from(8))]),
            "t2i".into()
        )
        .is_ok());
    }
}
