//! Move the open DEAS environment to a new folder without reinstalling.
//! Same drive: one rename. Other drive: copy, then the original is removed only after the
//! moved copy passed the Python check. The venv, manifests and launchers store absolute
//! paths; they are rewritten from the old root to the new one.
use crate::install::{lock, Engine, Plan, VENV_BIN, VENV_PYTHON};
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentMove {
    pub root: String,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn native_environment_move(
    target: String,
    confirmed: bool,
    app: tauri::AppHandle,
) -> Result<EnvironmentMove, String> {
    if !confirmed {
        return Err("Explicit environment move confirmation is required".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let operations = app.state::<crate::FolderOperations>();
        let _guard = operations.enter()?;
        let engine = app.state::<Engine>();
        if !engine.installation_idle()
            || app.state::<crate::downloads::DownloadEngine>().status()?.state == "running"
            || app.state::<crate::jobs::Jobs>().workbench_busy()?
        {
            return Err("Wait for installations, downloads and Studio jobs before moving the environment. Nothing was stopped or moved.".into());
        }
        move_environment(&engine, &target)
    })
    .await
    .map_err(|e| format!("Environment move worker failed: {e}"))?
}

pub(crate) fn move_environment(engine: &Engine, target: &str) -> Result<EnvironmentMove, String> {
    if !engine.installation_idle() {
        return Err("Wait for the native installation before moving the environment".into());
    }
    crate::install::validate_target(target)?;
    let active = engine.stop_for_maintenance();
    let result = move_stopped_environment(engine, target);
    let resumed = engine.resume_after_maintenance(active, &lock(&engine.inner));
    match (result, resumed) {
        (Ok(mut moved), Err(error)) => {
            moved.warnings.push(error);
            Ok(moved)
        }
        (Err(error), resumed) => {
            let error = match resumed {
                Ok(()) => error,
                Err(restart) => format!("{error}. {restart}"),
            };
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Environment move: {error}"),
            );
            Err(error)
        }
        (Ok(moved), Ok(())) => Ok(moved),
    }
}

fn move_stopped_environment(engine: &Engine, target: &str) -> Result<EnvironmentMove, String> {
    let approved = lock(&engine.inner)
        .approved
        .clone()
        .ok_or("No DEAS environment is open")?;
    approved.validate()?;
    let old = PathBuf::from(&approved.plan().request.storage_path);
    let new = crate::install::validate_target(target)?;
    let parent = dunce::canonicalize(new.parent().ok_or("Missing target parent")?)
        .map_err(|e| e.to_string())?;
    let new = parent.join(new.file_name().ok_or("Missing target name")?);
    if new.starts_with(&old) {
        return Err("The new location must not be inside the current environment".into());
    }
    let from = old.to_str().ok_or("Environment path must be UTF-8")?;
    let to = new.to_str().ok_or("Target path must be UTF-8")?;
    let copied = match fs::rename(&old, &new) {
        Ok(()) => false,
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            if let Err(e) = copy_tree(&old, &new, &old.join("cache")) {
                return Err(format!(
                    "Copy to the other drive failed; the environment was not moved: {e}"
                ));
            }
            true
        }
        Err(e) => return Err(format!("Cannot move the environment: {e}")),
    };
    let mut warnings = vec![];
    let checked = (|| {
        rebase_environment(&new, from, to, &mut warnings)?;
        python_check(&new)?;
        let bytes = crate::backend::read_owned(&new.join("deas-plan.json"), 1024 * 1024)?;
        let plan: Plan = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        crate::backend::Approved::capture(&plan)
    })();
    let moved = match checked {
        Ok(moved) => moved,
        Err(e) => {
            let undo = if copied {
                fs::remove_dir_all(&new).map_err(|e| e.to_string())
            } else {
                rebase_environment(&new, to, from, &mut vec![])
                    .and_then(|()| fs::rename(&new, &old).map_err(|e| e.to_string()))
            };
            return Err(match undo {
                Ok(()) => {
                    // Atomic rewrites changed file identities even after restoring the original bytes.
                    match crate::backend::Approved::capture(approved.plan()) {
                        Ok(restored) => {
                            lock(&engine.inner).approved = Some(restored);
                            format!(
                                "The moved environment failed its check, so it was put back: {e}"
                            )
                        }
                        Err(check) => {
                            lock(&engine.inner).approved = None;
                            format!(
                                "The moved environment failed its check ({e}) and was put back, but could not be reopened safely: {check}"
                            )
                        }
                    }
                }
                Err(u) => format!(
                    "The moved environment failed its check ({e}) and could not be put back automatically ({u}); it is at {to}"
                ),
            });
        }
    };
    let mut g = lock(&engine.inner);
    g.approved = Some(moved);
    if let Some(plan) = g.repair.as_mut() {
        plan.request.storage_path = to.into();
    }
    drop(g);
    if copied {
        if let Err(e) = fs::remove_dir_all(&old) {
            warnings.push(format!(
                "The original at {from} could not be removed completely: {e}"
            ));
        }
    }
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!("Environment moved from {from} to {to}"),
    );
    Ok(EnvironmentMove {
        root: to.into(),
        warnings,
    })
}

