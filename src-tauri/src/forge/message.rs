//! The user turn for an image-model write. Ported from PromptForge's
//! buildUserMessage, for the controls the image models offer: settings are
//! labelled as context that must never reach the output, and the rules about
//! how to write (creativity, detail) are lines of their own.

use super::pack::{Pack, PackModel};
use serde_json::Value;

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Controls {
    /// 1..=10 along the creativity ladder.
    pub creativity: Option<u8>,
    /// 1..=10.
    pub detail: Option<u8>,
    /// 0..=10; 0 (the default) sends nothing.
    pub lighting: Option<u8>,
    /// The manual length: off unless set.
    pub max_length: Option<MaxLength>,
    /// Reference pictures with their roles (image models).
    pub image_references: Vec<super::refs::ImageRef>,
    /// The Layout Studio's canvas and the boxes drawn on it (MOSS Layout JSON).
    pub layout: Option<StudioLayout>,
    pub aspect_ratio: Option<String>,
    /// The consumed effective canvas. N/A leaves this absent; Auto/Custom are resolved by the UI.
    pub canvas: Option<Canvas>,
    /// A registry preset name, or "none" for no quality tags.
    pub quality_preset: Option<String>,
    pub style: Option<String>,
    /// Kept verbatim in the positive (a LoRA trigger, an artist tag).
    pub positive: Option<String>,
    /// Kept verbatim in the negative.
    pub negative: Option<String>,
    /// Tags picked from the database, kept exactly.
    pub tags: Vec<String>,
    pub camera_angle: Option<String>,
    pub camera_shot: Option<String>,
    pub camera_light: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width: f64,
    pub height: f64,
}

pub(crate) const CANVAS_ERROR: &str = "Invalid Prompt Forge canvas";

impl Canvas {
    pub(crate) fn validate(self, layout: bool) -> Result<(), String> {
        let (minimum, multiple) = if layout { (128.0, 16.0) } else { (64.0, 8.0) };
        if [self.width, self.height].iter().any(|edge| {
            !edge.is_finite()
                || edge.fract() != 0.0
                || !(minimum..=8192.0).contains(edge)
                || *edge % multiple != 0.0
        }) || self.width * self.height > 16_777_216.0
        {
            return Err(format!(
                "{CANVAS_ERROR}: dimensions must be {minimum}–8192, aligned to {multiple}, within 16777216 pixels"
            ));
        }
        Ok(())
    }
}

/// Numeric ratio syntax, independent of a preset table or canvas-sizing algorithm.
pub(crate) fn numeric_aspect(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once(':')?;
    let number = |part: &str| {
        if part.is_empty() || part.len() > 4 || !part.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        part.parse::<u32>().ok().filter(|n| (1..=8192).contains(n))
    };
    Some((number(width)?, number(height)?))
}

