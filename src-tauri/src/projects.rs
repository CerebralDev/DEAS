//! Persistent image settings and managed PNG input; no execution or job ownership.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageDraft {
    pub positive_prompt: String,
    pub negative_prompt: String,
    pub checkpoint: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    pub sampler: String,
    pub scheduler: String,
    pub seed: u64,
    pub denoise: f64,
    pub pipeline: String,
}
impl Default for ImageDraft {
    fn default() -> Self {
        Self {
            positive_prompt: String::new(),
            negative_prompt: String::new(),
            checkpoint: String::new(),
            width: 1024,
            height: 1024,
            steps: 20,
            cfg: 5.0,
            sampler: "euler".into(),
            scheduler: "normal".into(),
            seed: 0,
            denoise: 0.6,
            pipeline: "t2i".into(),
        }
    }
}
impl ImageDraft {
    pub fn validate(&self) -> Result<(), String> {
        if [&self.positive_prompt, &self.negative_prompt]
            .iter()
            .any(|s| s.encode_utf16().count() > 16384 || s.contains('\0'))
            || (!self.checkpoint.is_empty() && !crate::jobs::comfy_model_name(&self.checkpoint))
            || ![self.width, self.height]
                .iter()
                .all(|n| (64..=8192).contains(n) && n % 8 == 0)
            || u64::from(self.width) * u64::from(self.height) > 16_777_216
            || !(1..=150).contains(&self.steps)
            || !self.cfg.is_finite()
            || !(0.0..=30.0).contains(&self.cfg)
            || !crate::jobs::safe_choice(&self.sampler)
            || !crate::jobs::safe_choice(&self.scheduler)
            || self.seed > 9_007_199_254_740_991
            || !self.denoise.is_finite()
            || !(0.0..=1.0).contains(&self.denoise)
            || !["t2i", "i2i"].contains(&self.pipeline.as_str())
        {
            return Err("Invalid image draft settings".into());
        }
        Ok(())
    }
}
pub const MANIFEST: &str = "project.json";
const LOCK: &str = ".deas-project.lock";
const MAX_MANIFEST: u64 = 256 * 1024;
use crate::downloads::fs::Anchor;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectInput {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSnapshot {
    pub id: String,
    pub root: String,
    pub revision: u64,
    pub draft: ImageDraft,
    pub input: Option<ProjectInput>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    id: String,
    schema_version: u32,
    revision: u64,
    draft: ImageDraft,
    #[serde(deserialize_with = "Option::deserialize")]
    input: Option<ProjectInput>,
}
pub(crate) struct Root {
    anchor: Anchor,
    path: PathBuf,
}
impl Root {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let anchor =
            Anchor::open(path).map_err(|_| "Project path must contain only real directories")?;
        anchor
            .writable()
            .map_err(|_| "Project directory is not trusted or writable")?;
        let path = dunce::canonicalize(path).map_err(|_| "Cannot resolve project directory")?;
        path.to_str().ok_or("Project path must be Unicode")?;
        Ok(Self { anchor, path })
    }
    pub(crate) fn verify(&self) -> Result<(), String> {
        let again = Anchor::open(&self.path).map_err(|_| "Project directory was replaced")?;
        if again.identity()? != self.anchor.identity()? {
            return Err("Project directory was replaced".into());
        }
        Ok(())
    }
    pub(crate) fn file(&self, name: &str, lock: bool) -> Result<File, String> {
        self.verify()?;
        #[cfg(target_os = "linux")]
        let file = {
            use std::{
                ffi::CString,
                os::fd::{AsRawFd, FromRawFd},
            };
            let name = CString::new(name).map_err(|_| "Unsafe project filename")?;
            let flags = if lock {
                libc::O_RDWR | libc::O_CREAT
            } else {
                libc::O_RDONLY
            };
            let fd = unsafe {
                libc::openat(
                    self.anchor.0.as_raw_fd(),
                    name.as_ptr(),
                    flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                    0o600,
                )
            };
            if fd < 0 {
                return Err("Cannot open project file safely".into());
            }
            unsafe { File::from_raw_fd(fd) }
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
            let path = self.path.join(name);
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(lock)
                .create(lock)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)
                .map_err(|_| "Cannot open project file safely")?;
            let again = crate::win::open_no_follow(&path).map_err(|_| "Unsafe project file")?;
            if crate::win::identity(&file)? != crate::win::identity(&again)? {
                return Err("Project file was replaced".into());
            }
            file
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (name, lock);
            return Err("Projects require Linux or Windows".into());
        }
        #[cfg(any(target_os = "linux", windows))]
        {
            if !file
                .metadata()
                .map_err(|_| "Cannot inspect project file")?
                .is_file()
            {
                return Err("Project file must be regular".into());
            }
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::MetadataExt;
                if file
                    .metadata()
                    .map_err(|_| "Cannot inspect project file")?
                    .nlink()
                    != 1
                {
                    return Err("Hard-linked project files are not allowed".into());
                }
            }
            Ok(file)
        }
    }
    pub(crate) fn absent(&self, name: &str) -> Result<(), String> {
        self.verify()?;
        self.anchor.absent(name)
    }
    pub(crate) fn read(&self, name: &str, limit: u64) -> Result<Vec<u8>, String> {
        let file = self.file(name, false)?;
        if file
            .metadata()
            .map_err(|_| "Cannot inspect project file")?
            .len()
            > limit
        {
            return Err("Project file exceeds size limit".into());
        }
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read project file")?;
        if bytes.len() as u64 > limit {
            return Err("Project file exceeds size limit".into());
        }
        Ok(bytes)
    }
    fn sync(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        self.anchor
            .0
            .sync_all()
            .map_err(|_| "Cannot sync project directory")?;
        // Windows has no supported directory fsync; manifest MoveFileExW uses WRITE_THROUGH.
        Ok(())
    }
    fn write(&self, data: &Manifest, new: bool) -> Result<(), String> {
        let bytes = serde_json::to_vec(data).map_err(|_| "Cannot encode project manifest")?;
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("Project manifest exceeds size limit".into());
        }
        self.write_json_bytes(MANIFEST, &bytes, new)
    }
    /// Shared anchored atomic JSON publication. The caller owns schema/byte bounds and writer lock.
    pub(crate) fn write_json_bytes(
        &self,
        target: &str,
        bytes: &[u8],
        new: bool,
    ) -> Result<(), String> {
        if !target
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
            || target.is_empty()
        {
            return Err("Unsafe JSON filename".into());
        }
        self.verify()?;
        self.anchor.writable()?;
        self.sync()?;
        let name = format!(".manifest-{}.tmp", uuid::Uuid::new_v4());
        let mut file = self.anchor.partial(&name)?;
        let result = (|| {
            file.write_all(bytes)
                .map_err(|_| "Cannot write project manifest")?;
            file.sync_all()
                .map_err(|_| "Cannot sync project manifest")?;
            self.verify()?;
            if !self.anchor.owns_partial(&name, &file) {
                return Err("Manifest temporary file was replaced".into());
            }
            #[cfg(not(windows))]
            if new {
                return self.anchor.publish(&name, target);
            }
            // Never adopt a symlink or directory as the previous manifest.
            if !new {
                self.file(target, false)?;
            }
            #[cfg(target_os = "linux")]
            {
                use std::{ffi::CString, os::fd::AsRawFd};
                let from = CString::new(name.as_str()).unwrap();
                let to = CString::new(target).unwrap();
                if unsafe {
                    libc::renameat(
                        self.anchor.0.as_raw_fd(),
                        from.as_ptr(),
                        self.anchor.0.as_raw_fd(),
                        to.as_ptr(),
                    )
                } != 0
                {
                    return Err("Cannot replace project manifest".into());
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStrExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
                };
                let wide = |p: PathBuf| p.as_os_str().encode_wide().chain([0]).collect::<Vec<_>>();
                if unsafe {
                    MoveFileExW(
                        wide(self.path.join(&name)).as_ptr(),
                        wide(self.path.join(target)).as_ptr(),
                        MOVEFILE_WRITE_THROUGH | if new { 0 } else { MOVEFILE_REPLACE_EXISTING },
                    )
                } == 0
                {
                    return Err("Cannot replace project manifest".into());
                }
            }
            // A post-rename sync error has an uncertain durability outcome; never report it as saved.
            self.sync().map_err(|_| {
                "Project commit durability uncertain; close and reopen before further edits".into()
            })
        })();
        if self.anchor.owns_partial(&name, &file) {
            self.anchor.remove_partial(&name);
        }
        result
    }
}
struct Active {
    root: Root,
    data: Manifest,
    lock: File,
}
impl Active {
    fn snapshot(&self) -> ProjectSnapshot {
        let mut input = self.data.input.clone();
        if let Some(input) = input.as_mut() {
            input.path = self
                .root
                .path
                .join(&input.path)
                .to_str()
                .expect("validated Unicode root")
                .into();
        }
        ProjectSnapshot {
            id: self.data.id.clone(),
            root: self
                .root
                .path
                .to_str()
                .expect("validated Unicode root")
                .into(),
            revision: self.data.revision,
            draft: self.data.draft.clone(),
            input,
        }
    }
    fn check(&self, id: &str, revision: u64) -> Result<(), String> {
        if self.data.id != id {
            return Err("Project is not active".into());
        }
        if self.data.revision != revision {
            return Err("Stale project revision".into());
        }
        self.root.verify()?;
        if !self.root.anchor.owns_partial(LOCK, &self.lock) {
            return Err("Project lock was replaced".into());
        }
        let saved: Manifest = serde_json::from_slice(&self.root.read(MANIFEST, MAX_MANIFEST)?)
            .map_err(|_| "Invalid project manifest")?;
        if saved != self.data {
            return Err("Project manifest changed outside DEAS".into());
        }
        Ok(())
    }
}
fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id)
}
fn validate_manifest(data: &Manifest) -> Result<(), String> {
    if data.schema_version != 1 || !valid_id(&data.id) || data.revision > 9_007_199_254_740_991 {
        return Err("Unsupported or invalid project manifest".into());
    }
    data.draft.validate()
}
fn validate_input(root: &Root, input: &ProjectInput) -> Result<(), String> {
    validate_input_reference(input)?;
    validate_input_bytes(input, &root.read(&input.path, MAX_PNG)?)
}
fn validate_input_reference(input: &ProjectInput) -> Result<(), String> {
    if !valid_id(&input.id)
        || input.path != format!("input-{}.png", input.id)
        || input.sha256.len() != 64
        || !input
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid managed image reference".into());
    }
    Ok(())
}
fn validate_input_bytes(input: &ProjectInput, bytes: &[u8]) -> Result<(), String> {
    validate_input_reference(input)?;
    let dimensions = png_dimensions(bytes)?;
    use sha2::{Digest, Sha256};
    if dimensions != (input.width, input.height)
        || format!("{:x}", Sha256::digest(bytes)) != input.sha256
    {
        return Err("Managed image integrity mismatch".into());
    }
    Ok(())
}
fn acquire(root: &Root) -> Result<File, String> {
    let lock = root.file(LOCK, true)?;
    lock.try_lock()
        .map_err(|_| "Project already has a writer or cannot be locked")?;
    Ok(lock)
}
fn report<T>(operation: &str, id: Option<&str>, result: Result<T, String>) -> Result<T, String> {
    use std::{
        collections::VecDeque,
        time::{Duration, Instant},
    };
    static FAILURES: Mutex<VecDeque<(String, Instant)>> = Mutex::new(VecDeque::new());
    let id = id.filter(|id| valid_id(id)).unwrap_or("none");
    let key = format!("operation={operation} project={id}");
    let mut failures = FAILURES.lock().unwrap_or_else(|e| e.into_inner());
    if result.is_err() {
        if failures
            .iter()
            .any(|(k, at)| k == &key && at.elapsed() < Duration::from_secs(60))
        {
            return result;
        }
        failures.retain(|(k, _)| k != &key);
        if failures.len() >= 32 {
            failures.pop_front();
        }
        failures.push_back((key.clone(), Instant::now()));
    } else {
        failures.retain(|(k, _)| k != &key);
    }
    let severity = if result.is_err() { "ERROR" } else { "INFO" };
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "{severity}: Projects {key} {}",
            if result.is_err() {
                "failed"
            } else {
                "completed"
            }
        ),
    );
    result
}
fn report_selection(
    operation: &str,
    id: Option<&str>,
    result: Result<Option<ProjectSnapshot>, String>,
) -> Result<Option<ProjectSnapshot>, String> {
    if matches!(result, Ok(None)) {
        let id = id.filter(|id| valid_id(id)).unwrap_or("none");
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("INFO: Projects operation={operation} project={id} cancelled"),
        );
        return result;
    }
    let known_id = result
        .as_ref()
        .ok()
        .and_then(|s| s.as_ref())
        .map(|s| s.id.clone());
    report(operation, known_id.as_deref().or(id), result)
}
#[derive(Clone, Default)]
pub struct Projects(std::sync::Arc<Mutex<Option<Active>>>);
fn validate_title(title: &str) -> Result<(), String> {
    report(
        "dialog-title",
        None,
        if title.trim().is_empty()
            || title.chars().count() > 240
            || title.chars().any(char::is_control)
        {
            Err("Invalid project dialog title".into())
        } else {
            Ok(())
        },
    )
}
const MAX_PNG: u64 = 64 * 1024 * 1024;
fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    crate::jobs::validate_png_envelope(bytes)?;
    let mut options = png::DecodeOptions::default();
    options.set_ignore_crc(false);
    options.set_ignore_adler32(false);
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let mut decoder = png::Decoder::new_with_options(std::io::Cursor::new(bytes), options);
    decoder.set_limits(png::Limits {
        bytes: 64 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|_| "Invalid PNG image")?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
        || info.animation_control.is_some()
        || reader.output_buffer_size() > 128 * 1024 * 1024
    {
        return Err("PNG dimensions or decoded memory exceed limits, or image is animated".into());
    }
    let mut decoded = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut decoded)
        .map_err(|_| "Invalid PNG pixels")?;
    reader.finish().map_err(|_| "Invalid PNG ending")?;
    Ok((width, height))
}
impl Projects {
    pub fn import_image(
        &self,
        id: &str,
        revision: u64,
        source: Option<&Path>,
    ) -> Result<Option<ProjectSnapshot>, String> {
        report_selection(
            "import-image",
            Some(id),
            (|| {
                let Some(source) = source else {
                    return Ok(None);
                };
                let mut active = self.0.lock().map_err(|_| "Project state unavailable")?;
                let current = active.as_mut().ok_or("No active project")?;
                current.check(id, revision)?;
                if !source
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                {
                    return Err("Only PNG input is supported".into());
                }
                let parent = source.parent().ok_or("Invalid input image path")?;
                let name = source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("Invalid input image filename")?;
                let anchor = Anchor::open(parent)
                    .map_err(|_| "Input image path must contain only real directories")?;
                let path = dunce::canonicalize(parent)
                    .map_err(|_| "Cannot resolve input image directory")?;
                let bytes = Root { anchor, path }.read(name, MAX_PNG)?;
                let (width, height) = png_dimensions(&bytes)?;
                use sha2::{Digest, Sha256};
                let input_id = uuid::Uuid::new_v4().to_string();
                let name = format!("input-{input_id}.png");
                current.root.verify()?;
                current.root.anchor.writable()?;
                let mut file = current.root.anchor.partial(&name)?;
                // A fresh UUID is the final no-clobber asset name; commit the manifest only after fsync.
                file.write_all(&bytes)
                    .map_err(|_| "Cannot copy input image")?;
                file.sync_all().map_err(|_| "Cannot sync input image")?;
                current.root.sync()?;
                if !current.root.anchor.owns_partial(&name, &file) {
                    return Err("Input image was replaced".into());
                }
                let mut next = current.data.clone();
                next.revision = next
                    .revision
                    .checked_add(1)
                    .filter(|r| *r <= 9_007_199_254_740_991)
                    .ok_or("Project revision exhausted")?;
                next.input = Some(ProjectInput {
                    id: input_id,
                    path: name,
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    width,
                    height,
                });
                current.root.write(&next, false)?;
                current.data = next;
                Ok(Some(current.snapshot()))
            })(),
        )
    }
    pub fn create(&self, parent: Option<&Path>) -> Result<Option<ProjectSnapshot>, String> {
        report_selection(
            "create",
            None,
            (|| {
                let Some(parent) = parent else {
                    return Ok(None);
                };
                let mut active = self.0.lock().map_err(|_| "Project state unavailable")?;
                let parent = Root::open(parent)?;
                let id = uuid::Uuid::new_v4().to_string();
                let name = format!("deas-project-{id}");
                let anchor = parent.anchor.create(&name)?;
                parent.sync()?;
                let root = Root {
                    anchor,
                    path: parent.path.join(name),
                };
                let lock = acquire(&root)?;
                let data = Manifest {
                    id,
                    schema_version: 1,
                    revision: 0,
                    draft: ImageDraft::default(),
                    input: None,
                };
                root.write(&data, true)?;
                let next = Active { root, data, lock };
                let snapshot = next.snapshot();
                *active = Some(next);
                Ok(Some(snapshot))
            })(),
        )
    }
    pub fn open(&self, path: Option<&Path>) -> Result<Option<ProjectSnapshot>, String> {
        report_selection(
            "open",
            None,
            (|| {
                let Some(path) = path else {
                    return Ok(None);
                };
                let mut active = self.0.lock().map_err(|_| "Project state unavailable")?;
                let root = Root::open(path)?;
                if let Some(current) = active.as_ref() {
                    if current.root.anchor.identity()? == root.anchor.identity()? {
                        current.check(&current.data.id, current.data.revision)?;
                        if let Some(input) = &current.data.input {
                            validate_input(&root, input)?;
                        }
                        return Ok(Some(current.snapshot()));
                    }
                }
                let lock = acquire(&root)?;
                let data: Manifest = serde_json::from_slice(&root.read(MANIFEST, MAX_MANIFEST)?)
                    .map_err(|_| "Invalid project manifest")?;
                validate_manifest(&data)?;
                if let Some(input) = &data.input {
                    validate_input(&root, input)?;
                }
                let next = Active { root, data, lock };
                let snapshot = next.snapshot();
                *active = Some(next);
                Ok(Some(snapshot))
            })(),
        )
    }
    pub fn save(
        &self,
        id: &str,
        revision: u64,
        draft: ImageDraft,
    ) -> Result<ProjectSnapshot, String> {
        report(
            "save",
            Some(id),
            (|| {
                draft.validate()?;
                let mut active = self.0.lock().map_err(|_| "Project state unavailable")?;
                let current = active.as_mut().ok_or("No active project")?;
                current.check(id, revision)?;
                let mut next = current.data.clone();
                next.revision = next
                    .revision
                    .checked_add(1)
                    .filter(|r| *r <= 9_007_199_254_740_991)
                    .ok_or("Project revision exhausted")?;
                next.draft = draft;
                current.root.write(&next, false)?;
                current.data = next;
                Ok(current.snapshot())
            })(),
        )
    }
    pub fn close(&self, id: &str) -> Result<(), String> {
        report(
            "close",
            Some(id),
            (|| {
                let mut active = self.0.lock().map_err(|_| "Project state unavailable")?;
                if active.as_ref().is_none_or(|p| p.data.id != id) {
                    return Err("Project is not active".into());
                }
                *active = None;
                Ok(())
            })(),
        )
    }
}
#[tauri::command]
pub async fn native_project_create(
    title: String,
    state: tauri::State<'_, Projects>,
) -> Result<Option<ProjectSnapshot>, String> {
    validate_title(&title)?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_folder()
        .await;
    state.create(selected.as_ref().map(|f| f.path()))
}
#[tauri::command]
pub async fn native_project_open(
    title: String,
    state: tauri::State<'_, Projects>,
) -> Result<Option<ProjectSnapshot>, String> {
    validate_title(&title)?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_folder()
        .await;
    state.open(selected.as_ref().map(|f| f.path()))
}
#[tauri::command]
pub fn native_project_save(
    project_id: String,
    expected_revision: u64,
    draft: ImageDraft,
    state: tauri::State<'_, Projects>,
) -> Result<ProjectSnapshot, String> {
    state.save(&project_id, expected_revision, draft)
}
#[tauri::command]
pub async fn native_project_import_image(
    title: String,
    project_id: String,
    expected_revision: u64,
    state: tauri::State<'_, Projects>,
) -> Result<Option<ProjectSnapshot>, String> {
    validate_title(&title)?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("PNG image", &["png"])
        .pick_file()
        .await;
    state.import_image(
        &project_id,
        expected_revision,
        selected.as_ref().map(|f| f.path()),
    )
}
#[tauri::command]
pub fn native_project_close(
    project_id: String,
    state: tauri::State<'_, Projects>,
) -> Result<(), String> {
    state.close(&project_id)
}
/// Fixed frontend close-guard diagnostics; never accepts prompts, paths or arbitrary log text.
#[tauri::command]
pub fn native_project_close_notice(reason: String) -> Result<(), String> {
    use std::time::{Duration, Instant};
    let (slot, text) = match reason.as_str() {
        "blocked" => (
            0,
            "INFO: Projects operation=window-close blocked pending project changes",
        ),
        "unavailable" => (
            1,
            "ERROR: Projects operation=window-close protection unavailable",
        ),
        _ => return Err("Unknown project close diagnostic".into()),
    };
    static LAST: Mutex<[Option<Instant>; 2]> = Mutex::new([None, None]);
    let mut last = LAST.lock().unwrap_or_else(|error| error.into_inner());
    if last[slot].is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
        last[slot] = Some(Instant::now());
        crate::logs::push(crate::logs::Source::Deas, text);
    }
    Ok(())
}

#[path = "projects_package.rs"]
mod package;
pub use package::{native_project_export_package, native_project_import_package};

#[cfg(test)]
#[path = "projects_tests.rs"]
mod tests;
