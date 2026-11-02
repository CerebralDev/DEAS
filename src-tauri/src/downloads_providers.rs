//! Compiled adapters only. URLs from indexes/Link metadata are revalidated, never executed.
//! References: HF .well-known/openapi.json; GitHub REST contents/users; Civitai REST
//! api/v1/models (items/modelVersions/files); Ollama GET api/tags (models).
use super::*;
use serde_json::Value;
pub(super) struct Page {
    pub resources: Vec<Resource>,
    pub next: Option<Url>,
    pub warnings: Vec<String>,
}
pub(super) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Malformed provider response: missing {key}"))
}
fn array(v: &Value) -> Result<&Vec<Value>, String> {
    v.as_array().ok_or_else(|| "Expected provider array".into())
}
fn segment(s: &str) -> Result<(), String> {
    if s.is_empty()
        || s.len() > 200
        || s == "."
        || s == ".."
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        Err("Invalid account/repository locator".into())
    } else {
        Ok(())
    }
}
fn path(s: &str) -> Result<(), String> {
    if s.len() > 1024
        || s.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || s.contains(['\\', '?', '#', '%'])
                || s.chars().any(char::is_control)
        })
    {
        Err("Invalid repository path".into())
    } else {
        Ok(())
    }
}
pub(super) fn endpoint(p: &Provider, parts: &[&str]) -> Result<Url, String> {
    let mut u = Url::parse(&format!("{}/", p.origin)).map_err(|_| "Invalid origin")?;
    {
        let mut seg = u.path_segments_mut().map_err(|_| "Invalid origin")?;
        seg.clear();
        for s in parts {
            seg.push(s);
        }
    }
    Ok(u)
}
pub(super) fn filename(r: &Resource) -> Result<String, String> {
    let s = r
        .locator
        .as_deref()
        .or(r.url.as_deref())
        .ok_or("Missing file locator")?;
    let name = s.rsplit('/').next().ok_or("Missing filename")?;
    safe_name(name)?;
    Ok(name.into())
}
fn file(
    id: String,
    label: String,
    url: String,
    size: Option<u64>,
    hash: Option<String>,
    license: Option<String>,
    name: String,
) -> Result<Resource, String> {
    safe_name(&name)?;
    Ok(Resource {
        id,
        label,
        kind: "file".into(),
        url: Some(url),
        size,
        sha256: hash,
        downloadable: true,
        locator: Some(name),
        license,
        preview: None,
        category: None,
        auth: false,
        nsfw: false,
        base: None,
        published_at: None,
        version: None,
        variant: None,
    })
}
async fn json(
    client: &reqwest::Client,
    p: &Provider,
    u: Url,
    header: &Option<HeaderValue>,
) -> Result<(Value, Option<String>), String> {
    fetch_json(client, p, u, header, false)
        .await?
        .ok_or_else(|| "Provider HTTP 404".into())
}
/// Lookup where 404 is a regular answer (e.g. a hash unknown to CivitAI); other failures stay errors.
pub(super) async fn json_optional(
    client: &reqwest::Client,
    p: &Provider,
    u: Url,
    header: &Option<HeaderValue>,
) -> Result<Option<Value>, String> {
    Ok(fetch_json(client, p, u, header, true)
        .await?
        .map(|(v, _)| v))
}
async fn fetch_json(
    client: &reqwest::Client,
    p: &Provider,
    u: Url,
    header: &Option<HeaderValue>,
    not_found_ok: bool,
) -> Result<Option<(Value, Option<String>)>, String> {
    let parsed = validate_url(u.as_str(), p.kind == "ollama", true)?;
    if parsed.origin().ascii_serialization() != p.origin {
        return Err("Provider pagination escaped configured origin".into());
    }
    let mut q = client
        .get(parsed)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(h) = header {
        q = q.header(reqwest::header::AUTHORIZATION, h.clone());
    }
    let mut response = q.send().await.map_err(|_| network_error())?;
    if not_found_ok && response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!(
            "Provider HTTP {} (authentication, gating or rate limits may apply)",
            response.status().as_u16()
        ));
    }
    let link = response
        .headers()
        .get(reqwest::header::LINK)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if response
        .content_length()
        .is_some_and(|n| n > MAX_JSON as u64)
    {
        return Err("Provider JSON exceeds 2 MiB".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| network_error())? {
        if bytes.len() + chunk.len() > MAX_JSON {
            return Err("Provider JSON exceeds 2 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let v = serde_json::from_slice(&bytes).map_err(|_| "Malformed provider JSON")?;
    Ok(Some((v, link)))
}
fn link_next(p: &Provider, u: &Url, link: Option<String>) -> Result<Option<Url>, String> {
    let Some(link) = link else { return Ok(None) };
    for item in link.split(',') {
        if item.contains("rel=\"next\"") || item.contains("rel=next") {
            let input = item
                .trim()
                .split('>')
                .next()
                .and_then(|s| s.strip_prefix('<'))
                .ok_or("Malformed next Link")?;
            let next = u.join(input).map_err(|_| "Invalid next Link")?;
            validate_url(next.as_str(), p.kind == "ollama", true)?;
            if next.origin() != u.origin() || next.path() != u.path() {
                return Err("Next page escaped adapter endpoint".into());
            }
            return Ok(Some(next));
        }
    }
    Ok(None)
}
fn hf_locator(s: &str) -> Result<(&str, &str), String> {
    let (kind, name) = s.split_once(':').unwrap_or(("models", s));
    if !matches!(kind, "models" | "datasets") {
        return Err("HF locator uses models: or datasets:".into());
    }
    Ok((kind, name))
}
/// ComfyUI model folder for a CivitAI model type; unknown types stay unsorted.
pub(super) fn civitai_category(model_type: Option<&str>) -> &'static str {
    match model_type {
        Some("Checkpoint") => "checkpoints",
        Some("LORA" | "LoCon" | "DoRA") => "loras",
        Some("TextualInversion") => "embeddings",
        Some("VAE") => "vae",
        Some("Controlnet") => "controlnet",
        Some("Upscaler") => "upscale_models",
        Some("Workflows") => "workflows",
        _ => "other",
    }
}
/// First still image of a model version as a 450px thumbnail; videos are skipped because
/// dozens of WebKitGTK media pipelines are unstable. Never fetched natively.
pub(super) fn civitai_preview(version: &Value) -> Option<String> {
    version.get("images")?.as_array()?.iter().find_map(|image| {
        let url = image.get("url")?.as_str()?;
        let still = image.get("type").and_then(Value::as_str) != Some("video")
            && !url.ends_with(".mp4")
            && !url.ends_with(".webm");
        (still
            && url.len() <= 2048
            && url.starts_with("https://")
            && !url.bytes().any(|b| b.is_ascii_control()))
        .then(|| url.replacen("/original=true/", "/width=450/", 1))
    })
}
/// Non-public availability or paid access means anonymous downloads will be refused.
fn civitai_restricted(version: &Value) -> bool {
    version
        .get("availability")
        .and_then(Value::as_str)
        .is_some_and(|a| a != "Public")
        || version.get("paidAccess").is_some_and(|p| !p.is_null())
}
/// Precision, size, format and rounded size of a CivitAI file so quants of one version are distinguishable.
fn civitai_variant(file: &Value) -> Option<String> {
    let meta = file.get("metadata");
    let mut parts: Vec<String> = ["fp", "size", "format"]
        .iter()
        .filter_map(|k| meta?.get(k)?.as_str())
        .filter(|v| !v.is_empty() && v.len() <= 40)
        .map(str::to_owned)
        .collect();
    if let Some(kb) = file
        .get("sizeKB")
        .and_then(Value::as_f64)
        .filter(|kb| *kb > 0.0)
    {
        parts.push(format!("≈{:.1} GB", kb / 1_048_576.0));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}
/// Short base-model family label from a CivitAI version; unknown names pass through bounded.
pub(super) fn civitai_base(version: &Value) -> Option<String> {
    let raw = version.get("baseModel")?.as_str()?;
    let short = match raw {
        "Wan Video 2.2 I2V-A14B" | "Wan Video 2.2 T2V-A14B" | "Wan Video 2.2 TI2V-5B" => "Wan 2.2",
        "Wan Video 14B i2v 720p" | "Wan Video 14B t2v" | "Wan Video 1.3B t2v" => "Wan 2.1",
        "LTXV 2.3" => "LTX 2.3",
        "LTXV 2.0" => "LTX 2.0",
        _ => raw,
    };
    (short.len() <= 64 && !short.chars().any(char::is_control)).then(|| short.to_owned())
}
fn civitai_published(version: &Value) -> Option<String> {
    version
        .get("publishedAt")
        .and_then(Value::as_str)
        .or_else(|| version.get("createdAt").and_then(Value::as_str))
        .filter(|s| s.len() <= 64 && !s.chars().any(char::is_control))
        .map(str::to_owned)
}
pub(super) async fn list(
    p: &Provider,
    r: &BrowseRequest,
    next: Option<Url>,
    header: Option<HeaderValue>,
) -> Result<Page, String> {
    let client = client_for(
        p.kind == "ollama",
        Some(Duration::from_secs(MAX_HTTP_SECONDS)),
    )?;
    let mut warnings = vec![];
    let mut u = match (p.kind.as_str(), r.scope.as_str()) {
        ("huggingface", "user") => {
            let (kind, name) = hf_locator(&r.locator)?;
            segment(name)?;
            let mut u = endpoint(p, &["api", kind])?;
            u.query_pairs_mut()
                .append_pair("author", name)
                .append_pair("limit", "50");
            warnings.push(
                "HF user locator defaults to models; use datasets:USERNAME for datasets".into(),
            );
            u
        }
        ("huggingface", "repo") => {
            let (kind, name) = hf_locator(&r.locator)?;
            path(name)?;
            let parts: Vec<_> = name.split('/').collect();
            if parts.len() != 2 {
                return Err("HF repo locator is [models:|datasets:]OWNER/REPO".into());
            }
            let mut u = endpoint(p, &["api", kind, parts[0], parts[1], "tree", "main"])?;
            u.query_pairs_mut()
                .append_pair("recursive", "true")
                .append_pair("limit", "50");
            warnings.push(
                "HF tree pinned to main; custom revisions not supported in this slice".into(),
            );
            u
        }
        ("github", "user") => {
            segment(&r.locator)?;
            let mut u = endpoint(p, &["users", &r.locator, "repos"])?;
            u.query_pairs_mut()
                .append_pair("per_page", "50")
                .append_pair("page", "1");
            u
        }
        ("github", "repo") => {
            path(&r.locator)?;
            let parts: Vec<_> = r.locator.split('/').collect();
            if parts.len() < 2 {
                return Err("GitHub locator is OWNER/REPO[/PATH]".into());
            }
            segment(parts[0])?;
            segment(parts[1])?;
            let mut parts2 = vec!["repos", parts[0], parts[1], "contents"];
            parts2.extend_from_slice(&parts[2..]);
            warnings.push("GitHub contents lists one directory, up to 1000 API entries; only first 50 shown. Browse a narrower directory; Git LFS pointers are not expanded".into());
            endpoint(p, &parts2)?
        }
        ("civitai", "user") => {
            segment(&r.locator)?;
            let mut u = endpoint(p, &["api", "v1", "models"])?;
            u.query_pairs_mut()
                .append_pair("username", &r.locator)
                .append_pair("sort", "Newest")
                .append_pair("limit", "10")
                .append_pair("page", "1")
                .append_pair("nsfw", if r.nsfw { "true" } else { "false" });
            u
        }
        ("civitai", "search") => {
            let q = r.locator.trim();
            if q.is_empty() || q.len() > 120 || q.bytes().any(|b| b.is_ascii_control()) {
                return Err("CivitAI search needs 1–120 printable characters".into());
            }
            let mut u = endpoint(p, &["api", "v1", "models"])?;
            u.query_pairs_mut()
                .append_pair("query", q)
                .append_pair("limit", "20")
                .append_pair("sort", "Newest")
                .append_pair("nsfw", if r.nsfw { "true" } else { "false" });
            u
        }
        ("civitai", "model") => {
            if r.locator.is_empty()
                || !r.locator.bytes().all(|b| b.is_ascii_digit())
                || r.locator.len() > 20
            {
                return Err("CivitAI model locator must be a numeric ID".into());
            }
            endpoint(p, &["api", "v1", "models", &r.locator])?
        }
        ("http", "index") => {
            let direct = r.locator.starts_with("direct:");
            let locator = r
                .locator
                .strip_prefix("direct:")
                .or_else(|| r.locator.strip_prefix("index:"))
                .unwrap_or(&r.locator);
            let u = same_origin_url(p, locator)?;
            if direct || (!r.locator.starts_with("index:") && !u.path().ends_with(".json")) {
                let name = u
                    .path()
                    .rsplit('/')
                    .next()
                    .ok_or("Direct URL requires filename")?
                    .to_string();
                safe_name(&name)?;
                warnings.push("Direct file: size/checksum/license unknown until supplied by a declarative JSON index; .json locators are treated as indexes".into());
                return Ok(Page {
                    resources: vec![file(
                        format!("{}:{}", p.id, r.locator),
                        name.clone(),
                        u.to_string(),
                        None,
                        None,
                        None,
                        name,
                    )?],
                    next: None,
                    warnings,
                });
            }
            u
        }
        ("ollama", "backend") => {
            if r.locator != p.origin {
                return Err("Ollama locator must exactly match configured local origin".into());
            }
            warnings
                .push("Read-only installed tags; no pull, export or download authorization".into());
            endpoint(p, &["api", "tags"])?
        }
        _ => return Err("Unsupported adapter scope".into()),
    };
    if let Some(next) = next {
        if next.origin() != u.origin() || next.path() != u.path() {
            return Err("Invalid continuation endpoint".into());
        }
        u = next;
    }
    let (v, link) = json(&client, p, u.clone(), &header).await?;
    let mut continuation = link_next(p, &u, link)?;
    let mut resources = vec![];
    match p.kind.as_str() {
        "huggingface" => {
            let (kind, _) = hf_locator(&r.locator)?;
            for item in array(&v)?.iter().take(MAX_RESOURCES) {
                if r.scope == "user" {
                    let id = text(item, "id")?;
                    path(id)?;
                    resources.push(Resource {
                        id: format!("{}:{id}", p.id),
                        label: id.into(),
                        kind: "repo".into(),
                        url: None,
                        size: None,
                        sha256: None,
                        downloadable: false,
                        locator: Some(format!("{kind}:{id}")),
                        license: None,
                        preview: None,
                        category: None,
                        auth: false,
                        nsfw: false,
                        base: None,
                        published_at: None,
                        version: None,
                        variant: None,
                    });
                } else if item.get("type").and_then(Value::as_str) == Some("file") {
                    let name = text(item, "path")?;
                    path(name)?;
                    let filename = name.rsplit('/').next().unwrap_or(name);
                    // Hidden repository metadata is not a model download; safe_name rejects it.
                    if filename.starts_with('.') {
                        continue;
                    }
                    let (_, repo) = hf_locator(&r.locator)?;
                    let prefix = if kind == "datasets" { "datasets/" } else { "" };
                    let url = format!("{}/{prefix}{repo}/resolve/main/{name}", p.origin);
                    let hash = item
                        .pointer("/lfs/oid")
                        .and_then(Value::as_str)
                        .map(|s| s.strip_prefix("sha256:").unwrap_or(s).into());
                    resources.push(file(
                        format!("{}:{kind}:{repo}:{name}", p.id),
                        name.rsplit('/').next().unwrap_or(name).into(),
                        url,
                        item.pointer("/lfs/size")
                            .and_then(Value::as_u64)
                            .or_else(|| item.get("size").and_then(Value::as_u64)),
                        hash,
                        None,
                        name.rsplit('/').next().unwrap_or(name).into(),
                    )?);
                }
            }
            if array(&v)?.len() > MAX_RESOURCES {
                warnings
                    .push("Response exceeded requested page size; only first 50 exposed".into());
            }
        }
        "github" => {
            if r.scope == "user" {
                for item in array(&v)?.iter().take(MAX_RESOURCES) {
                    let name = text(item, "full_name")?;
                    path(name)?;
                    resources.push(Resource {
                        id: format!("github:{name}"),
                        label: name.into(),
                        kind: "repo".into(),
                        url: None,
                        size: None,
                        sha256: None,
                        downloadable: false,
                        locator: Some(name.into()),
                        license: item
                            .pointer("/license/spdx_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        preview: None,
                        category: None,
                        auth: false,
                        nsfw: false,
                        base: None,
                        published_at: None,
                        version: None,
                        variant: None,
                    });
                }
            } else {
                let items = if v.is_array() {
                    array(&v)?.clone()
                } else {
                    vec![v.clone()]
                };
                for item in items.iter().take(MAX_RESOURCES) {
                    let name = text(item, "name")?;
                    if item.get("type").and_then(Value::as_str) == Some("dir") {
                        resources.push(Resource {
                            id: format!("github:{}:{}", r.locator, name),
                            label: name.into(),
                            kind: "repo".into(),
                            url: None,
                            size: None,
                            sha256: None,
                            downloadable: false,
                            locator: Some(format!("{}/{}", r.locator, name)),
                            license: None,
                            preview: None,
                            category: None,
                            auth: false,
                            nsfw: false,
                            base: None,
                            published_at: None,
                            version: None,
                            variant: None,
                        });
                    } else if item.get("type").and_then(Value::as_str) == Some("file")
                        && item.get("submodule_git_url").is_none()
                    {
                        // Use official contents API raw media type, so scoped private-repo auth
                        // stays on api.github.com rather than leaking into raw/CDN URLs.
                        let api = text(item, "url")?;
                        let api = validate_url(api, false, true)?;
                        if api.origin().ascii_serialization() != p.origin
                            || !api.path().starts_with("/repos/")
                        {
                            return Err("GitHub file URL escaped API".into());
                        }
                        let mut api = api;
                        api.set_query(None); // current default ref only; no secret URLs exposed
                        resources.push(file(
                            format!("github:{}:{}", r.locator, text(item, "path")?),
                            name.into(),
                            api.to_string(),
                            item.get("size").and_then(Value::as_u64),
                            None,
                            None,
                            name.into(),
                        )?);
                    }
                }
            }
        }
        "civitai" => {
            let models = if r.scope == "model" {
                vec![v.clone()]
            } else {
                array(v.get("items").ok_or("Missing CivitAI items")?)?.clone()
            };
            if r.scope != "model" {
                if let Some(input) = v.pointer("/metadata/nextPage").and_then(Value::as_str) {
                    let next = Url::parse(input).map_err(|_| "Invalid CivitAI pagination")?;
                    if next.origin() != u.origin() || next.path() != u.path() {
                        return Err("CivitAI pagination escaped endpoint".into());
                    }
                    continuation = Some(next);
                }
            }
            for model in models {
                if matches!(
                    model.get("mode").and_then(Value::as_str),
                    Some("Archived" | "TakenDown")
                ) {
                    warnings.push(
                        "CivitAI moderated/archived model has no downloadable public files".into(),
                    );
                    continue;
                }
                let modelid = model
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or("Missing CivitAI model ID")?;
                let versions = array(model.get("modelVersions").ok_or("Missing modelVersions")?)?;
                let category = civitai_category(model.get("type").and_then(Value::as_str));
                let nsfw = model.get("nsfw").and_then(Value::as_bool).unwrap_or(false)
                    || model.get("nsfwLevel").and_then(Value::as_u64).unwrap_or(0) >= 4;
                // Search results stay browsable model identities; files come from the model scope.
                if r.scope == "search" {
                    if resources.len() >= MAX_RESOURCES {
                        break;
                    }
                    let version = versions.first();
                    let label = format!(
                        "{} · {}",
                        text(&model, "name")?,
                        version
                            .and_then(|v| v.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                    );
                    bounded_text(&label, 240)?;
                    resources.push(Resource {
                        id: format!("civitai:{modelid}"),
                        label,
                        kind: "repo".into(),
                        url: None,
                        size: None,
                        sha256: None,
                        downloadable: false,
                        locator: Some(modelid.to_string()),
                        license: None,
                        preview: version.and_then(civitai_preview),
                        category: Some(category.into()),
                        auth: version.is_some_and(civitai_restricted),
                        nsfw,
                        base: version.and_then(civitai_base),
                        published_at: version.and_then(civitai_published),
                        version: None,
                        variant: None,
                    });
                    continue;
                }
                for version in versions {
                    let versionid = version
                        .get("id")
                        .and_then(Value::as_u64)
                        .ok_or("Missing model version ID")?;
                    for f in array(version.get("files").ok_or("Missing version files")?)? {
                        if resources.len() >= MAX_RESOURCES {
                            warnings.push("CivitAI version files truncated at 50; browse a specific model for narrower results".into());
                            break;
                        }
                        let name = text(f, "name")?;
                        let url = text(f, "downloadUrl")?;
                        let mut resource = file(
                            format!(
                                "civitai:{modelid}:{versionid}:{}",
                                f.get("id")
                                    .and_then(Value::as_u64)
                                    .ok_or("Missing CivitAI file ID")?
                            ),
                            name.into(),
                            url.into(),
                            None,
                            f.pointer("/hashes/SHA256")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            None,
                            name.into(),
                        )?;
                        resource.kind = "model".into();
                        resource.preview = civitai_preview(version);
                        resource.category = Some(category.into());
                        resource.auth = civitai_restricted(version);
                        resource.nsfw = nsfw;
                        resource.base = civitai_base(version);
                        resource.published_at = civitai_published(version);
                        resource.version = version
                            .get("name")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        resource.variant = civitai_variant(f);
                        resources.push(resource);
                    }
                }
            }
            warnings.push("CivitAI sizeKB is rounded and not an exact byte checksum; size remains unknown. Creator license permissions require upstream review".into());
        }
        "http" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Index {
                version: u32,
                resources: Vec<Resource>,
            }
            let index: Index = serde_json::from_value(v)
                .map_err(|_| "Expected strict JSON index {version:1,resources:[Resource]}")?;
            if index.version != 1 || index.resources.len() > 500 {
                return Err("Index version/bounds invalid".into());
            }
            resources = index.resources.into_iter().take(MAX_RESOURCES).collect();
            warnings.push("JSON index bounded to first 50 resources; no executable hooks or remote continuation".into());
        }
        "ollama" => {
            for item in array(v.get("models").ok_or("Missing Ollama models")?)?
                .iter()
                .take(MAX_RESOURCES)
            {
                let name = text(item, "name")?;
                resources.push(Resource {
                    id: format!("{}:{name}", p.id),
                    label: name.into(),
                    kind: "model".into(),
                    url: None,
                    size: item.get("size").and_then(Value::as_u64),
                    sha256: None,
                    downloadable: false,
                    locator: Some(name.into()),
                    license: None,
                    preview: None,
                    category: None,
                    auth: false,
                    nsfw: false,
                    base: None,
                    published_at: None,
                    version: None,
                    variant: None,
                });
            }
            warnings.push("Backend tags bounded to first 50; local installed digest is not a downloadable-file checksum".into());
        }
        _ => return Err("Unknown adapter".into()),
    }
    Ok(Page {
        resources,
        next: continuation,
        warnings,
    })
}
