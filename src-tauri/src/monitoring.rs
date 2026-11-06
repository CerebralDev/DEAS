//! One read-only telemetry sample for the Monitoring tab: CPU, RAM, swap, disk and NVIDIA GPUs.
//! Works without the backend; nothing here writes, installs or touches ComfyUI.
use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub at: u64,
    pub source: &'static str,
    pub cpu: Option<Cpu>,
    pub ram: Option<Memory>,
    pub swap: Option<Memory>,
    pub disks: Vec<Disk>,
    /// Host-wide read/write throughput in MiB/s over all physical block devices (Linux only).
    pub disk_io: Option<DiskIo>,
    pub gpus: Vec<Gpu>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskIo {
    pub read_mib_s: f64,
    pub write_mib_s: f64,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Cpu {
    pub percent: f64,
    pub count: u32,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Memory {
    pub used: u64,
    pub total: u64,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Disk {
    pub path: String,
    pub used: u64,
    pub total: u64,
}
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub index: u32,
    pub name: String,
    pub utilization: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub temperature: Option<f64>,
    pub power_w: Option<f64>,
    pub power_limit_w: Option<f64>,
}

/// CPU percent is a delta between two calls; the first call reports 0.
static CPU_TIMES: Mutex<Option<(u64, u64)>> = Mutex::new(None);
/// Disk I/O is a delta between two calls: (monotonic time, sectors read, sectors written).
static DISK_IO: Mutex<Option<(std::time::Instant, u64, u64)>> = Mutex::new(None);
/// Peak values per active job id, folded in by the background sampler while the job runs.
static PEAKS: Mutex<Option<std::collections::HashMap<String, Peaks>>> = Mutex::new(None);
/// A missing `nvidia-smi` is reported once and not retried every second.
static NVIDIA_SMI_MISSING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn cpu_percent(idle: u64, total: u64) -> f64 {
    let mut guard = CPU_TIMES.lock().unwrap_or_else(|e| e.into_inner());
    let previous = guard.replace((idle, total));
    match previous {
        Some((prev_idle, prev_total)) if total > prev_total => {
            let busy = (total - prev_total).saturating_sub(idle.saturating_sub(prev_idle));
            (busy as f64 * 100.0 / (total - prev_total) as f64).clamp(0.0, 100.0)
        }
        _ => 0.0,
    }
}

#[cfg(target_os = "linux")]
fn cpu() -> Option<Cpu> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?.strip_prefix("cpu ")?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .filter_map(|f| f.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal; idle = idle + iowait
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().take(8).sum();
    let count = std::thread::available_parallelism().map_or(0, |n| n.get() as u32);
    Some(Cpu {
        percent: cpu_percent(idle, total),
        count,
    })
}
#[cfg(target_os = "linux")]
fn memory() -> (Option<Memory>, Option<Memory>) {
    let Ok(info) = std::fs::read_to_string("/proc/meminfo") else {
        return (None, None);
    };
    let kib = |key: &str| -> Option<u64> {
        info.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|rest| {
                rest.trim_start_matches(':')
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .map(|v| v * 1024)
    };
    let pair = |total: &str, free: &str| {
        let (total, free) = (kib(total)?, kib(free)?);
        Some(Memory {
            used: total.saturating_sub(free),
            total,
        })
    };
    (
        pair("MemTotal", "MemAvailable"),
        pair("SwapTotal", "SwapFree"),
    )
}
/// `/proc/diskstats` fields: major minor name reads … sectors_read(6) … writes … sectors_written(10).
/// Whole devices only (no partition numbers), so each byte counts once.
pub fn parse_diskstats(text: &str) -> (u64, u64) {
    text.lines().fold((0, 0), |(r, w), line| {
        let f: Vec<&str> = line.split_whitespace().collect();
        let whole = f.len() >= 14
            && !f[2].starts_with("loop")
            && !f[2].starts_with("ram")
            && !f[2].starts_with("dm-")
            && !f[2].starts_with("zram")
            && (!f[2].chars().last().is_some_and(|c| c.is_ascii_digit())
                || f[2].starts_with("nvme") && !f[2].contains('p')
                || f[2].starts_with("mmcblk") && !f[2].contains('p'));
        if whole {
            (
                r + f[5].parse::<u64>().unwrap_or(0),
                w + f[9].parse::<u64>().unwrap_or(0),
            )
        } else {
            (r, w)
        }
    })
}
type IoPoint = (std::time::Instant, u64, u64);
fn disk_io_between(then: IoPoint, now: IoPoint) -> Option<DiskIo> {
    let seconds = now.0.duration_since(then.0).as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }
    // 512-byte sectors regardless of the device's logical block size.
    let mib = |delta: u64| delta as f64 * 512.0 / 1_048_576.0 / seconds;
    Some(DiskIo {
        read_mib_s: mib(now.1.saturating_sub(then.1)),
        write_mib_s: mib(now.2.saturating_sub(then.2)),
    })
}
fn disk_io_rate(read_sectors: u64, write_sectors: u64) -> Option<DiskIo> {
    let now = (std::time::Instant::now(), read_sectors, write_sectors);
    let previous = DISK_IO
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(now);
    disk_io_between(previous?, now)
}
#[cfg(target_os = "linux")]
fn disk_io() -> Option<DiskIo> {
    let (r, w) = parse_diskstats(&std::fs::read_to_string("/proc/diskstats").ok()?);
    disk_io_rate(r, w)
}
/// Windows: `IOCTL_DISK_PERFORMANCE` on every physical drive, as psutil does; the handle is
/// opened with no access rights, which a standard user may do.
#[cfg(windows)]
fn disk_io() -> Option<DiskIo> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{DISK_PERFORMANCE, IOCTL_DISK_PERFORMANCE};
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let (mut read, mut written) = (0u64, 0u64);
    for n in 0..32 {
        let path: Vec<u16> = std::ffi::OsString::from(format!(r"\\.\PhysicalDrive{n}"))
            .encode_wide()
            .chain([0])
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            continue;
        }
        let mut perf: DISK_PERFORMANCE = unsafe { std::mem::zeroed() };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_PERFORMANCE,
                std::ptr::null(),
                0,
                (&mut perf as *mut DISK_PERFORMANCE).cast(),
                std::mem::size_of::<DISK_PERFORMANCE>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        unsafe { CloseHandle(handle) };
        if ok != 0 {
            read += perf.BytesRead.max(0) as u64;
            written += perf.BytesWritten.max(0) as u64;
        }
    }
    // Byte counters expressed in 512-byte sectors so the Linux rate math applies unchanged.
    disk_io_rate(read / 512, written / 512)
}
#[cfg(not(any(target_os = "linux", windows)))]
fn disk_io() -> Option<DiskIo> {
    None
}
#[cfg(unix)]
fn disk(path: &std::path::Path) -> Option<Disk> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let frag = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * frag;
    Some(Disk {
        path: path.display().to_string(),
        used: total.saturating_sub(stat.f_bavail as u64 * frag),
        total,
    })
}

