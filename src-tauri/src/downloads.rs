//! Optional staged data downloads. Catalogs never grant execution or credential access.
#[path = "downloads/fs.rs"]
pub(crate) mod fs;
#[path = "downloads_models.rs"]
pub mod models;
#[path = "downloads_providers.rs"]
mod providers;
#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
use crate::auth::AuthState;
use reqwest::{header::HeaderValue, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
const MAX_RESOURCES: usize = 50;
const MAX_FILES: usize = 16;
/// One resource: a single 22–26 GB checkpoint must fit; the aggregate stays separate.
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_JOB_BYTES: u64 = 256 * 1024 * 1024 * 1024;
const MAX_JSON: usize = 2 * 1024 * 1024;
/// Catalog/metadata requests; downloads themselves are bounded by an idle read timeout.
const MAX_HTTP_SECONDS: u64 = 3600;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub downloadable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// HTTPS image shown beside the resource; never fetched natively.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// ComfyUI model folder name (checkpoints, loras, …) when the source declares a type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Source signals restricted access (non-public availability or paid access).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auth: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nsfw: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Version publication date supplied by the provider (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Model version name; groups the files (quants, formats) of one release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Display-only file variant, e.g. "fp8 · pruned · SafeTensor · ≈13.5 GB"; never used for verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provider {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub origin: String,
    pub scopes: Vec<String>,
    pub anonymous_notice: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub label: String,
    pub provider_id: String,
    pub scope: String,
    pub locator: String,
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// "image" or "video" generation model; used for filtering only.
    /// Base model family label (e.g. "Wan 2.2", "LTX 2.3"); used for filtering only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Version publication date supplied by the provider (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    /// Source rejects anonymous downloads (observed 401 when the catalog was generated).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auth: bool,
    /// Adult content; hidden unless the user switched the view to NSFW.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nsfw: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub entries: Vec<Entry>,
}
impl Catalog {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 1024 * 1024 {
            return Err("Catalog exceeds 1 MiB".into());
        }
        let c: Self = toml::from_str(text).map_err(|_| "Invalid strict data-only TOML schema")?;
        c.validate()?;
        Ok(c)
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.providers.len() > 32 || self.entries.len() > 500 {
            return Err("Catalog version or bounds invalid".into());
        }
        let mut ids = HashSet::new();
        for p in &self.providers {
            safe_id(&p.id)?;
            bounded_text(&p.label, 160)?;
            bounded_text(&p.anonymous_notice, 1000)?;
            if !ids.insert(&p.id) {
                return Err("Duplicate provider ID".into());
            }
            let scopes = match p.kind.as_str() {
                "huggingface" => vec!["user", "repo"],
                "civitai" => vec!["user", "model", "search"],
                "github" => vec!["user", "repo"],
                "http" => vec!["index"],
                "ollama" => vec!["backend"],
                _ => return Err("Unknown compiled provider adapter".into()),
            };
            if p.scopes.is_empty()
                || p.scopes.len() > scopes.len()
                || p.scopes.iter().any(|s| !scopes.contains(&s.as_str()))
                || p.scopes.iter().collect::<HashSet<_>>().len() != p.scopes.len()
            {
                return Err("Invalid adapter scopes".into());
            }
            let u = validate_url(&p.origin, p.kind == "ollama", false)?;
            if u.path() != "/" || p.origin != u.origin().ascii_serialization() {
                return Err("Origin must be an exact canonical origin without slash".into());
            }
            let official = match p.kind.as_str() {
                "huggingface" => Some(("huggingface", "https://huggingface.co")),
                "github" => Some(("github", "https://api.github.com")),
                "civitai" => Some(("civitai", "https://civitai.com")),
                _ => None,
            };
            if let Some((id, origin)) = official {
                if p.id != id || p.origin != origin {
                    return Err("Official adapter ID/origin cannot be overridden".into());
                }
            }
            if official.is_none() && matches!(p.id.as_str(), "huggingface" | "github" | "civitai") {
                return Err("Reserved authentication provider ID".into());
            }
        }
        let mut ids = HashSet::new();
        for e in &self.entries {
            safe_id(&e.id)?;
            bounded_text(&e.label, 240)?;
            bounded_text(&e.locator, 4096)?;
            safe_name(&e.target)?;
            validate_kind(&e.kind)?;
            if let Some(date) = &e.published_at {
                bounded_text(date, 64)?;
            }
            if let Some(base) = &e.base {
                bounded_text(base, 64)?;
            }
            if e.media
                .as_deref()
                .is_some_and(|m| !matches!(m, "image" | "video"))
            {
                return Err("Entry media must be image or video".into());
            }
            validate_metadata(e.sha256.as_deref(), e.license.as_deref(), None)?;
            if let Some(preview) = &e.preview {
                validate_url(preview, false, true)?;
            }
            if !ids.insert(&e.id) || !self.provider(&e.provider_id)?.scopes.contains(&e.scope) {
                return Err("Duplicate entry or unsupported scope".into());
            }
            if self.provider(&e.provider_id)?.kind == "http" {
                same_origin_url(
                    self.provider(&e.provider_id)?,
                    e.locator
                        .strip_prefix("direct:")
                        .or_else(|| e.locator.strip_prefix("index:"))
                        .unwrap_or(&e.locator),
                )?;
            }
        }
        Ok(())
    }
    fn load() -> Result<Self, String> {
        let mut c = Self::parse(include_str!("../downloads/default.toml"))?;
        // Bundled entry-only catalogs reuse the compiled providers and are validated together.
        let extra: Catalog = toml::from_str(include_str!("../downloads/darksidewalker.toml"))
            .map_err(|_| "Invalid bundled catalog")?;
        c.entries.extend(extra.entries);
        c.validate()?;
        Ok(c)
    }
    pub fn provider(&self, id: &str) -> Result<&Provider, String> {
        self.providers
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "Unknown catalog provider".into())
    }
}
fn bounded_text(s: &str, max: usize) -> Result<(), String> {
    if s.is_empty() || s.len() > max || s.chars().any(char::is_control) {
        Err("Invalid text field".into())
    } else {
        Ok(())
    }
}
fn safe_id(s: &str) -> Result<(), String> {
    if s.is_empty()
        || s.len() > 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Err("Invalid stable ID".into())
    } else {
        Ok(())
    }
}
fn safe_name(s: &str) -> Result<(), String> {
    bounded_text(s, 200)?;
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if s == "."
        || s == ".."
        || s.starts_with('.')
        || s.ends_with(['.', ' '])
        || s.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
    {
        Err("Unsafe filename or subfolder".into())
    } else {
        Ok(())
    }
}
fn validate_kind(s: &str) -> Result<(), String> {
    if matches!(
        s,
        "file" | "model" | "dataset" | "repo" | "workflow" | "preset"
    ) {
        Ok(())
    } else {
        Err("Unsupported resource kind".into())
    }
}
fn validate_metadata(
    hash: Option<&str>,
    license: Option<&str>,
    size: Option<u64>,
) -> Result<(), String> {
    if hash.is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("Invalid SHA-256".into());
    }
    if size.is_some_and(|s| s > MAX_FILE_BYTES) {
        return Err("Resource exceeds the 128 GiB limit".into());
    }
    if let Some(s) = license {
        bounded_text(s, 512)?;
    }
    Ok(())
}
fn validate_url(s: &str, local: bool, query: bool) -> Result<Url, String> {
    if s.len() > 8192
        || s.trim() != s
        || s.contains('\\')
        || s.bytes().any(|b| b.is_ascii_control())
        || s.split_once("://")
            .is_some_and(|(_, s)| s.split('/').next().unwrap_or("").contains('@'))
    {
        return Err("Unsafe source URL".into());
    }
    let u = Url::parse(s).map_err(|_| "Invalid URL")?;
    if !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || (!query && u.query().is_some())
    {
        return Err("Source URL contains credentials, query or fragment".into());
    }
    let host = u.host_str().ok_or("URL requires host")?;
    // Plain HTTP transport exists only in test builds for actual scratch fixtures.
    #[cfg(test)]
    if u.scheme() == "http" && host == "127.0.0.1" {
        return Ok(u);
    }
    if local {
        if !matches!(host, "127.0.0.1" | "localhost" | "[::1]")
            || !matches!(u.scheme(), "http" | "https")
        {
            return Err("Ollama must be explicitly loopback-bound".into());
        }
    } else {
        if u.scheme() != "https" || u.port_or_known_default() != Some(443) {
            return Err("Remote sources require standard HTTPS".into());
        }
        if matches!(host, "localhost" | "[::1]")
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.parse::<std::net::IpAddr>().is_ok()
        {
            return Err("Remote sources require a public DNS hostname".into());
        }
    }
    Ok(u)
}
fn same_origin_url(p: &Provider, s: &str) -> Result<Url, String> {
    let u = validate_url(s, p.kind == "ollama", false)?;
    if u.origin().ascii_serialization() != p.origin {
        return Err("Source must match configured provider origin".into());
    }
    Ok(u)
}
struct PublicDns;
impl reqwest::dns::Resolve for PublicDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 443))
                .await?
                .collect();
            if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
                return Err(
                    Box::new(std::io::Error::other("Non-public DNS destination blocked"))
                        as Box<dyn std::error::Error + Send + Sync>,
                );
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}
fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => {
            let b = v.octets();
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_multicast()
                && !v.is_broadcast()
                && !v.is_unspecified()
                && b[0] != 0
                && b[0] < 224
                && !(b[0] == 100 && (64..=127).contains(&b[1]))
                && !(b[0] == 198 && (b[1] == 18 || b[1] == 19))
        }
        std::net::IpAddr::V6(v) => {
            let b = v.octets();
            !v.is_loopback()
                && !v.is_unspecified()
                && !v.is_multicast()
                && (b[0] & 0xe0) == 0x20 // global unicast only, not site-local/translation ranges
                && !(b[0] == 0x20 && b[1] == 0x01 && b[2] == 0x0d && b[3] == 0xb8)
                && v.to_ipv4_mapped().is_none()
        }
    }
}
fn client_for(local: bool, timeout: Option<Duration>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .user_agent("DEAS/0.1 optional-downloads");
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    if !local {
        builder = builder.dns_resolver(Arc::new(PublicDns));
    }
    builder
        .build()
        .map_err(|_| "Cannot construct HTTP client".into())
}
fn network_error() -> String {
    "Network request failed (details withheld to protect credentials)".into()
}
fn credential(
    auth: &AuthState,
    p: &Provider,
    enabled: bool,
) -> Result<Option<HeaderValue>, String> {
    if !enabled {
        return Ok(None);
    }
    let token = auth.credential(&p.id, &p.origin)?;
    let token = token.ok_or("No scoped credential configured")?;
    let mut h = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| "Invalid credential header")?;
    h.set_sensitive(true);
    Ok(Some(h))
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowseRequest {
    pub provider_id: String,
    pub scope: String,
    pub locator: String,
    #[serde(default)]
    pub cursor: Option<String>,
    pub authenticated: bool,
    /// Ask the source for adult content too; off by default.
    #[serde(default)]
    pub nsfw: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub resources: Vec<Resource>,
    pub next_cursor: Option<String>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRequest {
    pub provider_id: String,
    pub resources: Vec<Resource>,
    pub parent_path: String,
    pub folder_name: String,
    pub authenticated: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlan {
    pub id: String,
    pub target: String,
    pub resources: Vec<Resource>,
    pub total_known_bytes: u64,
    pub unknown_size_count: usize,
    pub warnings: Vec<String>,
    pub can_execute: bool,
}
struct StoredPlan {
    view: DownloadPlan,
    provider: Provider,
    authenticated: bool,
    parent: PathBuf,
    subfolder: String,
    identity: (u64, u64),
    created: Instant,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub state: String,
    pub completed: usize,
    pub total: usize,
    pub bytes_downloaded: u64,
    pub error: Option<String>,
}
impl Default for DownloadStatus {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            completed: 0,
            total: 0,
            bytes_downloaded: 0,
            error: None,
        }
    }
}
struct Continuation {
    request: BrowseRequest,
    url: Url,
    created: Instant,
}
struct Inner {
    catalog: Result<Catalog, String>,
    known: HashMap<(String, String), (Resource, String)>,
    cursors: HashMap<String, Continuation>,
    plans: HashMap<String, StoredPlan>,
    status: DownloadStatus,
    cancel: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct DownloadEngine {
    inner: Arc<Mutex<Inner>>,
}
impl Default for DownloadEngine {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                catalog: Catalog::load(),
                known: HashMap::new(),
                cursors: HashMap::new(),
                plans: HashMap::new(),
                status: DownloadStatus::default(),
                cancel: Arc::new(AtomicBool::new(false)),
            })),
        }
    }
}
impl DownloadEngine {
    pub fn catalog(&self) -> Result<Catalog, String> {
        self.inner
            .lock()
            .map_err(|_| "Download state unavailable")?
            .catalog
            .clone()
    }
    pub fn import(&self, text: String) -> Result<Catalog, String> {
        if text.len() > 1024 * 1024 {
            return Err("Catalog exceeds 1 MiB".into());
        }
        // Validate after merging so new entries may reference existing providers;
        // provider definitions still cannot overwrite any trusted registration.
        let extra: Catalog =
            toml::from_str(&text).map_err(|_| "Invalid strict data-only TOML schema")?;
        if extra.version != 1 {
            return Err("Unsupported catalog version".into());
        }
        let mut i = self
            .inner
            .lock()
            .map_err(|_| "Download state unavailable")?;
        if i.status.state == "running" {
            return Err("Cannot import while downloading".into());
        }
        let mut c = i.catalog.clone()?;
        c.providers.extend(extra.providers);
        c.entries.extend(extra.entries);
        c.validate()?;
        i.catalog = Ok(c.clone());
        i.known.clear();
        i.plans.clear();
        i.cursors.clear();
        Ok(c)
    }
    pub async fn browse(
        &self,
        mut request: BrowseRequest,
        auth: &AuthState,
    ) -> Result<Listing, String> {
        bounded_text(&request.locator, 4096)?;
        let c = self.catalog()?;
        let p = c.provider(&request.provider_id)?;
        if !p.scopes.contains(&request.scope) {
            return Err("Scope not declared by provider".into());
        }
        let next = if let Some(cursor) = request.cursor.take() {
            let mut i = self
                .inner
                .lock()
                .map_err(|_| "Download state unavailable")?;
            let n = i
                .cursors
                .remove(&cursor)
                .ok_or("Unknown or consumed continuation")?;
            if n.created.elapsed() > Duration::from_secs(600) || n.request != request {
                return Err("Continuation expired or request changed".into());
            }
            Some(n.url)
        } else {
            None
        };
        let h = credential(auth, p, request.authenticated)?;
        let mut page = tokio::time::timeout(
            Duration::from_secs(30),
            providers::list(p, &request, next, h),
        )
        .await
        .map_err(|_| "Browse exceeded 30 seconds")??;
        if p.kind == "http"
            && page.resources.len() == 1
            && (request.locator.starts_with("direct:") || !request.locator.ends_with(".json"))
        {
            if let Some(entry) = c.entries.iter().find(|e| {
                e.provider_id == p.id && e.scope == request.scope && e.locator == request.locator
            }) {
                page.resources[0].license = entry.license.clone();
                page.resources[0].sha256 = entry.sha256.clone();
                page.resources[0].kind = entry.kind.clone();
            }
        }
        let mut i = self
            .inner
            .lock()
            .map_err(|_| "Download state unavailable")?;
        if i.known.len() + page.resources.len() > 1000 {
            i.known.clear();
        }
        for r in &page.resources {
            validate_resource(p, r)?;
            if r.downloadable {
                let filename = providers::filename(r)?;
                i.known
                    .insert((p.id.clone(), r.id.clone()), (r.clone(), filename));
            }
        }
        let cursor = if let Some(url) = page.next.take() {
            if i.cursors.len() >= 64 {
                i.cursors.clear();
            }
            let id = uuid::Uuid::new_v4().to_string();
            i.cursors.insert(
                id.clone(),
                Continuation {
                    request,
                    url,
                    created: Instant::now(),
                },
            );
            Some(id)
        } else {
            None
        };
        if !p.anonymous_notice.is_empty() {
            page.warnings.push(p.anonymous_notice.clone());
        }
        Ok(Listing {
            resources: page.resources,
            next_cursor: cursor,
            warnings: page.warnings,
        })
    }
    pub fn plan(&self, mut request: PlanRequest) -> Result<DownloadPlan, String> {
        if request.resources.is_empty() || request.resources.len() > MAX_FILES {
            return Err("Select 1–16 files".into());
        }
        let mut parent = PathBuf::from(request.parent_path);
        let target = if request.folder_name.is_empty() {
            parent.clone()
        } else {
            parent.join(&request.folder_name)
        };
        // Only one absent leaf can reuse the anchored, confirmation-only new-folder path.
        if request.folder_name.is_empty()
            && matches!(std::fs::symlink_metadata(&parent), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            let leaf = parent
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Unsafe destination folder")?;
            safe_name(leaf)?;
            request.folder_name = leaf.to_owned();
            parent = parent
                .parent()
                .ok_or("Missing destination parent")?
                .to_owned();
        }
        let anchor = fs::Anchor::open(&parent)?;
        // Empty folder name stages directly into the existing parent (e.g. ComfyUI/models/loras);
        // files are still created exclusively, so nothing is overwritten.
        if request.folder_name.is_empty() {
            anchor.writable()?;
        } else {
            safe_name(&request.folder_name)?;
            anchor.absent(&request.folder_name)?;
        }
        let mut i = self
            .inner
            .lock()
            .map_err(|_| "Download state unavailable")?;
        if i.status.state == "running" {
            return Err("Download already running".into());
        }
        let provider = i
            .catalog
            .as_ref()
            .map_err(Clone::clone)?
            .provider(&request.provider_id)?
            .clone();
        let mut names = HashSet::new();
        let mut warnings =
            vec!["Staged data only: no execution, extraction or ComfyUI installation".into()];
        let mut total = 0;
        for r in &request.resources {
            validate_resource(&provider, r)?;
            if !r.downloadable || provider.kind == "ollama" {
                return Err("Selected resource is not downloadable".into());
            }
            let (known, name) = i
                .known
                .get(&(provider.id.clone(), r.id.clone()))
                .ok_or("Refresh native browse before planning")?;
            if known != r {
                return Err("Resource identity or metadata changed".into());
            }
            if name.eq_ignore_ascii_case("deas-download-manifest.json")
                || !names.insert(name.to_lowercase())
            {
                return Err(
                    "Selected target filenames collide or use reserved manifest name".into(),
                );
            }
            total += r.size.unwrap_or(0);
            if r.sha256.is_none() {
                warnings.push(format!(
                    "{name}: no SHA-256; integrity not cryptographically verified"
                ));
            }
            if r.license.is_none() {
                warnings.push(format!(
                    "{name}: license not supplied; review upstream terms"
                ));
            }
        }
        if total > MAX_JOB_BYTES {
            return Err("Combined known size exceeds the 256 GiB limit".into());
        }
        let view = DownloadPlan {
            id: uuid::Uuid::new_v4().to_string(),
            target: target.to_string_lossy().into_owned(),
            total_known_bytes: total,
            unknown_size_count: request
                .resources
                .iter()
                .filter(|r| r.size.is_none())
                .count(),
            resources: request.resources,
            warnings,
            can_execute: cfg!(any(target_os = "linux", windows)),
        };
        i.plans
            .retain(|_, p| p.created.elapsed() < Duration::from_secs(600));
        if i.plans.len() >= 8 {
            i.plans.clear();
        }
        i.plans.insert(
            view.id.clone(),
            StoredPlan {
                view: view.clone(),
                provider,
                authenticated: request.authenticated,
                parent,
                subfolder: request.folder_name,
                identity: anchor.identity()?,
                created: Instant::now(),
            },
        );
        Ok(view)
    }
    pub fn status(&self) -> Result<DownloadStatus, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "Download state unavailable")?
            .status
            .clone())
    }
    pub fn cancel(&self) -> Result<DownloadStatus, String> {
        let i = self
            .inner
            .lock()
            .map_err(|_| "Download state unavailable")?;
        if i.status.state == "running" {
            i.cancel.store(true, Ordering::SeqCst);
        }
        Ok(i.status.clone())
    }
    pub fn start(
        &self,
        plan_id: String,
        confirmed: bool,
        auth: &AuthState,
    ) -> Result<DownloadStatus, String> {
        if !confirmed {
            return Err("Explicit confirmation required".into());
        }
        let mut i = self
            .inner
            .lock()
            .map_err(|_| "Download state unavailable")?;
        if i.status.state == "running" {
            return Err("Download already running".into());
        }
        let p = i.plans.remove(&plan_id).ok_or("Unknown or consumed plan")?;
        if p.created.elapsed() > Duration::from_secs(600) || !p.view.can_execute {
            return Err("Plan expired or unsupported".into());
        }
        let header = credential(auth, &p.provider, p.authenticated)?;
        let parent = fs::Anchor::open(&p.parent)?;
        if parent.identity() != Ok(p.identity) {
            return Err("Parent changed since planning".into());
        }
        let target = if p.subfolder.is_empty() {
            parent.writable()?;
            parent
        } else {
            parent.absent(&p.subfolder)?;
            parent.create(&p.subfolder)?
        };
        let cancel = Arc::new(AtomicBool::new(false));
        i.cancel = cancel.clone();
        i.status = DownloadStatus {
            state: "running".into(),
            total: p.view.resources.len(),
            ..Default::default()
        };
        let engine = self.clone();
        std::thread::Builder::new()
            .name("deas-download".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| "Cannot start network runtime".into())
                    .and_then(|rt| rt.block_on(engine.run(&p, target, header, &cancel)));
                if let Ok(mut i) = engine.inner.lock() {
                    i.status.state = if result.is_ok() {
                        "succeeded"
                    } else if cancel.load(Ordering::SeqCst) {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    i.status.error = result.err();
                }
            })
            .map_err(|_| {
                i.status.state = "failed".into();
                i.status.error = Some("Cannot start download worker".into());
                "Cannot start download worker".to_string()
            })?;
        Ok(i.status.clone())
    }
    async fn run(
        &self,
        p: &StoredPlan,
        target: fs::Anchor,
        header: Option<HeaderValue>,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let client = client_for(false, None)?;
        for r in &p.view.resources {
            self.transfer(r, &p.provider, &target, &client, header.clone(), cancel)
                .await?;
        }
        if cancel.load(Ordering::SeqCst) {
            return Err("Download cancelled".into());
        }
        if p.subfolder.is_empty() {
            return Ok(()); // shared model folders get no manifest, add per-file sidecars when provenance is needed
        }
        let name = format!("deas-{}.partial", uuid::Uuid::new_v4());
        let result = (|| {
            let mut file = target.partial(&name)?;
            let manifest = serde_json::json!({"version":1,"providerId":p.provider.id,"sourceOrigin":p.provider.origin,"plan":p.view});
            serde_json::to_writer_pretty(&mut file, &manifest)
                .map_err(|_| "Cannot write staged metadata")?;
            file.sync_all().map_err(|_| "Cannot sync staged metadata")?;
            target.publish(&name, "deas-download-manifest.json")
        })();
        target.remove_partial(&name);
        result
    }
    /// One resource. A persisted partial resumes only when the catalog pins size and hash,
    /// so a changed remote file cannot be appended silently; verification decides the retry.
    async fn transfer(
        &self,
        r: &Resource,
        provider: &Provider,
        target: &fs::Anchor,
        client: &reqwest::Client,
        header: Option<HeaderValue>,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let name = format!("deas-{}.partial", providers::filename(r)?);
        for attempt in 0..2 {
            let resumable = attempt == 0 && r.size.is_some() && r.sha256.is_some();
            let (mut file, mut count) = if resumable {
                match target.resume_partial(&name)? {
                    Some(kept) => kept,
                    None => (target.partial(&name)?, 0),
                }
            } else {
                target.remove_partial(&name);
                (target.partial(&name)?, 0)
            };
            let mut digest = Sha256::new();
            // shortcut: a resume re-reads the kept bytes (a 25 GB file costs 1–2 min); persist a
            // rolling hash only when resumes get frequent.
            if count > 0 {
                let mut buf = vec![0; 1 << 20];
                loop {
                    let n = std::io::Read::read(&mut file, &mut buf)
                        .map_err(|_| "Cannot read partial")?;
                    if n == 0 {
                        break;
                    }
                    digest.update(&buf[..n]);
                }
            }
            let mut length = None;
            let mut start = count;
            // A kept partial that already holds the whole file (a crash before publish) needs no request.
            if !r.size.is_some_and(|n| n == count) {
                let mut response = fetch(
                    client,
                    provider,
                    r.url.as_deref().ok_or("Missing download URL")?,
                    header.clone(),
                    cancel,
                    (count > 0).then_some(count),
                )
                .await?;
                length = response.content_length();
                if length.is_some_and(|n| n.saturating_add(count) > MAX_FILE_BYTES) {
                    return Err("Content-Length exceeds the 128 GiB limit".into());
                }
                if count > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                    // The slice must start where the kept bytes end, or it cannot be appended.
                    let from = response
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|s| s.to_str().ok())
                        .and_then(|s| s.strip_prefix("bytes "))
                        .and_then(|s| s.split('-').next())
                        .and_then(|s| s.parse::<u64>().ok());
                    if from != Some(count) {
                        target.remove_partial(&name);
                        return Err("Resume range mismatch".into());
                    }
                } else if count > 0 {
                    // 200: the server ignored Range and sent the whole file again.
                    file.set_len(0).map_err(|_| "Cannot reset partial")?;
                    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(0))
                        .map_err(|_| "Cannot reset partial")?;
                    digest = Sha256::new();
                    count = 0;
                }
                start = count;
                loop {
                    let chunk = cancellable(response.chunk(), cancel, 30)
                        .await?
                        .map_err(|_| network_error())?;
                    let Some(chunk) = chunk else { break };
                    count += chunk.len() as u64;
                    {
                        let mut i = self
                            .inner
                            .lock()
                            .map_err(|_| "Download state unavailable")?;
                        if i.status.bytes_downloaded.saturating_add(chunk.len() as u64)
                            > MAX_JOB_BYTES
                        {
                            return Err("Job exceeds the 256 GiB limit".into());
                        }
                        i.status.bytes_downloaded += chunk.len() as u64;
                    }
                    if r.size.is_some_and(|n| count > n) {
                        return Err("Download exceeds declared size".into());
                    }
                    digest.update(&chunk);
                    std::io::Write::write_all(&mut file, &chunk)
                        .map_err(|_| "Cannot write partial")?;
                }
            }
            if r.size.is_some_and(|n| n != count) || length.is_some_and(|n| start + n != count) {
                target.remove_partial(&name);
                // Only a resumed prefix can fail verification (the remote file changed): retry clean.
                if start > 0 {
                    continue;
                }
                return Err("Download size mismatch".into());
            }
            let actual = format!("{:x}", digest.finalize());
            if r.sha256
                .as_ref()
                .is_some_and(|s| !s.eq_ignore_ascii_case(&actual))
            {
                target.remove_partial(&name);
                if start > 0 {
                    continue;
                }
                return Err("SHA-256 mismatch; partial discarded".into());
            }
            if cancel.load(Ordering::SeqCst) {
                return Err("Download cancelled".into());
            }
            file.sync_all().map_err(|_| "Cannot sync partial")?;
            target.publish(&name, &providers::filename(r)?)?;
            self.inner
                .lock()
                .map_err(|_| "Download state unavailable")?
                .status
                .completed += 1;
            return Ok(());
        }
        Ok(())
    }
}
fn source_url(p: &Provider, s: &str) -> Result<Url, String> {
    let u = validate_url(s, false, p.kind == "civitai")?;
    if p.kind == "civitai"
        && u.query_pairs().any(|(k, v)| {
            !matches!(k.as_ref(), "type" | "format" | "size" | "fp" | "fileId")
                || (k == "fileId" && !v.bytes().all(|b| b.is_ascii_digit()))
                || v.len() > 80
                || !v
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'-' | b'_'))
        })
    {
        return Err("Unsupported CivitAI file selector query".into());
    }
    Ok(u)
}
fn validate_resource(p: &Provider, r: &Resource) -> Result<(), String> {
    bounded_text(&r.id, 2048)?;
    bounded_text(&r.label, 240)?;
    if let Some(date) = &r.published_at {
        bounded_text(date, 64)?;
    }
    if let Some(version) = &r.version {
        bounded_text(version, 240)?;
    }
    if let Some(variant) = &r.variant {
        bounded_text(variant, 240)?;
    }
    validate_kind(&r.kind)?;
    validate_metadata(
        r.sha256.as_deref(),
        r.license.as_deref(),
        if r.downloadable { r.size } else { None },
    )?;
    if let Some(s) = &r.locator {
        bounded_text(s, 4096)?;
    }
    if let Some(s) = &r.preview {
        validate_url(s, false, true)?;
    }
    if let Some(s) = &r.category {
        safe_name(s)?;
    }
    if r.downloadable {
        if p.kind == "ollama" || r.kind == "repo" {
            return Err("Read-only identity cannot be downloaded".into());
        }
        let u = source_url(p, r.url.as_deref().ok_or("Missing URL")?)?;
        let origin = u.origin().ascii_serialization();
        let allowed = match p.kind.as_str() {
            "github" => origin == p.origin || origin == "https://raw.githubusercontent.com",
            "http" | "huggingface" | "civitai" => origin == p.origin,
            _ => false,
        };
        if !allowed {
            return Err("Resource source outside adapter authority".into());
        }
        safe_name(&providers::filename(r)?)?;
    }
    Ok(())
}
async fn cancellable<F: std::future::Future>(
    future: F,
    cancel: &AtomicBool,
    seconds: u64,
) -> Result<F::Output, String> {
    tokio::pin!(future);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Download cancelled".into());
        }
        tokio::select! {v=&mut future=>return Ok(v),_=tokio::time::sleep_until(deadline)=>return Err("Network operation timed out".into()),_=tokio::time::sleep(Duration::from_millis(100))=>{}}
    }
}
async fn fetch(
    client: &reqwest::Client,
    p: &Provider,
    input: &str,
    header: Option<HeaderValue>,
    cancel: &AtomicBool,
    range: Option<u64>,
) -> Result<reqwest::Response, String> {
    let mut url = source_url(p, input)?;
    let mut authorized = url.origin().ascii_serialization() == p.origin;
    for hop in 0..=5 {
        let mut request = client.get(url.clone());
        if let Some(from) = range {
            request = request.header(reqwest::header::RANGE, format!("bytes={from}-"));
        }
        if p.kind == "github" && url.origin().ascii_serialization() == p.origin {
            request = request.header(reqwest::header::ACCEPT, "application/vnd.github.raw+json");
        }
        if authorized {
            if let Some(h) = &header {
                request = request.header(reqwest::header::AUTHORIZATION, h.clone());
            }
        }
        let response = cancellable(request.send(), cancel, 30)
            .await?
            .map_err(|_| network_error())?;
        if response.status().is_redirection() {
            if hop == 5 {
                return Err("Redirect limit exceeded".into());
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|s| s.to_str().ok())
                .ok_or("Invalid redirect")?;
            let next = validate_url(
                url.join(location).map_err(|_| "Invalid redirect")?.as_str(),
                false,
                true,
            )?;
            // A hop off the originally authorized origin permanently strips credentials,
            // including if the server later redirects back to that origin.
            if next.origin() != url.origin() {
                authorized = false;
            }
            url = next;
            continue;
        }
        if response.status() != reqwest::StatusCode::OK
            && !(range.is_some() && response.status() == reqwest::StatusCode::PARTIAL_CONTENT)
        {
            return Err(format!(
                "Provider returned HTTP {}; check gating, license and rate limits",
                response.status().as_u16()
            ));
        }
        return Ok(response);
    }
    Err("Redirect limit exceeded".into())
}
#[tauri::command]
pub fn native_download_catalog(
    engine: tauri::State<'_, DownloadEngine>,
) -> Result<Catalog, String> {
    engine.catalog()
}
#[tauri::command]
pub fn native_download_import_catalog(
    engine: tauri::State<'_, DownloadEngine>,
    text: String,
) -> Result<Catalog, String> {
    engine.import(text)
}
#[tauri::command]
pub async fn native_download_browse(
    engine: tauri::State<'_, DownloadEngine>,
    auth: tauri::State<'_, AuthState>,
    request: BrowseRequest,
) -> Result<Listing, String> {
    engine.browse(request, &auth).await
}
#[tauri::command]
pub fn native_download_plan(
    engine: tauri::State<'_, DownloadEngine>,
    request: PlanRequest,
) -> Result<DownloadPlan, String> {
    engine.plan(request)
}
#[tauri::command]
pub fn native_download_start(
    engine: tauri::State<'_, DownloadEngine>,
    auth: tauri::State<'_, AuthState>,
    plan_id: String,
    confirmed: bool,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<DownloadStatus, String> {
    let _guard = operations.enter()?;
    engine.start(plan_id, confirmed, &auth)
}
#[tauri::command]
pub fn native_download_status(
    engine: tauri::State<'_, DownloadEngine>,
) -> Result<DownloadStatus, String> {
    engine.status()
}
#[tauri::command]
pub fn native_download_cancel(
    engine: tauri::State<'_, DownloadEngine>,
) -> Result<DownloadStatus, String> {
    engine.cancel()
}
