//! Optional enrichment in app-owned storage; legacy model sidecars retain their format.
use super::*;
use tauri::Manager;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assets {
    pub sha256: String,
    pub description: String,
    pub source_url: String,
    pub thumbnails: Vec<String>,
    pub warnings: Vec<String>,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn plain(s: &str) -> String {
    let mut tag = false;
    let mut out = String::new();
    for c in s.chars().take(32000) {
        match c {
            '<' => {
                tag = true;
                out.push(' ');
            }
            '>' => tag = false,
            _ if !tag && !c.is_control() => out.push(c),
            _ => (),
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .chars()
        .take(8000)
        .collect()
}
fn safe_url(value: &str) -> Result<Url, String> {
    let u = Url::parse(value).map_err(|_| "Invalid thumbnail URL")?;
    let h = u.host_str().unwrap_or("");
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port_or_known_default() != Some(443)
        || !(h == "image.civitai.com" || h.ends_with(".civitai.com"))
    {
        return Err("Unsupported thumbnail host".into());
    }
    Ok(u)
}
fn publish(root: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|_| "Cannot create model asset cache")?;
    let mut temp = tempfile::NamedTempFile::new_in(root).map_err(|_| "Cannot stage model asset")?;
    temp.write_all(bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| "Cannot write model asset")?;
    temp.persist(root.join(name))
        .map_err(|_| "Cannot publish model asset")?;
    Ok(())
}
async fn thumbnail(root: &Path, url: &str) -> Result<String, String> {
    let u = safe_url(url)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|_| "Thumbnail client unavailable")?;
    let mut response = client
        .get(u)
        .send()
        .await
        .map_err(|_| "Thumbnail request failed")?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > 4 * 1024 * 1024)
    {
        return Err("Thumbnail unavailable or too large".into());
    }
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Thumbnail transfer failed")?
    {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err("Thumbnail exceeds 4 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        return Err("Unsupported thumbnail encoding".into());
    };
    let name = format!("{:x}.{ext}", Sha256::digest(&bytes));
    publish(root, &name, &bytes)?;
    Ok(name)
}
pub async fn enrich(
    p: &Provider,
    path: PathBuf,
    header: Option<HeaderValue>,
    root: PathBuf,
) -> Result<LocalModel, String> {
    let model = fetch(p, path.clone(), header.clone()).await?;
    let Some(info) = model.info.as_ref() else {
        return Ok(model);
    };
    let Some(c) = info.civitai.as_ref() else {
        return Ok(model);
    };
    let client = client_for(false, None)?;
    let url = providers::endpoint(
        p,
        &["api", "v1", "model-versions", &c.version_id.to_string()],
    )?;
    let value = tokio::time::timeout(
        Duration::from_secs(30),
        providers::json_optional(&client, p, url, &header),
    )
    .await
    .map_err(|_| "Model enrichment timed out")??
    .ok_or("Model version is no longer available")?;
    if value["id"].as_u64() != Some(c.version_id) || value["modelId"].as_u64() != Some(c.model_id) {
        return Err("Model enrichment identity mismatch".into());
    }
    let mut assets = Assets {
        sha256: info.sha256.clone(),
        description: plain(value["description"].as_str().unwrap_or("")),
        source_url: format!(
            "https://civitai.com/models/{}?modelVersionId={}",
            c.model_id, c.version_id
        ),
        ..Default::default()
    };
    // General author guidance often lives on the model, not its individual version.
    let model_url = providers::endpoint(p, &["api", "v1", "models", &c.model_id.to_string()])?;
    match tokio::time::timeout(
        Duration::from_secs(30),
        providers::json_optional(&client, p, model_url, &header),
    )
    .await
    {
        Ok(Ok(Some(v))) if v["id"].as_u64() == Some(c.model_id) => {
            let description = plain(v["description"].as_str().unwrap_or(""));
            if !description.is_empty() {
                assets.description = format!("{}\n\n{}", assets.description, description)
                    .trim()
                    .chars()
                    .take(8000)
                    .collect();
            }
        }
        _ => assets
            .warnings
            .push("General author notes unavailable; version metadata retained.".into()),
    }
    // Keep existing bounded choices available when refresh returns different previews.
    let previous = root.join(format!("{}.json", info.sha256));
    if let Ok(meta) = std::fs::symlink_metadata(&previous) {
        if meta.is_file() && !meta.file_type().is_symlink() && meta.len() < 128 * 1024 {
            if let Ok(bytes) = std::fs::read(&previous) {
                if let Ok(old) = serde_json::from_slice::<Assets>(&bytes) {
                    if old.sha256 == info.sha256 {
                        assets.thumbnails = old
                            .thumbnails
                            .into_iter()
                            .filter(|n| {
                                n.rsplit_once('.').is_some_and(|(h, e)| {
                                    digest(h) && matches!(e, "png" | "jpg" | "webp")
                                })
                            })
                            .take(8)
                            .collect();
                    }
                }
            }
        }
    }
    if let Some(images) = value["images"].as_array() {
        for image in images.iter().filter(|i| i["type"] != "video").take(4) {
            if let Some(url) = providers::civitai_preview(&serde_json::json!({"images":[image]})) {
                match thumbnail(&root, &url).await {
                    Ok(name) => {
                        if assets.thumbnails.len() < 8 && !assets.thumbnails.contains(&name) {
                            assets.thumbnails.push(name);
                        }
                    }
                    Err(_) => assets
                        .warnings
                        .push("A thumbnail could not be cached; retry refresh explicitly.".into()),
                }
            }
        }
    }
    // No cache may be associated with replaced source bytes after network work.
    let source = Source::open(&path)?;
    if info.source.as_ref() != Some(&source.stamp) {
        return Err("Model changed during enrichment".into());
    }
    source.verify()?;
    let bytes = serde_json::to_vec(&assets).map_err(|_| "Cannot encode model assets")?;
    publish(&root, &format!("{}.json", info.sha256), &bytes)?;
    Ok(model)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedAssets {
    pub assets: Assets,
    pub images: BTreeMap<String, String>,
}
use std::collections::BTreeMap;
#[tauri::command]
pub fn native_model_assets(
    app: tauri::AppHandle,
    hashes: Vec<String>,
) -> Result<BTreeMap<String, CachedAssets>, String> {
    if hashes.len() > 5000 || hashes.iter().any(|s| !digest(s)) {
        return Err("Invalid model asset identity".into());
    }
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Model asset directory unavailable")?
        .join("model-assets");
    let mut found = BTreeMap::new();
    for hash in hashes {
        let path = root.join(format!("{hash}.json"));
        if !path.exists() {
            continue;
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|_| "Cannot inspect model assets")?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 128 * 1024 {
            return Err("Invalid model asset cache".into());
        }
        let assets: Assets =
            serde_json::from_slice(&std::fs::read(&path).map_err(|_| "Cannot read model assets")?)
                .map_err(|_| "Invalid model asset cache")?;
        if assets.sha256 != hash || assets.thumbnails.len() > 8 {
            return Err("Mismatched model asset cache".into());
        }
        let mut images = BTreeMap::new();
        for name in &assets.thumbnails {
            let Some((h, ext)) = name.rsplit_once('.') else {
                continue;
            };
            if !digest(h) || !matches!(ext, "png" | "jpg" | "webp") {
                continue;
            }
            let path = root.join(name);
            if std::fs::symlink_metadata(&path).is_ok_and(|m| {
                m.is_file() && !m.file_type().is_symlink() && m.len() <= 4 * 1024 * 1024
            }) {
                images.insert(name.clone(), path.to_string_lossy().into());
            }
        }
        found.insert(hash, CachedAssets { assets, images });
    }
    Ok(found)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_assets_reject_unsafe_urls_and_strip_html() {
        assert!(safe_url("https://image.civitai.com/a.png").is_ok());
        for u in [
            "file:///x",
            "https://image.civitai.com.evil.test/a",
            "http://image.civitai.com/a",
            "https://user:secret@image.civitai.com/a",
        ] {
            assert!(safe_url(u).is_err());
        }
        assert_eq!(plain("<p>Steps &amp; CFG</p>"), " Steps & CFG ");
        assert!(!digest("../x"));
    }
}
