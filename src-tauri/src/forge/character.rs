//! Which characters a brief names, decided against the local tag database.
//! Ported from PromptForge server/character.mjs. The rule the whole file is
//! built on: a character tag's NAME PART must equal a run of the brief, and
//! any doubt is an abstention, never a guess — a wrong character puts somebody
//! else's hair colour in the prompt, which is the bug this exists to stop.

use super::tags::{name_part, TagStore};

const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "and",
    "as",
    "at",
    "but",
    "by",
    "for",
    "from",
    "in",
    "into",
    "of",
    "on",
    "or",
    "the",
    "to",
    "with",
    "her",
    "his",
    "its",
    "their",
    "he",
    "she",
    "they",
    "it",
    "is",
    "are",
    "was",
    "were",
    "be",
    "being",
    "been",
    "that",
    "this",
    "these",
    "those",
    "not",
    "no",
    "up",
    "down",
    "out",
    "over",
    "under",
    "while",
    "during",
    "after",
    "before",
    "about",
    "against",
    "between",
    "wearing",
    "standing",
    "sitting",
    "holding",
    "looking",
    "style",
    "art",
    "anime",
    "girl",
    "boy",
    "man",
    "woman",
    "character",
    "scene",
    "shot",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
];
const DESCRIPTION_WORDS: &[&str] = &[
    "dawn", "misty", "mist", "sunny", "hare", "reed", "thunder", "thistle", "scholar", "velvet",
    "knight",
];
const DESCRIPTION_RATIO: i64 = 3;
const APPEARANCE_NOUNS: &[&str] = &[
    "hair", "eye", "eyes", "skin", "fur", "wings", "wing", "tail", "ears", "ear", "horns", "horn",
    "pupils", "irises", "nails", "lips", "scales", "feathers",
];
const JOINING_WORDS: &[&str] = &["a", "an", "the", "of", "no", "and", "de", "in", "on", "to"];
const ORDINARY_TOKEN_POSTS: i64 = 1000;
const DOMINANCE: i64 = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub tag: String,
    pub name: String,
    pub via: &'static str,
    pub post_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ambiguous {
    pub name: String,
    pub candidates: Vec<String>,
}

pub struct Options {
    pub min_post_count: i64,
    pub min_solo_post_count: i64,
    pub max_characters: usize,
}

impl Default for Options {
    /// PromptForge's live settings (config/server.yaml `characters`).
    fn default() -> Self {
        Self {
            min_post_count: 50,
            min_solo_post_count: 250,
            max_characters: 3,
        }
    }
}

/// Everything inside the final `_(...)`, or None.
pub fn franchise_part(tag: &str) -> Option<&str> {
    let inner = tag.strip_suffix(')')?;
    let open = inner.rfind("_(")?;
    let fr = &inner[open + 2..];
    (!fr.is_empty() && !fr.contains(['(', ')'])).then_some(fr)
}

/// Lowercase, underscores as spaces, anything but `a-z0-9:'` as a space.
fn normalise(text: &str) -> String {
    let mapped: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == ':' || c == '\'' || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Contiguous 1..3 word runs, longest first.
pub fn ngrams(text: &str) -> Vec<String> {
    let norm = normalise(text);
    let words: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
    let mut out: Vec<String> = vec![];
    for n in (1..=3).rev() {
        for run in words.windows(n) {
            if n == 1 {
                let w = run[0];
                if STOPWORDS.contains(&w)
                    || (w.len() < 3 && !w.chars().any(|c| c.is_ascii_digit()))
                    || w.len() < 2
                {
                    continue;
                }
            }
            let gram = run.join(" ");
            if !out.contains(&gram) {
                out.push(gram);
            }
        }
    }
    out
}

/// Whole words only.
fn has_phrase(norm: &str, phrase: &str) -> bool {
    !phrase.is_empty() && format!(" {norm} ").contains(&format!(" {phrase} "))
}

fn has_word(norm: &str, word: &str) -> bool {
    norm.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| w == word)
}

fn without_own_name(tag: &str, norm: &str) -> String {
    let own = normalise(name_part(tag));
    if own.is_empty() {
        return norm.to_string();
    }
    let padded = format!(" {norm} ");
    padded
        .replacen(&format!(" {own} "), " ", 1)
        .trim()
        .to_string()
}

