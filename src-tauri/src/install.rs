//! Native provisioning and checked existing-environment reopening. No sidecar, shell, system package manager or wipe.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

#[cfg(not(windows))]
const UV_URL: &str =
    "https://github.com/astral-sh/uv/releases/download/0.8.22/uv-x86_64-unknown-linux-gnu.tar.gz";
#[cfg(not(windows))]
const UV_SHA: &str = "741ff1f5742c5a4a25d2f829e8395355e43f7a5ae2ebc6368e9ae2df0efb69cf";
// Digest from the release asset uv-x86_64-pc-windows-msvc.zip.sha256 (0.8.22).
#[cfg(windows)]
const UV_URL: &str =
    "https://github.com/astral-sh/uv/releases/download/0.8.22/uv-x86_64-pc-windows-msvc.zip";
#[cfg(windows)]
const UV_SHA: &str = "5049375aa2a5162f132b2c1cb992e25d42d47d934cab8c174dbe6f60973dcc12";
const UV_STALL: Duration = Duration::from_secs(60);
const UNFINISHED_INSTALL: &str = "This folder holds an unfinished DEAS installation: an earlier install failed or was cancelled. DEAS never deletes or reuses it: choose a new directory (you may delete this one yourself).";
pub(crate) const VENV_BIN: &str = if cfg!(windows) {
    "venv/Scripts"
} else {
    "venv/bin"
};
pub(crate) const VENV_PYTHON: &str = if cfg!(windows) {
    "venv/Scripts/python.exe"
} else {
    "venv/bin/python"
};
const COMFY: &str = "https://github.com/comfyanonymous/ComfyUI.git";
const NODEPACK: &str = "https://github.com/darksidewalker/ComfyUI-DaSiWa-Nodes.git";
const GGUF: &str = "https://github.com/city96/ComfyUI-GGUF.git";
const GGUF_REVISION: &str = "6ea2651e7df66d7585f6ffee804b20e92fb38b8a";
#[path = "install_torch.rs"]
pub(crate) mod torch_profile;
// --help exits before imports. Initialize actual core/nodes without binding a
// listener, then assert the selected package registered its required nodes.
const COMFY_PROBE: &str = r#"import sys,json
backend,selected=sys.argv[1:3]
gguf=len(sys.argv)>3 and sys.argv[3]=='gguf'
sys.argv=['main.py','--disable-auto-launch','--cpu']
import main,nodes
main.start_comfyui()
required={'MiniMaxH3Director','MiniMaxH3DirectorGuide','DaSiWaH3ContinuityAppend','DaSiWaH3ContinuityPublish'} if selected=='nodes' else set()
if gguf: required.update({'UnetLoaderGGUF','CLIPLoaderGGUF','DualCLIPLoaderGGUF'})
missing=required-set(nodes.NODE_CLASS_MAPPINGS)
assert not missing, 'Missing selected nodes: '+', '.join(sorted(missing))
print(json.dumps({'coreImported':True,'selectedNodes':sorted(required),'generationReady':False}))
"#;
const MAX_LOGS: usize = 300;
const FRAMEWORK_PROBE: &str = r#"import json,sys,os
p=json.loads(sys.argv[2]); h=json.loads(sys.argv[3])
if h: os.environ['CUDA_VISIBLE_DEVICES']=h['uuid']
import torch,torchvision,torchaudio
assert sys.version_info[:2]==(3,12)
assert sys.prefix!=sys.base_prefix
assert os.path.normcase(os.path.commonpath([sys.base_prefix,sys.argv[1]]))==os.path.normcase(sys.argv[1])
for module,spec in zip([torch,torchvision,torchaudio],p['packages']):
    expected=spec.split('==')[1]
    expected += '+' + p['indexUrl'].rsplit('/',1)[1]
    assert module.__version__==expected, (module.__version__,expected)
assert not torch.version.hip
metadata={'python':sys.version,'basePrefix':sys.base_prefix,'torch':torch.__version__,'torchvision':torchvision.__version__,'torchaudio':torchaudio.__version__,'cuda':torch.version.cuda,'generationReady':False}
if h:
    assert torch.version.cuda==p['cuda']
    assert torch.cuda.is_available(), 'CUDA unavailable; check driver and device access'
    props=torch.cuda.get_device_properties(0)
    assert f'{props.major}.{props.minor}'==h['computeCapability'], 'GPU architecture changed; re-plan'
    if hasattr(props,'uuid'):
        assert str(props.uuid).removeprefix('GPU-').lower()==h['uuid'].removeprefix('GPU-').lower(), 'GPU identity changed; re-plan'
    x=torch.ones((2,2),device='cuda'); y=x@x
    torch.cuda.synchronize()
    assert bool(torch.isfinite(y).all().item() and (y==2).all().item())
    metadata.update(gpu=props.name,uuid=str(getattr(props,'uuid',h['uuid'])),computeCapability=f'{props.major}.{props.minor}',computed=True)
else:
    assert torch.version.cuda is None
    assert (torch.ones(2)+1).tolist()==[2,2]
print(json.dumps(metadata))
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cpu,
    Nvidia,
    Amd,
    Intel,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub sage: bool,
    pub radial: bool,
    pub flash: bool,
    pub ffmpeg: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub storage_path: String,
    pub backend: Backend,
    pub features: Vec<String>,
    pub options: Options,
    pub comfy_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folders: Option<crate::settings::Folders>,
    /// Rebuild an existing foreign ComfyUI folder in place: its venv and runtime are replaced,
    /// the requested ref and selected node packs are fetched, and DEAS manifests mark the folder
    /// as managed. Models, inputs, outputs and untracked files are never deleted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub takeover: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub label_key: String,
    pub detail: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gguf_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen_id: Option<String>,
    pub request: Request,
    pub steps: Vec<Step>,
    pub warnings: Vec<String>,
    pub unsupported_options: Vec<String>,
    pub can_execute: bool,
    pub hardware: Option<torch_profile::Hardware>,
    pub torch: Option<torch_profile::Torch>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: String,
    pub step_id: Option<String>,
    pub logs: Vec<String>,
    pub error: Option<String>,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            step_id: None,
            logs: vec![],
            error: None,
        }
    }
}
/// What a confirmed backend update changes: ComfyUI with its node packs and Python
/// dependencies under the pinned Torch, or only Torch to a newer profile this build knows.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateScope {
    #[default]
    Comfy,
    Torch,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorchUpdate {
    pub from: Vec<String>,
    pub to: Option<Vec<String>>,
    pub blocked: Option<String>,
}
#[derive(Clone, Default)]
pub struct Engine {
    pub(crate) inner: Arc<Mutex<Inner>>,
    pub(crate) backend: Arc<crate::backend::Lifecycle>,
}
#[derive(Default)]
pub(crate) struct Inner {
    plans: HashMap<String, Plan>,
    pub(crate) approved: Option<crate::backend::Approved>,
    reopen: Option<(String, crate::backend::Approved)>,
    // Environment of the last upgrade/reinstall, so a failed or cancelled run can be repaired.
    pub(crate) repair: Option<Plan>,
    status: Status,
    cancel: Arc<AtomicBool>,
}
pub(crate) fn lock(engine: &Arc<Mutex<Inner>>) -> std::sync::MutexGuard<'_, Inner> {
    engine.lock().unwrap_or_else(|e| e.into_inner())
}
fn log(engine: &Arc<Mutex<Inner>>, text: String) {
    crate::logs::push(crate::logs::Source::Deas, &text);
    keep(engine, text);
}
/// Status tail only; the full history lives in `crate::logs`.
fn keep(engine: &Arc<Mutex<Inner>>, text: String) {
    let mut g = lock(engine);
    if g.status.logs.len() >= MAX_LOGS {
        g.status.logs.remove(0);
    }
    g.status.logs.push(text.chars().take(4096).collect());
}

