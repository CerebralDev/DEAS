//! Web search for Chat, ported from PromptForge's server/search.mjs. Off unless the
//! person turns it on in `<environment>/deas-chat.json`:
//!
//! - DuckDuckGo needs nothing installed. It reads the plain HTML page the site
//!   serves without JavaScript, so it is scraping (the markup can change) and the
//!   query goes to DuckDuckGo directly.
//! - SearXNG proxies the query and answers JSON, but somebody has to run one.
//!
//! Every failure comes back as a result with a reason, never an error: a search
//! that did not happen must not stop the answer.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

const FILE: &str = "deas-chat.json";
const MAX_RESULTS: usize = 5;
const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_PAGE: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "off", "duckduckgo" or "searxng".
    pub search: String,
    pub searxng_url: String,
}

impl Settings {
    pub fn enabled(&self) -> bool {
        matches!(self.search.as_str(), "duckduckgo" | "searxng")
    }
}

pub fn read(root: &Path) -> Settings {
    std::fs::read(root.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .map(|mut s| {
            if !s.enabled() {
                s.search = "off".into();
            }
            s
        })
        .unwrap_or(Settings {
            search: "off".into(),
            searxng_url: String::new(),
        })
}

pub fn write(root: &Path, s: &Settings) -> Result<(), String> {
    if !matches!(s.search.as_str(), "off" | "duckduckgo" | "searxng") {
        return Err("Search must be off, duckduckgo or searxng.".into());
    }
    if s.search == "searxng" {
        let url = reqwest::Url::parse(s.searxng_url.trim())
            .map_err(|_| "Enter the SearXNG address, e.g. http://127.0.0.1:8080")?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("The SearXNG address must start with http:// or https://".into());
        }
    }
    let tmp = root.join(format!("{FILE}.tmp"));
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, root.join(FILE)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub n: usize,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub query: String,
    pub results: Vec<Hit>,
    /// Why there is nothing, when there is nothing.
    pub detail: Option<String>,
}

fn domain_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_default()
}

fn decode_entities(t: &str) -> String {
    t.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// DuckDuckGo wraps every result link in a redirect; the real address is `uddg`.
fn unwrap_ddg(href: &str) -> Option<String> {
    let href = decode_entities(href);
    let url = href
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("uddg="))
        .map(percent_decode)
        .unwrap_or_else(|| href.to_string());
    let url = if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url
    };
    (url.starts_with("http://") || url.starts_with("https://")).then_some(url)
}