/// Rewrite every absolute path under the environment that points at `from`.
fn rebase_environment(
    root: &Path,
    from: &str,
    to: &str,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    for name in ["deas-plan.json", "deas-install.json", "deas-settings.json"] {
        rewrite(&root.join(name), |text| {
            rebase(text, &json(from), &json(to))
        })?;
    }
    for name in ["deas-models.yaml", "venv/pyvenv.cfg"] {
        rewrite(&root.join(name), |text| rebase(text, from, to))?;
    }
    let bin = root.join(VENV_BIN);
    for entry in fs::read_dir(&bin).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
        {
            relaunch(&path, from, to)?;
        } else if entry.metadata().map_err(|e| e.to_string())?.len() < 1024 * 1024 {
            rewrite(&path, |text| rebase(text, from, to))?;
        }
    }
    relink(
        root,
        Path::new(from),
        Path::new(to),
        &root.join("cache"),
        warnings,
    )
}

fn json(path: &str) -> String {
    let quoted = serde_json::to_string(path).unwrap_or_default();
    quoted[1..quoted.len() - 1].to_owned()
}

/// Replace `from` only where it is a whole path prefix, never inside a longer name.
fn rebase(text: &str, from: &str, to: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(i) = rest.find(from) {
        let after = &rest[i + from.len()..];
        let whole = after
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '/' | '\\' | '"' | '\'' | '\n' | '\r' | ' '));
        out.push_str(&rest[..i]);
        out.push_str(if whole { to } else { from });
        changed |= whole;
        rest = after;
    }
    out.push_str(rest);
    changed.then_some(out)
}

/// Atomic in-place rewrite of a text file; non-UTF-8 files and missing files are left alone.
fn rewrite(path: &Path, edit: impl Fn(&str) -> Option<String>) -> Result<(), String> {
    let Ok(bytes) = fs::read(path) else {
        return Ok(());
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(());
    };
    match edit(text) {
        Some(next) => replace(path, next.as_bytes()),
        None => Ok(()),
    }
}

fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let permissions = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    let tmp = path.with_file_name(format!(".deas-move-{}", uuid::Uuid::new_v4()));
    let written = fs::write(&tmp, bytes)
        .and_then(|()| fs::set_permissions(&tmp, permissions))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.map_err(|e| format!("{}: {e}", path.display()))
}