/// Fixed, bounded validation diagnostics; never forward aspect text, pictures or request bodies.
pub(crate) fn report_canvas_failure(operation: &str) {
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };
    static LAST: Mutex<[Option<Instant>; 4]> = Mutex::new([None; 4]);
    let index = match operation {
        "write" => 0,
        "revise" => 1,
        "layout" => 2,
        "h3" => 3,
        _ => return,
    };
    let mut last = LAST.lock().unwrap_or_else(|error| error.into_inner());
    if last[index].is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
        return;
    }
    last[index] = Some(Instant::now());
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "ERROR: origin=prompt-forge operation={operation} canvas validation failed before inference; inspect the operation result (repeats limited to once per minute)"
        ),
    );
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct StudioLayout {
    pub width: f64,
    pub height: f64,
    pub items: Vec<DrawnItem>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct DrawnItem {
    /// What the person called the box in the studio.
    pub name: String,
    pub label: String,
    pub caption: String,
    pub text: String,
    /// `[y0, x0, y1, x1]`, centred on the canvas.
    pub bbox: Vec<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MaxLength {
    /// "words" (asked for, not enforced) or "tokens" (a hard cut-off).
    pub unit: String,
    pub value: u32,
}

const LENGTH_MIN: u32 = 10;
const LENGTH_MAX: u32 = 4000;
/// How far above a word request the hard ceiling sits: the word count is one segment, the
/// ceiling caps the whole answer (negative, reasoning). It stops a runaway, never a real answer.
const WORD_LIMIT_HEADROOM: f64 = 3.0;

fn length(controls: &Controls) -> Result<Option<&MaxLength>, String> {
    let Some(l) = &controls.max_length else {
        return Ok(None);
    };
    if l.unit != "words" && l.unit != "tokens" {
        return Err("Length must be in words or tokens.".into());
    }
    if !(LENGTH_MIN..=LENGTH_MAX).contains(&l.value) {
        return Err(format!(
            "Length must be between {LENGTH_MIN} and {LENGTH_MAX} {}; got {}.",
            l.unit, l.value
        ));
    }
    Ok(Some(l))
}

/// The hard token ceiling for one generation, or None for none.
pub fn token_budget(controls: &Controls) -> Result<Option<u32>, String> {
    Ok(length(controls)?.map(|l| {
        if l.unit == "tokens" {
            l.value
        } else {
            (f64::from(l.value) / 0.75 * WORD_LIMIT_HEADROOM).round() as u32
        }
    }))
}

fn title_case(v: &str) -> String {
    v.replace('_', " ")
        .split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn creativity_name(pack: &Pack, model: &PackModel, notch: Option<u8>) -> String {
    notch
        .and_then(|n| {
            pack.creativity_order
                .get(usize::from(n).checked_sub(1)?)
                .cloned()
        })
        .or_else(|| model.str("default_creativity").map(String::from))
        .unwrap_or_else(|| "balanced".into())
}

/// (temperature, top_p) for the creativity notch, falling back to balanced.
pub fn sampling(pack: &Pack, model: &PackModel, notch: Option<u8>) -> (Option<f64>, Option<f64>) {
    let table = model.entry.get("creativity_presets");
    let name = creativity_name(pack, model, notch);
    let entry = table.and_then(|t| t.get(&name).or_else(|| t.get("balanced")));
    (
        entry
            .and_then(|e| e.get("temperature"))
            .and_then(Value::as_f64),
        entry.and_then(|e| e.get("top_p")).and_then(Value::as_f64),
    )
}

/// A style pasted word for word after writing (`style_placement: append`).
pub fn pasted_style(model: &PackModel, controls: &Controls) -> Option<String> {
    if model.str("style_placement") != Some("append") {
        return None;
    }
    let name = controls.style.as_deref().or(model.str("default_style"))?;
    if name == "None" {
        return None;
    }
    model
        .entry
        .get("styles")?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// The camera picks' payloads in this model's dialect.
pub fn camera_picks(model: &PackModel, controls: &Controls) -> Vec<String> {
    let dialect = if model.tag_style().is_some() {
        "tags"
    } else {
        "prose"
    };
    [
        ("camera_angles", &controls.camera_angle),
        ("camera_shots", &controls.camera_shot),
        ("camera_lights", &controls.camera_light),
    ]
    .iter()
    .filter_map(|(table, pick)| {
        model
            .entry
            .get(*table)?
            .get(pick.as_deref()?)?
            .get(dialect)?
            .as_str()
            .map(String::from)
    })
    .collect()
}

pub fn build(
    pack: &Pack,
    model: &PackModel,
    brief: &str,
    controls: &Controls,
    negative_written: bool,
    character_text: &str,
    canvas_line: Option<String>,
) -> String {
    let mut lines = vec![format!("Brief: \"{}\"", brief.trim())];
    let tag_model = model.tag_style().is_some();
    let style_block = pasted_style(model, controls);

    let mut parts = vec![];
    if let Some(preset) = controls
        .quality_preset
        .as_deref()
        .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("none"))
    {
        let ladder: Vec<String> = model
            .entry
            .get("quality_presets")
            .and_then(|q| q.get(preset))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        parts.push(if ladder.is_empty() {
            format!("Preset: {}", title_case(preset))
        } else {
            format!("Preset: {} — use exactly these quality tags and no others, at the front of the prompt: {}", title_case(preset), ladder.join(", "))
        });
    }
    if let Some(style) = controls
        .style
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "None" && style_block.is_none())
    {
        let terms: Vec<String> = model
            .entry
            .get("styles")
            .and_then(|s| s.get(style))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        parts.push(match (terms.is_empty(), tag_model) {
            (true, _) => format!("Style: {}", title_case(style)),
            (false, true) => format!("Style: {style} — include these tags: {}", terms.join(", ")),
            (false, false) => format!(
                "Style: {style} — carry this look through the description: {}",
                terms.join(", ")
            ),
        });
    }
    if let Some(aspect) = controls
        .aspect_ratio
        .as_deref()
        .filter(|a| !a.is_empty() && *a != "N/A")
    {
        parts.push(format!("Aspect: {}", title_case(aspect)));
    }
    let creativity = creativity_name(pack, model, controls.creativity);
    parts.push(format!("Creativity: {}", title_case(&creativity)));
    lines.push(format!(
        "Settings (context for how to write, never text to include): {}",
        parts.join(" · ")
    ));

    if let Some(block) = &style_block {
        let name = controls
            .style
            .as_deref()
            .or(model.str("default_style"))
            .unwrap_or("");
        lines.push(format!(
            "Style: {name}. This style block is added to the end of your prompt word for word, after you finish:\n\"{block}\"\n\
             Write the scene so it agrees with that style, but do not restate, paraphrase or end on it - it is already there."
        ));
    }
    if let Some(canvas) = canvas_line {
        lines.push(canvas);
    }

    if let Some(rule) = model
        .entry
        .get("creativity_presets")
        .and_then(|t| t.get(&creativity))
        .and_then(|e| e.get("rule"))
        .and_then(Value::as_str)
    {
        lines.push(format!("Creativity - {}. {rule}", title_case(&creativity)));
    }
    if let Some(table) = model.entry.get("detail_levels").and_then(Value::as_object) {
        let fallback = model
            .entry
            .get("default_detail")
            .and_then(Value::as_u64)
            .unwrap_or(5);
        let level = controls
            .detail
            .map(u64::from)
            .filter(|l| table.contains_key(&l.to_string()))
            .unwrap_or(fallback);
        if let Some(entry) = table.get(&level.to_string()) {
            if let Some(rule) = entry.get("rule").and_then(Value::as_str) {
                let of = table.keys().filter(|k| k.parse::<u64>().is_ok()).count();
                let label = entry
                    .get("label")
                    .and_then(Value::as_str)
                    .map_or(level.to_string(), String::from);
                lines.push(format!("Detail level {level} of {of} - {label}. {rule}"));
            }
        }
    }
    // Lighting only when somebody asked for it: level 0 carries no rule.
    if let Some(table) = model
        .entry
        .get("lighting_levels")
        .and_then(Value::as_object)
    {
        let fallback = model
            .entry
            .get("default_lighting")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let level = controls
            .lighting
            .map(u64::from)
            .filter(|l| table.contains_key(&l.to_string()))
            .unwrap_or(fallback);
        let entry = table.get(&level.to_string());
        if let Some(rule) = entry.and_then(|e| e.get("rule")).and_then(Value::as_str) {
            let of = table.keys().filter(|k| k.parse::<u64>().is_ok()).count() - 1;
            let label = entry
                .and_then(|e| e.get("label"))
                .and_then(Value::as_str)
                .map_or(level.to_string(), String::from);
            let standing = model
                .str("lighting_rule")
                .or(pack.defaults.get("lighting_rule").and_then(Value::as_str))
                .map(|r| format!(" {r}"))
                .unwrap_or_default();
            lines.push(format!(
                "Lighting level {level} of {of} - {label}. {rule}{standing}"
            ));
        }
    }
    if let Ok(Some(l)) = length(controls) {
        lines.push(if l.unit == "words" {
            format!("Length: approximately {} words. This takes precedence over the detail level above and over any length or tag count this model's format normally expects.", l.value)
        } else {
            format!("Length: your output will be cut off at {} tokens, roughly {} words. Finish inside that. This takes precedence over the detail level above and over any length or tag count this model's format normally expects.", l.value, (f64::from(l.value) * 0.75).round())
        });
    }
    if controls
        .quality_preset
        .as_deref()
        .is_some_and(|p| p.eq_ignore_ascii_case("none"))
    {
        lines.push(
            "Quality preset: none. Add no quality, score or aesthetic tags of your own.".into(),
        );
    }
    if !controls.tags.is_empty() {
        lines.push(format!(
            "Required tags (keep all of these, exactly as written): {}",
            controls.tags.join(", ")
        ));
    }
    if let Some(p) = controls
        .positive
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        lines.push(format!(
            "Must appear in the positive prompt, exactly as written: {p}"
        ));
    }
    if let Some(n) = controls
        .negative
        .as_deref()
        .filter(|n| !n.trim().is_empty() && negative_written)
    {
        lines.push(format!(
            "Must appear in the negative prompt, exactly as written: {n}"
        ));
    }
    let camera = camera_picks(model, controls);
    if !camera.is_empty() {
        lines.push(if tag_model {
            format!("Camera: include these tags: {}", camera.join(", "))
        } else {
            format!("Camera: frame the picture as {}.", camera.join(", and "))
        });
    }
    if !character_text.is_empty() {
        lines.push(String::new());
        lines.push(character_text.to_string());
    }
    lines.join("\n")
}
