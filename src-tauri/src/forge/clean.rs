//! Mechanical repairs to a written prompt, applied after the model answers.
//!
//! Ported from PromptForge (server/positive.mjs, negative.mjs, parens.mjs,
//! tagspacing.mjs, aspect.mjs and the comma tidy in handleGenerate). Each rule
//! is one the prompt files already state; code enforces it because a model,
//! measured, does not. Nothing here judges which tags are good — length,
//! order and choice stay with the prompt files.

use std::collections::HashSet;

fn split(text: &str) -> Vec<&str> {
    text.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect()
}

/// A tag reduced to what two spellings of one concept share: lowercase, one
/// layer of `(tag:1.3)` weight removed, no brackets, `_` and space alike.
pub fn normalise_tag(tag: &str) -> String {
    let mut t = tag.trim().to_lowercase();
    if let Some(inner) = unweighted(&t) {
        t = inner.to_string();
    }
    let t: String = t.chars().filter(|c| !"()[]{}".contains(*c)).collect();
    t.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `(tag:1.30)` -> `tag`; None when the text is not one weight expression.
fn unweighted(t: &str) -> Option<&str> {
    let body = t.strip_prefix('(')?.strip_suffix(')')?;
    let colon = body.rfind(':')?;
    let number = body[colon + 1..].trim_start();
    (!number.is_empty() && number.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .then(|| &body[..colon])
}

fn tag_set<'a>(sources: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    let mut set: HashSet<String> = sources
        .into_iter()
        .flat_map(split)
        .map(normalise_tag)
        .collect();
    set.remove("");
    set
}

/// Repeated commas and missing spaces after them. Not before a quote or a
/// digit: `"Wait," she says` and `1,000` are already right.
pub fn tidy_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ',' {
            // `,  ,  ,` -> `,`
            let mut j = i + 1;
            loop {
                let mut k = j;
                while k < chars.len() && (chars[k] == ' ' || chars[k] == '\t') {
                    k += 1;
                }
                if k < chars.len() && chars[k] == ',' {
                    j = k + 1;
                } else {
                    break;
                }
            }
            while out.ends_with([' ', '\t']) {
                out.pop();
            }
            out.push(',');
            if let Some(&next) = chars.get(j) {
                if !next.is_whitespace()
                    && !"\"'\u{201d}\u{2019}".contains(next)
                    && !next.is_ascii_digit()
                {
                    out.push(' ');
                }
            }
            i = j;
            continue;
        }
        if (c == ' ' || c == '\t') && out.ends_with([' ', '\t']) {
            i += 1;
            continue;
        }
        out.push(if c == '\t' { ' ' } else { c });
        i += 1;
    }
    // Whitespace (including a newline) before a comma.
    let mut tidy = String::with_capacity(out.len());
    for c in out.chars() {
        if c == ',' {
            while tidy.ends_with(char::is_whitespace) {
                tidy.pop();
            }
        }
        tidy.push(c);
    }
    tidy
}

/// Anima's prose half is comma separated too; a long or capitalised item is a
/// sentence, and everything after it is left alone.
pub fn looks_like_prose(item: &str) -> bool {
    let words = item.split_whitespace().count();
    words >= 8 || (words >= 5 && item.trim().starts_with(|c: char| c.is_ascii_uppercase()))
}

pub struct Positive {
    pub text: String,
    pub duplicates: Vec<String>,
    pub invented: Vec<String>,
}

/// Duplicate tags, and invented `name_(series)` tags the database does not
/// hold. Only parenthesised unknowns go: most of a good prompt is not danbooru
/// vocabulary, and that shape is the one nobody writes outside booru tags.
pub fn clean_positive(
    positive: &str,
    known: Option<&dyn Fn(&str) -> bool>,
    protected: &[&str],
) -> Positive {
    let spared = tag_set(protected.iter().copied());
    let mut seen = HashSet::new();
    let (mut kept, mut duplicates, mut invented) = (vec![], vec![], vec![]);
    let mut in_prose = false;
    for raw in split(positive) {
        if in_prose || looks_like_prose(raw) {
            in_prose = true;
            kept.push(raw);
            continue;
        }
        let key = normalise_tag(raw);
        if key.is_empty() {
            continue;
        }
        if seen.contains(&key) {
            duplicates.push(raw.to_string());
            continue;
        }
        // Tested on the raw form (normalise strips brackets) and looked up
        // without Anima's `\(` escaping, or every escaped character is a miss.
        if let Some(known) = known {
            if !spared.contains(&key)
                && ends_with_paren_group(raw)
                && !known(&raw.replace("\\(", "(").replace("\\)", ")"))
            {
                invented.push(raw.to_string());
                continue;
            }
        }
        seen.insert(key);
        kept.push(raw);
    }
    Positive {
        text: kept.join(", "),
        duplicates,
        invented,
    }
}