/// uv's Windows launchers end with `<python path><u32 LE length><UVSC|UVPY>`.
fn relaunch(path: &Path, from: &str, to: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let Some(n) = bytes.len().checked_sub(8) else {
        return Ok(());
    };
    if !matches!(&bytes[n + 4..], b"UVSC" | b"UVPY") {
        return Ok(());
    }
    let length = u32::from_le_bytes(bytes[n..n + 4].try_into().unwrap()) as usize;
    let Some(start) = n.checked_sub(length) else {
        return Ok(());
    };
    let Ok(python) = std::str::from_utf8(&bytes[start..n]) else {
        return Ok(());
    };
    let Some(python) = rebase(python, from, to) else {
        return Ok(());
    };
    let mut out = bytes[..start].to_vec();
    out.extend_from_slice(python.as_bytes());
    out.extend_from_slice(&(python.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes[n + 4..]);
    replace(path, &out)
}

/// Absolute links into the old root (uv's cache uses them) follow the move. On Windows a
/// cache link is dropped instead (uv rebuilds the entry); other links are reported.
#[allow(clippy::only_used_in_recursion)] // each platform uses a different subset of the parameters
fn relink(
    dir: &Path,
    from: &Path,
    to: &Path,
    cache: &Path,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            relink(&path, from, to, cache, warnings)?;
            continue;
        }
        if !kind.is_symlink() {
            continue;
        }
        let Ok(target) = fs::read_link(&path) else {
            continue;
        };
        let Ok(rest) = target.strip_prefix(from) else {
            continue;
        };
        #[cfg(unix)]
        {
            fs::remove_file(&path)
                .and_then(|()| std::os::unix::fs::symlink(to.join(rest), &path))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        #[cfg(not(unix))]
        {
            let _ = rest;
            if path.starts_with(cache) {
                let _ = fs::remove_file(&path).or_else(|_| fs::remove_dir(&path));
            } else {
                warnings.push(format!(
                    "Link still points to the old location: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Copy a tree for a move across drives: files with permissions, links as links. Windows links
/// under the uv cache are skipped (uv rebuilds them); any other Windows link aborts the copy.
#[allow(clippy::only_used_in_recursion)] // `cache` is read by the Windows branch only
fn copy_tree(src: &Path, dst: &Path, cache: &Path) -> io::Result<()> {
    fs::create_dir(dst)?;
    let copied = (|| {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let (from, to) = (entry.path(), dst.join(entry.file_name()));
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                #[cfg(unix)]
                std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
                #[cfg(not(unix))]
                if !from.starts_with(cache) {
                    return Err(io::Error::other(format!(
                        "{} is a link; move the environment on the same drive instead",
                        from.display()
                    )));
                }
            } else if kind.is_dir() {
                copy_tree(&from, &to, cache)?;
            } else if fs::copy(&from, &to)? != entry.metadata()?.len() {
                return Err(io::Error::other(format!(
                    "{}: copied size differs",
                    from.display()
                )));
            }
        }
        fs::set_permissions(dst, fs::metadata(src)?.permissions())
    })();
    // A failed create_dir never grants ownership of a colliding destination.
    if copied.is_err() {
        let _ = fs::remove_dir_all(dst);
    }
    copied
}

/// The moved interpreter must start from its new prefix and load PyTorch.
fn python_check(root: &Path) -> Result<(), String> {
    let mut command = std::process::Command::new(root.join(VENV_PYTHON));
    command
        .args(["-I", "-c", "import sys,torch;print(sys.prefix)"])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::win::CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Moved Python does not start: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(300);
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Moved Python check timed out".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let expected = dunce::canonicalize(root.join("venv")).map_err(|e| e.to_string())?;
    if !output.status.success() || dunce::canonicalize(&prefix).ok() != Some(expected) {
        return Err(format!(
            "Moved Python check failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .chars()
                .take(2000)
                .collect::<String>()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_drive_copy_preserves_a_colliding_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("source");
        let dst = tmp.path().join("destination");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&dst).unwrap();
        fs::write(src.join("source.txt"), b"original").unwrap();
        fs::write(dst.join("keep.txt"), b"not ours").unwrap();
        assert_eq!(
            copy_tree(&src, &dst, &src.join("cache"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(dst.join("keep.txt")).unwrap(), b"not ours");
        assert_eq!(fs::read(src.join("source.txt")).unwrap(), b"original");
        let missing = tmp.path().join("missing source");
        let partial = tmp.path().join("partial copy");
        assert!(copy_tree(&missing, &partial, &missing.join("cache")).is_err());
        assert!(!partial.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn maintenance_move_restarts_only_previously_active_backend() {
        for active in [false, true] {
            let (tmp, approved) = crate::backend::tests::fixture(crate::backend::tests::SERVER);
            let site = fs::read_dir(tmp.path().join("venv/lib"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            fs::create_dir_all(site.join("site-packages/torch")).unwrap();
            fs::write(site.join("site-packages/torch/__init__.py"), "").unwrap();
            let engine = Engine::default();
            lock(&engine.inner).approved = Some(approved);
            if active {
                engine.backend_start().unwrap();
                let before = crate::backend::tests::await_state(&engine.backend, "running");
                assert!(move_environment(&engine, tmp.path().to_str().unwrap()).is_err());
                assert_eq!(engine.backend.status().endpoint, before.endpoint);
            }
            let parent = tempfile::tempdir().unwrap();
            let target = parent.path().join("moved environment");
            let gate = crate::FolderOperations::default();
            let guard = gate.enter().unwrap();
            let moved = move_environment(&engine, target.to_str().unwrap()).unwrap();
            assert_eq!(moved.root, target.to_str().unwrap());
            assert!(moved.warnings.is_empty(), "{:?}", moved.warnings);
            assert!(gate.enter().is_err());
            assert_eq!(engine.backend.active(), active);
            if active {
                crate::backend::tests::await_state(&engine.backend, "running");
            }
            drop(guard);
            assert!(gate.enter().is_ok());
            engine.shutdown_backend();
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn maintenance_move_rolls_back_and_resumes_after_python_check_failure() {
        let (tmp, approved) = crate::backend::tests::fixture(crate::backend::tests::SERVER);
        // No Torch module: the moved interpreter fails its check, but the original HTTP fixture works.
        let engine = Engine::default();
        lock(&engine.inner).approved = Some(approved);
        engine.backend_start().unwrap();
        crate::backend::tests::await_state(&engine.backend, "running");
        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("rejected move");
        let error = move_environment(&engine, target.to_str().unwrap()).unwrap_err();
        assert!(error.contains("put back"), "{error}");
        assert!(!error.contains("could not restart"), "{error}");
        assert!(tmp.path().is_dir() && !target.exists());
        lock(&engine.inner)
            .approved
            .as_ref()
            .unwrap()
            .validate()
            .unwrap();
        crate::backend::tests::await_state(&engine.backend, "running");
        engine.shutdown_backend();
    }

    #[test]
    fn rebase_replaces_whole_path_prefixes_only() {
        let text = "home = /a/env/runtime/bin\nx=/a/env2/y '/a/env' \"/a/env\"";
        assert_eq!(
            rebase(text, "/a/env", "/b/new env").unwrap(),
            "home = /b/new env/runtime/bin\nx=/a/env2/y '/b/new env' \"/b/new env\""
        );
        assert!(rebase("/a/env2", "/a/env", "/b").is_none());
        assert_eq!(json(r"C:\x\env"), r"C:\\x\\env");
        assert_eq!(
            rebase(
                r#"{"p":"C:\\x\\env\\venv"}"#,
                &json(r"C:\x\env"),
                &json(r"D:\e")
            )
            .unwrap(),
            r#"{"p":"D:\\e\\venv"}"#
        );
    }

    #[test]
    fn windows_launcher_trailer_is_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("tool.exe");
        let path = r"C:\x\env\venv\Scripts\python.exe";
        let mut bytes = b"MZpayload".to_vec();
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"UVSC");
        fs::write(&exe, &bytes).unwrap();
        relaunch(&exe, r"C:\x\env", r"D:\longer\env").unwrap();
        let out = fs::read(&exe).unwrap();
        let new = r"D:\longer\env\venv\Scripts\python.exe";
        assert!(out.starts_with(b"MZpayload") && out.ends_with(b"UVSC"));
        assert_eq!(&out[9..9 + new.len()], new.as_bytes());
        assert_eq!(
            u32::from_le_bytes(out[9 + new.len()..13 + new.len()].try_into().unwrap()) as usize,
            new.len()
        );
        // Not a launcher: untouched.
        fs::write(&exe, b"plain").unwrap();
        relaunch(&exe, r"C:\x\env", r"D:\e").unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"plain");
    }

    /// Real interpreter: `DEAS_MOVE_FIXTURE=<root with runtime/ and a uv venv/ (torch may be a stub)>`.
    /// Renames it away, rewrites, runs the moved Python, then moves it back the same way.
    #[test]
    #[ignore = "needs a prepared uv venv fixture"]
    fn real_venv_survives_a_move_and_back() {
        let old = PathBuf::from(std::env::var("DEAS_MOVE_FIXTURE").unwrap());
        let new = old.with_file_name(format!("moved-{}", uuid::Uuid::new_v4()));
        let (o, n) = (old.to_str().unwrap(), new.to_str().unwrap());
        fs::rename(&old, &new).unwrap();
        rebase_environment(&new, o, n, &mut vec![]).unwrap();
        python_check(&new).unwrap();
        rebase_environment(&new, n, o, &mut vec![]).unwrap();
        fs::rename(&new, &old).unwrap();
        python_check(&old).unwrap();
    }

    /// Whole command path on a real venv: same-drive rename and, when /dev/shm is another
    /// device, the copy path. The moved environment must pass the launch identity checks.
    #[cfg(target_os = "linux")]
    #[test]
    fn open_environment_moves_and_stays_launchable() {
        use std::os::unix::fs::MetadataExt;
        let (tmp, approved) = crate::backend::tests::fixture(crate::backend::tests::SERVER);
        let lib = tmp.path().join("venv/lib");
        let site = fs::read_dir(&lib).unwrap().next().unwrap().unwrap().path();
        fs::create_dir_all(site.join("site-packages/torch")).unwrap();
        fs::write(site.join("site-packages/torch/__init__.py"), "").unwrap();
        let approved = crate::backend::Approved::capture(approved.plan()).unwrap();
        let engine = Engine::default();
        lock(&engine.inner).approved = Some(approved);
        let parent = tempfile::tempdir().unwrap();
        let shm = Path::new("/dev/shm");
        let mut targets = vec![parent.path().join("same drive")];
        if shm.is_dir()
            && fs::metadata(shm).unwrap().dev() != fs::metadata(parent.path()).unwrap().dev()
        {
            targets.push(shm.join(format!("deas-move-{}", uuid::Uuid::new_v4())));
        }
        let mut current = tmp.path().to_owned();
        for target in targets {
            let moved = move_environment(&engine, target.to_str().unwrap()).unwrap();
            assert_eq!(Path::new(&moved.root), target);
            assert!(!current.exists() && target.join("deas-plan.json").is_file());
            let g = lock(&engine.inner);
            let approved = g.approved.as_ref().unwrap();
            assert_eq!(approved.plan().request.storage_path, moved.root);
            approved.validate().unwrap();
            drop(g);
            current = target;
        }
        assert!(move_environment(&engine, current.join("inside").to_str().unwrap()).is_err());
        let _ = fs::remove_dir_all(&current);
    }

    #[cfg(unix)]
    #[test]
    fn copy_and_rebase_keep_a_venv_usable() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        let bin = old.join(VENV_BIN);
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(old.join("cache/archive")).unwrap();
        let o = old.to_str().unwrap();
        fs::write(
            old.join("venv/pyvenv.cfg"),
            format!("home = {o}/runtime/bin\n"),
        )
        .unwrap();
        fs::write(
            bin.join("tool"),
            format!("#!{o}/venv/bin/python\nimport x\n"),
        )
        .unwrap();
        fs::set_permissions(bin.join("tool"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("python", bin.join("python3")).unwrap();
        symlink(old.join("cache/archive"), old.join("cache/entry")).unwrap();
        fs::write(
            old.join("deas-plan.json"),
            format!("{{\"storagePath\":\"{o}\"}}"),
        )
        .unwrap();
        let new = dir.path().join("new");
        copy_tree(&old, &new, &old.join("cache")).unwrap();
        let n = new.to_str().unwrap();
        rebase_environment(&new, o, n, &mut vec![]).unwrap();
        assert_eq!(
            fs::read_to_string(new.join("venv/pyvenv.cfg")).unwrap(),
            format!("home = {n}/runtime/bin\n")
        );
        assert!(fs::read_to_string(new.join("venv/bin/tool"))
            .unwrap()
            .starts_with(&format!("#!{n}/venv/bin/python\n")));
        assert_eq!(
            fs::metadata(new.join("venv/bin/tool"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::read_link(new.join("venv/bin/python3")).unwrap(),
            Path::new("python")
        );
        assert_eq!(
            fs::read_link(new.join("cache/entry")).unwrap(),
            new.join("cache/archive")
        );
        assert_eq!(
            fs::read_to_string(new.join("deas-plan.json")).unwrap(),
            format!("{{\"storagePath\":\"{n}\"}}")
        );
        // The original is untouched until the caller removes it.
        assert_eq!(
            fs::read_to_string(old.join("venv/pyvenv.cfg")).unwrap(),
            format!("home = {o}/runtime/bin\n")
        );
    }
}
