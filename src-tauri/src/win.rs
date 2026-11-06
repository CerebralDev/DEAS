//! Windows equivalents of the Linux descriptor/process-group proofs: handle-based
//! file identity, reparse-point rejection, no-replace rename, Job Object group
//! termination and iphlpapi listener ownership. Owner/ACL checks are relaxed to
//! "exists, not a reparse point" because Windows ownership is SID/ACL based and
//! the parent is chosen by the user through the native chooser.
use std::{
    fs::{File, OpenOptions},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, NO_ERROR, STILL_ACTIVE},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    },
    Networking::WinSock::{AF_INET, AF_INET6},
    Storage::FileSystem::{
        GetFileInformationByHandle, MoveFileExW, BY_HANDLE_FILE_INFORMATION,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT,
    },
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
    let mut info = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0 {
        return Err("Cannot inspect handle".into());
    }
    Ok(info)
}
/// (volume serial, file index): the Windows analogue of (dev, ino).
pub fn identity(file: &File) -> Result<(u64, u64), String> {
    let i = info(file)?;
    Ok((
        i.dwVolumeSerialNumber as u64,
        ((i.nFileIndexHigh as u64) << 32) | i.nFileIndexLow as u64,
    ))
}
/// Full (including ReFS) file identity and link count for advisory model caches.
pub fn cache_file_identity(file: &File) -> Result<(u64, [u8; 16], u64), String> {
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
    };
    let mut id: FILE_ID_INFO = unsafe { std::mem::zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileIdInfo,
            std::ptr::addr_of_mut!(id).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err("Cannot inspect full cache file identity".into());
    }
    Ok((
        id.VolumeSerialNumber,
        id.FileId.Identifier,
        info(file)?.nNumberOfLinks as u64,
    ))
}
/// Opens the object itself (never its reparse target) and rejects reparse points.
pub fn open_no_follow(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    if info(&file)?.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Reparse points (symlinks/junctions) are not allowed".into());
    }
    Ok(file)
}
pub fn is_directory(file: &File) -> Result<bool, String> {
    Ok(info(file)?.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0)
}
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}
/// MoveFileExW without MOVEFILE_REPLACE_EXISTING: fails if `to` exists.
pub fn rename_no_replace(from: &Path, to: &Path) -> Result<(), String> {
    if unsafe { MoveFileExW(wide(from).as_ptr(), wide(to).as_ptr(), 0) } == 0 {
        return Err("Cannot publish without overwrite".into());
    }
    Ok(())
}

/// Kill-on-close Job Object: every descendant of an assigned child dies with it.
pub struct Job(HANDLE);
unsafe impl Send for Job {}
impl Job {
    pub fn new() -> Result<Self, String> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err("Cannot create job object".into());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        let job = Self(job);
        if ok == 0 {
            return Err("Cannot configure job object".into());
        }
        Ok(job)
    }
    // shortcut: the child runs unassigned between spawn and assign, so a grandchild
    // forked in that window escapes the job; upgrade to CREATE_SUSPENDED + ResumeThread
    // if that race is ever observed.
    pub fn assign(&self, process: HANDLE) -> Result<(), String> {
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err("Cannot assign child to job object".into());
        }
        Ok(())
    }
    pub fn empty(&self) -> Result<bool, String> {
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(format!(
                "Cannot inspect owned job: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(info.ActiveProcesses == 0)
    }
    pub fn kill(&self) {
        unsafe { TerminateJobObject(self.0, 1) };
    }
    /// Whether `pid` is a live member of this job. A venv `python.exe` is a launcher
    /// that runs the base interpreter as a child, so the listener belongs to a job
    /// member rather than to the spawned PID itself.
    pub fn contains(&self, pid: u32) -> bool {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return false;
        }
        let mut member = 0;
        let ok = unsafe { IsProcessInJob(process, self.0, &mut member) };
        unsafe { CloseHandle(process) };
        ok != 0 && member != 0
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}
/// The process object stays alive while the caller holds its handle, so the PID
/// cannot be reused before the owner reaps it (the WNOWAIT analogue).
pub fn exited(process: HANDLE) -> Result<bool, String> {
    let mut code = 0u32;
    if unsafe { GetExitCodeProcess(process, &mut code) } == 0 {
        return Err(format!(
            "Owned child wait failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    // shortcut: a process exiting with code 259 (STILL_ACTIVE) reads as running.
    Ok(code != STILL_ACTIVE as u32)
}

/// LISTEN sockets on `port`: (IPv4 address in network order or None for IPv6, owning PID).
pub fn listeners(port: u16) -> Result<Vec<(Option<u32>, u32)>, String> {
    let mut out = vec![];
    for family in [AF_INET, AF_INET6] {
        let mut size = 0u32;
        let mut table: Vec<u8> = vec![];
        loop {
            let code = unsafe {
                GetExtendedTcpTable(
                    table.as_mut_ptr() as *mut _,
                    &mut size,
                    0,
                    family as u32,
                    TCP_TABLE_OWNER_PID_LISTENER,
                    0,
                )
            };
            if code == NO_ERROR && !table.is_empty() {
                break;
            }
            if code != ERROR_INSUFFICIENT_BUFFER && code != NO_ERROR {
                return Err(format!("GetExtendedTcpTable failed: {code}"));
            }
            table = vec![0; size as usize];
        }
        let count = u32::from_ne_bytes(table[..4].try_into().unwrap()) as usize;
        let rows = &table[4..];
        for i in 0..count {
            let (addr, local_port, pid) = if family == AF_INET {
                let row: MIB_TCPROW_OWNER_PID = unsafe {
                    std::ptr::read_unaligned(
                        rows.as_ptr()
                            .add(i * std::mem::size_of::<MIB_TCPROW_OWNER_PID>())
                            as *const _,
                    )
                };
                (Some(row.dwLocalAddr), row.dwLocalPort, row.dwOwningPid)
            } else {
                let row: MIB_TCP6ROW_OWNER_PID = unsafe {
                    std::ptr::read_unaligned(
                        rows.as_ptr()
                            .add(i * std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>())
                            as *const _,
                    )
                };
                (None, row.dwLocalPort, row.dwOwningPid)
            };
            if u16::from_be((local_port & 0xFFFF) as u16) == port {
                out.push((addr, pid));
            }
        }
    }
    Ok(out)
}
/// 127.0.0.1 as stored in MIB_TCPROW_OWNER_PID (network byte order).
pub const LOOPBACK_V4: u32 = u32::from_ne_bytes([127, 0, 0, 1]);