fn franchise_tokens(tag: &str) -> Vec<String> {
    let own = normalise(name_part(tag));
    normalise(franchise_part(tag).unwrap_or(""))
        .split([' ', ':'])
        .filter(|t| t.len() >= 4 && *t != own)
        .map(String::from)
        .collect()
}

pub fn franchise_mentioned(tag: &str, norm: &str, store: Option<&TagStore>, posts: i64) -> bool {
    let Some(fr) = franchise_part(tag) else {
        return false;
    };
    let whole = normalise(fr);
    if has_phrase(&without_own_name(tag, norm), &whole) {
        return true;
    }
    franchise_tokens(tag)
        .iter()
        .filter(|t| {
            store.is_none_or(|s| !(is_common_word(t, s, posts) || is_descriptive_word(t, s, posts)))
        })
        .any(|t| has_word(norm, t))
}

/// An ordinary noun: an exact general tag at least as busy as the character.
pub fn is_common_word(name: &str, store: &TagStore, character_posts: i64) -> bool {
    let general = store
        .exact_rows(name)
        .iter()
        .find(|(c, _)| *c == 0)
        .map_or(0, |(_, p)| *p);
    general > 0 && general >= character_posts
}

/// A description: a word inside general tags far busier than the character.
pub fn is_descriptive_word(name: &str, store: &TagStore, character_posts: i64) -> bool {
    if character_posts == 0 || name.contains('_') {
        return false;
    }
    if DESCRIPTION_WORDS.contains(&name.to_lowercase().as_str()) {
        return true;
    }
    store.general_token_top(name) >= DESCRIPTION_RATIO * character_posts
}

/// `platinum` in "platinum blonde hair" is a colour, not Platinum.
pub fn is_appearance_modifier(name: &str, brief: &str) -> bool {
    let word = name.to_lowercase();
    if word.contains(['_', ' ']) {
        return false;
    }
    for field in brief.split([',', ';', '\n']) {
        let norm = normalise(field);
        let words: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
        let Some(at) = words.iter().position(|w| *w == word) else {
            continue;
        };
        if at + 1 >= words.len() {
            continue;
        }
        if APPEARANCE_NOUNS.contains(&words[at + 1])
            || words
                .get(at + 2)
                .is_some_and(|w| APPEARANCE_NOUNS.contains(w))
        {
            return true;
        }
    }
    false
}

fn tokens(raw: &str) -> Vec<&str> {
    raw.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
        .filter(|t| !t.is_empty())
        .collect()
}

fn capitalised(t: &str) -> bool {
    t.chars()
        .next()
        .is_some_and(|c| c.to_lowercase().next() != Some(c))
}

/// Capitalised somewhere, or alone in its comma field.
pub fn written_as_name(name: &str, brief: &str) -> bool {
    let word = name.to_lowercase();
    tokens(brief)
        .iter()
        .any(|t| t.to_lowercase() == word && capitalised(t))
        || brief.split([',', ';', '\n']).any(|f| normalise(f) == word)
}

/// Every word capitalised (joining words exempt), or alone in its field.
pub fn phrase_written_as_name(gram: &str, brief: &str) -> bool {
    let words: Vec<String> = gram
        .to_lowercase()
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect();
    let toks = tokens(brief);
    for run in toks.windows(words.len().max(1)) {
        if !run.iter().zip(&words).all(|(t, w)| t.to_lowercase() == *w) {
            continue;
        }
        if run
            .iter()
            .zip(&words)
            .all(|(t, w)| JOINING_WORDS.contains(&w.as_str()) || capitalised(t))
        {
            return true;
        }
    }
    brief
        .split([',', ';', '\n'])
        .any(|f| normalise(f) == words.join(" "))
}

/// A name made only of ordinary vocabulary ("red fox", "iron man").
pub fn ordinary_phrase(key: &str, store: &TagStore) -> bool {
    let words: Vec<&str> = key
        .split('_')
        .filter(|w| !w.is_empty() && !JOINING_WORDS.contains(w))
        .collect();
    words.len() >= 2
        && words
            .iter()
            .all(|w| store.general_token_top(w) >= ORDINARY_TOKEN_POSTS)
}