fn ends_with_paren_group(raw: &str) -> bool {
    let t = raw.trim_end();
    t.ends_with(')')
        && t[..t.len() - 1]
            .rfind('(')
            .is_some_and(|open| !t[open + 1..t.len() - 1].contains(')'))
}

pub struct Negative {
    pub text: String,
    pub duplicates: Vec<String>,
    pub contradictions: Vec<String>,
    /// Kept: danbooru's labels are not what a community checkpoint understands.
    pub unknown: Vec<String>,
}

/// Duplicates and anything the positive asked for. Unknown tags are reported,
/// never removed.
pub fn clean_negative(
    negative: &str,
    positives: &[&str],
    known: Option<&dyn Fn(&str) -> bool>,
    protected: &[&str],
) -> Negative {
    let banned = tag_set(positives.iter().copied());
    let spared = tag_set(protected.iter().copied());
    let mut seen = HashSet::new();
    let mut out = Negative {
        text: String::new(),
        duplicates: vec![],
        contradictions: vec![],
        unknown: vec![],
    };
    let mut kept = vec![];
    for raw in split(negative) {
        let key = normalise_tag(raw);
        if key.is_empty() {
            continue;
        }
        if banned.contains(&key) {
            out.contradictions.push(raw.to_string());
            continue;
        }
        if seen.contains(&key) {
            out.duplicates.push(raw.to_string());
            continue;
        }
        if let Some(known) = known {
            if !spared.contains(&key) && !known(&key) {
                out.unknown.push(raw.to_string());
            }
        }
        seen.insert(key);
        kept.push(raw);
    }
    out.text = kept.join(", ");
    out
}

/// Put back the model's `negative_defaults` it left out, first, except any the
/// positive asked for.
pub fn ensure_defaults(
    negative: &str,
    defaults: &str,
    positives: &[&str],
) -> (String, Vec<String>) {
    let mut present: HashSet<String> = split(negative).into_iter().map(normalise_tag).collect();
    let banned = tag_set(positives.iter().copied());
    let mut missing = vec![];
    for raw in split(defaults) {
        let key = normalise_tag(raw);
        if key.is_empty() || present.contains(&key) || banned.contains(&key) {
            continue;
        }
        present.insert(key);
        missing.push(raw.to_string());
    }
    if missing.is_empty() {
        return (negative.to_string(), missing);
    }
    let text = missing
        .iter()
        .map(String::as_str)
        .chain(split(negative))
        .collect::<Vec<_>>()
        .join(", ");
    (text, missing)
}

/// Keep the floor and at most `max` tags the model added on top of it.
pub fn cap_extras(negative: &str, floor: &str, max: usize) -> (String, Vec<String>) {
    let floor: HashSet<String> = split(floor).into_iter().map(normalise_tag).collect();
    let (mut kept, mut dropped, mut extras) = (vec![], vec![], 0);
    for raw in split(negative) {
        if floor.contains(&normalise_tag(raw)) {
            kept.push(raw);
        } else if extras < max {
            extras += 1;
            kept.push(raw);
        } else {
            dropped.push(raw.to_string());
        }
    }
    (kept.join(", "), dropped)
}

/// Escape every parenthesis pair that is not a weight, so `makima (chainsaw
/// man)` stays a name instead of a weight that lost its number. Last of all:
/// nothing may reason about tags after this turns them into generator syntax.
pub fn escape_literal_parens(text: &str) -> (String, Vec<String>) {
    if !text.contains('(') {
        return (text.to_string(), vec![]);
    }
    let mut escaped = vec![];
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '\\' && (rest[1..].starts_with('(') || rest[1..].starts_with(')')) {
            out.push_str(&rest[..2]);
            rest = &rest[2..];
            continue;
        }
        if c != '(' {
            out.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let Some(close) = rest[1..].find(')').map(|i| i + 1) else {
            out.push(c);
            rest = &rest[1..];
            continue;
        };
        let inner = &rest[1..close];
        if inner.contains('(') {
            out.push(c);
            rest = &rest[1..];
            continue;
        }
        if is_weight(inner) {
            out.push_str(&rest[..=close]);
        } else {
            escaped.push(format!("({inner})"));
            out.push_str(&format!("\\({inner}\\)"));
        }
        rest = &rest[close + 1..];
    }
    (out, escaped)
}

