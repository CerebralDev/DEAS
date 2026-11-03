//! The `===SEGMENT: Label===` output contract every Forge prompt file states.
//! Ported from PromptForge server/segments.mjs. A reply that breaks the
//! contract is an error naming what went wrong, never raw text shown as if it
//! were a prompt.

pub const REFUSED: &str = "Refused";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub label: String,
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    NoSegments,
    Unknown(Vec<String>),
    Missing(Vec<String>),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let quoted = |l: &[String]| {
            l.iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Self::NoSegments => write!(f, "The model returned no ===SEGMENT: markers. Its instructions define an exact output format that it did not follow."),
            Self::Unknown(l) => write!(f, "The model wrote {} not in its format: {}.", if l.len() == 1 { "a segment" } else { "segments" }, quoted(l)),
            Self::Missing(l) => write!(f, "The model left out {}: {}. Retrying usually fixes it.", if l.len() == 1 { "a segment" } else { "segments" }, quoted(l)),
        }
    }
}

fn delimiter(line: &str) -> Option<&str> {
    let label = line
        .trim_end()
        .strip_prefix("===SEGMENT:")?
        .strip_suffix("===")?
        .trim();
    (!label.is_empty()).then_some(label)
}

/// Two things a model copies into a prompt that are never part of one: a bare
/// `===` line left over from the delimiters, and the tag search's
/// `[retired by danbooru]` flag (`tools.rs`).
fn tidy_body(body: &str) -> String {
    let kept: Vec<&str> = body
        .lines()
        .filter(|line| {
            let line = line.trim();
            line.len() < 3 || line.chars().any(|c| c != '=')
        })
        .collect();
    kept.join("\n")
        .split("[retired by danbooru]")
        .map(|part| part.trim_matches(' '))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Labels chat-tuned models give their explaining notes when they rename the
/// segment themselves (Qwen3.x, Hermes-family and similar habits). Notes are
/// never prompt text, so such a segment is dropped, exactly like a switched-off
/// `discard` label. Expected labels are matched first — a dialect that names
/// `Summary` itself keeps it.
fn is_meta_notes(label: &str) -> bool {
    matches!(
        label.trim().to_lowercase().as_str(),
        "notes" | "note" | "notes about the prompt" | "explanation" | "explanations"
            | "reasoning notes" | "thinking" | "thoughts" | "analysis" | "commentary"
            | "comment" | "comments" | "meta" | "summary" | "feedback"
    )
}

/// Segments in contract order. `discard` labels are allowed but dropped (the
/// model's file may still list them); a `Refused` segment satisfies the
/// contract on its own and comes back alone with `refused = true`.
pub fn parse(
    text: &str,
    expected: &[&str],
    discard: &[&str],
) -> Result<(Vec<Segment>, bool), ParseError> {
    let mut segments: Vec<Segment> = vec![];
    for line in text.lines() {
        if let Some(label) = delimiter(line) {
            segments.push(Segment {
                label: label.to_string(),
                body: String::new(),
            });
        } else if let Some(current) = segments.last_mut() {
            current.body.push_str(line);
            current.body.push('\n');
        }
    }
    if segments.is_empty() {
        return Err(ParseError::NoSegments);
    }
    for s in &mut segments {
        s.body = tidy_body(&s.body);
    }
    if segments.iter().any(|s| s.label == REFUSED) {
        return Ok((
            segments
                .into_iter()
                .filter(|s| s.label == REFUSED)
                .collect(),
            true,
        ));
    }
    let unknown: Vec<String> = segments
        .iter()
        .filter(|s| {
            !expected.contains(&s.label.as_str())
                && !discard.contains(&s.label.as_str())
                && !is_meta_notes(&s.label)
        })
        .map(|s| s.label.clone())
        .collect();
    if !unknown.is_empty() {
        return Err(ParseError::Unknown(unknown));
    }
    segments.retain(|s| {
        !discard.contains(&s.label.as_str())
            && !(is_meta_notes(&s.label) && !expected.contains(&s.label.as_str()))
    });
    let missing: Vec<String> = expected
        .iter()
        .filter(|l| !segments.iter().any(|s| s.label == **l))
        .map(|l| l.to_string())
        .collect();
    if !missing.is_empty() {
        return Err(ParseError::Missing(missing));
    }
    segments.sort_by_key(|s| expected.iter().position(|l| *l == s.label));
    // A label written twice: the first one is the answer.
    segments.dedup_by(|b, a| a.label == b.label);
    Ok((segments, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED: [&str; 3] = ["Positive prompt", "Negative prompt", "Reasoning"];

    #[test]
    fn bare_equals_line_and_tag_search_flag_are_not_prompt_text() {
        let text = "===SEGMENT: Positive prompt===\nknight, [retired by danbooru]my trigger, wide shot\n===\n===SEGMENT: Negative prompt===\nbad hands\n===SEGMENT: Reasoning===\nshort\n";
        let (segs, _) = parse(text, &EXPECTED, &[]).unwrap();
        assert_eq!(segs[0].body, "knight, my trigger, wide shot");
    }

    #[test]
    fn parses_in_contract_order() {
        let text = "===SEGMENT: Negative prompt===\nbad hands\n===SEGMENT: Positive prompt===\n1girl, solo\n\n===SEGMENT: Reasoning===\nshort\n";
        let (segs, refused) = parse(text, &EXPECTED, &[]).unwrap();
        assert!(!refused);
        assert_eq!(
            segs.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            EXPECTED
        );
        assert_eq!(segs[0].body, "1girl, solo");
    }

    #[test]
    fn preamble_ignored_refusal_stands_alone() {
        let (segs, refused) =
            parse("Here you go\n===SEGMENT: Refused===\nNo.\n", &EXPECTED, &[]).unwrap();
        assert!(refused);
        assert_eq!(
            segs,
            [Segment {
                label: REFUSED.into(),
                body: "No.".into()
            }]
        );
    }

    #[test]
    fn errors_name_what_broke() {
        assert_eq!(
            parse("1girl, solo", &EXPECTED, &[]),
            Err(ParseError::NoSegments)
        );
        let text = "===SEGMENT: Positive prompt===\na\n===SEGMENT: Tags===\nb\n";
        assert_eq!(
            parse(text, &EXPECTED, &[]),
            Err(ParseError::Unknown(vec!["Tags".into()]))
        );
        let text = "===SEGMENT: Positive prompt===\na\n";
        assert_eq!(
            parse(text, &EXPECTED, &[]),
            Err(ParseError::Missing(vec![
                "Negative prompt".into(),
                "Reasoning".into()
            ]))
        );
    }

    #[test]
    fn discarded_segments_allowed_and_dropped() {
        let text = "===SEGMENT: Positive prompt===\na\n===SEGMENT: Negative prompt===\nb\n===SEGMENT: Reasoning===\nc\n";
        let (segs, _) = parse(text, &EXPECTED[..2], &["Reasoning"]).unwrap();
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn model_renamed_explaining_notes_are_dropped_not_an_error() {
        let text = "===SEGMENT: Positive prompt===\na\n===SEGMENT: Negative prompt===\nb\n===SEGMENT: Notes===\nmodel habits noted\n===SEGMENT: Explanation===\nmore\n";
        let (segs, _) = parse(text, &EXPECTED[..2], &[]).unwrap();
        assert_eq!(segs.len(), 2);
        let text = "===SEGMENT: Positive prompt===\na\n===SEGMENT: Negative prompt===\nb\n===SEGMENT: Summary===\nkeep me\n";
        let expected = ["Positive prompt", "Negative prompt", "Summary"];
        let (segs, _) = parse(text, &expected, &[]).unwrap();
        assert_eq!(segs.iter().find(|s| s.label == "Summary").unwrap().body, "keep me");
    }
}