/// Quoted speech is not who is in the picture.
pub fn without_dialogue(brief: &str) -> String {
    let mut out = String::with_capacity(brief.len());
    let mut chars = brief.chars().peekable();
    while let Some(c) = chars.next() {
        let close = match c {
            '"' => Some('"'),
            '\u{201c}' => Some('\u{201d}'),
            '\u{300c}' => Some('\u{300d}'),
            _ => None,
        };
        if let Some(close) = close {
            let rest: String = chars.clone().collect();
            if let Some(end) = rest.find([close, '\n']) {
                if rest[end..].starts_with(close) {
                    out.push_str(", ");
                    for _ in 0..rest[..end].chars().count() + 1 {
                        chars.next();
                    }
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

pub fn needs_capital(name: &str, store: &TagStore) -> bool {
    if name.contains('_') || name.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    name.chars().count() < 4
        || store
            .exact_rows(name)
            .iter()
            .any(|(c, p)| *c == 0 && *p > 0)
}

/// "Fern FROM Frieren": the series, not a second subject.
pub fn is_franchise_reference(name: &str, norm: &str, store: &TagStore) -> bool {
    let words: Vec<&str> = norm.split(' ').collect();
    let named = name.replace('_', " ");
    let n = named.split(' ').count();
    if words
        .windows(n + 1)
        .any(|w| w[0] == "from" && w[1..].join(" ") == named)
    {
        return true;
    }
    store
        .exact_rows(name)
        .iter()
        .any(|(c, p)| *c == 3 && *p > 0)
}

type Candidate = (String, i64);

pub fn names_anothers_series(
    gram: &str,
    candidates: &[Candidate],
    grams: &[String],
    candidates_of: &mut dyn FnMut(&str) -> Vec<Candidate>,
) -> bool {
    if !candidates.iter().all(|(t, _)| franchise_part(t).is_some()) {
        return false;
    }
    let own: Vec<String> = candidates
        .iter()
        .map(|(t, _)| normalise(franchise_part(t).unwrap_or("")))
        .collect();
    for other in grams {
        if other == gram || other.contains(gram) || gram.contains(other.as_str()) {
            continue;
        }
        for (tag, _) in candidates_of(other) {
            let fr = normalise(franchise_part(&tag).unwrap_or(""));
            if fr.is_empty() || own.contains(&fr) {
                continue;
            }
            if fr == gram || fr.split([' ', ':']).any(|t| t == gram) {
                return true;
            }
        }
    }
    false
}

/// One person in three outfits is one candidate: same name and franchise (or
/// a bare tag owning the name) collapse to the plainest, busiest member.
pub fn collapse_variants(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let bare: Vec<String> = candidates
        .iter()
        .filter(|(t, _)| !t.contains("_("))
        .map(|(t, _)| t.clone())
        .collect();
    let mut groups: Vec<(String, Vec<Candidate>)> = vec![];
    for c in candidates {
        let name = name_part(&c.0).to_string();
        let key = if bare.contains(&name) {
            name
        } else {
            format!("{name}|{}", franchise_part(&c.0).unwrap_or(""))
        };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(c),
            None => groups.push((key, vec![c])),
        }
    }
    let depth = |t: &str| t.matches("_(").count();
    groups
        .into_iter()
        .map(|(_, mut list)| {
            list.sort_by(|a, b| depth(&a.0).cmp(&depth(&b.0)).then(b.1.cmp(&a.1)));
            list.swap_remove(0)
        })
        .collect()
}

pub fn disambiguate_by_franchise(candidates: &[Candidate], norm: &str) -> Option<Candidate> {
    let whole: Vec<&Candidate> = candidates
        .iter()
        .filter(|(t, _)| {
            franchise_part(t)
                .is_some_and(|fr| has_phrase(&without_own_name(t, norm), &normalise(fr)))
        })
        .collect();
    if whole.len() == 1 {
        return Some(whole[0].clone());
    }
    let mut counts: std::collections::HashMap<String, usize> = Default::default();
    for (t, _) in candidates {
        let mut seen = franchise_tokens(t);
        seen.sort();
        seen.dedup();
        for tok in seen {
            *counts.entry(tok).or_default() += 1;
        }
    }
    let unique: Vec<&Candidate> = candidates
        .iter()
        .filter(|(t, _)| {
            franchise_tokens(t)
                .iter()
                .any(|tok| counts.get(tok) == Some(&1) && has_word(norm, tok))
        })
        .collect();
    (unique.len() == 1).then(|| unique[0].clone())
}

pub fn dominant_candidate(candidates: &[Candidate]) -> Option<Candidate> {
    if candidates.len() < 2 {
        return None;
    }
    let mut sorted = candidates.to_vec();
    sorted.sort_by_key(|c| std::cmp::Reverse(c.1));
    (sorted[0].1 >= DOMINANCE * sorted[1].1).then(|| sorted[0].clone())
}

/// A two- or three-word run that is a character's full name through an alias.
pub fn full_name_of(gram: &str, store: &TagStore, min_posts: i64) -> Option<Candidate> {
    let words: Vec<&str> = gram.split(' ').collect();
    if !(2..=3).contains(&words.len()) {
        return None;
    }
    let mut orders = vec![words.clone()];
    if words.len() == 2 {
        orders.push(vec![words[1], words[0]]);
    }
    for order in orders {
        let Some(target) = store.alias_target(&order.join("_")) else {
            continue;
        };
        if !name_part(&target).split('_').all(|w| order.contains(&w)) {
            continue;
        }
        if let Some(row) = store
            .characters_named(name_part(&target))
            .into_iter()
            .find(|(t, _)| *t == target)
        {
            if row.1 >= min_posts {
                return Some(row);
            }
        }
    }
    None
}

pub fn resolve(brief: &str, store: &TagStore, opts: &Options) -> (Vec<Resolved>, Vec<Ambiguous>) {
    let (mut resolved, mut ambiguous) = (vec![], vec![]);
    if brief.trim().is_empty() {
        return (resolved, ambiguous);
    }
    let brief = without_dialogue(brief);
    let norm = normalise(&brief);
    let grams = ngrams(&brief);
    let mut claimed: Vec<String> = vec![];
    let mut cache: std::collections::HashMap<String, Vec<Candidate>> = Default::default();
    let mut candidates_of = |gram: &str| -> Vec<Candidate> {
        cache
            .entry(gram.to_string())
            .or_insert_with(|| {
                collapse_variants(
                    store
                        .characters_named(&gram.replace(' ', "_"))
                        .into_iter()
                        .filter(|(_, p)| *p >= opts.min_post_count)
                        .collect(),
                )
            })
            .clone()
    };

    for gram in &grams {
        if resolved.len() >= opts.max_characters {
            break;
        }
        if claimed
            .iter()
            .any(|c| c.contains(gram.as_str()) || gram.contains(c.as_str()))
        {
            continue;
        }
        let key = gram.replace(' ', "_");

        if let Some(full) = full_name_of(gram, store, opts.min_post_count) {
            if ordinary_phrase(&key, store)
                && !phrase_written_as_name(gram, &brief)
                && !franchise_mentioned(&full.0, &norm, Some(store), full.1)
            {
                continue;
            }
            resolved.push(Resolved {
                tag: full.0,
                name: gram.clone(),
                via: "full name",
                post_count: full.1,
            });
            claimed.push(gram.clone());
            continue;
        }

        let candidates = candidates_of(gram);
        if candidates.is_empty() {
            continue;
        }
        if is_franchise_reference(&key, &norm, store) {
            continue;
        }
        if names_anothers_series(gram, &candidates, &grams, &mut candidates_of) {
            continue;
        }
        let best_posts = candidates.iter().map(|(_, p)| *p).max().unwrap_or(0);
        let series_named = candidates
            .iter()
            .any(|(t, p)| franchise_mentioned(t, &norm, Some(store), *p));
        if is_appearance_modifier(&key, &brief) {
            continue;
        }
        if !series_named {
            let bare = ["a_", "an_", "the_"]
                .iter()
                .find_map(|a| key.strip_prefix(a))
                .unwrap_or(&key);
            let guard_key = if bare != key && !bare.contains('_') {
                bare
            } else {
                key.as_str()
            };
            let guard_gram = guard_key.replace('_', " ");
            if needs_capital(guard_key, store) && !written_as_name(&guard_gram, &brief) {
                continue;
            }
            if ordinary_phrase(guard_key, store) && !phrase_written_as_name(&guard_gram, &brief) {
                continue;
            }
            if is_common_word(guard_key, store, best_posts)
                || is_descriptive_word(guard_key, store, best_posts)
            {
                continue;
            }
            if !guard_key.contains('_') && best_posts < opts.min_solo_post_count {
                continue;
            }
        }
        let picked = if candidates.len() == 1 {
            Some((candidates[0].clone(), "unique name"))
        } else if let Some(c) = disambiguate_by_franchise(&candidates, &norm) {
            Some((c, "franchise named in brief"))
        } else {
            dominant_candidate(&candidates).map(|c| (c, "far the best known of that name"))
        };
        match picked {
            Some(((tag, posts), via)) => resolved.push(Resolved {
                tag,
                name: gram.clone(),
                via,
                post_count: posts,
            }),
            None => ambiguous.push(Ambiguous {
                name: gram.clone(),
                candidates: candidates.iter().take(5).map(|(t, _)| t.clone()).collect(),
            }),
        }
        claimed.push(gram.clone());
    }
    (resolved, ambiguous)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::tags::tests::fixture;

    fn tags_of(brief: &str) -> Vec<String> {
        let store = fixture();
        resolve(brief, &store, &Options::default())
            .0
            .into_iter()
            .map(|r| r.tag)
            .collect()
    }

    #[test]
    fn named_characters_resolve() {
        assert_eq!(
            tags_of("frieren sitting by a campfire, Fern asleep against her shoulder"),
            ["frieren", "fern_(sousou_no_frieren)"]
        );
        assert_eq!(
            tags_of("Makima from Chainsaw Man on a rooftop"),
            ["makima_(chainsaw_man)"]
        );
        assert_eq!(
            tags_of("1girl, fern, long hair"),
            ["fern_(sousou_no_frieren)"]
        );
    }

    #[test]
    fn ordinary_words_never_summon_strangers() {
        assert!(tags_of("a cup of tea on a low table").is_empty());
        assert!(tags_of("a long dusty road").is_empty());
        assert!(tags_of("fern growing in a workshop").is_empty());
        assert!(tags_of("a fairy with platinum blonde hair and red eyes").is_empty());
    }

    #[test]
    fn shared_names_need_a_reason() {
        // Rem from Re:Zero dwarfs the other Rem tenfold.
        assert_eq!(tags_of("Rem in a maid outfit"), ["rem_(re:zero)"]);
        // Costume variants are one person.
        assert_eq!(
            tags_of("Zhongli at the harbour"),
            ["zhongli_(genshin_impact)"]
        );
    }

    /// Prints one JSON line per brief for comparison with PromptForge's own
    /// resolver: `DEAS_FORGE_DATA=<dir> DEAS_FORGE_BRIEFS=<json> cargo test -- --ignored parity`.
    #[test]
    #[ignore = "needs DEAS_FORGE_DATA and DEAS_FORGE_BRIEFS"]
    fn parity() {
        let dir = std::path::PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap());
        let store = TagStore::open(&dir.join("tags.sqlite")).unwrap();
        let briefs: Vec<String> = serde_json::from_str(
            &std::fs::read_to_string(std::env::var("DEAS_FORGE_BRIEFS").unwrap()).unwrap(),
        )
        .unwrap();
        for brief in briefs {
            let (r, a) = resolve(&brief, &store, &Options::default());
            let line = serde_json::json!({
                "brief": brief,
                "resolved": r.iter().map(|x| &x.tag).collect::<Vec<_>>(),
                "ambiguous": a.iter().map(|x| &x.name).collect::<Vec<_>>(),
            });
            println!("PARITY {line}");
        }
    }

    #[test]
    fn dialogue_is_not_a_name() {
        assert_eq!(
            without_dialogue("the crowd shouts \"Encore!\" at her"),
            "the crowd shouts ,  at her"
        );
    }
}