pub fn parse_duckduckgo(html: &str, limit: usize) -> Vec<Hit> {
    let snippets: Vec<String> = html
        .split("class=\"result__snippet\"")
        .skip(1)
        .map(|b| {
            let start = b.find('>').map_or(0, |i| i + 1);
            let end = b.find("</a>").unwrap_or(b.len());
            strip_tags(&b[start.min(end)..end])
        })
        .collect();
    let mut out = vec![];
    for (i, block) in html.split("class=\"result__a\"").skip(1).enumerate() {
        if out.len() >= limit {
            break;
        }
        let href = block
            .split("href=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or("");
        let start = block.find('>').map_or(0, |i| i + 1);
        let end = block.find("</a>").unwrap_or(block.len());
        let title = strip_tags(&block[start.min(end)..end]);
        let Some(url) = unwrap_ddg(href) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        out.push(Hit {
            n: out.len() + 1,
            domain: domain_of(&url),
            title,
            url,
            snippet: snippets.get(i).cloned().unwrap_or_default(),
        });
    }
    out
}

pub fn parse_searxng(v: &Value, limit: usize) -> Vec<Hit> {
    v["results"]
        .as_array()
        .map(|a| {
            a.iter()
                .take(limit)
                .enumerate()
                .map(|(i, r)| {
                    let url = r["url"].as_str().unwrap_or("").to_string();
                    Hit {
                        n: i + 1,
                        title: r["title"].as_str().unwrap_or("").trim().to_string(),
                        domain: domain_of(&url),
                        url,
                        snippet: r["content"]
                            .as_str()
                            .unwrap_or("")
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch(
    url: reqwest::Url,
    accept_json: bool,
) -> Result<(reqwest::StatusCode, Vec<u8>), String> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .build()
        .map_err(|_| "Cannot create the search client")?;
    let mut res = client
        .get(url)
        .header("accept-language", "en-US,en;q=0.9")
        .header(
            "accept",
            if accept_json {
                "application/json"
            } else {
                "text/html"
            },
        )
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "The search timed out.".to_string()
            } else {
                format!("Search failed: {e}")
            }
        })?;
    let status = res.status();
    let mut bytes = vec![];
    while let Some(c) = res
        .chunk()
        .await
        .map_err(|e| format!("Search failed: {e}"))?
    {
        if bytes.len() + c.len() > MAX_PAGE {
            break;
        }
        bytes.extend_from_slice(&c);
    }
    Ok((status, bytes))
}

pub async fn run(settings: &Settings, query: &str) -> Found {
    let q = query.trim().chars().take(400).collect::<String>();
    let none = |detail: String| Found {
        query: q.clone(),
        results: vec![],
        detail: Some(detail),
    };
    if !settings.enabled() {
        return none("Web search is switched off.".into());
    }
    if q.is_empty() {
        return none("Nothing to search for.".into());
    }
    let results = if settings.search == "searxng" {
        let Ok(mut url) = reqwest::Url::parse(settings.searxng_url.trim()) else {
            return none("The SearXNG address is not set.".into());
        };
        url.set_path(&format!("{}/search", url.path().trim_end_matches('/')));
        url.query_pairs_mut()
            .append_pair("q", &q)
            .append_pair("format", "json");
        match fetch(url, true).await {
            Ok((s, _)) if s.as_u16() == 403 => return none("SearXNG refused the JSON request (403): its settings.yml needs search.formats to include json.".into()),
            Ok((s, _)) if !s.is_success() => return none(format!("SearXNG answered {}.", s.as_u16())),
            Ok((_, b)) => match serde_json::from_slice::<Value>(&b) {
                Ok(v) => parse_searxng(&v, MAX_RESULTS),
                Err(_) => return none("SearXNG did not answer JSON.".into()),
            },
            Err(e) => return none(e),
        }
    } else {
        let mut url = reqwest::Url::parse("https://html.duckduckgo.com/html/").expect("static url");
        url.query_pairs_mut().append_pair("q", &q);
        match fetch(url, false).await {
            Ok((s, _)) if !s.is_success() => {
                return none(format!("DuckDuckGo answered {}.", s.as_u16()));
            }
            Ok((_, b)) => parse_duckduckgo(&String::from_utf8_lossy(&b), MAX_RESULTS),
            Err(e) => return none(e),
        }
    };
    let detail = results.is_empty().then(|| {
        "No results came back: the page may have changed, or the query found nothing.".to_string()
    });
    Found {
        query: q,
        results,
        detail,
    }
}

/// The results as context for the answer, with PromptForge's citation rule.
pub fn as_context(f: &Found) -> Option<String> {
    if f.results.is_empty() {
        return None;
    }
    let lines: Vec<String> = f
        .results
        .iter()
        .map(|r| {
            format!(
                "[{}] {} — {}\n    {}\n    {}",
                r.n, r.title, r.domain, r.snippet, r.url
            )
        })
        .collect();
    Some(format!(
        "Web search results for \"{}\", retrieved just now:\n\n{}\n\nUse these where they help and cite them inline as [1], [2] and so on, immediately after the claim they support. Do not cite a result you did not use. If they do not answer the question, say so and answer from what you know, making clear which is which.",
        f.query,
        lines.join("\n\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duckduckgo_html_unwraps_links_and_pairs_snippets() {
        let html = r#"<div class="result"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.example.com%2Fltx%3Fa%3D1&amp;rut=x">LTX <b>prompting</b> guide</a>
<a class="result__snippet" href="x">How to <b>prompt</b> LTX &amp; more.</a></div>
<div class="result"><a class="result__a" href="javascript:void(0)">bad</a><a class="result__snippet">s2</a></div>"#;
        let hits = parse_duckduckgo(html, 5);
        assert_eq!(
            hits,
            vec![Hit {
                n: 1,
                title: "LTX prompting guide".into(),
                url: "https://www.example.com/ltx?a=1".into(),
                domain: "example.com".into(),
                snippet: "How to prompt LTX & more.".into()
            }]
        );
    }

    #[test]
    fn searxng_json_and_the_context_block() {
        let v = serde_json::json!({ "results": [{ "title": " Wan 2.2 ", "url": "https://wan.video/docs", "content": "Wan  prompts\n describe motion" }] });
        let hits = parse_searxng(&v, 5);
        assert_eq!(hits[0].domain, "wan.video");
        assert_eq!(hits[0].snippet, "Wan prompts describe motion");
        let ctx = as_context(&Found {
            query: "wan".into(),
            results: hits,
            detail: None,
        })
        .unwrap();
        assert!(
            ctx.starts_with("Web search results for \"wan\"")
                && ctx.contains("[1] Wan 2.2 — wan.video")
        );
        assert!(as_context(&Found {
            query: "x".into(),
            results: vec![],
            detail: None
        })
        .is_none());
    }

    #[test]
    fn settings_default_off_and_validate() {
        let dir = std::env::temp_dir().join(format!("deas-chat-search-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read(&dir).search, "off");
        assert!(write(
            &dir,
            &Settings {
                search: "searxng".into(),
                searxng_url: "nope".into()
            }
        )
        .is_err());
        write(
            &dir,
            &Settings {
                search: "searxng".into(),
                searxng_url: "http://127.0.0.1:8080".into(),
            },
        )
        .unwrap();
        assert!(read(&dir).enabled());
        std::fs::remove_dir_all(&dir).ok();
    }
}
