pub mod cpu;
pub mod disk;
pub mod memory;

use std::ffi::CString;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct MemoryInfo {
    pub total_kb: Option<u64>,
    pub free_kb: Option<u64>,
    pub available_kb: Option<u64>,
    pub buffers_kb: Option<u64>,
    pub cached_kb: Option<u64>,
    pub swap_total_kb: Option<u64>,
    pub swap_free_kb: Option<u64>,
}

#[derive(Debug)]
pub struct FilesystemInfo {
    pub filesystem: String,
    pub filesystem_type: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug)]
pub struct SystemInfo {
    pub hostname: Option<String>,
    pub kernel_version: Option<String>,
    pub architecture: String,
    pub uptime_seconds: Option<f64>,
    pub memory: MemoryInfo,
    pub filesystems: Vec<FilesystemInfo>,
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_memory_field(contents: &str, field: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let mut parts = line.split_whitespace();

        if parts.next()? == format!("{field}:") {
            parts.next()?.parse().ok()
        } else {
            None
        }
    })
}

fn collect_memory() -> MemoryInfo {
    let contents = fs::read_to_string("/proc/meminfo").unwrap_or_default();

    MemoryInfo {
        total_kb: read_memory_field(&contents, "MemTotal"),
        free_kb: read_memory_field(&contents, "MemFree"),
        available_kb: read_memory_field(&contents, "MemAvailable"),
        buffers_kb: read_memory_field(&contents, "Buffers"),
        cached_kb: read_memory_field(&contents, "Cached"),
        swap_total_kb: read_memory_field(&contents, "SwapTotal"),
        swap_free_kb: read_memory_field(&contents, "SwapFree"),
    }
}

fn collect_uptime() -> Option<f64> {
    let contents = fs::read_to_string("/proc/uptime").ok()?;
    contents.split_whitespace().next()?.parse().ok()
}

fn collect_filesystems() -> Vec<FilesystemInfo> {
    let contents = match fs::read_to_string("/proc/self/mounts") {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    contents
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();

            let device = parts.next()?.to_string();
            let mount_point = parts.next()?.to_string();
            let filesystem_type = parts.next()?.to_string();

            if mount_point == "/snap" || mount_point.starts_with("/snap/") {
                return None;
            }

            const VIRTUAL_FS: &[&str] = &[
                "proc",
                "sysfs",
                "devpts",
                "devtmpfs",
                "tmpfs",
                "cgroup",
                "cgroup2",
                "overlay",
                "squashfs",
                "mqueue",
                "hugetlbfs",
                "tracefs",
                "debugfs",
                "configfs",
                "fusectl",
                "nsfs",
                "securityfs",
                "autofs",
                "binfmt_misc",
            ];

            if VIRTUAL_FS.contains(&filesystem_type.as_str()) {
                return None;
            }

            let path = CString::new(mount_point.as_bytes()).ok()?;
            let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();

            let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };

            if result != 0 {
                return None;
            }

            let stats = unsafe { stats.assume_init() };

            let block_size = stats.f_frsize as u64;

            Some(FilesystemInfo {
                filesystem: device,
                filesystem_type,
                mount_point,
                total_bytes: stats.f_blocks as u64 * block_size,
                available_bytes: stats.f_bavail as u64 * block_size,
            })
        })
        .collect()
}

pub fn collect() -> SystemInfo {
    SystemInfo {
        hostname: read_trimmed("/proc/sys/kernel/hostname")
            .or_else(|| read_trimmed("/etc/hostname")),

        kernel_version: read_trimmed("/proc/sys/kernel/osrelease"),

        architecture: std::env::consts::ARCH.to_string(),

        uptime_seconds: collect_uptime(),

        memory: collect_memory(),

        filesystems: collect_filesystems(),
    }
}

pub fn display(info: &SystemInfo) {
    println!("========== SysTrace System Information ==========");

    println!("Hostname:       {:?}", info.hostname);
    println!("Kernel:         {:?}", info.kernel_version);
    println!("Architecture:   {}", info.architecture);
    println!("Uptime:         {:?} seconds", info.uptime_seconds);

    println!("\n========== Memory ==========");
    println!("Total:          {:?} KB", info.memory.total_kb);
    println!("Free:           {:?} KB", info.memory.free_kb);
    println!("Available:      {:?} KB", info.memory.available_kb);
    println!("Buffers:        {:?} KB", info.memory.buffers_kb);
    println!("Cached:         {:?} KB", info.memory.cached_kb);
    println!("Swap Total:     {:?} KB", info.memory.swap_total_kb);
    println!("Swap Free:      {:?} KB", info.memory.swap_free_kb);

    println!("\n========== Filesystems ==========");

    for fs in &info.filesystems {
        println!(
            "{} ({}) -> {} | Total: {} MB | Available: {} MB",
            fs.filesystem,
            fs.filesystem_type,
            fs.mount_point,
            fs.total_bytes / 1_048_576,
            fs.available_bytes / 1_048_576
        );
    }

    println!("===============================================");
}

pub fn is_linux() -> bool {
    Path::new("/proc").exists() && Path::new("/sys").exists()
}