#[cfg(windows)]
fn cpu() -> Option<Cpu> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;
    let mut idle: FILETIME = unsafe { std::mem::zeroed() };
    let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
    let mut user: FILETIME = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    // Kernel time includes idle time.
    let (idle, total) = (ticks(idle), ticks(kernel) + ticks(user));
    Some(Cpu {
        percent: cpu_percent(idle, total),
        count: std::thread::available_parallelism().map_or(0, |n| n.get() as u32),
    })
}
#[cfg(windows)]
fn memory() -> (Option<Memory>, Option<Memory>) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return (None, None);
    }
    let ram = Memory {
        used: status.ullTotalPhys.saturating_sub(status.ullAvailPhys),
        total: status.ullTotalPhys,
    };
    // The page file alone: commit limit minus physical RAM.
    let swap_total = status.ullTotalPageFile.saturating_sub(status.ullTotalPhys);
    let swap_free = status.ullAvailPageFile.saturating_sub(status.ullAvailPhys);
    let swap = (swap_total > 0).then(|| Memory {
        used: swap_total.saturating_sub(swap_free.min(swap_total)),
        total: swap_total,
    });
    (Some(ram), swap)
}
#[cfg(windows)]
fn disk(path: &std::path::Path) -> Option<Disk> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
        return None;
    }
    Some(Disk {
        path: path.display().to_string(),
        used: total.saturating_sub(avail),
        total,
    })
}
#[cfg(not(any(target_os = "linux", windows)))]
fn cpu() -> Option<Cpu> {
    None
}
#[cfg(not(any(target_os = "linux", windows)))]
fn memory() -> (Option<Memory>, Option<Memory>) {
    (None, None)
}

