//! Linux descriptor-anchored writes: no traversal, no symlink adoption, no clobber.
//! Creation requires an effective-user-owned parent without group/world write access,
//! including sticky shared directories. Opened staging children must be owned private
//! directories. This protects against cross-user substitution, not same-user adversaries.
//! Windows keeps a directory handle plus its path: every write re-opens the path without
//! following reparse points and requires the same handle identity, then creates with
//! CREATE_NEW / must-create / MoveFileExW-without-replace. Ownership/ACL checks are
//! relaxed to "exists, is a non-reparse directory" (see win.rs). Other platforms fail closed.
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::{
    ffi::CString,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::ffi::OsStrExt,
};
use std::{fs::File, path::Path};

#[cfg(all(test, target_os = "linux"))]
#[path = "fs_tests.rs"]
mod tests;
#[cfg(all(test, windows))]
#[path = "fs_tests_windows.rs"]
mod tests;

#[cfg(target_os = "linux")]
pub struct Anchor(pub(crate) File);
#[cfg(windows)]
pub struct Anchor(pub(crate) File, PathBuf);
#[cfg(not(any(target_os = "linux", windows)))]
pub struct Anchor;
impl Anchor {
    #[cfg(target_os = "linux")]
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("Parent must be absolute".into());
        }
        let root = CString::new("/").unwrap();
        let fd = unsafe {
            libc::open(
                root.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err("Cannot open parent".into());
        }
        let mut dir = Self(unsafe { File::from_raw_fd(fd) });
        for part in path.components().skip(1) {
            let std::path::Component::Normal(name) = part else {
                return Err("Unsafe parent path".into());
            };
            let name = CString::new(name.as_bytes()).map_err(|_| "Unsafe parent path")?;
            let fd = unsafe {
                libc::openat(
                    dir.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if fd < 0 {
                return Err("Parent components must be existing non-symlink directories".into());
            }
            dir = Self(unsafe { File::from_raw_fd(fd) });
        }
        Ok(dir)
    }
    #[cfg(windows)]
    pub fn open(path: &Path) -> Result<Self, String> {
        use std::path::Component;
        if !path.is_absolute() {
            return Err("Parent must be absolute".into());
        }
        let mut current = PathBuf::new();
        let mut dir = None;
        for part in path.components() {
            match part {
                Component::Prefix(_) => current.push(part),
                Component::RootDir | Component::Normal(_) => {
                    current.push(part);
                    let file = crate::win::open_no_follow(&current).map_err(
                        |_| "Parent components must be existing non-symlink directories",
                    )?;
                    if !crate::win::is_directory(&file)? {
                        return Err("Parent components must be directories".into());
                    }
                    dir = Some(file);
                }
                _ => return Err("Unsafe parent path".into()),
            }
        }
        Ok(Self(dir.ok_or("Unsafe parent path")?, current))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    pub fn open(_: &Path) -> Result<Self, String> {
        Err("Safe staged downloads currently require Linux or Windows".into())
    }

    pub fn identity(&self) -> Result<(u64, u64), String> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            let m = self.0.metadata().map_err(|_| "Cannot inspect parent")?;
            Ok((m.dev(), m.ino()))
        }
        #[cfg(windows)]
        {
            crate::win::identity(&self.0)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Err("Unsupported platform".into())
        }
    }
    // Path-based Win32 calls cannot take a directory handle; prove the path still
    // names this exact handle immediately before every write instead.
    #[cfg(windows)]
    fn child(&self, name: &str) -> Result<PathBuf, String> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name
                .chars()
                .any(|c| matches!(c, '/' | '\\' | ':') || c.is_control())
        {
            return Err("Unsafe filename".into());
        }
        let again = crate::win::open_no_follow(&self.1)?;
        if crate::win::identity(&again)? != self.identity()? {
            return Err("Anchored directory was replaced".into());
        }
        Ok(self.1.join(name))
    }
    pub fn absent(&self, name: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            let name = CString::new(name).map_err(|_| "Unsafe filename")?;
            let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
            let result = unsafe {
                libc::fstatat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    st.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT)
            {
                Ok(())
            } else {
                Err("Target already exists or cannot be inspected".into())
            }
        }
        #[cfg(windows)]
        {
            match std::fs::symlink_metadata(self.child(name)?) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                _ => Err("Target already exists or cannot be inspected".into()),
            }
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = name;
            Err("Unsupported platform".into())
        }
    }
    /// An existing user-owned directory may receive exclusively created files.
    pub fn writable(&self) -> Result<(), String> {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.validate_directory(false)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Err("Unsupported platform".into())
        }
    }
    pub fn create(&self, name: &str) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            self.create_checked(name, || {})
        }
        #[cfg(windows)]
        {
            self.validate_directory(false)?;
            let path = self.child(name)?;
            // CreateDirectoryW fails when anything with that name exists: must-create.
            std::fs::create_dir(&path)
                .map_err(|_| "Cannot exclusively create new staging folder")?;
            let file =
                crate::win::open_no_follow(&path).map_err(|_| "Cannot anchor staging folder")?;
            let child = Self(file, path);
            child.validate_directory(true)?;
            Ok(child)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = name;
            Err("Unsupported platform".into())
        }
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn validate_directory(&self, private: bool) -> Result<(), String> {
        use std::os::unix::fs::MetadataExt;
        let metadata = self.0.metadata().map_err(|_| "Cannot inspect directory")?;
        Self::validate_directory_metadata(metadata.uid(), metadata.mode(), private)
    }
    #[cfg(windows)]
    pub(crate) fn validate_directory(&self, _private: bool) -> Result<(), String> {
        if !crate::win::is_directory(&self.0)? {
            return Err("Download directory must be a directory".into());
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn validate_directory_metadata(uid: u32, mode: u32, private: bool) -> Result<(), String> {
        let forbidden = if private { 0o077 } else { 0o022 };
        if uid != unsafe { libc::geteuid() }
            || mode & libc::S_IFMT != libc::S_IFDIR
            || mode & forbidden != 0
        {
            return Err(
                "Download directory must be owned by the effective user with trusted permissions"
                    .into(),
            );
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn create_checked(&self, name: &str, after_mkdir: impl FnOnce()) -> Result<Self, String> {
        // Inspect the anchored descriptor immediately before any filesystem write.
        // O_NOFOLLOW alone cannot prevent mkdirat/openat substitution in shared parents.
        self.validate_directory(false)?;
        let name = CString::new(name).map_err(|_| "Unsafe filename")?;
        if unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err("Cannot exclusively create new staging folder".into());
        }
        // A deterministic test seam; production passes a no-op. Never delete an
        // untrusted replacement on error because its identity is not ours to manage.
        after_mkdir();
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err("Cannot anchor staging folder".into());
        }
        let child = Self(unsafe { File::from_raw_fd(fd) });
        child.validate_directory(true)?;
        Ok(child)
    }

    pub fn partial(&self, name: &str) -> Result<File, String> {
        #[cfg(target_os = "linux")]
        {
            let name = CString::new(name).map_err(|_| "Unsafe filename")?;
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW,
                    0o600,
                )
            };
            if fd < 0 {
                return Err("Cannot exclusively create partial".into());
            }
            Ok(unsafe { File::from_raw_fd(fd) })
        }
        #[cfg(windows)]
        {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.child(name)?)
                .map_err(|_| "Cannot exclusively create partial".into())
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = name;
            Err("Unsupported platform".into())
        }
    }
    /// The partial opened for reading and appending, with its current length; `None` when absent.
    pub fn resume_partial(&self, name: &str) -> Result<Option<(File, u64)>, String> {
        #[cfg(target_os = "linux")]
        {
            let name = CString::new(name).map_err(|_| "Unsafe filename")?;
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if fd < 0 {
                return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                    Ok(None)
                } else {
                    Err("Cannot resume partial".into())
                };
            }
            let file = unsafe { File::from_raw_fd(fd) };
            let size = file.metadata().map_err(|_| "Cannot resume partial")?.len();
            Ok(Some((file, size)))
        }
        #[cfg(windows)]
        {
            let file = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(self.child(name)?)
            {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("Cannot resume partial".into()),
            };
            let size = file.metadata().map_err(|_| "Cannot resume partial")?.len();
            Ok(Some((file, size)))
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = name;
            Err("Unsupported platform".into())
        }
    }
    /// True when `name` inside this directory is still the regular file behind `file`.
    pub fn owns_partial(&self, name: &str, file: &File) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            let (Ok(metadata), Ok(name)) = (file.metadata(), CString::new(name)) else {
                return false;
            };
            let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    st.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return false;
            }
            let st = unsafe { st.assume_init() };
            st.st_dev == metadata.dev()
                && st.st_ino == metadata.ino()
                && st.st_mode & libc::S_IFMT == libc::S_IFREG
        }
        #[cfg(windows)]
        {
            let Ok(path) = self.child(name) else {
                return false;
            };
            crate::win::open_no_follow(&path).is_ok_and(|again| {
                crate::win::identity(&again).ok() == crate::win::identity(file).ok()
                    && crate::win::is_directory(&again) == Ok(false)
            })
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (name, file);
            false
        }
    }
    pub fn publish(&self, partial: &str, name: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            let partial = CString::new(partial).map_err(|_| "Unsafe filename")?;
            let name = CString::new(name).map_err(|_| "Unsafe filename")?;
            if unsafe {
                libc::linkat(
                    self.0.as_raw_fd(),
                    partial.as_ptr(),
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    0,
                )
            } != 0
            {
                return Err("Cannot publish without overwrite".into());
            }
            unsafe { libc::unlinkat(self.0.as_raw_fd(), partial.as_ptr(), 0) };
            self.0
                .sync_all()
                .map_err(|_| "Cannot sync staging folder")?;
            Ok(())
        }
        #[cfg(windows)]
        {
            crate::win::rename_no_replace(&self.child(partial)?, &self.child(name)?)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (partial, name);
            Err("Unsupported platform".into())
        }
    }
    pub fn remove_partial(&self, name: &str) {
        #[cfg(target_os = "linux")]
        if let Ok(name) = CString::new(name) {
            unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), 0) };
        }
        #[cfg(windows)]
        if let Ok(path) = self.child(name) {
            let _ = std::fs::remove_file(path);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        let _ = name;
    }
}