/// `chibi:2` and `glitch:1.5` are weights; `nier:automata` is a name.
fn is_weight(inner: &str) -> bool {
    let Some(colon) = inner.rfind(':') else {
        return false;
    };
    let n = inner[colon + 1..].trim();
    let mut parts = n.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let frac = parts.next();
    !whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
}

/// Underscores between word characters become spaces, tag by tag, on a model
/// that wants spaces. Emoticons (`o_o`), score tags and anything the person
/// typed keep theirs.
pub fn space_underscores(text: &str, protected: &[&str]) -> (String, usize) {
    let kept: HashSet<String> = protected
        .iter()
        .flat_map(|p| p.split(','))
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    let mut changed = 0;
    let pieces: Vec<String> = text
        .split(',')
        .map(|piece| {
            let tag = piece.trim();
            if !tag.contains('_')
                || kept.contains(&tag.to_lowercase())
                || is_score(tag)
                || is_emoticon(tag)
            {
                return piece.to_string();
            }
            let chars: Vec<char> = piece.chars().collect();
            let spaced: String = chars
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    let before = i.checked_sub(1).map(|j| chars[j]);
                    let after = chars.get(i + 1).copied();
                    let left = before.is_some_and(|b| b.is_alphanumeric() || b == ')' || b == '\\');
                    let right = after.is_some_and(|a| a.is_alphanumeric() || a == '(' || a == '\\');
                    if c == '_' && left && right {
                        ' '
                    } else {
                        c
                    }
                })
                .collect();
            if spaced != piece {
                changed += 1;
            }
            spaced
        })
        .collect();
    (pieces.join(","), changed)
}

fn is_score(tag: &str) -> bool {
    let t = tag.strip_prefix('(').unwrap_or(tag).to_lowercase();
    t.strip_prefix("score_")
        .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
}

/// One character (or none) either side of the underscore: a face, not a phrase.
fn is_emoticon(tag: &str) -> bool {
    let t = tag.strip_prefix('(').unwrap_or(tag);
    let t = t.strip_suffix(')').unwrap_or(t);
    let Some((left, right)) = t.split_once('_') else {
        return false;
    };
    let side = |s: &str, outer_first: bool| {
        let cs: Vec<char> = s.chars().collect();
        match cs.len() {
            0 => true,
            1 => true,
            2 => {
                let (outer, inner) = if outer_first {
                    (cs[0], cs[1])
                } else {
                    (cs[1], cs[0])
                };
                !outer.is_alphanumeric() && inner.is_alphanumeric()
            }
            _ => false,
        }
    };
    !right.contains('_') && side(left, true) && side(right, false)
}

