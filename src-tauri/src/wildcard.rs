//! The original DaSiWa node owns prompt assembly; this adapter only validates its inputs and reads its library.
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};

pub(crate) const NODE: &str = "DaSiWa_WildcardPresetPromptBuilder";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    enabled: bool,
    weight: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    style: String,
    seed: u32,
    reroll: u32,
    token_budget: u32,
    auto_reroll: bool,
    selection_state: BTreeMap<String, Selection>,
    /// JSON `{variable: [choices]}` for `$name` prompt variables (node pack 0.5.39).
    #[serde(default)]
    variables_json: Option<String>,
    /// JSON `{"name:index": "pinned choice"}`; pins win over the seed pick.
    #[serde(default)]
    variable_selections: Option<String>,
}
pub(crate) fn validate(value: &Value) -> Result<(), String> {
    let r: Rules = serde_json::from_value(value.clone()).map_err(|_| "Invalid wildcard rules")?;
    if !["Booru", "Natural Language"].contains(&r.style.as_str())
        || r.reroll > 0x7fffffff
        || !(1..=10000).contains(&r.token_budget)
        || r.selection_state.len() > 4096
        || r.variables_json
            .as_ref()
            .is_some_and(|v| v.len() > 1_000_000)
        || r.variable_selections
            .as_ref()
            .is_some_and(|v| v.len() > 256_000)
        || r.selection_state.iter().any(|(k, s)| {
            k.is_empty()
                || k.encode_utf16().count() > 512
                || k.contains('\0')
                || !s.weight.is_finite()
                || s.weight.abs() > 10.0
        })
    {
        return Err("Invalid wildcard rules".into());
    }
    // Deserialization checks these types, including the node's uint32 seed range.
    let _ = (
        r.seed,
        r.auto_reroll,
        r.selection_state.values().any(|s| s.enabled),
    );
    Ok(())
}

/// The node resolves validated rules outside the graph: `{positive, negative}` as the run would use them.
#[tauri::command]
pub async fn native_wildcard_build(base_url: String, rules: Value) -> Result<Value, String> {
    validate(&rules)?;
    let url = crate::validate_base_url(&base_url)?
        .join("dasiwa/wildcard-preset-prompt-builder/build")
        .map_err(|_| "Invalid backend URL")?;
    // Packs older than 0.5.39 reject the optional variable fields; empty ones carry no information.
    let mut payload = rules;
    if let Some(map) = payload.as_object_mut() {
        for key in ["variables_json", "variable_selections"] {
            if map
                .get(key)
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.trim() == "{}")
            {
                map.remove(key);
            }
        }
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Cannot create wildcard client")?;
    let response = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|_| "Wildcard build request failed")?;
    if !response.status().is_success() {
        return Err(format!(
            "{} Wildcard build unavailable",
            response.status().as_u16()
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Invalid wildcard build JSON")?;
    if !(value["positive"].is_string() && value["negative"].is_string()) {
        return Err("Invalid wildcard build result".into());
    }
    Ok(value)
}

/// Return raw JSON to retain the library's category order without changing serde_json globally.
#[tauri::command]
pub async fn native_wildcard_library(base_url: String) -> Result<String, String> {
    let base = crate::validate_base_url(&base_url)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Cannot create wildcard client")?;
    let work = async {
        let url = base
            .join("dasiwa/wildcard-preset-prompt-builder/library")
            .map_err(|_| "Invalid backend URL")?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|_| "Wildcard library request failed")?;
        if !response.status().is_success() {
            return Err(format!(
                "{} Wildcard library unavailable",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Wildcard library transfer failed")?
        {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("Wildcard library exceeds byte limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(bytes).map_err(|_| "Invalid wildcard library encoding")?;
        let text = crate::strip_bom(&text).to_string();
        let value: Value =
            serde_json::from_str(&text).map_err(|_| "Invalid wildcard library JSON")?;
        if !value["categories"].is_object() {
            return Err("Invalid wildcard library categories".into());
        }
        Ok(text)
    };
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| "Wildcard library timed out")?
}
