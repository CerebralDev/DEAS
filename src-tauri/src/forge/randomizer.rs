//! The brief randomizer, ported from PromptForge's scripts/randomizer.py: tick
//! categories, roll, and the result goes into the brief. No model is involved;
//! it picks from forge-data/randomizer.json (the owner's vocabulary) and the
//! person edits what comes out.
//!
//! A template is `{a|b:3|c}` groups with text around them: one option from each
//! group, weights honoured (`b:3`, or a repeated option), everything outside a
//! group kept verbatim, and an option of `none`/`nothing`/`no …` contributing
//! nothing.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Deserialize)]
struct File {
    categories: Vec<Category>,
}

#[derive(Debug, Clone, Deserialize)]
struct Category {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    children: Vec<Category>,
    #[serde(default)]
    templates: Vec<String>,
}

impl Category {
    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.id)
    }
    /// Every leaf under a box; a leaf is its own.
    fn leaves(&self) -> Vec<&Category> {
        if self.children.is_empty() {
            vec![self]
        } else {
            self.children.iter().collect()
        }
    }
}

pub struct Randomizer {
    categories: Vec<Category>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Roll {
    /// Box ids and leaf ids alike; a box id means everything in it.
    pub categories: Vec<String>,
    pub women: u8,
    pub men: u8,
    /// Pick the categories too, from the ticked ones (or everything but NSFW).
    pub surprise: bool,
    pub surprise_count: Option<usize>,
    /// At most this many leaves per box (Controlled chaos).
    pub limits: HashMap<String, usize>,
    pub seed: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Pick {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct BoxPicks {
    pub id: String,
    pub label: String,
    pub picks: Vec<Pick>,
}

#[derive(Debug, Serialize)]
pub struct Rolled {
    pub boxes: Vec<BoxPicks>,
    pub cast: Vec<String>,
    pub text: String,
}

const SURPRISE_COUNT: usize = 10;
/// A Surprise takes at most one box from each of these sets.
const SURPRISE_EXCLUSIVE: [&[&str]; 2] = [
    &[
        "Mammals",
        "Birds, reptiles & amphibians",
        "Aquatic",
        "Insects & arachnids",
        "Plants & fungi",
        "Mythical",
        "Undead & horror",
    ],
    &["Clothing female", "Clothing male"],
];

/// splitmix64: small, seedable, and plenty for picking words.
struct Rng(u64);
impl Rng {
    fn new(seed: Option<u64>) -> Self {
        Self(seed.unwrap_or_else(|| {
            let b = *uuid::Uuid::new_v4().as_bytes();
            u64::from_le_bytes(b[..8].try_into().unwrap())
        }))
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

/// `a|b:3|c` → [(a,1), (b,3), (c,1)].
fn options(body: &str) -> Vec<(String, u32)> {
    body.split('|')
        .map(|piece| {
            let text = piece.trim();
            match text.rsplit_once(':') {
                Some((t, w))
                    if !w.is_empty() && w.len() <= 3 && w.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    (t.trim().to_string(), w.parse::<u32>().unwrap_or(1).max(1))
                }
                _ => (text.to_string(), 1),
            }
        })
        .collect()
}

fn is_noop(t: &str) -> bool {
    let l = t.to_lowercase();
    l == "none" || l == "nothing" || (l.starts_with("no ") && l.len() > 3)
}

/// Innermost `{…}` groups first, as the regex `\{([^{}]*)\}` finds them.
fn resolve(template: &str, rng: &mut Rng) -> String {
    let mut out = template.to_string();
    for _ in 0..4 {
        let mut next = String::with_capacity(out.len());
        let mut rest = out.as_str();
        let mut found = false;
        while let Some(close) = rest.find('}') {
            match rest[..close].rfind('{') {
                Some(open) => {
                    found = true;
                    next.push_str(&rest[..open]);
                    let opts = options(&rest[open + 1..close]);
                    let total: u32 = opts.iter().map(|(_, w)| w).sum();
                    let mut roll = (rng.next() % u64::from(total.max(1))) as u32;
                    let chosen = opts
                        .iter()
                        .find(|(_, w)| {
                            if roll < *w {
                                true
                            } else {
                                roll -= w;
                                false
                            }
                        })
                        .map_or("", |(t, _)| t.as_str());
                    if !is_noop(chosen) {
                        next.push_str(chosen);
                    }
                }
                None => next.push_str(&rest[..=close]),
            }
            rest = &rest[close + 1..];
        }
        next.push_str(rest);
        out = next;
        if !found {
            break;
        }
    }
    // Commas left behind by an empty pick, and stray spacing.
    out.split(',')
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn cast(women: u8, men: u8) -> Vec<String> {
    let w = match women {
        1 => Some("1girl"),
        2 => Some("2girls"),
        3 => Some("3girls, multiple girls"),
        4 => Some("4girls, multiple girls"),
        _ => None,
    };
    let m = match men {
        1 => Some("1boy"),
        2 => Some("2boys"),
        3 => Some("3boys, multiple boys"),
        4 => Some("4boys, multiple men"),
        _ => None,
    };
    w.into_iter().chain(m).map(String::from).collect()
}

impl Randomizer {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let bytes = super::data::read(&dir.join("randomizer.json"))
            .map_err(|_| "The randomizer's vocabulary (forge-data/randomizer.json) is missing.")?;
        let file: File = serde_json::from_slice(&bytes)
            .map_err(|e| format!("forge-data/randomizer.json is not valid: {e}"))?;
        if file.categories.is_empty() {
            return Err("forge-data/randomizer.json has no categories.".into());
        }
        Ok(Self {
            categories: file.categories,
        })
    }

    /// Boxes and their pieces, with how many options each holds.
    pub fn list(&self) -> Value {
        let count = |t: &str| -> usize {
            let mut n = 0;
            let mut rest = t;
            while let Some(close) = rest.find('}') {
                if let Some(open) = rest[..close].rfind('{') {
                    n += options(&rest[open + 1..close]).len();
                }
                rest = &rest[close + 1..];
            }
            n
        };
        json!(self
            .categories
            .iter()
            .map(|c| json!({
                "id": c.id,
                "label": c.label(),
                "children": c.leaves().iter().map(|k| json!({
                    "id": k.id,
                    "label": k.label(),
                    "options": k.templates.iter().map(|t| count(t)).sum::<usize>(),
                })).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>())
    }

    fn surprise(&self, req: &Roll, rng: &mut Rng) -> HashSet<String> {
        let allow: HashSet<&str> = req.categories.iter().map(String::as_str).collect();
        let mut skip = vec![];
        if allow.is_empty() {
            skip.push("NSFW");
        }
        if req.men > 0 && req.women == 0 {
            skip.push("Clothing female");
        } else if req.women > 0 {
            skip.push("Clothing male");
        }
        let mut pool: Vec<(&str, &str)> = vec![];
        for c in &self.categories {
            if skip.contains(&c.label()) {
                continue;
            }
            for k in c.leaves() {
                if k.templates.is_empty()
                    || (!allow.is_empty()
                        && !allow.contains(k.id.as_str())
                        && !allow.contains(c.id.as_str()))
                {
                    continue;
                }
                pool.push((c.label(), &k.id));
            }
        }
        rng.shuffle(&mut pool);
        let count = req.surprise_count.unwrap_or(SURPRISE_COUNT).clamp(1, 40);
        let mut used = [false; SURPRISE_EXCLUSIVE.len()];
        let mut picked = HashSet::new();
        for (label, id) in pool {
            if picked.len() >= count {
                break;
            }
            let clash = SURPRISE_EXCLUSIVE.iter().enumerate().any(|(i, group)| {
                if !group.contains(&label) {
                    return false;
                }
                std::mem::replace(&mut used[i], true)
            });
            if !clash {
                picked.insert(id.to_string());
            }
        }
        picked
    }

    fn limit(&self, mut wanted: HashSet<String>, req: &Roll, rng: &mut Rng) -> HashSet<String> {
        for c in &self.categories {
            let Some(&n) = req.limits.get(&c.id).filter(|n| **n > 0) else {
                continue;
            };
            let kids: Vec<String> = c.leaves().iter().map(|k| k.id.clone()).collect();
            let mut here: Vec<String> = if wanted.remove(&c.id) {
                kids.clone()
            } else {
                kids.iter()
                    .filter(|k| wanted.contains(*k))
                    .cloned()
                    .collect()
            };
            if here.len() > n {
                rng.shuffle(&mut here);
                here.truncate(n);
            }
            for k in &kids {
                wanted.remove(k);
            }
            wanted.extend(here);
        }
        wanted
    }

    pub fn roll(&self, req: &Roll) -> Result<Rolled, String> {
        let mut rng = Rng::new(req.seed);
        let wanted: HashSet<String> = if req.surprise {
            self.surprise(req, &mut rng)
        } else {
            let ticked = req.categories.iter().cloned().collect();
            self.limit(ticked, req, &mut rng)
        };
        let cast = cast(req.women, req.men);
        if wanted.is_empty() && cast.is_empty() {
            return Err("Pick at least one category.".into());
        }
        let known: HashSet<&str> = self
            .categories
            .iter()
            .flat_map(|c| {
                std::iter::once(c.id.as_str()).chain(c.leaves().into_iter().map(|k| k.id.as_str()))
            })
            .collect();
        let mut unknown: Vec<&String> = wanted
            .iter()
            .filter(|w| !known.contains(w.as_str()))
            .collect();
        if !unknown.is_empty() {
            unknown.sort();
            return Err(format!(
                "Unknown categor{}: {}",
                if unknown.len() == 1 { "y" } else { "ies" },
                unknown
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let mut boxes = vec![];
        for c in &self.categories {
            let whole = wanted.contains(&c.id);
            let mut picks = vec![];
            for k in c.leaves() {
                if !(whole || wanted.contains(&k.id)) || k.templates.is_empty() {
                    continue;
                }
                let template = &k.templates[rng.below(k.templates.len())];
                let text = resolve(template, &mut rng);
                if !text.is_empty() {
                    picks.push(Pick {
                        id: k.id.clone(),
                        label: k.label().to_string(),
                        text,
                    });
                }
            }
            if !picks.is_empty() {
                boxes.push(BoxPicks {
                    id: c.id.clone(),
                    label: c.label().to_string(),
                    picks,
                });
            }
        }
        // Cast leads; each term once, first mention wins.
        let mut seen = HashSet::new();
        let terms: Vec<String> = cast
            .iter()
            .cloned()
            .chain(
                boxes
                    .iter()
                    .flat_map(|b| b.picks.iter().map(|p| p.text.clone())),
            )
            .flat_map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
            .collect();
        Ok(Rolled {
            boxes,
            cast,
            text: terms.join(", "),
        })
    }
}

fn load(root: &str) -> Result<Randomizer, String> {
    Randomizer::load(&super::engine::data_dir(Path::new(root)))
}

#[tauri::command]
pub async fn native_forge_randomizer_list(root: String) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || load(&root).map(|r| r.list()))
        .await
        .map_err(|_| "The randomizer failed to load")?
        .inspect_err(|e| {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Prompt Forge randomizer: {e}"),
            )
        })
}

#[tauri::command]
pub async fn native_forge_randomizer_roll(root: String, request: Roll) -> Result<Rolled, String> {
    tokio::task::spawn_blocking(move || load(&root)?.roll(&request))
        .await
        .map_err(|_| "The randomizer failed")?
        .inspect_err(|e| {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Prompt Forge randomizer: {e}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(categories: Value) -> Randomizer {
        Randomizer {
            categories: serde_json::from_value(categories).unwrap(),
        }
    }

    #[test]
    fn templates_pick_one_from_each_group_and_keep_the_rest() {
        let mut rng = Rng::new(Some(7));
        for _ in 0..50 {
            let out = resolve("slimegirl, {gooey|gelatinous}, {halo|no halo:3}", &mut rng);
            assert!(out.starts_with("slimegirl, "), "{out}");
            assert!(
                !out.contains('{') && !out.contains("no halo") && !out.ends_with(','),
                "{out}"
            );
        }
        assert_eq!(
            options("a|b:3|c"),
            vec![("a".into(), 1), ("b".into(), 3), ("c".into(), 1)]
        );
        assert_eq!(resolve("{none}", &mut rng), "");
    }

    #[test]
    fn weights_are_honoured() {
        let mut rng = Rng::new(Some(1));
        let b = (0..2000)
            .filter(|_| resolve("{a|b:9}", &mut rng) == "b")
            .count();
        assert!((1650..1950).contains(&b), "{b}");
    }

    #[test]
    fn a_roll_leads_with_the_cast_says_each_term_once_and_limits_boxes() {
        let rz = r(json!([
            { "id": "hair", "label": "Hair", "children": [
                { "id": "hair_colour", "label": "Colour", "templates": ["{red hair}, fangs"] },
                { "id": "hair_len", "label": "Length", "templates": ["{long hair}"] },
                { "id": "hair_style", "label": "Style", "templates": ["{ponytail}"] } ] },
            { "id": "tiger", "label": "Mammals", "children": [ { "id": "tiger_t", "label": "Tiger", "templates": ["fangs, {stripes}"] } ] },
            { "id": "nsfw", "label": "NSFW", "children": [ { "id": "nsfw_x", "label": "X", "templates": ["{x}"] } ] }
        ]));
        let out = rz
            .roll(&Roll {
                categories: vec!["hair".into(), "tiger_t".into()],
                women: 2,
                seed: Some(3),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            out.text,
            "2girls, red hair, fangs, long hair, ponytail, stripes"
        );
        let limited = rz
            .roll(&Roll {
                categories: vec!["hair".into()],
                limits: HashMap::from([("hair".into(), 1)]),
                seed: Some(3),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited.boxes[0].picks.len(), 1);
        // A Surprise with nothing ticked never reaches NSFW.
        for seed in 0..40 {
            let s = rz
                .roll(&Roll {
                    surprise: true,
                    seed: Some(seed),
                    ..Default::default()
                })
                .unwrap();
            assert!(s.boxes.iter().all(|b| b.label != "NSFW"));
        }
        assert!(rz.roll(&Roll::default()).is_err());
        assert!(rz
            .roll(&Roll {
                categories: vec!["nope".into()],
                ..Default::default()
            })
            .unwrap_err()
            .contains("nope"));
    }

    /// `DEAS_FORGE_DATA=<dir> cargo test --lib -- --ignored real_randomizer --nocapture`
    #[test]
    #[ignore = "needs forge-data/randomizer.json"]
    fn real_randomizer() {
        let rz = Randomizer::load(Path::new(&std::env::var("DEAS_FORGE_DATA").unwrap())).unwrap();
        println!("{} boxes", rz.list().as_array().unwrap().len());
        for seed in 0..3 {
            let out = rz
                .roll(&Roll {
                    surprise: true,
                    women: 1,
                    seed: Some(seed),
                    ..Default::default()
                })
                .unwrap();
            println!("surprise {seed}: {}", out.text);
        }
        let out = rz
            .roll(&Roll {
                categories: vec!["hair".into(), "eyes".into()],
                women: 1,
                seed: Some(9),
                ..Default::default()
            })
            .unwrap();
        println!("hair + eyes: {}", out.text);
    }
}