/// Remove the aspect ratio when it leaked from the controls into the prompt.
/// Only the ratio this request asked for, in the spellings a model writes it;
/// never anything the person typed.
pub fn strip_aspect(body: &str, aspect: &str, protected: &[&str]) -> (String, Vec<String>) {
    let spellings = aspect_spellings(aspect);
    if body.trim().is_empty() || spellings.is_empty() {
        return (body.to_string(), vec![]);
    }
    let guarded = protected
        .iter()
        .map(|t| t.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let protected = |needle: &str| guarded.contains(needle);
    let orientation = remove_ratio(aspect).trim().to_lowercase();
    let mut removed = vec![];
    if body.contains(',') {
        let items: Vec<&str> = body.split(',').collect();
        let norm = |item: &str| {
            let t = item.trim().to_lowercase();
            let t = t.strip_prefix('(').unwrap_or(&t);
            // Only a weight's tail, `:1.2)`: a bare `9_16` keeps its digits.
            let t = if t.ends_with(')') {
                t.trim_end_matches(|c: char| c == ':' || c == ')' || c == '.' || c.is_ascii_digit())
            } else {
                t
            };
            t.trim().to_string()
        };
        let mut drop = HashSet::new();
        for (i, item) in items.iter().enumerate() {
            let n = norm(item);
            if n.is_empty() || !spellings.contains(&n) || protected(&n) {
                continue;
            }
            drop.insert(i);
            for j in [i.wrapping_sub(1), i + 1] {
                if j < items.len()
                    && !orientation.is_empty()
                    && norm(items[j]) == orientation
                    && !protected(&orientation)
                {
                    drop.insert(j);
                }
            }
        }
        if !drop.is_empty() {
            let mut order: Vec<_> = drop.iter().copied().collect();
            order.sort_unstable();
            removed.extend(order.iter().map(|&i| items[i].trim().to_string()));
            let kept: Vec<&str> = items
                .iter()
                .enumerate()
                .filter(|(i, _)| !drop.contains(i))
                .map(|(_, t)| *t)
                .collect();
            let text = split(&kept.join(",")).join(", ");
            return (text, removed);
        }
    }
    // Prose: only `(2:3 portrait)` or a trailing `, 2:3 portrait`.
    let mut out = body.to_string();
    for spelling in &spellings {
        if protected(spelling) {
            continue;
        }
        loop {
            let lower = out.to_lowercase();
            let hit =
                find_parenthetical(&lower, spelling).or_else(|| find_trailing(&lower, spelling));
            let Some((start, end)) = hit else { break };
            out.replace_range(start..end, "");
            removed.push(spelling.clone());
        }
    }
    if removed.is_empty() {
        (body.to_string(), removed)
    } else {
        (
            out.split_whitespace().collect::<Vec<_>>().join(" "),
            removed,
        )
    }
}

fn find_parenthetical(lower: &str, spelling: &str) -> Option<(usize, usize)> {
    for prefix in ["(in ", "("] {
        let needle = format!("{prefix}{spelling})");
        if let Some(at) = lower.find(&needle) {
            let start = lower[..at].trim_end().len();
            return Some((start, at + needle.len()));
        }
    }
    None
}

fn find_trailing(lower: &str, spelling: &str) -> Option<(usize, usize)> {
    for (at, _) in lower.match_indices([',', ';']) {
        let after = lower[at + 1..].trim_start();
        let after = after.strip_prefix("in ").unwrap_or(after);
        if let Some(rest) = after.strip_prefix(spelling) {
            let rest = rest.trim_start();
            if rest.is_empty() || rest.starts_with(['.', ';', ',']) {
                return Some((at, lower.len() - rest.len()));
            }
        }
    }
    None
}

fn remove_ratio(label: &str) -> String {
    match ratio_in(label) {
        Some((start, end, _)) => format!("{}{}", &label[..start], &label[end..]),
        None => label.to_string(),
    }
}

/// The first `N:M` (spaces allowed around the colon) and its tight form.
fn ratio_in(label: &str) -> Option<(usize, usize, String)> {
    let b = label.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let left = &label[start..i];
            let mut j = i;
            while j < b.len() && b[j] == b' ' {
                j += 1;
            }
            if j < b.len() && b[j] == b':' {
                j += 1;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                let r = j;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j > r {
                    return Some((start, j, format!("{left}:{}", &label[r..j])));
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

fn aspect_spellings(aspect: &str) -> Vec<String> {
    let label = aspect.trim();
    if label.is_empty() {
        return vec![];
    }
    let mut out: Vec<String> = vec![];
    let mut add = |v: String| {
        let t = v.trim().to_lowercase();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    let joined = |s: &str, sep: &str| s.split_whitespace().collect::<Vec<_>>().join(sep);
    add(label.to_string());
    add(joined(label, "_"));
    add(joined(label, "-"));
    if let Some((_, _, tight)) = ratio_in(label) {
        add(tight.clone());
        add(tight.replace(':', "_"));
        add(tight.replace(':', "-"));
        add(tight.replace(':', " "));
        add(format!("{tight} aspect ratio"));
        add(format!("{tight}_aspect_ratio"));
        add(format!("aspect ratio {tight}"));
        let words = remove_ratio(label).trim().to_string();
        if !words.is_empty() {
            add(format!("{tight} {words}"));
            add(format!("{tight}_{}", joined(&words, "_")));
            add(format!("{tight}-{}", joined(&words, "-")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_survives_weights_and_spelling() {
        assert_eq!(normalise_tag("(purple_hair:1.30)"), "purple hair");
        assert_eq!(normalise_tag(" Purple  Hair "), "purple hair");
        assert_eq!(
            normalise_tag("makima_(chainsaw_man)"),
            "makima chainsaw man"
        );
    }

    #[test]
    fn commas_tidied_but_quotes_and_numbers_kept() {
        assert_eq!(tidy_commas("a,, ,b,c"), "a, b, c");
        assert_eq!(tidy_commas("1girl ,solo"), "1girl, solo");
        assert_eq!(tidy_commas("\"Wait,\" she says"), "\"Wait,\" she says");
        assert_eq!(tidy_commas("1,000 lanterns"), "1,000 lanterns");
        assert_eq!(tidy_commas("a  b"), "a b");
    }

    #[test]
    fn positive_drops_duplicates_and_invented_parenthesised_tags() {
        // The 1 Sep 2026 Illustrious output that started positive.mjs.
        let known = |t: &str| {
            matches!(
                t,
                "frieren" | "fern_(sousou_no_frieren)" | "fern (sousou no frieren)"
            )
        };
        let out = clean_positive(
            "masterpiece, frieren, frieren_(frieren), frieren, solo, fern (sousou no frieren)",
            Some(&known),
            &[],
        );
        assert_eq!(
            out.text,
            "masterpiece, frieren, solo, fern (sousou no frieren)"
        );
        assert_eq!(out.duplicates, ["frieren"]);
        assert_eq!(out.invented, ["frieren_(frieren)"]);
    }

    #[test]
    fn positive_reads_anima_escapes_and_leaves_prose_alone() {
        let known = |t: &str| t == "shiroko terror (blue archive)";
        let out = clean_positive(
            "1girl, shiroko terror \\(blue archive\\), She walks home through the rain, slowly, slowly, under a grey sky",
            Some(&known),
            &[],
        );
        assert!(out.invented.is_empty());
        assert!(out.text.ends_with("slowly, slowly, under a grey sky"));
    }

    #[test]
    fn positive_keeps_modifiers_that_are_five_lowercase_words() {
        let out = clean_positive(
            "1girl, warm highlights and cool shadows, solo, solo",
            None,
            &[],
        );
        assert_eq!(out.text, "1girl, warm highlights and cool shadows, solo");
    }

    #[test]
    fn negative_drops_repeats_and_positive_contradictions() {
        let out = clean_negative(
            "worst_quality, mage_staff, extra_eyes, floating_eyes, extra_eyes, floating_eyes",
            &["fern, mage_staff"],
            Some(&|t: &str| t != "floating eyes"),
            &[],
        );
        assert_eq!(out.text, "worst_quality, extra_eyes, floating_eyes");
        assert_eq!(out.contradictions, ["mage_staff"]);
        assert_eq!(out.duplicates, ["extra_eyes", "floating_eyes"]);
        assert_eq!(out.unknown, ["floating_eyes"]);
    }

    #[test]
    fn defaults_restored_first_unless_the_positive_wants_them() {
        let (text, added) = ensure_defaults(
            "blurry",
            "worst quality, low quality, text",
            &["1girl, text"],
        );
        assert_eq!(text, "worst quality, low quality, blurry");
        assert_eq!(added, ["worst quality", "low quality"]);
    }

    #[test]
    fn extras_capped_above_the_floor() {
        let (text, dropped) = cap_extras("worst quality, url, link, share", "worst quality", 1);
        assert_eq!(text, "worst quality, url");
        assert_eq!(dropped, ["link", "share"]);
    }

    #[test]
    fn literal_parens_escaped_weights_kept() {
        let (text, escaped) = escape_literal_parens(
            "makima (chainsaw man), (chibi:2), (glitch:1.5), 2b (nier:automata), already \\(done\\)",
        );
        assert_eq!(
            text,
            "makima \\(chainsaw man\\), (chibi:2), (glitch:1.5), 2b \\(nier:automata\\), already \\(done\\)"
        );
        assert_eq!(escaped, ["(chainsaw man)", "(nier:automata)"]);
        let (nested, _) = escape_literal_parens("(makima \\(chainsaw man\\):2)");
        assert_eq!(nested, "(makima \\(chainsaw man\\):2)");
    }

    #[test]
    fn underscores_spaced_except_faces_scores_and_typed_tags() {
        let (text, changed) = space_underscores(
            "best_quality, cat_ears, sousou no_frieren, o_o, ^_^, score_7_up, my_tag, makima_(chainsaw_man)",
            &["my_tag"],
        );
        assert_eq!(
            text,
            "best quality, cat ears, sousou no frieren, o_o, ^_^, score_7_up, my_tag, makima (chainsaw man)"
        );
        assert_eq!(changed, 4);
    }

    #[test]
    fn leaked_aspect_removed_from_tags_and_prose() {
        let (text, removed) = strip_aspect("1girl, rain, night, 2:3_portrait", "2:3 Portrait", &[]);
        assert_eq!(text, "1girl, rain, night");
        assert_eq!(removed, ["2:3_portrait"]);
        let (text, _) = strip_aspect("1girl, tall, 9_16, rain", "9:16 Tall", &[]);
        assert_eq!(text, "1girl, rain");
        let (text, _) = strip_aspect("1girl, portrait, rain", "2:3 Portrait", &[]);
        assert_eq!(text, "1girl, portrait, rain");
        let (text, _) = strip_aspect(
            "A girl stands in the rain (2:3 portrait).",
            "2:3 Portrait",
            &[],
        );
        assert_eq!(text, "A girl stands in the rain.");
        let (text, removed) = strip_aspect("1girl, 16:9", "16:9 Wide", &["a 16:9 television"]);
        assert_eq!(text, "1girl, 16:9");
        assert!(removed.is_empty());
    }
}
