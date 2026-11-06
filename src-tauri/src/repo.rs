//! The DEAS GitHub repository: open it, read its star count, star it with the saved GitHub token,
//! and the read-only update check (latest release + upstream heads of the owned checkouts).
//! The repository is compiled in; nothing from the UI selects a URL.
use crate::auth::AuthState;
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;
use std::path::Path;

const PAGE: &str = "https://github.com/darksidewalker/DEAS";
const RELEASES: &str = "https://github.com/darksidewalker/DEAS/releases/latest";
const API: &str = "https://api.github.com";
const REPO: &str = "darksidewalker/DEAS";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStars {
    count: u64,
    /// `None` without a saved GitHub token.
    starred: Option<bool>,
}

#[tauri::command]
pub fn native_open_repo() -> Result<(), String> {
    crate::token_page::open_url(PAGE)
}

#[tauri::command]
pub fn native_open_releases() -> Result<(), String> {
    crate::token_page::open_url(RELEASES)
}

#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Updates {
    /// Tag of a newer GitHub release than this build, e.g. `v0.2.0`.
    app: Option<String>,
    comfy: bool,
    nodepack: bool,
}

async fn git(git: &Path, dir: &Path, args: &[&str]) -> Option<String> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::new(git)
            .args(args)
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether `origin/<reference>` points at a commit other than the checked-out HEAD.
/// Unreadable/unreachable checkouts report `false`: this is a hint, never a blocker.
async fn behind(exe: &Path, dir: &Path, reference: &str) -> bool {
    if !dir.join(".git").is_dir() {
        return false;
    }
    let (Some(head), Some(remote)) = (
        git(exe, dir, &["rev-parse", "HEAD"]).await,
        git(exe, dir, &["ls-remote", "origin", reference]).await,
    ) else {
        return false;
    };
    // An annotated tag lists the tag object and its peeled `^{}` commit; the commit decides.
    let lines: Vec<&str> = remote.lines().collect();
    let line = lines.iter().find(|l| l.ends_with("^{}")).or(lines.first());
    match line.and_then(|l| l.get(..40)) {
        Some(sha) => sha != head.trim(),
        None => false,
    }
}

/// Tag of the latest GitHub release when it is newer than this build. Unlike the hint below,
/// an unreadable release is an error: platforms without self-update gate the backend on it.
pub(crate) async fn newer_release(auth: &AuthState) -> Result<Option<String>, String> {
    let (response, _) = call(auth, Method::GET, &format!("/repos/{REPO}/releases/latest")).await?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND => return Err("Cannot read the latest DEAS release (GitHub answered 404): while the repository is private, add a GitHub token in Provider authorization".into()),
        status => return Err(format!("Cannot read the latest DEAS release (GitHub answered {status})")),
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "Unexpected GitHub response".to_string())?;
    release_update(&value)
}

fn release_update(value: &serde_json::Value) -> Result<Option<String>, String> {
    let tag = value["tag_name"]
        .as_str()
        .ok_or("Invalid release version")?;
    let latest = crate::app_update::version(tag).ok_or("Invalid release version")?;
    let current = crate::app_update::version(env!("CARGO_PKG_VERSION"))
        .ok_or("Invalid application version")?;
    Ok((latest > current).then(|| tag.to_owned()))
}

#[tauri::command]
pub async fn native_update_check(
    auth: tauri::State<'_, AuthState>,
    engine: tauri::State<'_, crate::install::Engine>,
) -> Result<Updates, String> {
    let mut updates = Updates {
        app: newer_release(&auth).await.ok().flatten(),
        ..Updates::default()
    };
    if let (Some(plan), Some(exe)) = (engine.approved_plan(), crate::install::git_path()) {
        let comfy = Path::new(&plan.request.storage_path).join("ComfyUI");
        updates.comfy = behind(&exe, &comfy, &plan.request.comfy_ref).await;
        updates.nodepack = behind(
            &exe,
            &comfy.join("custom_nodes/ComfyUI-DaSiWa-Nodes"),
            "main",
        )
        .await;
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_strictly_newer_release_tags_count() {
        use serde_json::json;
        assert_eq!(
            release_update(&json!({"tag_name":"v999.0.0"})),
            Ok(Some("v999.0.0".into()))
        );
        assert_eq!(
            release_update(&json!({"tag_name":env!("CARGO_PKG_VERSION")})),
            Ok(None)
        );
        assert_eq!(release_update(&json!({"tag_name":"v0.0.1"})), Ok(None));
        for value in [
            json!({}),
            json!({"tag_name":42}),
            json!({"tag_name":"nightly"}),
            json!({"tag_name":"release-0.1.65"}),
            json!({"tag_name":"v1.2.3.4"}),
        ] {
            assert_eq!(
                release_update(&value),
                Err("Invalid release version".into())
            );
        }
    }
}

async fn call(
    state: &AuthState,
    method: Method,
    path: &str,
) -> Result<(reqwest::Response, bool), String> {
    let url = Url::parse(&format!("{API}{path}")).map_err(|e| e.to_string())?;
    let auth = state.authorization("github", &url)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(4))
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("DEAS")
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client
        .request(method, url)
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::CONTENT_LENGTH, "0");
    let authed = auth.is_some();
    if let Some(auth) = auth {
        request = request.header(header::AUTHORIZATION, auth);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "GitHub is unreachable".to_string())?;
    Ok((response, authed))
}

async fn stars(state: &AuthState) -> Result<RepoStars, String> {
    let (response, authed) = call(state, Method::GET, &format!("/repos/{REPO}")).await?;
    if response.status() != StatusCode::OK
        || response.content_length().is_some_and(|n| n > 256 * 1024)
    {
        return Err(format!("GitHub answered {}", response.status()));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "Unexpected GitHub response".to_string())?;
    let count = value["stargazers_count"]
        .as_u64()
        .ok_or("Unexpected GitHub response")?;
    let starred = if authed {
        let (r, _) = call(state, Method::GET, &format!("/user/starred/{REPO}")).await?;
        match r.status() {
            StatusCode::NO_CONTENT => Some(true),
            StatusCode::NOT_FOUND => Some(false),
            _ => None,
        }
    } else {
        None
    };
    Ok(RepoStars { count, starred })
}

#[tauri::command]
pub async fn native_repo_stars(state: tauri::State<'_, AuthState>) -> Result<RepoStars, String> {
    stars(&state).await
}

#[tauri::command]
pub async fn native_repo_star(state: tauri::State<'_, AuthState>) -> Result<RepoStars, String> {
    if state.credential("github", API)?.is_none() {
        return Err("A GitHub token is needed to star".into());
    }
    let (response, _) = call(&state, Method::PUT, &format!("/user/starred/{REPO}")).await?;
    if response.status() != StatusCode::NO_CONTENT {
        return Err(format!("GitHub refused the star ({})", response.status()));
    }
    stars(&state).await
}