const GPU_QUERY: &str = "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit";

/// `nvidia-smi --format=csv,noheader,nounits` rows; unavailable fields (`[N/A]`) become `None`.
pub fn parse_gpus(csv: &str) -> Vec<Gpu> {
    csv.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() != 8 {
                return None;
            }
            let num = |s: &str| s.parse::<f64>().ok();
            let mib = |s: &str| s.parse::<u64>().ok().map(|v| v * 1024 * 1024);
            Some(Gpu {
                index: f[0].parse().ok()?,
                name: f[1].chars().take(120).collect(),
                utilization: num(f[2]),
                memory_used: mib(f[3]),
                memory_total: mib(f[4]),
                temperature: num(f[5]),
                power_w: num(f[6]),
                power_limit_w: num(f[7]),
            })
        })
        .take(16)
        .collect()
}

async fn gpus() -> Result<Vec<Gpu>, String> {
    if NVIDIA_SMI_MISSING.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(vec![]);
    }
    let mut command = tokio::process::Command::new("nvidia-smi");
    command
        .args([GPU_QUERY, "--format=csv,noheader,nounits"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(crate::win::CREATE_NO_WINDOW);
    let output = match tokio::time::timeout(Duration::from_secs(3), command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            NVIDIA_SMI_MISSING.store(true, std::sync::atomic::Ordering::Relaxed);
            return Err("nvidia-smi not found: GPU telemetry unavailable".into());
        }
        Ok(Err(e)) => return Err(format!("nvidia-smi failed: {e}")),
        Err(_) => return Err("nvidia-smi timed out".into()),
    };
    if !output.status.success() {
        return Err(format!("nvidia-smi exited with {}", output.status));
    }
    let text =
        String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(16384)]).into_owned();
    Ok(parse_gpus(&text))
}

pub async fn sample(root: Option<&std::path::Path>) -> Sample {
    let (ram, swap) = memory();
    let mut sample = Sample {
        at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        source: "native",
        cpu: cpu(),
        ram,
        swap,
        disks: root
            .filter(|r| r.is_absolute())
            .and_then(disk)
            .into_iter()
            .collect(),
        disk_io: disk_io(),
        gpus: vec![],
        warnings: vec![],
    };
    match gpus().await {
        Ok(gpus) => sample.gpus = gpus,
        Err(warning) => sample.warnings.push(warning),
    }
    sample
}

/// Read-only: one telemetry sample; `root` only names the disk to report.
#[tauri::command]
pub async fn native_monitoring_sample(root: Option<String>) -> Sample {
    sample(root.as_deref().map(std::path::Path::new)).await
}