/// Require a new directory directly inside an existing, non-symlink parent.
/// Reject lexical traversal before any filesystem writes, even if it resolves safely.
pub(crate) fn validate_target(input: &str) -> Result<PathBuf, String> {
    let path = Path::new(input);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || input.split(['/', '\\']).any(|s| s == ".." || s == ".")
    {
        return Err("Choose an absolute new directory without traversal".into());
    }
    if path.file_name().is_none() {
        return Err("A filesystem root cannot be an installation target".into());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Installation paths must not contain symlinks".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && current == path => {}
            Err(e) => return Err(format!("Installation parent must exist: {e}")),
        }
    }
    if path.exists() {
        return Err(
            "Choose a new directory: existing environments are never modified or wiped".into(),
        );
    }
    let parent = path.parent().ok_or("Missing installation parent")?;
    if !parent.is_dir() {
        return Err("Installation parent must be a directory".into());
    }
    Ok(path.to_owned())
}
/// An existing foreign ComfyUI folder to rebuild in place. Same absolute-path, traversal and
/// symlink rules as `validate_target`; the target itself must exist as a directory containing
/// `ComfyUI/main.py`. While `creating`, folders that already carry a DEAS manifest are rejected:
/// they reopen through the normal owned-environment flow.
fn validate_takeover(input: &str, creating: bool) -> Result<PathBuf, String> {
    let path = Path::new(input);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || input.split(['/', '\\']).any(|s| s == ".." || s == ".")
    {
        return Err("Choose an existing directory without traversal".into());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Takeover paths must not contain symlinks".into());
            }
            Ok(_) => {}
            Err(e) => return Err(format!("Takeover parent must exist: {e}")),
        }
    }
    if !path.is_dir() {
        return Err("Takeover target must be an existing directory".into());
    }
    match std::fs::symlink_metadata(path.join("deas-plan.json")) {
        Ok(m) if m.file_type().is_symlink() => {
            return Err("DEAS manifest must be a regular file".into());
        }
        Ok(_) if creating && !path.join("deas-install.json").exists() => {
            return Err(UNFINISHED_INSTALL.into());
        }
        Ok(_) if creating => {
            return Err("This folder is already DEAS-managed; reopen it from Setup instead".into());
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    let comfy = path.join("ComfyUI");
    match std::fs::symlink_metadata(&comfy) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        Ok(_) => return Err("Takeover target must contain a regular ComfyUI folder".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("Takeover target must contain a ComfyUI folder".into());
        }
        Err(e) => return Err(e.to_string()),
    }
    if !comfy.join("main.py").is_file() {
        return Err("Takeover target must contain ComfyUI/main.py".into());
    }
    Ok(path.to_owned())
}
fn validate_ref(reference: &str) -> Result<(), String> {
    if reference.is_empty()
        || reference.len() > 128
        || reference.starts_with('-')
        || reference.contains("..")
        || !reference
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
    {
        return Err("ComfyUI ref must be a branch, tag or commit (for example master); no command options or URLs".into());
    }
    Ok(())
}
pub(crate) fn git_path() -> Option<PathBuf> {
    git_in_path(&std::env::var_os("PATH")?)
}
fn git_in_path(paths: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(paths).find_map(|dir| {
        let path = dir.join(if cfg!(windows) { "git.exe" } else { "git" });
        let meta = path.metadata().ok()?;
        if !meta.is_file() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                return None;
            }
        }
        dunce::canonicalize(path).ok()
    })
}
impl Engine {
    /// The plan of the currently approved (installed or reopened) environment, if any.
    pub(crate) fn approved_plan(&self) -> Option<Plan> {
        lock(&self.inner)
            .approved
            .as_ref()
            .map(|a| a.plan().clone())
    }
    pub fn shutdown_backend(&self) {
        self.backend.stop();
    }
    pub(crate) fn installation_idle(&self) -> bool {
        lock(&self.inner).status.state != "running"
    }
    pub(crate) fn folders_idle(&self) -> bool {
        self.installation_idle() && !self.backend.active()
    }
    pub(crate) fn stop_for_maintenance(&self) -> bool {
        let active = self.backend.active();
        if active {
            self.backend.stop();
        }
        active
    }
    /// Start only the current checked environment; a failed mutation must never authorize it.
    pub(crate) fn resume_after_maintenance(&self, active: bool, g: &Inner) -> Result<(), String> {
        if active {
            g.approved
                .clone()
                .ok_or_else(|| {
                    "No checked environment remains; repair it before restarting".to_string()
                })
                .and_then(|approved| self.backend.start(approved).map(|_| ()))
                .map_err(|e| {
                    let error = format!("Owned backend could not restart after maintenance: {e}");
                    crate::logs::push(crate::logs::Source::Deas, &format!("ERROR: {error}"));
                    error
                })?;
        }
        Ok(())
    }
    pub(crate) fn backend_start(&self) -> Result<crate::backend::Status, String> {
        let g = lock(&self.inner);
        if g.status.state == "running" {
            return Err("Provisioning is still running".into());
        }
        let approved = g
            .approved
            .clone()
            .ok_or("No successful native installation or verified existing environment is open")?;
        validate_gguf_install(
            approved.plan(),
            Path::new(&approved.plan().request.storage_path),
        )?;
        self.backend.start(approved)
    }
    /// Read-only dependency check for the owned backend; lists every check, launches nothing.
    pub(crate) fn backend_test(&self) -> Vec<crate::backend::DependencyCheck> {
        let g = lock(&self.inner);
        let root = g
            .approved
            .as_ref()
            .map(|a| Path::new(a.plan().request.storage_path.as_str()));
        [
            ("environment", root.is_some()),
            (
                "python",
                root.is_some_and(|r| r.join(VENV_PYTHON).is_file()),
            ),
            (
                "comfyui",
                root.is_some_and(|r| r.join("ComfyUI/main.py").is_file()),
            ),
            (
                "nodes",
                root.is_some_and(|r| r.join("ComfyUI/custom_nodes/ComfyUI-DaSiWa-Nodes").is_dir()),
            ),
            ("api", self.backend.status().state == "running"),
        ]
        .into_iter()
        .map(|(id, ok)| crate::backend::DependencyCheck { id, ok })
        .collect()
    }
    fn open_existing(&self, id: &str, confirmed: bool) -> Result<Status, String> {
        if !confirmed {
            return Err("Explicit existing environment approval is required".into());
        }
        let mut g = lock(&self.inner);
        if g.status.state == "running" || self.backend.active() {
            return Err("Wait for installation and stop only the owned backend before changing environments; no service was stopped".into());
        }
        if !g.reopen.as_ref().is_some_and(|(token, _)| token == id) {
            return Err(
                "Existing environment preview is missing or stale; choose the folder again".into(),
            );
        }
        let (_, approved) = g.reopen.take().unwrap();
        approved.validate()?;
        validate_gguf_install(
            approved.plan(),
            Path::new(&approved.plan().request.storage_path),
        )?;
        g.approved = Some(approved);
        g.status = Status { state: "succeeded".into(), step_id: None, logs: vec!["Existing DEAS environment opened. No installation, downloads or backend start performed.".into()], error: None };
        Ok(g.status.clone())
    }
    fn existing_plan(&self, input: &str) -> Result<Plan, String> {
        let path = Path::new(input);
        if !path.is_absolute() || input.split(['/', '\\']).any(|s| s == ".." || s == ".") {
            return Err("Existing environment needs an absolute path without traversal".into());
        }
        let root = dunce::canonicalize(path)
            .map_err(|e| format!("Existing environment is unavailable: {e}"))?;
        let bytes = crate::backend::read_owned(&root.join("deas-plan.json"), 1024 * 1024)
            .map_err(|_| "This folder is not a complete DEAS environment; choose a new directory or a completed DEAS installation".to_string())?;
        // provision() writes the plan first and the install manifest last: plan alone = an interrupted install.
        if !root.join("deas-install.json").exists() {
            return Err(UNFINISHED_INSTALL.into());
        }
        let mut plan: Plan = serde_json::from_slice(&bytes)
            .map_err(|_| "Existing DEAS plan is invalid".to_string())?;
        let persisted_root = dunce::canonicalize(&plan.request.storage_path)
            .map_err(|e| format!("Recorded environment location is unavailable: {e}"))?;
        if persisted_root != root
            || plan.reopen_id.is_some()
            || uuid::Uuid::parse_str(&plan.id).is_err()
            || !plan.can_execute
            || !plan.unsupported_options.is_empty()
            || !matches!(plan.request.backend, Backend::Cpu | Backend::Nvidia)
            || plan.request.features.iter().any(|id| {
                !["installer", "director", "forge", "refmods", "continuity"].contains(&id.as_str())
            })
            || plan.request.options.sage
            || plan.request.options.radial
            || plan.request.options.flash
        {
            return Err(
                "Existing DEAS environment is incompatible or its recorded location changed".into(),
            );
        }
        validate_ref(&plan.request.comfy_ref)?;
        validate_gguf_install(&plan, &root)?;
        let approved = crate::backend::Approved::capture(&plan)?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut g = lock(&self.inner);
        if g.status.state == "running" || self.backend.active() {
            return Err(
                "An owned installation/backend is active; no environment was changed".into(),
            );
        }
        g.reopen = Some((id.clone(), approved));
        plan.id = id.clone();
        plan.reopen_id = Some(id);
        plan.steps.clear();
        plan.warnings = vec!["Existing DEAS installation: opening does not reinstall, update or remove files. Backend start remains a separate explicit action; generation readiness is not verified.".into()];
        Ok(plan)
    }
    fn plan(&self, mut request: Request) -> Result<Plan, String> {
        if request.takeover {
            validate_takeover(&request.storage_path, true)?;
        } else if Path::new(&request.storage_path).exists() {
            return self.existing_plan(&request.storage_path);
        } else {
            validate_target(&request.storage_path)?;
        }
        validate_ref(&request.comfy_ref)?;
        if let Some(folders) = &request.folders {
            crate::settings::validate(folders)?;
        }
        // Director is one selected package: its companion capabilities are included,
        // not independently optional. Persist the complete request before consent.
        if request.features.iter().any(|feature| feature == "director") {
            for included in ["forge", "refmods", "continuity"] {
                if !request.features.iter().any(|feature| feature == included) {
                    request.features.push(included.into());
                }
            }
        }
        let mut unsupported = vec![];
        let mut hardware = None;
        let mut torch = None;
        match request.backend {
            Backend::Cpu => torch = Some(torch_profile::cpu()),
            Backend::Nvidia => match torch_profile::probe() {
                Ok(found) => {
                    match torch_profile::select(&found) {
                        Ok(selected) => torch = Some(selected),
                        Err(error) => unsupported.push(error),
                    }
                    hardware = Some(found);
                }
                Err(error) => unsupported.push(error),
            },
            Backend::Amd | Backend::Intel => {}
        }
        if matches!(request.backend, Backend::Amd | Backend::Intel) {
            unsupported.push(format!(
                "backend:{:?} (GPU provisioning/preflight not ported)",
                request.backend
            ));
        }
        if !(cfg!(any(target_os = "linux", windows)) && cfg!(target_arch = "x86_64")) {
            unsupported.push(
                "platform: only Linux/Windows x86_64 CPU/NVIDIA baseline is supported".into(),
            );
        }
        for (name, selected) in [
            ("sage", request.options.sage),
            ("radial", request.options.radial),
            ("flash", request.options.flash),
        ] {
            if selected {
                unsupported.push(name.into());
            }
        }
        for feature in &request.features {
            if !["installer", "director", "forge", "refmods", "continuity"]
                .contains(&feature.as_str())
            {
                unsupported.push(format!("feature:{feature}"));
            }
        }
        let mut warnings=vec!["New environment only. Downloads Python, selected pinned Torch and selected source dependencies; may require several GB. No models, launch, API readiness or generation verification. Failed/cancelled partial directories are retained, never wiped.".into(),"ComfyUI branches/tags and selected nodepack main are fetched at execution time; final Git revisions are recorded. Dependency installation executes upstream package build code inside the managed environment.".into()];
        if git_path().is_none() {
            unsupported
                .push("git: install Git through your OS tools, then create a new plan".into());
        }
        if request
            .features
            .iter()
            .any(|f| ["director", "forge", "refmods", "continuity"].contains(&f.as_str()))
        {
            warnings.push("Selected DaSiWa features select the shared nodepack only; model recipes, weights, workflows and feature readiness are not provisioned.".into());
        }
        let mut steps = vec![];
        let takeover = request.takeover;
        if takeover {
            warnings.push("Existing folder is not DEAS-managed. DEAS will delete and rebuild its venv and runtime, fetch the selected ComfyUI ref and node packs, and write DEAS manifests that mark the folder as DEAS-managed. Models, inputs, outputs and untracked files are never deleted.".into());
        }
        let comfy_detail = if takeover {
            "Fetch requested ComfyUI ref into target/ComfyUI; tracked files reset, untracked files kept"
        } else {
            "Fetch requested ComfyUI ref into new target/ComfyUI"
        };
        let venv_detail = if takeover {
            "Delete the existing target/venv and create a fresh isolated venv"
        } else {
            "Create isolated target/venv"
        };
        for (id, detail) in [
            (
                "uv",
                "Download official uv 0.8.22 archive and verify embedded SHA-256 before extracting executable",
            ),
            (
                "python",
                "Install managed Python 3.12 under target/runtime; no system Python or PATH changes",
            ),
            ("comfy", comfy_detail),
            ("venv", venv_detail),
            (
                "torch",
                "Install torch 2.8.0 / torchvision 0.23.0 / torchaudio 2.8.0 from official CPU wheel index",
            ),
            (
                "requirements",
                "Install ComfyUI requirements with Torch constraints",
            ),
        ] {
            steps.push(Step {
                id: id.into(),
                label_key: format!("installer.steps.{id}"),
                detail: detail.into(),
            });
        }
        if request.options.ffmpeg {
            steps.push(Step{id:"ffmpeg".into(),label_key:"installer.steps.ffmpeg".into(),detail:"Install imageio-ffmpeg; copy its static FFmpeg binary into the managed venv bin".into()});
        }
        steps.push(Step {
            id: "gguf".into(),
            label_key: "installer.steps.nodes".into(),
            detail: format!("Fetch ComfyUI-GGUF at {GGUF_REVISION}; install required GGUF loader dependencies in managed venv (no models or LoRAs)"),
        });
        if request
            .features
            .iter()
            .any(|f| ["director", "forge", "refmods", "continuity"].contains(&f.as_str()))
        {
            steps.push(Step {
                id: "nodes".into(),
                label_key: "installer.steps.nodes".into(),
                detail: "Fetch DaSiWa nodepack main; install its requirements in managed venv"
                    .into(),
            });
        }
        steps.push(Step{id:"verify".into(),label_key:"installer.steps.verify".into(),detail:"Import exact Torch trio, confirm CPU tensors and initialize actual ComfyUI core/selected nodes without a listener; record manifest (not generation readiness)".into()});
        if let Some(selected) = &torch {
            if let Some(step) = steps.iter_mut().find(|s| s.id == "torch") {
                step.detail = format!(
                    "Install {} from {}",
                    selected.packages.join(" / "),
                    selected.index_url
                );
            }
        }
        if matches!(request.backend, Backend::Nvidia) {
            warnings.push("NVIDIA preflight is read-only, bounded to 5 seconds, and supports one visible GPU only. CUDA 13 requires driver >=580; Pascal uses CUDA 12.1. No CUDA toolkit or attention extensions are installed.".into());
            steps.last_mut().unwrap().detail = "Verify exact framework trio, CUDA metadata, planned GPU identity and synchronized CUDA matrix multiplication; initialize actual ComfyUI core/selected nodes without a listener (not generation readiness)".into();
        }
        let plan = Plan {
            gguf_ref: Some(GGUF_REVISION.into()),
            reopen_id: None,
            id: uuid::Uuid::new_v4().to_string(),
            request,
            steps,
            warnings,
            can_execute: unsupported.is_empty(),
            unsupported_options: unsupported,
            hardware,
            torch,
        };
        let mut g = lock(&self.inner);
        if g.plans.len() >= 64 {
            return Err("Plan limit reached; restart app to discard old plans".into());
        }
        g.plans.insert(plan.id.clone(), plan.clone());
        Ok(plan)
    }
    fn start(&self, id: &str, confirmed: bool) -> Result<Status, String> {
        if !confirmed {
            return Err("Explicit confirmation is required".into());
        }
        let mut g = lock(&self.inner);
        if g.status.state == "running" {
            return Err("A native installation is already running".into());
        }
        if self.backend.active() {
            return Err("Stop the owned backend before starting a new provision".into());
        }
        let plan = g
            .plans
            .get(id)
            .ok_or("Unknown or already consumed plan ID")?
            .clone();
        if !plan.can_execute {
            return Err("Plan contains unsupported selections or prerequisites".into());
        }
        if plan.request.takeover {
            validate_takeover(&plan.request.storage_path, false)?;
        } else {
            validate_target(&plan.request.storage_path)?;
        }
        let git = git_path().ok_or("Git is no longer available; install Git and re-plan")?;
        g.plans.remove(id);
        g.approved = None;
        g.cancel = Arc::new(AtomicBool::new(false));
        g.status = Status {
            state: "running".into(),
            ..Status::default()
        };
        launch(self.clone(), g.cancel.clone(), plan, git, None, None);
        Ok(g.status.clone())
    }
    // Windows locks the running manifest exclusively; mutation revalidates after owned shutdown.
    fn maintenance_plan(&self, g: &Inner) -> Result<Plan, String> {
        if self.backend.active() {
            g.approved
                .as_ref()
                .map(|approved| approved.plan().clone())
                .ok_or_else(|| "No checked owned environment is open".into())
        } else {
            current_plan(g)
        }
    }
    /// Read-only: the newer Torch profile this build would install for the open environment.
    fn torch_check(&self) -> Result<TorchUpdate, String> {
        let plan = self.maintenance_plan(&lock(&self.inner))?;
        let from = plan.torch.as_ref().ok_or("Missing planned Torch profile")?;
        let (to, blocked) = match torch_target(&plan) {
            Err(error) => (None, Some(error)),
            Ok(None) => (None, None),
            Ok(Some((torch, _))) => (Some(torch.packages.to_vec()), foreign_nodes(&plan)),
        };
        Ok(TorchUpdate {
            from: from.packages.to_vec(),
            to,
            blocked,
        })
    }
    /// Update (`reinstall=false`) or rebuild the venv of (`reinstall=true`) the open environment.
    fn update(
        &self,
        reinstall: bool,
        confirmed: bool,
        scope: UpdateScope,
    ) -> Result<Status, String> {
        if !confirmed {
            return Err("Explicit confirmation is required".into());
        }
        let mut g = lock(&self.inner);
        if g.status.state == "running" {
            return Err("A native installation is already running".into());
        }
        let mut plan = self.maintenance_plan(&g)?;
        let original = plan.clone();
        let git = git_path().ok_or("Git is not available; install Git first")?;
        if scope == UpdateScope::Torch {
            if reinstall {
                return Err("A PyTorch update cannot be combined with reinstallation".into());
            }
            // Re-check the gates at execution time; the dialog's earlier answer is not trusted.
            let (torch, hardware) = torch_target(&plan)?
                .ok_or("No newer PyTorch profile is known for this environment")?;
            if let Some(reason) = foreign_nodes(&plan) {
                return Err(reason);
            }
            plan.torch = Some(torch);
            if hardware.is_some() {
                plan.hardware = hardware;
            }
        } else {
            enable_gguf_upgrade(&mut plan);
            if let Some(recorded) = &plan.hardware {
                plan.hardware = Some(refreshed_hardware(recorded, torch_profile::probe()?)?);
            }
        }
        let previous = g.approved.clone();
        let active = self.stop_for_maintenance();
        if let Some(previous) = &previous {
            if let Err(error) = previous.validate() {
                crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!("ERROR: Update refused after owned shutdown: {error}"),
                );
                return Err(error);
            }
        }
        // A Torch change is recorded only after its verify step passes (in `launch`).
        g.repair = Some(if scope == UpdateScope::Torch {
            original
        } else {
            plan.clone()
        });
        g.approved = None;
        g.cancel = Arc::new(AtomicBool::new(false));
        g.status = Status {
            state: "running".into(),
            ..Status::default()
        };
        launch(
            self.clone(),
            g.cancel.clone(),
            plan,
            git,
            Some((reinstall, scope)),
            Some((active, previous)),
        );
        Ok(g.status.clone())
    }
}
/// Run provisioning (`refresh=None`) or an update of the existing environment in the background.
fn launch(
    engine: Engine,
    cancel: Arc<AtomicBool>,
    plan: Plan,
    git: PathBuf,
    refresh: Option<(bool, UpdateScope)>,
    resume: Option<(bool, Option<crate::backend::Approved>)>,
) {
    tauri::async_runtime::spawn(async move {
        let inner = engine.inner.clone();
        let result = match refresh {
            None => provision(&plan, &git, &inner, &cancel).await,
            Some((reinstall, scope)) => {
                update(&plan, &git, &inner, &cancel, reinstall, scope).await
            }
        };
        let result = result.and_then(|()| crate::backend::Approved::capture(&plan));
        let torch = matches!(refresh, Some((_, UpdateScope::Torch)));
        let previous = torch.then(|| current_plan(&lock(&inner)).ok()).flatten();
        let result = result.map_err(|e| match previous.as_ref().and_then(|p| p.torch.as_ref()) {
            Some(old) => format!("PyTorch update failed; deas-plan.json still records {}. The venv may be partially changed: Update ComfyUI reinstalls the recorded profile. {e}", old.packages.join(", ")),
            None => e,
        });
        let mut g = lock(&inner);
        if !cancel.load(Ordering::SeqCst) {
            if let Ok(approved) = &result {
                g.approved = Some(approved.clone());
                if torch {
                    g.repair = Some(plan.clone());
                }
            }
        }
        g.status.state = if cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else if result.is_ok() {
            "succeeded"
        } else {
            "failed"
        }
        .into();
        if let Err(error) = &result {
            crate::logs::push(crate::logs::Source::Deas, &format!("ERROR: {error}"));
        }
        g.status.error = result.err();
        g.status.step_id = None;
        if let Some((active, previous)) = resume {
            // Failed/cancelled updates may resume only when the original launch proof still holds.
            if g.approved.is_none() {
                g.approved = previous.filter(|approved| approved.validate().is_ok());
            }
            // Keep the engine locked until startup is issued: no terminal-status/start race.
            if let Err(error) = engine.resume_after_maintenance(active, &g) {
                g.status.error = Some(match g.status.error.take() {
                    Some(original) => format!("{original}. {error}"),
                    None => error,
                });
            }
        }
    });
}
#[tauri::command]
pub fn native_install_plan(
    request: Request,
    engine: tauri::State<'_, Engine>,
) -> Result<Plan, String> {
    engine.plan(request)
}
#[tauri::command]
pub fn native_install_start(
    plan_id: String,
    confirmed: bool,
    engine: tauri::State<'_, Engine>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<Status, String> {
    let _guard = operations.enter()?;
    engine.start(&plan_id, confirmed)
}
#[tauri::command]
pub fn native_install_open(
    plan_id: String,
    confirmed: bool,
    engine: tauri::State<'_, Engine>,
    operations: tauri::State<'_, crate::FolderOperations>,
    downloads: tauri::State<'_, crate::downloads::DownloadEngine>,
) -> Result<Status, String> {
    let _guard = operations.enter()?;
    if downloads.status()?.state == "running" {
        return Err(
            "Wait for downloads before changing environments; no download was stopped".into(),
        );
    }
    engine.open_existing(&plan_id, confirmed)
}
#[tauri::command]
pub async fn native_install_update(
    reinstall: bool,
    confirmed: bool,
    scope: Option<UpdateScope>,
    app: tauri::AppHandle,
) -> Result<Status, String> {
    use tauri::Manager;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    // The worker owns the folder gate until asynchronous mutation and resume have finished.
    tauri::async_runtime::spawn_blocking(move || {
        let operations = app.state::<crate::FolderOperations>();
        let engine = app.state::<Engine>();
        let result = (|| {
            let guard = operations.enter()?;
            if app
                .state::<crate::downloads::DownloadEngine>()
                .status()?
                .state
                == "running"
            {
                return Err(
                    "Wait for downloads before updating the environment; no download was stopped"
                        .into(),
                );
            }
            if app.state::<crate::jobs::Jobs>().workbench_busy()? {
                return Err(
                    "Wait for running Studio jobs before updating; no job was stopped".into(),
                );
            }
            let status = engine.update(reinstall, confirmed, scope.unwrap_or_default())?;
            Ok((guard, status))
        })();
        match result {
            Ok((_guard, status)) => {
                let _ = sender.send(Ok(status));
                while !engine.installation_idle() {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(error) => {
                let _ = sender.send(Err(error));
            }
        }
    });
    receiver
        .await
        .map_err(|e| format!("Update worker failed: {e}"))?
}
/// Read-only check for the PyTorch-only update: from → to and why it is blocked, if it is.
#[tauri::command]
pub fn native_torch_update_check(engine: tauri::State<'_, Engine>) -> Result<TorchUpdate, String> {
    engine.torch_check()
}
#[tauri::command]
pub fn native_install_status(engine: tauri::State<'_, Engine>) -> Status {
    lock(&engine.inner).status.clone()
}
#[tauri::command]
pub fn native_install_cancel(engine: tauri::State<'_, Engine>) -> Status {
    let g = lock(&engine.inner);
    if g.status.state == "running" {
        g.cancel.store(true, Ordering::SeqCst);
    }
    g.status.clone()
}

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("Cancelled; partial installation retained".into())
    } else {
        Ok(())
    }
}
fn step(inner: &Arc<Mutex<Inner>>, id: &str) {
    lock(inner).status.step_id = Some(id.into());
    log(inner, format!("Step: {id}"));
}
fn isolated(command: &mut Command, root: &Path) {
    // Keep only OS execution essentials. Remove inherited Python/pip/uv, proxies,
    // Git overrides and credentials so user configuration cannot redirect writes.
    let path = std::env::var_os("PATH");
    command.env_clear();
    if let Some(path) = path {
        command.env("PATH", path);
    }
    for name in ["SYSTEMROOT", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (key, folder) in [
        ("HOME", "home"),
        ("USERPROFILE", "home"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_CONFIG_HOME", "config"),
        ("UV_CACHE_DIR", "cache/uv"),
        ("UV_PYTHON_INSTALL_DIR", "runtime/python"),
        ("UV_PYTHON_BIN_DIR", "runtime/bin"),
        ("TMPDIR", "tmp"),
        ("TMP", "tmp"),
        ("TEMP", "tmp"),
    ] {
        command.env(key, root.join(folder));
    }
    let constraints = root.join("constraints.txt");
    if constraints.is_file() {
        if let Ok(uri) = reqwest::Url::from_file_path(&constraints) {
            // uv splits bare paths on spaces; an encoded URI propagates safely
            // into child package installers, with final imports still mandatory.
            command.env("UV_CONSTRAINT", uri.as_str());
        }
    }
    command
        .env("UV_NO_CONFIG", "1")
        .env("PYTHONUTF8", "1")
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_GLOBAL", root.join("config/empty-gitconfig"));
}
async fn capture<R: AsyncRead + Unpin>(
    stream: R,
    inner: Arc<Mutex<Inner>>,
) -> Result<String, String> {
    use tokio::io::AsyncBufReadExt;
    let mut reader = tokio::io::BufReader::new(stream);
    let mut collected = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = (&mut reader)
            .take(crate::backend::MAX_LOG_LINE)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let text = crate::backend::clean_log_line(&line);
        if !text.is_empty() {
            crate::logs::push(crate::logs::Source::Cli, &text);
            keep(&inner, text);
        }
        if collected.len() < 262144 {
            let take = n.min(262144 - collected.len());
            collected.extend_from_slice(&line[..take]);
        }
    }
    Ok(String::from_utf8_lossy(&collected).into_owned())
}
async fn run(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    root: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    run_with_timeout(
        program,
        args,
        cwd,
        root,
        inner,
        cancel,
        Duration::from_secs(3600),
    )
    .await
}

async fn run_with_timeout(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    root: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
    timeout: Duration,
) -> Result<String, String> {
    cancelled(cancel)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    isolated(&mut command, root);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(crate::win::CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot run {}: {e}", program.display()))?;
    #[cfg(unix)]
    let pid = child.id();
    // Kill-on-close job: descendants die with the runner even on early return.
    #[cfg(windows)]
    let job = crate::win::Job::new()?;
    #[cfg(windows)]
    job.assign(child.raw_handle().ok_or("Missing child handle")?)?;
    let stdout = tokio::spawn(capture(
        child.stdout.take().ok_or("Missing stdout")?,
        inner.clone(),
    ));
    let stderr = tokio::spawn(capture(
        child.stderr.take().ok_or("Missing stderr")?,
        inner.clone(),
    ));
    let deadline = tokio::time::Instant::now() + timeout;
    let status = loop {
        if cancel.load(Ordering::SeqCst) || tokio::time::Instant::now() > deadline {
            #[cfg(unix)]
            if let Some(pid) = pid {
                // Only our freshly spawned private process group.
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            #[cfg(windows)]
            job.kill();
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout.abort();
            stderr.abort();
            return Err(if cancel.load(Ordering::SeqCst) {
                "Cancelled; partial installation retained".into()
            } else {
                format!("Command timed out after {} seconds", timeout.as_secs())
            });
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    // A package builder must not leave descendants holding pipes or working
    // after its parent exits. This group was created exclusively by this runner.
    #[cfg(unix)]
    if let Some(pid) = pid {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    job.kill();
    let out = stdout.await.map_err(|e| e.to_string())??;
    let err = stderr.await.map_err(|e| e.to_string())??;
    if !status.success() {
        return Err(format!(
            "{} exited {status}: {}",
            program.display(),
            err.lines()
                .map(|l| crate::backend::clean_log_line(l.as_bytes()))
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(2048)
                .collect::<String>()
        ));
    }
    cancelled(cancel)?;
    Ok(out)
}
async fn bootstrap(
    root: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
) -> Result<PathBuf, String> {
    // A slow but moving download is fine; only a stalled one fails. A whole-request deadline
    // surfaced mid-body as reqwest's misleading "error decoding response body".
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(UV_STALL)
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response=tokio::select! {r=client.get(UV_URL).send()=>r.map_err(uv_download_error)?, _=wait_cancel(cancel)=>return Err("Cancelled".into())}.error_for_status().map_err(uv_download_error)?;
    let mut bytes = vec![];
    loop {
        let chunk = tokio::select! { r=response.chunk()=>r.map_err(uv_download_error)?, _=wait_cancel(cancel)=>return Err("Cancelled".into()) };
        let Some(chunk) = chunk else { break };
        if bytes.len() + chunk.len() > 64 * 1024 * 1024 {
            return Err("uv archive exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let uv = extract_verified_uv(&bytes, root)?;
    log(
        inner,
        "uv archive SHA-256 verified against embedded release digest".into(),
    );
    Ok(uv)
}
fn uv_download_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        format!(
            "uv download stalled (no data for {} s, or over 15 minutes in total); check the network connection or pause other downloads, then retry",
            UV_STALL.as_secs()
        )
    } else {
        format!("uv download failed: {error}")
    }
}
async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
fn extract_verified_uv(bytes: &[u8], root: &Path) -> Result<PathBuf, String> {
    if format!("{:x}", Sha256::digest(bytes)) != UV_SHA {
        return Err("uv archive checksum mismatch; refusing execution".into());
    }
    if cfg!(windows) {
        let uv = root.join("runtime/uv.exe");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&uv)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut zip_member(bytes, "uv.exe")?.as_slice(), &mut file)
            .map_err(|e| e.to_string())?;
        return Ok(uv);
    }
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if entry.path().map_err(|e| e.to_string())?.as_ref()
            != Path::new("uv-x86_64-unknown-linux-gnu/uv")
        {
            continue;
        }
        if !entry.header().entry_type().is_file() || entry.size() > 128 * 1024 * 1024 {
            return Err("Invalid uv archive executable".into());
        }
        let uv = root.join("runtime/uv");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&uv)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        return Ok(uv);
    }
    Err("Official archive did not contain uv".into())
}
// Minimal reader for one stored/deflated member of a verified zip (no zip64, no encryption).
fn zip_member(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let u16_at = |i: usize| {
        bytes
            .get(i..i + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32_at = |i: usize| {
        bytes
            .get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let eocd = (0..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or("Invalid uv archive")?;
    let count = u16_at(eocd + 10).ok_or("Invalid uv archive")?;
    let mut offset = u32_at(eocd + 16).ok_or("Invalid uv archive")?;
    for _ in 0..count {
        if bytes.get(offset..offset + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("Invalid uv archive".into());
        }
        let (method, size, len) = (
            u16_at(offset + 10).ok_or("Invalid uv archive")?,
            u32_at(offset + 20).ok_or("Invalid uv archive")?,
            u32_at(offset + 24).ok_or("Invalid uv archive")?,
        );
        let (n, m, k) = (
            u16_at(offset + 28).ok_or("Invalid uv archive")?,
            u16_at(offset + 30).ok_or("Invalid uv archive")?,
            u16_at(offset + 32).ok_or("Invalid uv archive")?,
        );
        let local = u32_at(offset + 42).ok_or("Invalid uv archive")?;
        if bytes.get(offset + 46..offset + 46 + n) == Some(name.as_bytes()) {
            if len > 128 * 1024 * 1024
                || bytes.get(local..local + 4) != Some(&[0x50, 0x4b, 0x03, 0x04])
            {
                return Err("Invalid uv archive executable".into());
            }
            let start = local
                + 30
                + u16_at(local + 26).ok_or("Invalid uv archive")?
                + u16_at(local + 28).ok_or("Invalid uv archive")?;
            let data = bytes.get(start..start + size).ok_or("Invalid uv archive")?;
            let mut out = Vec::with_capacity(len);
            match method {
                0 => out.extend_from_slice(data),
                8 => flate2::read::DeflateDecoder::new(data)
                    .take(len as u64 + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| e.to_string())
                    .map(|_| ())?,
                _ => return Err("Unsupported uv archive compression".into()),
            }
            if out.len() != len {
                return Err("Invalid uv archive executable".into());
            }
            return Ok(out);
        }
        offset += 46 + n + m + k;
    }
    Err("Official archive did not contain uv".into())
}
async fn checkout(
    git: &Path,
    url: &str,
    reference: &str,
    target: &Path,
    root: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    if target.join(".git").is_dir() {
        let actual = run(
            git,
            &["rev-parse", "--show-toplevel"],
            target,
            root,
            inner,
            cancel,
        )
        .await?;
        if dunce::canonicalize(actual.trim()).map_err(|e| e.to_string())?
            != dunce::canonicalize(target).map_err(|e| e.to_string())?
        {
            return Err("Git worktree redirects outside the approved checkout".into());
        }
        // Existing owned checkout (upgrade/reinstall): point origin at the canonical URL again.
        run(
            git,
            &["remote", "set-url", "origin", url],
            target,
            root,
            inner,
            cancel,
        )
        .await?;
    } else {
        std::fs::create_dir(target).map_err(|e| e.to_string())?;
        run(git, &["init", "."], target, root, inner, cancel).await?;
        run(
            git,
            &["remote", "add", "origin", url],
            target,
            root,
            inner,
            cancel,
        )
        .await?;
    }
    run(
        git,
        &["fetch", "--depth", "1", "origin", reference],
        target,
        root,
        inner,
        cancel,
    )
    .await?;
    run(
        git,
        &[
            "-c",
            "advice.detachedHead=false",
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.eol=lf",
            "checkout",
            "--force",
            "--detach",
            "FETCH_HEAD",
        ],
        target,
        root,
        inner,
        cancel,
    )
    .await?;
    Ok(
        run(git, &["rev-parse", "HEAD"], target, root, inner, cancel)
            .await?
            .trim()
            .into(),
    )
}
/// Revision of an existing owned checkout, without fetching; rejects redirected worktrees.
async fn head(
    git: &Path,
    target: &Path,
    root: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    if !target.join(".git").is_dir() {
        return Err(format!(
            "{} is not an owned checkout; run Update ComfyUI first",
            target.display()
        ));
    }
    let actual = run(
        git,
        &["rev-parse", "--show-toplevel"],
        target,
        root,
        inner,
        cancel,
    )
    .await?;
    if dunce::canonicalize(actual.trim()).map_err(|e| e.to_string())?
        != dunce::canonicalize(target).map_err(|e| e.to_string())?
    {
        return Err("Git worktree redirects outside the approved checkout".into());
    }
    Ok(
        run(git, &["rev-parse", "HEAD"], target, root, inner, cancel)
            .await?
            .trim()
            .into(),
    )
}
// Reject redirected custom-node/Git directories before touching the owned checkout.
fn gguf_target(comfy: &Path) -> Result<PathBuf, String> {
    let node = comfy.join("custom_nodes/ComfyUI-GGUF");
    for path in [
        comfy.to_owned(),
        comfy.join("custom_nodes"),
        node.clone(),
        node.join(".git"),
    ] {
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err("ComfyUI-GGUF directories must not be reparse points".into());
                    }
                }
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    return Err("ComfyUI-GGUF directories must be regular owned directories".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Cannot validate ComfyUI-GGUF directory: {e}")),
        }
    }
    Ok(node)
}

fn validate_gguf_sources(node: &Path, reference: &str) -> Result<(), String> {
    // Preserve previous manifests here if a later release adds another supported pin.
    let source: serde_json::Value =
        serde_json::from_str(include_str!("../gguf-source.json")).map_err(|e| e.to_string())?;
    if source["revision"] != reference {
        return Err("Unsupported ComfyUI-GGUF source revision".into());
    }
    for (name, expected) in source["files"]
        .as_object()
        .ok_or("Invalid bundled GGUF source manifest")?
    {
        let actual = Sha256::digest(crate::backend::read_owned(&node.join(name), 1024 * 1024)?);
        if Some(format!("{actual:x}").as_str()) != expected.as_str() {
            return Err(format!(
                "ComfyUI-GGUF source changed: {name}; re-provision required"
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_gguf_install(plan: &Plan, root: &Path) -> Result<(), String> {
    let result = (|| {
        let Some(reference) = plan.gguf_ref.as_deref() else {
            return Ok(());
        };
        if reference.len() != 40 || !reference.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid immutable ComfyUI-GGUF revision".into());
        }
        let manifest: serde_json::Value = serde_json::from_slice(&crate::backend::read_owned(
            &root.join("deas-install.json"),
            1024 * 1024,
        )?)
        .map_err(|e| format!("Invalid ComfyUI-GGUF install manifest: {e}"))?;
        let node = gguf_target(&root.join("ComfyUI"))?;
        let head = crate::backend::read_owned(&node.join(".git/HEAD"), 1024)?;
        if manifest["ggufRevision"] != reference
            || std::str::from_utf8(&head)
                .map_err(|e| e.to_string())?
                .trim()
                != reference
        {
            return Err(
                "ComfyUI-GGUF install does not match its pinned revision; re-provision required"
                    .into(),
            );
        }
        validate_gguf_sources(&node, reference)?;
        Ok(())
    })();
    result.inspect_err(|error| {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("ERROR installer.gguf verify: {error}"),
        );
    })
}

// Only explicit Upgrade/Reinstall extends a legacy plan; opening/starting stays read-only.
fn enable_gguf_upgrade(plan: &mut Plan) {
    if plan.gguf_ref.is_none() {
        plan.gguf_ref = Some(GGUF_REVISION.into());
        let index = plan
            .steps
            .iter()
            .position(|s| s.id == "nodes" || s.id == "verify")
            .unwrap_or(plan.steps.len());
        plan.steps.insert(index, Step {
            id: "gguf".into(), label_key: "installer.steps.nodes".into(),
            detail: format!("Fetch ComfyUI-GGUF at {GGUF_REVISION}; install required GGUF loaders (no models or LoRAs)"),
        });
    }
}

/// The environment an update acts on: the checked open environment, else the last update's plan.
fn current_plan(g: &Inner) -> Result<Plan, String> {
    match &g.approved {
        Some(approved) => {
            approved.validate()?;
            Ok(approved.plan().clone())
        }
        None => g
            .repair
            .clone()
            .ok_or_else(|| "No installed DEAS environment is open".into()),
    }
}
/// Newer Torch for `plan` plus the hardware it was gated on (CUDA probes the GPU read-only).
fn torch_target(
    plan: &Plan,
) -> Result<Option<(torch_profile::Torch, Option<torch_profile::Hardware>)>, String> {
    let current = plan.torch.as_ref().ok_or("Missing planned Torch profile")?;
    let hardware = match current.cuda {
        Some(_) => Some(torch_profile::probe()?),
        None => None,
    };
    Ok(torch_profile::newer(current, hardware.as_ref())?.map(|torch| (torch, hardware)))
}
/// Hardware a reinstall/update may record: a driver update or same-capability GPU keeps the
/// installed Torch valid; another compute capability needs a new plan and Torch profile.
fn refreshed_hardware(
    recorded: &torch_profile::Hardware,
    probed: torch_profile::Hardware,
) -> Result<torch_profile::Hardware, String> {
    if probed.compute_capability != recorded.compute_capability {
        return Err(format!(
            "NVIDIA_GPU: compute capability changed ({} -> {}); the installed PyTorch profile may not support it, create a new installation plan",
            recorded.compute_capability, probed.compute_capability
        ));
    }
    torch_profile::select(&probed)?;
    Ok(probed)
}
/// Node packs DEAS does not install have unknown compatibility with another Torch.
fn foreign_nodes(plan: &Plan) -> Option<String> {
    let dir = Path::new(&plan.request.storage_path).join("ComfyUI/custom_nodes");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() || kind.is_symlink())
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            !["ComfyUI-GGUF", "ComfyUI-DaSiWa-Nodes", "__pycache__"].contains(&name.as_str())
        })
        .collect();
    names.sort();
    (!names.is_empty()).then(|| format!("Node packs not installed by DEAS ({}) are not known to work with another PyTorch; remove them or keep the current profile", names.join(", ")))
}
fn persist_updated_plan(plan: &Plan) -> Result<(), String> {
    use std::io::Write;
    let root = Path::new(&plan.request.storage_path);
    let path = root.join("deas-plan.json");
    let before = crate::backend::read_owned(&path, 1024 * 1024)?;
    let mut expected: Plan = serde_json::from_slice(&before).map_err(|e| e.to_string())?;
    let actual = serde_json::to_value(plan).map_err(|e| e.to_string())?;
    if serde_json::to_value(&expected).map_err(|e| e.to_string())? == actual {
        return Ok(());
    }
    // Allowed differences: the GGUF upgrade and a verified PyTorch update.
    if plan.gguf_ref.is_some() {
        enable_gguf_upgrade(&mut expected);
    }
    expected.torch = plan.torch.clone();
    expected.hardware = plan.hardware.clone();
    if serde_json::to_value(&expected).map_err(|e| e.to_string())? != actual {
        return Err("Owned plan changed during GGUF upgrade; refusing manifest replacement".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    temp.write_all(&serde_json::to_vec_pretty(plan).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if crate::backend::read_owned(&path, 1024 * 1024)? != before {
        return Err("Owned plan changed during GGUF upgrade; refusing manifest replacement".into());
    }
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

// uv normally links the venv executable to managed Python. Materialize a regular
// copy so launch never follows an interpreter symlink or escapes this runtime.
fn own_interpreter(root: &Path, python: &Path) -> Result<(), String> {
    let resolved = dunce::canonicalize(python).map_err(|e| e.to_string())?;
    // Windows: uv places a regular trampoline .exe inside the venv, so compare canonical
    // roots; Linux: a symlink into the managed runtime.
    let inside = |folder: &str| {
        dunce::canonicalize(root.join(folder)).is_ok_and(|base| resolved.starts_with(base))
    };
    if !inside("runtime/python") && !(cfg!(windows) && inside("venv")) {
        return Err("Venv interpreter is not inside the approved managed runtime".into());
    }
    let temporary = python.with_extension("deas-copy");
    let mut source = std::fs::File::open(&resolved).map_err(|e| e.to_string())?;
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    std::io::copy(&mut source, &mut destination).map_err(|e| e.to_string())?;
    std::fs::set_permissions(
        &temporary,
        std::fs::metadata(&resolved)
            .map_err(|e| e.to_string())?
            .permissions(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, python).map_err(|e| e.to_string())?;
    Ok(())
}
// NVIDIA's PyPI sdist is a wheel-downloader stub. Resolve the real wheel
// explicitly, without making that index authoritative for bulk requirements.
fn nvidia_vfx_requirement(requirements: &str) -> Option<&str> {
    requirements
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .find(|line| {
            let name = line
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .next()
                .unwrap_or("");
            name.replace('_', "-").eq_ignore_ascii_case("nvidia-vfx")
        })
}

// Match the reference installer's backend-specific llama wheels. Never silently
// compile a CPU library for a CUDA plan or depend on a host CMake/toolchain.
fn node_wheel_args(requirements: &str, profile: &torch_profile::Torch) -> Vec<String> {
    let needs_llama = requirements.lines().any(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        let name = line
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .next()
            .unwrap_or("");
        name.replace('_', "-")
            .eq_ignore_ascii_case("llama-cpp-python")
    });
    if !needs_llama {
        return Vec::new();
    }
    let family = profile.cuda.as_ref().map_or_else(
        || "cpu".to_owned(),
        |cuda| format!("cu{}", cuda.replace('.', "")),
    );
    vec![
        "--only-binary".into(),
        "llama-cpp-python".into(),
        "--index".into(),
        format!("https://abetlen.github.io/llama-cpp-python/whl/{family}"),
    ]
}

fn prepare_dirs(root: &Path) -> Result<(), String> {
    for folder in ["runtime", "home", "cache", "config", "tmp"] {
        // create_dir_all: takeover targets already carry some of these folders.
        std::fs::create_dir_all(root.join(folder)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
async fn provision(
    plan: &Plan,
    git: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    if let Some(expected) = &plan.hardware {
        let actual = torch_profile::probe()?;
        if actual.uuid != expected.uuid
            || actual.compute_capability != expected.compute_capability
            || actual.driver_version != expected.driver_version
        {
            return Err("NVIDIA_GPU: hardware/driver changed since confirmation; create a new plan (target not touched)".into());
        }
    }
    let takeover = plan.request.takeover;
    let root = if takeover {
        validate_takeover(&plan.request.storage_path, false)?
    } else {
        validate_target(&plan.request.storage_path)?
    };
    cancelled(cancel)?;
    if !takeover {
        // Exclusive claim; never accept a target introduced after planning.
        std::fs::create_dir(&root).map_err(|e| format!("Cannot claim new target: {e}"))?;
    }
    prepare_dirs(&root)?;
    if let Some(folders) = &plan.request.folders {
        crate::settings::write(&root, folders.clone())?;
    }
    std::fs::write(
        root.join("deas-plan.json"),
        serde_json::to_vec_pretty(plan).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    step(inner, "uv");
    let uv = bootstrap(&root, inner, cancel).await?;
    step(inner, "python");
    run(
        &uv,
        &["python", "install", "--managed-python", "--no-bin", "3.12"],
        &root,
        &root,
        inner,
        cancel,
    )
    .await?;
    step(inner, "comfy");
    let comfy = root.join("ComfyUI");
    let revision = checkout(
        git,
        COMFY,
        &plan.request.comfy_ref,
        &comfy,
        &root,
        inner,
        cancel,
    )
    .await?;
    packages(
        plan, git, &root, &uv, &comfy, revision, true, true, inner, cancel,
    )
    .await?;
    log(inner,"Provisioning checks succeeded; backend is not launched and generation readiness is not verified".into());
    Ok(())
}
/// Upgrade or reinstall an existing owned environment in place: ComfyUI and nodepack are
/// fetched again, packages upgraded within the planned Torch profile, then re-verified.
/// Models, inputs, outputs, workflows and settings are left untouched.
async fn update(
    plan: &Plan,
    git: &Path,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
    reinstall: bool,
    scope: UpdateScope,
) -> Result<(), String> {
    let root = dunce::canonicalize(&plan.request.storage_path).map_err(|e| e.to_string())?;
    let uv = root.join(if cfg!(windows) {
        "runtime/uv.exe"
    } else {
        "runtime/uv"
    });
    if !uv.is_file() || !root.join("ComfyUI/.git").is_dir() {
        return Err("Environment is missing its managed uv or ComfyUI checkout".into());
    }
    cancelled(cancel)?;
    let comfy = root.join("ComfyUI");
    let sources = scope == UpdateScope::Comfy;
    // A PyTorch update keeps the current ComfyUI and node-pack checkouts.
    let revision = if sources {
        step(inner, "comfy");
        checkout(
            git,
            COMFY,
            &plan.request.comfy_ref,
            &comfy,
            &root,
            inner,
            cancel,
        )
        .await?
    } else {
        head(git, &comfy, &root, inner, cancel).await?
    };
    packages(
        plan, git, &root, &uv, &comfy, revision, reinstall, sources, inner, cancel,
    )
    .await?;
    persist_updated_plan(plan)?;
    log(
        inner,
        if reinstall {
            "Reinstallation checks succeeded; generation readiness is not verified"
        } else {
            "Upgrade checks succeeded; generation readiness is not verified"
        }
        .into(),
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn packages(
    plan: &Plan,
    git: &Path,
    root: &Path,
    uv: &Path,
    comfy: &Path,
    revision: String,
    fresh_venv: bool,
    // false: install against the existing checkouts without upgrading other packages.
    upgrade: bool,
    inner: &Arc<Mutex<Inner>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    step(inner, "venv");
    let venv = root.join("venv");
    let python = root.join(VENV_PYTHON);
    if fresh_venv {
        if let Ok(meta) = std::fs::symlink_metadata(&venv) {
            if !meta.is_dir() {
                return Err("venv is not a directory; refusing to replace it".into());
            }
            log(
                inner,
                "Removing the existing venv for a clean reinstall".into(),
            );
            std::fs::remove_dir_all(&venv).map_err(|e| format!("Cannot remove old venv: {e}"))?;
        }
        run(
            uv,
            &[
                "venv",
                "--managed-python",
                "--python",
                "3.12",
                venv.to_str().ok_or("Path must be UTF-8")?,
            ],
            root,
            root,
            inner,
            cancel,
        )
        .await?;
        own_interpreter(root, &python)?;
    } else if !python.is_file() {
        return Err("The environment has no venv; use Re-run installation".into());
    }
    let py = python.to_str().ok_or("Python path must be UTF-8")?;
    let constraint = root.join("constraints.txt");
    let selected = plan.torch.as_ref().ok_or("Missing planned Torch profile")?;
    let wheel_family = selected
        .index_url
        .rsplit('/')
        .next()
        .ok_or("Invalid planned index")?;
    let constraints = selected
        .packages
        .iter()
        .map(|s| format!("{s}+{wheel_family}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&constraint, constraints).map_err(|e| e.to_string())?;
    step(inner, "torch");
    run(
        uv,
        &[
            "pip",
            "install",
            "--python",
            py,
            "--index-url",
            &selected.index_url,
            &selected.packages[0],
            &selected.packages[1],
            &selected.packages[2],
        ],
        root,
        root,
        inner,
        cancel,
    )
    .await?;
    step(inner, "requirements");
    let mut args = vec![
        "pip",
        "install",
        "--python",
        py,
        "--index",
        &selected.index_url,
    ];
    if upgrade {
        args.push("--upgrade");
    }
    args.extend(["-r", "requirements.txt"]);
    run(uv, &args, comfy, root, inner, cancel).await?;
    if plan.request.options.ffmpeg {
        step(inner, "ffmpeg");
        run(
            uv,
            &["pip", "install", "--python", py, "imageio-ffmpeg"],
            comfy,
            root,
            inner,
            cancel,
        )
        .await?;
        run(
            python.as_path(),
            &["-c", "import imageio_ffmpeg,os,shutil,sys;b=imageio_ffmpeg.get_ffmpeg_exe();d=os.path.join(sys.prefix,'Scripts' if os.name=='nt' else 'bin');shutil.copy2(b,os.path.join(d,'ffmpeg.exe' if os.name=='nt' else 'ffmpeg'))"],
            root,
            root,
            inner,
            cancel,
        )
        .await?;
    }
    let mut gguf_revision = None;
    // Legacy immutable plans keep their original dependency scope.
    if let Some(reference) = plan.gguf_ref.as_deref() {
        if reference.len() != 40 || !reference.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid immutable ComfyUI-GGUF revision".into());
        }
        step(inner, "gguf");
        let node = gguf_target(comfy)?;
        if node.join(".git").exists() {
            crate::backend::read_owned(&node.join(".git/config"), 1024 * 1024)?;
        }
        std::fs::create_dir_all(comfy.join("custom_nodes")).map_err(|e| e.to_string())?;
        let fetched = if upgrade {
            checkout(git, GGUF, reference, &node, root, inner, cancel).await?
        } else {
            head(git, &node, root, inner, cancel).await?
        };
        if fetched != reference {
            return Err("ComfyUI-GGUF revision does not match the pinned source".into());
        }
        validate_gguf_sources(&node, reference)?;
        run(
            uv,
            &[
                "pip",
                "install",
                "--python",
                py,
                "--index",
                &selected.index_url,
                "-r",
                "requirements.txt",
            ],
            &node,
            root,
            inner,
            cancel,
        )
        .await
        .map_err(|e| format!("Required ComfyUI-GGUF dependencies failed: {e}"))?;
        gguf_revision = Some(fetched);
    }
    let mut node_revision = None;
    if plan
        .request
        .features
        .iter()
        .any(|f| ["director", "forge", "refmods", "continuity"].contains(&f.as_str()))
    {
        step(inner, "nodes");
        let nodes = comfy.join("custom_nodes");
        if !nodes.exists() {
            std::fs::create_dir(&nodes).map_err(|e| e.to_string())?;
        }
        let node = nodes.join("ComfyUI-DaSiWa-Nodes");
        node_revision = Some(if upgrade {
            checkout(git, NODEPACK, "main", &node, root, inner, cancel).await?
        } else {
            head(git, &node, root, inner, cancel).await?
        });
        if node.join("requirements.txt").is_file() {
            let requirements = std::fs::read_to_string(node.join("requirements.txt"))
                .map_err(|e| format!("Cannot read required node dependencies: {e}"))?;
            if let Some(requirement) = nvidia_vfx_requirement(&requirements) {
                run(
                    uv,
                    &[
                        "pip",
                        "install",
                        "--python",
                        py,
                        "--index",
                        "https://pypi.nvidia.com/",
                        "--only-binary",
                        "nvidia-vfx",
                        requirement,
                    ],
                    &node,
                    root,
                    inner,
                    cancel,
                )
                .await
                .map_err(|e| format!("Required NVIDIA VFX wheel installation failed: {e}"))?;
            }
            let wheel_args = node_wheel_args(&requirements, selected);
            let mut args = vec![
                "pip",
                "install",
                "--python",
                py,
                "--index",
                &selected.index_url,
            ];
            args.extend(wheel_args.iter().map(String::as_str));
            if upgrade {
                args.push("--upgrade");
            }
            args.extend(["-r", "requirements.txt"]);
            run(uv, &args, &node, root, inner, cancel)
                .await
                .map_err(|e| format!("Required node dependencies failed (llama-cpp-python requires a matching backend wheel; no source/CPU fallback): {e}"))?;
        }
    }
    step(inner, "verify");
    let profile_json = serde_json::to_string(selected).map_err(|e| e.to_string())?;
    let hardware_json = serde_json::to_string(&plan.hardware).map_err(|e| e.to_string())?;
    let versions = run(
        &python,
        &[
            "-I",
            "-c",
            FRAMEWORK_PROBE,
            root.join("runtime/python")
                .to_str()
                .ok_or("Managed Python path must be UTF-8")?,
            &profile_json,
            &hardware_json,
        ],
        comfy,
        root,
        inner,
        cancel,
    )
    .await?;
    run_with_timeout(
        &python,
        &[
            "-c",
            COMFY_PROBE,
            if selected.cuda.is_some() {
                "nvidia"
            } else {
                "cpu"
            },
            if node_revision.is_some() {
                "nodes"
            } else {
                "core"
            },
            if gguf_revision.is_some() {
                "gguf"
            } else {
                "legacy"
            },
        ],
        comfy,
        root,
        inner,
        cancel,
        Duration::from_secs(120),
    )
    .await?;
    cancelled(cancel)?;
    let manifest = serde_json::json!({"planId":plan.id,"comfyRevision":revision,"nodepackRevision":node_revision,"ggufRevision":gguf_revision,"backend":plan.request.backend,"hardware":plan.hardware,"torchProfile":selected,"python":py,"versions":versions.trim(),"generationReady":false});
    std::fs::write(
        root.join("deas-install.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("install_gguf_tests.rs");
    include!("install_update_tests.rs");

    #[test]
    fn git_lookup_skips_unusable_candidates_and_preserves_native_paths() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let usable = root.path().join("日本語 git tools");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&usable).unwrap();
        let name = if cfg!(windows) { "git.exe" } else { "git" };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(first.join(name), "not executable").unwrap();
            std::fs::set_permissions(first.join(name), std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        #[cfg(windows)]
        std::fs::create_dir(first.join(name)).unwrap();
        let git = usable.join(name);
        std::fs::write(&git, "fixture executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let paths = std::env::join_paths([first, usable]).unwrap();
        assert_eq!(git_in_path(&paths), Some(dunce::canonicalize(git).unwrap()));
    }

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn request(root: &Path) -> Request {
        Request {
            storage_path: root.join("new").to_str().unwrap().into(),
            backend: Backend::Cpu,
            features: vec!["installer".into()],
            options: Options::default(),
            comfy_ref: "master".into(),
            folders: None,
            takeover: false,
        }
    }
    fn assert_baseline_platform_support(plan: &Plan) {
        #[cfg(all(any(target_os = "linux", windows), target_arch = "x86_64"))]
        assert!(plan.can_execute, "{:?}", plan.unsupported_options);

        #[cfg(not(all(any(target_os = "linux", windows), target_arch = "x86_64")))]
        {
            assert!(!plan.can_execute);
            assert!(
                plan.unsupported_options
                    .iter()
                    .any(|option| option.starts_with("platform:"))
            );
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn managed_interpreter_is_copied_not_external_or_symlinked() {
        let tmp = scratch();
        std::fs::create_dir_all(tmp.path().join("runtime/python")).unwrap();
        std::fs::create_dir_all(tmp.path().join("venv/bin")).unwrap();
        let managed = tmp.path().join("runtime/python/python");
        std::fs::write(&managed, "owned interpreter").unwrap();
        let py = tmp.path().join("venv/bin/python");
        std::os::unix::fs::symlink(&managed, &py).unwrap();
        own_interpreter(tmp.path(), &py).unwrap();
        assert!(
            !std::fs::symlink_metadata(&py)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&py).unwrap(), "owned interpreter");
        std::fs::remove_file(&py).unwrap();
        std::os::unix::fs::symlink("/usr/bin/python3", &py).unwrap();
        assert!(own_interpreter(tmp.path(), &py).is_err());
    }
    #[test]
    fn optional_folders_are_validated_and_held_in_the_plan_without_writes() {
        let tmp = scratch();
        let models = scratch();
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.folders = Some(crate::settings::Folders {
            models: Some(models.path().to_str().unwrap().into()),
            ..Default::default()
        });
        let plan = engine.plan(r.clone()).unwrap();
        r.folders.as_mut().unwrap().models = Some("relative".into());
        assert!(engine.plan(r).is_err());
        assert_eq!(
            plan.request.folders.as_ref().unwrap().models.as_deref(),
            models.path().to_str()
        );
        assert!(!tmp.path().join("new").exists());
        assert!(!models.path().join(crate::settings::FILE).exists());
        assert!(engine.folders_idle());
        lock(&engine.inner).status.state = "running".into();
        assert!(!engine.folders_idle());
    }
    #[test]
    fn interrupted_install_folder_says_so_for_reopen_and_takeover() {
        // provision() writes deas-plan.json before any download and deas-install.json last.
        let tmp = scratch();
        std::fs::create_dir(tmp.path().join("ComfyUI")).unwrap();
        std::fs::write(tmp.path().join("ComfyUI/main.py"), "fixture").unwrap();
        std::fs::write(tmp.path().join("deas-plan.json"), "{}").unwrap();
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.storage_path = tmp.path().to_str().unwrap().into();
        assert_eq!(engine.plan(r.clone()).unwrap_err(), UNFINISHED_INSTALL);
        r.takeover = true;
        assert_eq!(engine.plan(r).unwrap_err(), UNFINISHED_INSTALL);
        assert!(tmp.path().join("deas-plan.json").exists());
    }

    #[test]
    fn uv_download_stall_is_reported_as_a_stall() {
        // A loopback server that sends headers and part of the body, then goes silent.
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\npartial");
            std::thread::sleep(Duration::from_secs(3));
        });
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime.block_on(async {
            let client = reqwest::Client::builder()
                .read_timeout(Duration::from_millis(300))
                .build()
                .unwrap();
            let mut response = client
                .get(format!("http://{address}/"))
                .send()
                .await
                .unwrap();
            loop {
                match response.chunk().await {
                    Ok(Some(_)) => continue,
                    Ok(None) => panic!("stalled body must not complete"),
                    Err(error) => break uv_download_error(error),
                }
            }
        });
        assert!(error.starts_with("uv download stalled"), "{error}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn existing_environment_requires_explicit_open_without_reinstall_or_autostart() {
        let (tmp, _) = crate::backend::tests::fixture(crate::backend::tests::SERVER);
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.storage_path = tmp.path().to_str().unwrap().into();
        let before = std::fs::read(tmp.path().join("deas-install.json")).unwrap();
        let plan = engine.plan(r).unwrap();
        assert!(plan.reopen_id.is_some());
        assert!(lock(&engine.inner).approved.is_none());
        assert!(engine.start(&plan.id, true).is_err());
        assert!(engine.open_existing(&plan.id, false).is_err());
        assert_eq!(
            engine.open_existing(&plan.id, true).unwrap().state,
            "succeeded"
        );
        assert!(lock(&engine.inner).approved.is_some());
        assert!(!engine.backend.active());
        assert_eq!(
            before,
            std::fs::read(tmp.path().join("deas-install.json")).unwrap()
        );
        assert!(engine.open_existing(&plan.id, true).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn changed_existing_environment_cannot_be_opened() {
        let (tmp, _) = crate::backend::tests::fixture(crate::backend::tests::SERVER);
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.storage_path = tmp.path().to_str().unwrap().into();
        let plan = engine.plan(r).unwrap();
        std::fs::write(tmp.path().join("ComfyUI/main.py"), "changed after preview").unwrap();
        assert!(engine.open_existing(&plan.id, true).is_err());
        assert!(lock(&engine.inner).approved.is_none());
        assert!(!engine.backend.active());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn reopened_environment_can_explicitly_start_only_its_owned_fixture_backend() {
        let (tmp, _) = crate::backend::tests::fixture(crate::backend::tests::SERVER);
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.storage_path = tmp.path().to_str().unwrap().into();
        let plan = engine.plan(r).unwrap();
        engine.open_existing(&plan.id, true).unwrap();
        assert!(!engine.backend.active());
        engine.backend_start().unwrap();
        let status = crate::backend::tests::await_state(&engine.backend, "running");
        engine.shutdown_backend();
        assert!(!status.generation_ready);
        assert!(status.endpoint.unwrap().starts_with("http://127.0.0.1:"));
    }
    #[cfg(windows)]
    #[test]
    fn existing_environment_reopens_non_verbatim_windows_path_without_changing_manifests() {
        let tmp = scratch();
        let canonical = std::fs::canonicalize(tmp.path()).unwrap();
        let root = Path::new(
            canonical
                .to_str()
                .unwrap()
                .strip_prefix(r"\\?\")
                .expect("Windows canonical path must be verbatim"),
        );
        assert!(matches!(
            root.components().next(),
            Some(std::path::Component::Prefix(prefix))
                if matches!(prefix.kind(), std::path::Prefix::Disk(_))
        ));
        assert_ne!(root, canonical);
        for name in [
            "runtime/python",
            "ComfyUI",
            VENV_BIN,
            "home",
            "cache",
            "config",
            "tmp",
        ] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        for name in ["ComfyUI/main.py", VENV_PYTHON, "venv/pyvenv.cfg"] {
            std::fs::write(root.join(name), "owned fixture; never executed").unwrap();
        }
        let mut r = request(root);
        r.storage_path = root.to_str().unwrap().into();
        let persisted = Plan {
            gguf_ref: None,
            reopen_id: None,
            id: uuid::Uuid::new_v4().to_string(),
            request: r.clone(),
            steps: vec![],
            warnings: vec![],
            unsupported_options: vec![],
            can_execute: true,
            hardware: None,
            torch: Some(torch_profile::cpu()),
        };
        let plan_bytes = serde_json::to_vec(&persisted).unwrap();
        let install_bytes = serde_json::to_vec(&serde_json::json!({
            "planId": persisted.id, "backend": "cpu", "hardware": null,
            "torchProfile": persisted.torch, "python": root.join(VENV_PYTHON),
            "generationReady": false
        }))
        .unwrap();
        std::fs::write(root.join("deas-plan.json"), &plan_bytes).unwrap();
        std::fs::write(root.join("deas-install.json"), &install_bytes).unwrap();
        let engine = Engine::default();
        let plan = engine.plan(r.clone()).unwrap();
        assert_eq!(plan.request.storage_path, persisted.request.storage_path);
        assert!(plan.reopen_id.is_some());
        assert!(lock(&engine.inner).approved.is_none());
        assert!(engine.open_existing(&plan.id, false).is_err());
        assert_eq!(
            engine.open_existing(&plan.id, true).unwrap().state,
            "succeeded"
        );
        assert!(lock(&engine.inner).approved.is_some());
        assert!(!engine.backend.active());
        assert_eq!(
            std::fs::read(root.join("deas-plan.json")).unwrap(),
            plan_bytes
        );
        assert_eq!(
            std::fs::read(root.join("deas-install.json")).unwrap(),
            install_bytes
        );

        let preview = engine.plan(r).unwrap();
        let mut changed = plan_bytes;
        changed.push(b' ');
        std::fs::write(root.join("deas-plan.json"), changed).unwrap();
        assert!(engine.open_existing(&preview.id, true).is_err());
        assert!(!engine.backend.active());
    }
    #[test]
    #[ignore = "explicit read-only inspection/open of DEAS_EXISTING_ROOT; never starts a backend"]
    fn inspect_existing_environment_from_env() {
        let root = std::env::var("DEAS_EXISTING_ROOT").expect("Set DEAS_EXISTING_ROOT explicitly");
        let engine = Engine::default();
        let mut r = request(Path::new(&root));
        r.storage_path = root;
        let plan = engine.plan(r).unwrap();
        assert!(plan.reopen_id.is_some());
        let status = engine.open_existing(&plan.id, true).unwrap();
        assert!(!engine.backend.active());
        println!(
            "Existing environment checked/opened without writes or backend start: root={}, backend={:?}, features={:?}, state={}",
            plan.request.storage_path, plan.request.backend, plan.request.features, status.state
        );
    }
    #[test]
    fn backend_requires_current_successful_provision() {
        let engine = Engine::default();
        assert!(engine.backend_start().unwrap_err().contains("successful"));
    }
    #[test]
    fn prepared_plan_or_success_label_does_not_authorize_backend() {
        let tmp = scratch();
        let engine = Engine::default();
        let plan = engine.plan(request(tmp.path())).unwrap();
        assert_baseline_platform_support(&plan);
        assert!(engine.backend_start().is_err());
        lock(&engine.inner).status.state = "succeeded".into();
        assert!(engine.backend_start().is_err());
        lock(&engine.inner).status.state = "failed".into();
        assert!(engine.backend_start().is_err());
        assert!(!tmp.path().join("new").exists());
    }
    #[test]
    fn safe_read_only_plans_and_confirmation() {
        let tmp = scratch();
        let engine = Engine::default();
        let r = request(tmp.path());
        let plan = engine.plan(r.clone()).unwrap();
        assert!(!Path::new(&r.storage_path).exists());
        assert!(engine.start(&plan.id, false).is_err());
        let mut changed = r;
        changed.options.sage = true;
        let blocked = engine.plan(changed).unwrap();
        assert!(!blocked.can_execute);
        assert!(engine.start(&blocked.id, true).is_err());
        assert!(
            !lock(&engine.inner)
                .plans
                .get(&plan.id)
                .unwrap()
                .request
                .options
                .sage
        );
        std::fs::create_dir(tmp.path().join("new")).unwrap();
        assert!(engine.start(&plan.id, true).is_err());
    }
    #[test]
    fn rejects_unsafe_paths_refs_and_binary() {
        let tmp = scratch();
        assert!(validate_target("/").is_err());
        assert!(validate_target(&format!("{}/../escape", tmp.path().display())).is_err());
        assert!(validate_ref("--upload-pack=evil").is_err());
        assert!(
            extract_verified_uv(b"unverified", tmp.path())
                .unwrap_err()
                .contains("checksum")
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path(), tmp.path().join("link")).unwrap();
            assert!(validate_target(tmp.path().join("link/new").to_str().unwrap()).is_err());
        }
    }
    #[test]
    fn zip_member_reads_stored_and_deflated_entries_only() {
        use std::io::Write;
        fn zip(members: &[(&str, &[u8], u16)]) -> Vec<u8> {
            let (mut out, mut central) = (vec![], vec![]);
            for (name, raw, method) in members {
                let data = if *method == 8 {
                    let mut e = flate2::write::DeflateEncoder::new(vec![], Default::default());
                    e.write_all(raw).unwrap();
                    e.finish().unwrap()
                } else {
                    raw.to_vec()
                };
                let local = out.len() as u32;
                let mut header = vec![];
                header.extend_from_slice(&method.to_le_bytes());
                header.extend_from_slice(&[0; 8]);
                header.extend_from_slice(&(data.len() as u32).to_le_bytes());
                header.extend_from_slice(&(raw.len() as u32).to_le_bytes());
                header.extend_from_slice(&(name.len() as u16).to_le_bytes());
                out.extend_from_slice(&[0x50, 0x4b, 3, 4, 20, 0, 0, 0]);
                out.extend_from_slice(&header);
                out.extend_from_slice(&[0, 0]);
                out.extend_from_slice(name.as_bytes());
                out.extend_from_slice(&data);
                central.extend_from_slice(&[0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0]);
                central.extend_from_slice(&header);
                central.extend_from_slice(&[0; 8]);
                central.extend_from_slice(&[0; 4]);
                central.extend_from_slice(&local.to_le_bytes());
                central.extend_from_slice(name.as_bytes());
            }
            let (start, size) = (out.len() as u32, central.len() as u32);
            out.extend(central);
            out.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
            out.extend_from_slice(&(members.len() as u16).to_le_bytes());
            out.extend_from_slice(&(members.len() as u16).to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&[0, 0]);
            out
        }
        let big = vec![7u8; 100_000];
        let archive = zip(&[
            ("uvw.exe", b"other", 0),
            ("uv.exe", &big, 8),
            ("x", b"s", 0),
        ]);
        assert_eq!(zip_member(&archive, "uv.exe").unwrap(), big);
        assert_eq!(zip_member(&archive, "x").unwrap(), b"s");
        assert!(zip_member(&archive, "missing").is_err());
        assert!(zip_member(&zip(&[("uv.exe", b"lzma", 14)]), "uv.exe").is_err());
        assert!(zip_member(b"not a zip", "uv.exe").is_err());
    }
    #[tokio::test]
    async fn real_git_runner_and_checkout_in_scratch() {
        let tmp = scratch();
        prepare_dirs(tmp.path()).unwrap();
        let source = tmp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let git = git_path().expect("Git prerequisite");
        let inner = Arc::new(Mutex::new(Inner::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        run(&git, &["init", "."], &source, tmp.path(), &inner, &cancel)
            .await
            .unwrap();
        std::fs::write(source.join("fixture.txt"), "real fixture").unwrap();
        run(
            &git,
            &["add", "fixture.txt"],
            &source,
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        run(
            &git,
            &[
                "-c",
                "user.name=DEAS test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
            &source,
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        let rev = checkout(
            &git,
            source.to_str().unwrap(),
            "HEAD",
            &tmp.path().join("checkout"),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(rev.len(), 40);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("checkout/fixture.txt")).unwrap(),
            "real fixture"
        );
        // Upgrade: same target again picks up the new commit, overwrites tracked edits, keeps untracked data.
        std::fs::write(source.join("fixture.txt"), "updated").unwrap();
        std::fs::write(tmp.path().join("checkout/fixture.txt"), "local edit").unwrap();
        std::fs::write(tmp.path().join("checkout/model.safetensors"), "user data").unwrap();
        run(
            &git,
            &[
                "-c",
                "user.name=DEAS test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-am",
                "update",
            ],
            &source,
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        let updated = checkout(
            &git,
            source.to_str().unwrap(),
            "HEAD",
            &tmp.path().join("checkout"),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        assert_ne!(updated, rev);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("checkout/fixture.txt")).unwrap(),
            "updated"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("checkout/model.safetensors")).unwrap(),
            "user data"
        );
        cancel.store(true, Ordering::SeqCst);
        assert!(
            run(&git, &["status"], &source, tmp.path(), &inner, &cancel)
                .await
                .unwrap_err()
                .contains("Cancelled")
        );
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn real_runner_enforces_short_timeout_without_deleting_partial_install() {
        let tmp = scratch();
        prepare_dirs(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("retained.txt"), "keep").unwrap();
        let inner = Arc::new(Mutex::new(Inner::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let start = std::time::Instant::now();
        let error = run_with_timeout(
            Path::new("/usr/bin/sleep"),
            &["30"],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Command timed out after 1 seconds");
        assert!(start.elapsed() < Duration::from_secs(3));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("retained.txt")).unwrap(),
            "keep"
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn real_runner_cancels_and_reaps_owned_process() {
        let tmp = scratch();
        prepare_dirs(tmp.path()).unwrap();
        let inner = Arc::new(Mutex::new(Inner::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            signal.store(true, Ordering::SeqCst);
        });
        let start = std::time::Instant::now();
        let result = run(
            Path::new("/usr/bin/sleep"),
            &["30"],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await;
        assert!(result.unwrap_err().contains("Cancelled"));
        assert!(start.elapsed() < Duration::from_secs(3));
        for _ in 0..400 {
            log(&inner, "bounded".repeat(1000));
        }
        assert_eq!(lock(&inner).status.logs.len(), MAX_LOGS);
        assert!(
            lock(&inner)
                .status
                .logs
                .iter()
                .all(|line| line.len() <= 4096)
        );
    }

    #[test]
    fn every_unsupported_selection_blocks_execution() {
        let tmp = scratch();
        for backend in [Backend::Amd, Backend::Intel] {
            let engine = Engine::default();
            let mut request = request(tmp.path());
            request.backend = backend;
            let plan = engine.plan(request).unwrap();
            assert!(!plan.can_execute);
            assert!(engine.start(&plan.id, true).is_err());
        }
        for option in ["sage", "radial", "flash", "unknown-feature"] {
            let engine = Engine::default();
            let mut request = request(tmp.path());
            match option {
                "sage" => request.options.sage = true,
                "radial" => request.options.radial = true,
                "flash" => request.options.flash = true,
                _ => request.features.push(option.into()),
            }
            assert!(!engine.plan(request).unwrap().can_execute);
        }
        assert!(!tmp.path().join("new").exists());
    }

    #[test]
    fn ffmpeg_opt_in_is_supported_and_adds_an_install_step() {
        let tmp = scratch();
        let engine = Engine::default();
        let mut request = request(tmp.path());
        request.options.ffmpeg = true;
        let plan = engine.plan(request).unwrap();
        assert!(plan.can_execute, "{:?}", plan.unsupported_options);
        assert!(plan.steps.iter().any(|s| s.id == "ffmpeg"));
        assert!(!tmp.path().join("new").exists());
    }

    #[test]
    fn managed_constraints_use_encoded_uri_and_no_inherited_configuration() {
        let tmp = scratch();
        let root = tmp.path().join("path with spaces");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("constraints.txt"), "torch==2.14.1+cu130\n").unwrap();
        let mut command = Command::new("python");
        isolated(&mut command, &root);
        let env: HashMap<_, _> = command
            .as_std()
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|v| {
                    (
                        key.to_string_lossy().into_owned(),
                        v.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect();
        assert!(env["UV_CONSTRAINT"].starts_with("file://"));
        assert!(env["UV_CONSTRAINT"].contains("path%20with%20spaces"));
        assert_eq!(env["HOME"], root.join("home").to_string_lossy());
        assert!(!env.contains_key("PYTHONPATH"));
        assert!(!env.contains_key("CUDA_VISIBLE_DEVICES"));
        assert!(!env.contains_key("PIP_INDEX_URL"));
    }

    #[test]
    fn takeover_plan_warns_about_the_foreign_rebuild() {
        let tmp = scratch();
        std::fs::create_dir_all(tmp.path().join("ComfyUI")).unwrap();
        std::fs::write(tmp.path().join("ComfyUI/main.py"), "").unwrap();
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.storage_path = tmp.path().to_str().unwrap().into();
        r.takeover = true;
        let plan = engine.plan(r).unwrap();
        assert!(plan.request.takeover);
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("not DEAS-managed") && w.contains("never deleted"))
        );
        assert!(
            plan.steps
                .iter()
                .any(|s| s.id == "venv" && s.detail.contains("Delete the existing"))
        );
    }

    #[test]
    fn takeover_requires_main_py_and_rejects_managed_folders() {
        let tmp = scratch();
        let engine = Engine::default();
        // Legacy manifests predate `takeover`: plain plans must serialize without the key so reopening stays byte-compatible.
        assert!(
            serde_json::to_value(request(tmp.path()))
                .unwrap()
                .get("takeover")
                .is_none()
        );
        let mut r = request(tmp.path());
        r.storage_path = tmp.path().to_str().unwrap().into();
        r.takeover = true;
        assert!(
            engine
                .plan(r.clone())
                .unwrap_err()
                .contains("ComfyUI folder")
        );
        std::fs::create_dir_all(tmp.path().join("ComfyUI")).unwrap();
        std::fs::write(tmp.path().join("ComfyUI/main.py"), "").unwrap();
        assert!(engine.plan(r.clone()).is_ok());
        std::fs::write(tmp.path().join("deas-plan.json"), "{}").unwrap();
        // Plan without install manifest = interrupted install (see interrupted_install_folder_…).
        std::fs::write(tmp.path().join("deas-install.json"), "{}").unwrap();
        assert!(engine.plan(r).unwrap_err().contains("already DEAS-managed"));
    }

    #[test]
    fn director_plan_includes_companions_without_writes() {
        let tmp = scratch();
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.features = vec!["director".into()];
        let plan = engine.plan(r).unwrap();
        assert_eq!(
            plan.request.features,
            ["director", "forge", "refmods", "continuity"]
        );
        assert_eq!(plan.steps.iter().filter(|s| s.id == "nodes").count(), 1);
        assert!(!Path::new(&plan.request.storage_path).exists());
        assert_eq!(
            lock(&engine.inner).plans[&plan.id].request.features,
            plan.request.features
        );
    }

    #[test]
    fn all_selected_features_share_one_main_nodepack_and_keep_request() {
        let tmp = scratch();
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.features = vec![
            "director".into(),
            "forge".into(),
            "refmods".into(),
            "continuity".into(),
        ];
        let original = serde_json::to_value(&r).unwrap();
        let plan = engine.plan(r).unwrap();
        assert_baseline_platform_support(&plan);
        assert_eq!(serde_json::to_value(&plan.request).unwrap(), original);
        assert_eq!(plan.steps.iter().filter(|s| s.id == "nodes").count(), 1);
        assert!(
            plan.steps
                .iter()
                .find(|s| s.id == "nodes")
                .unwrap()
                .detail
                .contains("main")
        );
        assert!(!Path::new(&plan.request.storage_path).exists());
        assert_eq!(
            plan.torch.unwrap().index_url,
            "https://download.pytorch.org/whl/cpu"
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    #[ignore = "Read-only live NVIDIA immutable plan; no installation"]
    fn live_nvidia_plan() {
        let tmp = scratch();
        let engine = Engine::default();
        let mut r = request(tmp.path());
        r.backend = Backend::Nvidia;
        r.features = vec![
            "director".into(),
            "forge".into(),
            "refmods".into(),
            "continuity".into(),
        ];
        let original = serde_json::to_value(&r).unwrap();
        let plan = engine.plan(r).unwrap();
        assert!(plan.can_execute, "{:?}", plan.unsupported_options);
        assert_eq!(serde_json::to_value(&plan.request).unwrap(), original);
        assert_eq!(plan.steps.iter().filter(|s| s.id == "nodes").count(), 1);
        assert!(plan.hardware.is_some());
        println!("{}", serde_json::to_string(&plan).unwrap());
        assert!(!Path::new(&plan.request.storage_path).exists());
        assert!(engine.start(&plan.id, false).is_err());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn comfy_probe_runs_without_unix_signal_alarm() {
        let tmp = scratch();
        prepare_dirs(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("main.py"), "def start_comfyui(): pass\n").unwrap();
        std::fs::write(tmp.path().join("nodes.py"), "NODE_CLASS_MAPPINGS = {}\n").unwrap();
        let inner = Arc::new(Mutex::new(Inner::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let probe = format!("import signal; del signal.alarm\n{COMFY_PROBE}");
        for backend in ["cpu", "nvidia"] {
            let result = run(
                Path::new("/usr/bin/python3"),
                &["-c", &probe, backend, "core"],
                tmp.path(),
                tmp.path(),
                &inner,
                &cancel,
            )
            .await
            .unwrap();
            let result: serde_json::Value = serde_json::from_str(result.trim()).unwrap();
            assert_eq!(result["coreImported"], true);
            assert_eq!(result["generationReady"], false);
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn comfy_probe_rejects_missing_selected_nodes_after_real_import() {
        let tmp = scratch();
        prepare_dirs(tmp.path()).unwrap();
        std::fs::write(
            tmp.path().join("main.py"),
            "import nodes\ndef start_comfyui(): pass\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("nodes.py"), "NODE_CLASS_MAPPINGS = {}\n").unwrap();
        let inner = Arc::new(Mutex::new(Inner::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = run(
            Path::new("/usr/bin/python3"),
            &["-c", COMFY_PROBE, "cpu", "nodes"],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await;
        assert!(
            result.is_err(),
            "Selected nodepack must not silently fail import"
        );
        assert!(result.unwrap_err().contains("Missing selected nodes"));
    }

    #[test]
    fn nvidia_vfx_uses_explicit_wheels_only_for_exact_requirement() {
        assert_eq!(
            nvidia_vfx_requirement("torch\n nvidia-vfx>=0.2 # stub\n"),
            Some("nvidia-vfx>=0.2")
        );
        assert_eq!(
            nvidia_vfx_requirement("# nvidia-vfx\nnvidia-vfx-extra\n"),
            None
        );
    }

    #[test]
    fn node_llama_wheels_match_approved_backend_without_source_fallback() {
        let cpu = node_wheel_args("torch\nllama-cpp-python>=0.3.26\n", &torch_profile::cpu());
        assert_eq!(
            cpu,
            vec![
                "--only-binary",
                "llama-cpp-python",
                "--index",
                "https://abetlen.github.io/llama-cpp-python/whl/cpu"
            ]
        );
        let mut cuda = torch_profile::cpu();
        cuda.cuda = Some("13.0".into());
        let gpu = node_wheel_args(" LLAMA_CPP_PYTHON>=0.3.26 # wheel\n", &cuda);
        assert_eq!(
            gpu[3],
            "https://abetlen.github.io/llama-cpp-python/whl/cu130"
        );
        assert!(node_wheel_args("# llama-cpp-python\nllama-cpp-python-extra\n", &cuda).is_empty());
    }

    /// Full upstream baseline acceptance; only an owned temporary CPU environment.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[tokio::test]
    #[ignore = "Downloads real ComfyUI, managed Python and CPU Torch; run explicitly"]
    async fn real_comfy_cpu_install_and_owned_lifecycle() {
        real_comfy_install_and_owned_lifecycle(Backend::Cpu, false).await;
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[tokio::test]
    #[ignore = "Downloads real CUDA Torch and ComfyUI; exercises planned NVIDIA GPU"]
    async fn real_comfy_nvidia_install_and_owned_lifecycle() {
        real_comfy_install_and_owned_lifecycle(Backend::Nvidia, false).await;
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[tokio::test]
    #[ignore = "Downloads CUDA Torch, ComfyUI and complete selected Director nodepack"]
    async fn real_comfy_director_install_and_owned_lifecycle() {
        real_comfy_install_and_owned_lifecycle(Backend::Nvidia, true).await;
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    async fn real_comfy_install_and_owned_lifecycle(backend: Backend, director: bool) {
        let tmp = scratch();
        let engine = Engine::default();
        let mut request = request(tmp.path());
        request.backend = backend;
        if director {
            request.features.push("director".into());
        }
        let plan = engine.plan(request).unwrap();
        assert!(plan.can_execute, "{:?}", plan.unsupported_options);
        assert!(engine.start(&plan.id, false).is_err());
        engine.start(&plan.id, true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1800);
        loop {
            let status = lock(&engine.inner).status.clone();
            if status.state != "running" {
                assert_eq!(status.state, "succeeded", "{status:?}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                lock(&engine.inner).cancel.store(true, Ordering::SeqCst);
                panic!("Full installation exceeded acceptance deadline: {status:?}");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let root = Path::new(&plan.request.storage_path);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("deas-install.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["generationReady"], false);
        assert_eq!(manifest["planId"], plan.id);
        assert_eq!(engine.backend.status().state, "stopped");
        engine.backend_start().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(150);
        loop {
            let status = engine.backend.status();
            if status.state == "running" {
                assert!(status.endpoint.is_some());
                assert!(!status.generation_ready);
                println!(
                    "Real {:?} ComfyUI installed and API ready: {manifest}; {status:?}",
                    plan.request.backend
                );
                break;
            }
            if status.state == "failed" || std::time::Instant::now() >= deadline {
                engine.shutdown_backend();
                panic!("Real ComfyUI failed readiness: {status:?}");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        engine.shutdown_backend();
        assert_eq!(engine.backend.status().state, "stopped");
    }

    /// Explicit network smoke only: no Torch, models or user target touched.
    #[tokio::test]
    #[ignore = "Downloads checksum-verified uv and isolated Python; run explicitly"]
    async fn real_uv_isolated_minimal_package() {
        let tmp = scratch();
        prepare_dirs(tmp.path()).unwrap();
        let inner = Arc::new(Mutex::new(Inner::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let uv = bootstrap(tmp.path(), &inner, &cancel).await.unwrap();
        run(
            &uv,
            &["python", "install", "--managed-python", "--no-bin", "3.12"],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        let venv = tmp.path().join("venv");
        run(
            &uv,
            &[
                "venv",
                "--managed-python",
                "--python",
                "3.12",
                venv.to_str().unwrap(),
            ],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        let python = venv.join("bin/python");
        own_interpreter(tmp.path(), &python).unwrap();
        assert!(
            !std::fs::symlink_metadata(&python)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        run(
            &uv,
            &[
                "pip",
                "install",
                "--python",
                python.to_str().unwrap(),
                "packaging==25.0",
            ],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await
        .unwrap();
        let output=run(&python,&["-I","-c","import packaging,sys,os;assert packaging.__version__=='25.0';assert sys.prefix!=sys.base_prefix;assert sys.version_info[:2]==(3,12);assert os.path.commonpath([sys.base_prefix,sys.argv[1]])==sys.argv[1];print(sys.prefix)",tmp.path().join("runtime/python").to_str().unwrap()],tmp.path(),tmp.path(),&inner,&cancel).await.unwrap();
        assert!(output.contains(venv.to_str().unwrap()));
        assert!(tmp.path().join("runtime/python").is_dir());
        // Real managed uv/Python + local API fixture, not a full Comfy/Torch install.
        #[cfg(target_os = "linux")]
        {
            std::fs::create_dir(tmp.path().join("ComfyUI")).unwrap();
            std::fs::write(
                tmp.path().join("ComfyUI/main.py"),
                crate::backend::tests::SERVER,
            )
            .unwrap();
            let mut r = request(tmp.path());
            r.storage_path = tmp.path().to_str().unwrap().into();
            let plan = Plan {
                gguf_ref: None,
                reopen_id: None,
                id: uuid::Uuid::new_v4().to_string(),
                request: r,
                steps: vec![],
                warnings: vec![],
                unsupported_options: vec![],
                can_execute: true,
                hardware: None,
                torch: Some(torch_profile::cpu()),
            };
            std::fs::write(
                tmp.path().join("deas-plan.json"),
                serde_json::to_vec(&plan).unwrap(),
            )
            .unwrap();
            std::fs::write(tmp.path().join("deas-install.json"), serde_json::to_vec(&serde_json::json!({"planId":plan.id,"backend":"cpu","hardware":null,"torchProfile":plan.torch,"python":python,"generationReady":false})).unwrap()).unwrap();
            let engine = Engine::default();
            lock(&engine.inner).approved = Some(crate::backend::Approved::capture(&plan).unwrap());
            assert_eq!(engine.backend_start().unwrap().state, "starting");
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                let status = engine.backend.status();
                if status.state == "running" {
                    assert!(!status.generation_ready);
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "{status:?}");
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            engine.shutdown_backend();
            assert_eq!(engine.backend.status().state, "stopped");
        }
    }
}
