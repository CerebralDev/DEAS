//! In-app spellcheck: the same Hunspell-compatible engine (`spellbook`) on every platform over
//! pinned dictionaries that the user installs explicitly. No WebView or OS dictionary involved.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager;

// Accept the bounded BCP47 subset language[-Script][-REGION], not POSIX locales/extensions.
fn valid_language(language: &str) -> bool {
    fn letters(value: &str, min: usize, max: usize) -> bool {
        (min..=max).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_alphabetic())
    }
    if language.len() > 12 {
        return false;
    }
    let mut parts = language.split('-');
    if !parts.next().is_some_and(|part| letters(part, 2, 3)) {
        return false;
    }
    let mut next = parts.next();
    if next.is_some_and(|part| letters(part, 4, 4)) {
        next = parts.next();
    }
    if let Some(region) = next {
        if !letters(region, 2, 2)
            && !(region.len() == 3 && region.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return false;
        }
    }
    parts.next().is_none()
}

/// A language without a published dictionary is a known, UI-explained limit, and no dictionary
/// yet is every first run; only real download/install failures are errors.
fn severity(code: &str) -> &'static str {
    if matches!(
        code,
        "SPELLCHECK_DICTIONARY_UNAVAILABLE" | "SPELLCHECK_DICTIONARY_MISSING"
    ) {
        "WARN"
    } else {
        "ERROR"
    }
}

fn failure(code: &str) -> String {
    let message = match code {
        "SPELLCHECK_DICTIONARY_MISSING" => {
            "No dictionary is installed for the selected language; install it under Backend → Language"
        }
        "SPELLCHECK_DICTIONARY_UNAVAILABLE" => {
            "No Hunspell dictionary is published for this language"
        }
        "SPELLCHECK_DICTIONARY_FAILED" => "Could not download or install the spellcheck dictionary",
        "SPELLCHECK_INVALID_LANGUAGE" => "Invalid spellchecking language tag",
        _ => "Could not load the spellcheck dictionary",
    };
    let level = severity(code);
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|error| error.into_inner());
    if last.is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!(
                "{level}: PromptSpellcheck operation=spellcheck: {code}: {message} (repeats limited to once per minute)"
            ),
        );
        *last = Some(Instant::now());
    }
    code.to_owned()
}

/// A pinned Hunspell pair for the spellcheck languages that have a published dictionary.
struct Dictionary {
    language: &'static str,
    /// (sha256, size)
    dic: (&'static str, u64),
    aff: (&'static str, u64),
    license: &'static str,
}

// Pinned commit: the same files the sizes/hashes below were verified against.
const DICTIONARY_COMMIT: &str = "8cfea406b505e4d7df52d5a19bce525df98c54ab";
const MAX_DICTIONARY_BYTES: u64 = 12 * 1024 * 1024;
const MAX_WORDS: usize = 2000;
const MAX_WORD_CHARS: usize = 64;
const MAX_SUGGESTIONS: usize = 8;
const DICTIONARIES: [Dictionary; 4] = [
    Dictionary {
        language: "de",
        dic: (
            "b5c781a0cf6f285fb6b9b8ab02fbea104b987104a1efdda8a835837e89e3ec77",
            1_118_194,
        ),
        aff: (
            "57fdd1b16aac2131003c91e0cf2a488becb970382a402a9ce089307301cb3ef0",
            19_199,
        ),
        license: "igerman98 · GPL-2.0/3",
    },
    Dictionary {
        language: "en",
        dic: (
            "f0b1a234bd178bdd01875b2a392a9647f888b8fe879f79c52aae62c2759b3647",
            551_762,
        ),
        aff: (
            "8ae1f19d4840d957728ad90555d5a8dff6cc5c046279c95ff0c00fc0a0136c7b",
            3_086,
        ),
        license: "SCOWL · public domain",
    },
    Dictionary {
        language: "fr",
        dic: (
            "984e933237bc1224a48f42828233be9b03228260ef67aa8e2bdddcd03a26230d",
            1_229_133,
        ),
        aff: (
            "05a735d34c912e4e381ff08ee7c747923ccf5cf9dca81d8467982fa1ca51c2b7",
            199_870,
        ),
        license: "Dictionnaires français · MPL",
    },
    Dictionary {
        language: "tr",
        dic: (
            "dc1d8f7724f4aefd117274678de431e4ea1765f2e02e6983b6ca48cbf5a7fc7c",
            9_117_192,
        ),
        aff: (
            "6a390c79b054ad427cf9a6bf6de43463953cc1918e7f727e9dd4ebd4f9702f42",
            240_814,
        ),
        license: "MIT",
    },
];

/// The dictionary for a BCP47 tag's base language; ja/zh have no Hunspell dictionary upstream.
fn dictionary(language: &str) -> Option<&'static Dictionary> {
    let base = language
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    DICTIONARIES.iter().find(|entry| entry.language == base)
}

/// DEAS-owned dictionary folder inside the app data dir; identical layout on every platform.
fn dictionary_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("dictionaries"))
        .map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())
}