/// Highest values seen while a job was queued/running; absent components stay `None`.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Peaks {
    pub samples: u32,
    pub cpu_percent: Option<f64>,
    pub ram_used: Option<u64>,
    pub gpu_utilization: Option<f64>,
    pub vram_used: Option<u64>,
    pub temperature: Option<f64>,
    pub power_w: Option<f64>,
    pub disk_read_mib_s: Option<f64>,
    pub disk_write_mib_s: Option<f64>,
}
fn max_f(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
fn max_u(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
impl Peaks {
    pub fn fold(&mut self, s: &Sample) {
        self.samples = self.samples.saturating_add(1);
        self.cpu_percent = max_f(self.cpu_percent, s.cpu.as_ref().map(|c| c.percent));
        self.ram_used = max_u(self.ram_used, s.ram.as_ref().map(|m| m.used));
        // The first GPU: `CUDA_VISIBLE_DEVICES` pins the owned backend to one device.
        let gpu = s.gpus.first();
        self.gpu_utilization = max_f(self.gpu_utilization, gpu.and_then(|g| g.utilization));
        self.vram_used = max_u(self.vram_used, gpu.and_then(|g| g.memory_used));
        self.temperature = max_f(self.temperature, gpu.and_then(|g| g.temperature));
        self.power_w = max_f(self.power_w, gpu.and_then(|g| g.power_w));
        self.disk_read_mib_s = max_f(self.disk_read_mib_s, s.disk_io.map(|d| d.read_mib_s));
        self.disk_write_mib_s = max_f(self.disk_write_mib_s, s.disk_io.map(|d| d.write_mib_s));
    }
}
const PEAK_JOBS: usize = 64;
/// Folds one sample into every listed job's peaks; the map is bounded to the newest jobs.
pub fn record_peaks(job_ids: &[String], s: &Sample) {
    let mut guard = PEAKS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(Default::default);
    for id in job_ids {
        map.entry(id.clone()).or_default().fold(s);
    }
    if map.len() > PEAK_JOBS {
        let keep: std::collections::HashSet<_> = job_ids.iter().cloned().collect();
        map.retain(|id, _| keep.contains(id));
    }
}
/// Removes and returns a job's peaks once it reached a terminal state.
pub fn take_peaks(job_id: &str) -> Option<Peaks> {
    PEAKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()?
        .remove(job_id)
}
/// Samples once per second while any job is queued/running (host + GPU), nothing otherwise.
pub fn spawn_job_sampler(jobs: crate::jobs::Jobs) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let ids = jobs.active_job_ids();
            if ids.is_empty() {
                continue;
            }
            let s = sample(None).await;
            record_peaks(&ids, &s);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_smi_rows_and_unavailable_fields() {
        let rows = parse_gpus(
            "0, NVIDIA GeForce RTX 5090, 3, 31355, 32607, 52, 38.64, 420.00\n1, Some GPU, [N/A], 10, 100, [N/A], [N/A], [N/A]\nbroken line\n",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "NVIDIA GeForce RTX 5090");
        assert_eq!(rows[0].utilization, Some(3.0));
        assert_eq!(rows[0].memory_used, Some(31355 * 1024 * 1024));
        assert_eq!(rows[0].power_w, Some(38.64));
        assert_eq!(rows[0].power_limit_w, Some(420.0));
        assert_eq!(rows[1].utilization, None);
        assert_eq!(rows[1].temperature, None);
    }

    #[test]
    fn cpu_percent_is_a_delta_clamped_to_range() {
        *CPU_TIMES.lock().unwrap() = None;
        assert_eq!(cpu_percent(100, 200), 0.0);
        // +100 total, +25 idle → 75 % busy
        assert_eq!(cpu_percent(125, 300), 75.0);
        // no progress → 0, never NaN
        assert_eq!(cpu_percent(125, 300), 0.0);
    }

    #[test]
    fn diskstats_counts_whole_devices_once_and_rates_are_a_delta() {
        let text = "   7 0 loop0 1 0 999 0 0 0 999 0 0 0 0 0 0 0 0\n 259 0 nvme0n1 10 0 1000 5 20 0 2000 7 0 1 1 0 0 0 0\n 259 1 nvme0n1p1 10 0 500 5 20 0 500 7 0 1 1 0 0 0 0\n   8 0 sda 1 0 100 0 1 0 200 0 0 0 0 0 0 0 0\n   8 1 sda1 1 0 50 0 1 0 50 0 0 0 0 0 0 0 0\n 253 0 dm-0 1 0 777 0 1 0 777 0 0 0 0 0 0 0 0\n";
        assert_eq!(parse_diskstats(text), (1100, 2200));
        let t0 = std::time::Instant::now();
        let t1 = t0 + Duration::from_secs(2);
        // 4096 sectors × 512 B = 2 MiB over 2 s → 1 MiB/s; a counter reset never goes negative.
        let rate = disk_io_between((t0, 1100, 2200), (t1, 1100 + 4096, 2000)).unwrap();
        assert_eq!((rate.read_mib_s, rate.write_mib_s), (1.0, 0.0));
        assert_eq!(disk_io_between((t1, 0, 0), (t0, 1, 1)), None);
    }

    #[test]
    fn peaks_keep_the_maximum_per_job_and_are_taken_once() {
        let gpu = |u: f64, mem: u64| Gpu {
            index: 0,
            name: "g".into(),
            utilization: Some(u),
            memory_used: Some(mem),
            memory_total: Some(100),
            temperature: None,
            power_w: Some(100.0),
            power_limit_w: None,
        };
        let a = Sample {
            cpu: Some(Cpu {
                percent: 10.0,
                count: 1,
            }),
            ram: Some(Memory { used: 5, total: 10 }),
            gpus: vec![gpu(30.0, 40)],
            disk_io: Some(DiskIo {
                read_mib_s: 1.0,
                write_mib_s: 9.0,
            }),
            ..Default::default()
        };
        let b = Sample {
            cpu: Some(Cpu {
                percent: 80.0,
                count: 1,
            }),
            ram: Some(Memory { used: 3, total: 10 }),
            gpus: vec![gpu(99.0, 20)],
            disk_io: None,
            ..Default::default()
        };
        let ids = vec!["job-a".to_string()];
        record_peaks(&ids, &a);
        record_peaks(&ids, &b);
        let peaks = take_peaks("job-a").expect("recorded");
        assert_eq!(peaks.samples, 2);
        assert_eq!(peaks.cpu_percent, Some(80.0));
        assert_eq!(peaks.ram_used, Some(5));
        assert_eq!(peaks.gpu_utilization, Some(99.0));
        assert_eq!(peaks.vram_used, Some(40));
        assert_eq!(peaks.power_w, Some(100.0));
        assert_eq!(peaks.disk_write_mib_s, Some(9.0));
        assert_eq!(take_peaks("job-a"), None);
        assert_eq!(take_peaks("never"), None);
    }

    #[tokio::test]
    #[ignore = "Prints a live host sample; run on the target machine (Windows acceptance)"]
    async fn live_sample_dump() {
        // Two samples: CPU percent and disk I/O are deltas.
        let _ = sample(None).await;
        tokio::time::sleep(Duration::from_millis(800)).await;
        let value = sample(Some(std::env::temp_dir().as_path())).await;
        println!("LIVE_SAMPLE {}", serde_json::to_string(&value).unwrap());
    }

    #[tokio::test]
    async fn sample_reports_host_memory_and_a_disk_for_an_absolute_root() {
        let value = sample(Some(std::env::temp_dir().as_path())).await;
        let ram = value.ram.expect("host RAM");
        assert!(ram.total > 0 && ram.used <= ram.total);
        assert_eq!(value.disks.len(), 1);
        assert!(value.disks[0].total >= value.disks[0].used);
        assert_eq!(value.source, "native");
        assert!(sample(Some(std::path::Path::new("relative")))
            .await
            .disks
            .is_empty());
    }
}