fn pair_paths(dir: &Path, language: &str) -> (PathBuf, PathBuf) {
    (
        dir.join(format!("{language}.dic")),
        dir.join(format!("{language}.aff")),
    )
}

async fn fetch_dictionary(
    client: &reqwest::Client,
    url: &str,
    size: u64,
    sha256: &str,
) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?;
    if response.status() != reqwest::StatusCode::OK {
        return Err("SPELLCHECK_DICTIONARY_FAILED".to_owned());
    }
    if response
        .content_length()
        .is_some_and(|length| length != size || length > MAX_DICTIONARY_BYTES)
    {
        return Err("SPELLCHECK_DICTIONARY_FAILED".to_owned());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    let mut digest = Sha256::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?
    {
        if bytes.len() as u64 + chunk.len() as u64 > size {
            return Err("SPELLCHECK_DICTIONARY_FAILED".to_owned());
        }
        digest.update(&chunk);
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != size || format!("{:x}", digest.finalize()) != sha256 {
        return Err("SPELLCHECK_DICTIONARY_FAILED".to_owned());
    }
    Ok(bytes)
}

/// `FLAG num` dictionaries may use flag `0`, which Nuspell/spellbook reject; renumber it to one
/// past the highest flag in use. Both files change together, so the rewritten pair stays coherent.
fn normalize_zero_flag(aff: &str, dic: &str) -> Option<(String, String)> {
    let numeric = aff.lines().any(|line| line.trim_end() == "FLAG num");
    let zero_rule = |line: &str| {
        let mut parts = line.split_whitespace();
        matches!(parts.next(), Some("SFX" | "PFX")) && parts.next() == Some("0")
    };
    if !numeric || !aff.lines().any(zero_rule) {
        return None;
    }
    let highest = aff
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            matches!(parts.next(), Some("SFX" | "PFX"))
                .then(|| parts.next()?.parse::<u32>().ok())?
        })
        .max()
        .unwrap_or(0);
    let replacement = (highest + 1).to_string();
    let aff = aff
        .lines()
        .map(|line| {
            if zero_rule(line) {
                let mut parts = line.splitn(3, ' ');
                let kind = parts.next().unwrap_or_default();
                parts.next();
                format!("{kind} {replacement} {}", parts.next().unwrap_or_default())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dic = dic
        .lines()
        .map(|line| match line.split_once('/') {
            Some((word, flags)) => {
                let flags = flags
                    .split(',')
                    .map(|flag| {
                        if flag == "0" {
                            replacement.as_str()
                        } else {
                            flag
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{word}/{flags}")
            }
            None => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some((aff + "\n", dic + "\n"))
}

fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let partial = target.with_extension("partial");
    std::fs::write(&partial, bytes).map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?;
    std::fs::rename(&partial, target).map_err(|_| {
        let _ = std::fs::remove_file(&partial);
        "SPELLCHECK_DICTIONARY_FAILED".to_owned()
    })
}

type Loaded = Mutex<HashMap<String, Arc<spellbook::Dictionary>>>;
fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(Default::default)
}

fn load(app: &tauri::AppHandle, language: &str) -> Result<Arc<spellbook::Dictionary>, String> {
    let entry = dictionary(language).ok_or("SPELLCHECK_DICTIONARY_UNAVAILABLE")?;
    if let Some(dict) = loaded()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(entry.language)
    {
        return Ok(dict.clone());
    }
    let (dic, aff) = pair_paths(&dictionary_dir(app)?, entry.language);
    let aff = std::fs::read_to_string(aff).map_err(|_| "SPELLCHECK_DICTIONARY_MISSING")?;
    let dic = std::fs::read_to_string(dic).map_err(|_| "SPELLCHECK_DICTIONARY_MISSING")?;
    let dict = Arc::new(
        spellbook::Dictionary::new(&aff, &dic).map_err(|_| "SPELLCHECK_DICTIONARY_FAILED")?,
    );
    loaded()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(entry.language.to_owned(), dict.clone());
    Ok(dict)
}

#[derive(serde::Serialize)]
pub struct SpellcheckStatus {
    /// A pinned pair exists for this language.
    available: bool,
    /// The pair is installed in the DEAS dictionary folder.
    installed: bool,
    license: Option<String>,
    bytes: Option<u64>,
}

/// Read-only: no network, no writes.
#[tauri::command]
pub fn native_spellcheck_status(
    app: tauri::AppHandle,
    language: String,
) -> Result<SpellcheckStatus, String> {
    if !valid_language(&language) {
        return Err("SPELLCHECK_INVALID_LANGUAGE".to_owned());
    }
    Ok(match dictionary(&language) {
        Some(entry) => {
            let (dic, aff) = pair_paths(&dictionary_dir(&app)?, entry.language);
            SpellcheckStatus {
                available: true,
                installed: dic.is_file() && aff.is_file(),
                license: Some(entry.license.to_owned()),
                bytes: Some(entry.dic.1 + entry.aff.1),
            }
        }
        None => SpellcheckStatus {
            available: false,
            installed: false,
            license: None,
            bytes: None,
        },
    })
}

#[derive(serde::Serialize)]
pub struct SpellcheckDictionary {
    /// Absolute path of the installed `.dic` file.
    path: String,
    license: String,
}

/// Explicit install of the pinned pair; nothing is downloaded by reading settings or typing.
#[tauri::command]
pub async fn native_spellcheck_install(
    app: tauri::AppHandle,
    language: String,
) -> Result<SpellcheckDictionary, String> {
    if !valid_language(&language) {
        return Err(failure("SPELLCHECK_INVALID_LANGUAGE"));
    }
    let entry =
        dictionary(&language).ok_or_else(|| failure("SPELLCHECK_DICTIONARY_UNAVAILABLE"))?;
    let dir = dictionary_dir(&app).map_err(|code| failure(&code))?;
    std::fs::create_dir_all(&dir).map_err(|_| failure("SPELLCHECK_DICTIONARY_FAILED"))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| failure("SPELLCHECK_DICTIONARY_FAILED"))?;
    let base = format!(
        "https://raw.githubusercontent.com/wooorm/dictionaries/{DICTIONARY_COMMIT}/dictionaries/{}/index",
        entry.language
    );
    let result: Result<(), String> = async {
        let dic =
            fetch_dictionary(&client, &format!("{base}.dic"), entry.dic.1, entry.dic.0).await?;
        let aff =
            fetch_dictionary(&client, &format!("{base}.aff"), entry.aff.1, entry.aff.0).await?;
        let aff_text =
            String::from_utf8(aff).map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?;
        let dic_text =
            String::from_utf8(dic).map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?;
        let (aff_text, dic_text) =
            normalize_zero_flag(&aff_text, &dic_text).unwrap_or((aff_text, dic_text));
        // Parse before publishing so a broken pair never becomes the installed one.
        spellbook::Dictionary::new(&aff_text, &dic_text)
            .map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?;
        let (dic_path, aff_path) = pair_paths(&dir, entry.language);
        write_atomic(&aff_path, aff_text.as_bytes())?;
        write_atomic(&dic_path, dic_text.as_bytes())?;
        Ok(())
    }
    .await;
    result.map_err(|code| failure(&code))?;
    loaded()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(entry.language);
    Ok(SpellcheckDictionary {
        path: pair_paths(&dir, entry.language).0.display().to_string(),
        license: entry.license.to_owned(),
    })
}

/// Returns the words from `words` that the dictionary does not know. Over-long words are
/// skipped (never flagged) rather than checked.
#[tauri::command]
pub async fn native_spellcheck_check(
    app: tauri::AppHandle,
    language: String,
    words: Vec<String>,
) -> Result<Vec<String>, String> {
    if !valid_language(&language) {
        return Err("SPELLCHECK_INVALID_LANGUAGE".to_owned());
    }
    if words.len() > MAX_WORDS {
        return Err("SPELLCHECK_TOO_MANY_WORDS".to_owned());
    }
    let dict = tokio::task::spawn_blocking(move || load(&app, &language))
        .await
        .map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?
        .map_err(|code| failure(&code))?;
    Ok(words
        .into_iter()
        .filter(|word| word.chars().count() <= MAX_WORD_CHARS && !dict.check(word))
        .collect())
}

#[tauri::command]
pub async fn native_spellcheck_suggest(
    app: tauri::AppHandle,
    language: String,
    word: String,
) -> Result<Vec<String>, String> {
    if !valid_language(&language) {
        return Err("SPELLCHECK_INVALID_LANGUAGE".to_owned());
    }
    if word.is_empty() || word.chars().count() > MAX_WORD_CHARS {
        return Ok(Vec::new());
    }
    let dict = tokio::task::spawn_blocking(move || load(&app, &language))
        .await
        .map_err(|_| "SPELLCHECK_DICTIONARY_FAILED".to_owned())?
        .map_err(|code| failure(&code))?;
    let mut suggestions = Vec::new();
    dict.suggest(&word, &mut suggestions);
    suggestions.truncate(MAX_SUGGESTIONS);
    Ok(suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bundled_languages_and_safe_system_language_tags() {
        for language in [
            "en",
            "de",
            "fr",
            "ja",
            "tr",
            "zh",
            "en-US",
            "de-DE",
            "zh-Hans",
            "zh-Hant-TW",
            "es-419",
            "fil-PH",
            "EN-us",
        ] {
            assert!(valid_language(language), "{language}");
        }
    }

    #[test]
    fn rejects_unsafe_malformed_and_out_of_subset_tags() {
        for language in [
            "",
            "e",
            "english",
            "en_US",
            "en-US.UTF-8",
            " en",
            "en ",
            "en\0",
            "en\n",
            "éé",
            "en/US",
            "en--US",
            "en-",
            "en-US-",
            "en-Latn-US-extra",
            "en-US-Latn",
            "en-u-ca",
            "x-private",
            "en-1234",
            "en-1A",
            "en-ABCDE",
            "en-<script>",
        ] {
            assert!(!valid_language(language), "{language:?}");
        }
    }

    #[test]
    fn failure_is_shared_logged_without_untrusted_input_and_rate_limited() {
        let message = "SPELLCHECK_DICTIONARY_MISSING";
        assert_eq!(failure(message), message);
        assert_eq!(failure(message), message);
        let matching: Vec<_> = crate::logs::native_logs(0)
            .into_iter()
            .filter(|entry| {
                entry
                    .text
                    .contains("PromptSpellcheck operation=spellcheck:")
            })
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].source, crate::logs::Source::Deas);
        assert!(matching[0].text.contains(message));
        assert!(matching[0].text.contains("Backend → Language"));
        assert!(!matching[0].text.contains("<script>"));
        assert!(matching[0].text.starts_with("WARN: "));
    }

    #[test]
    fn unpublished_language_is_a_warning_not_an_error() {
        assert_eq!(severity("SPELLCHECK_DICTIONARY_UNAVAILABLE"), "WARN");
        assert_eq!(severity("SPELLCHECK_DICTIONARY_MISSING"), "WARN");
        for code in [
            "SPELLCHECK_DICTIONARY_FAILED",
            "SPELLCHECK_INVALID_LANGUAGE",
            "OTHER",
        ] {
            assert_eq!(severity(code), "ERROR");
        }
    }

    #[test]
    fn dictionary_pairs_are_pinned_for_published_languages_only() {
        for (language, base) in [
            ("de", "de"),
            ("de-DE", "de"),
            ("EN-us", "en"),
            ("fr", "fr"),
            ("tr_TR", "tr"),
        ] {
            let entry = dictionary(language).unwrap_or_else(|| panic!("{language}"));
            assert_eq!(entry.language, base);
            assert_eq!(entry.dic.0.len(), 64);
            assert!(entry.dic.0.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(entry.dic.1 > 0 && entry.dic.1 <= MAX_DICTIONARY_BYTES);
            assert_eq!(entry.aff.0.len(), 64);
            assert!(entry.aff.1 > 0 && entry.aff.1 <= MAX_DICTIONARY_BYTES);
        }
        for language in ["ja", "zh", "zh-Hans", "es", "ko"] {
            assert!(dictionary(language).is_none(), "{language}");
        }
    }

    #[test]
    fn zero_flag_is_renumbered_only_for_numeric_flag_dictionaries() {
        let aff = "SET UTF-8\nFLAG num\n\nSFX 0 N 1\nSFX 0 0 de .\n\nSFX 12 N 1\nSFX 12 0 ler .\n";
        let dic = "3\nev/0,12\nokul/12\nsu\n";
        let (aff2, dic2) = normalize_zero_flag(aff, dic).expect("renumbered");
        assert!(aff2.contains("SFX 13 N 1\nSFX 13 0 de ."));
        assert!(!aff2.contains("SFX 0 "));
        assert!(dic2.contains("ev/13,12\n") && dic2.contains("okul/12\n") && dic2.contains("su\n"));
        let dict = spellbook::Dictionary::new(&aff2, &dic2).expect("parses after renumbering");
        assert!(dict.check("evde") && dict.check("evler") && !dict.check("sude"));
        assert!(
            spellbook::Dictionary::new(aff, dic).is_err(),
            "unchanged pair is rejected"
        );
        assert!(normalize_zero_flag("SET UTF-8\nSFX A Y 1\nSFX A 0 s .\n", "1\ncat/A\n").is_none());
    }

    #[test]
    fn check_and_suggest_work_on_an_inline_dictionary() {
        let aff = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZ'\nSFX S Y 1\nSFX S 0 s .\n";
        let dic = "3\nhouse/S\nbeautiful\nlight\n";
        let dict = spellbook::Dictionary::new(aff, dic).unwrap();
        assert!(dict.check("houses") && dict.check("beautiful"));
        assert!(!dict.check("beautifull"));
        let mut suggestions = Vec::new();
        dict.suggest("beautifull", &mut suggestions);
        assert_eq!(suggestions.first().map(String::as_str), Some("beautiful"));
    }

    #[test]
    #[ignore = "network: verifies the pinned Hunspell URLs, sizes and digests, and that every pair parses"]
    fn downloads_pinned_dictionary_pairs_and_parses_them() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for entry in &DICTIONARIES {
            let base = format!(
                "https://raw.githubusercontent.com/wooorm/dictionaries/{DICTIONARY_COMMIT}/dictionaries/{}/index",
                entry.language
            );
            let (dic, aff) = runtime.block_on(async {
                (
                    fetch_dictionary(&client, &format!("{base}.dic"), entry.dic.1, entry.dic.0)
                        .await
                        .unwrap(),
                    fetch_dictionary(&client, &format!("{base}.aff"), entry.aff.1, entry.aff.0)
                        .await
                        .unwrap(),
                )
            });
            let aff = String::from_utf8(aff).unwrap();
            let dic = String::from_utf8(dic).unwrap();
            let (aff, dic) = normalize_zero_flag(&aff, &dic).unwrap_or((aff, dic));
            let dict = spellbook::Dictionary::new(&aff, &dic)
                .unwrap_or_else(|e| panic!("{}: {e:?}", entry.language));
            let sample = match entry.language {
                "de" => "Sonnenuntergang",
                "en" => "beautiful",
                "fr" => "éclairage",
                _ => "güzel",
            };
            assert!(dict.check(sample), "{}: {sample}", entry.language);
        }
    }
}
