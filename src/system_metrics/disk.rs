use std::collections::HashMap;
use std::fs;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use super::FilesystemInfo;

#[derive(Debug, Clone, Default, PartialEq)]
struct DiskStats {
    reads_completed: u64,
    sectors_read: u64,
    read_time_ms: u64,
    writes_completed: u64,
    sectors_written: u64,
    write_time_ms: u64,
    io_in_progress: u64,
    io_time_ms: u64,
}

#[derive(Debug, Clone)]
struct DiskSnapshot {
    devices: HashMap<String, DiskStats>,
}

#[derive(Debug)]
struct DiskUsage {
    name: String,
    read_ops: u64,
    write_ops: u64,
    read_bytes: u64,
    write_bytes: u64,
    read_bytes_per_sec: f64,
    write_bytes_per_sec: f64,
    utilization_percent: f64,
    io_in_progress: u64,
}

fn parse_diskstats(contents: &str) -> DiskSnapshot {
    let mut devices = HashMap::new();

    for line in contents.lines() {
        let mut parts = line.split_whitespace();

        let (Some(_major), Some(_minor), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };

        if name.starts_with("ram") || name.starts_with("loop") {
            continue;
        }

        let values: Vec<u64> = parts.filter_map(|value| value.parse().ok()).collect();

        if values.len() < 11 {
            continue;
        }

        devices.insert(
            name.to_string(),
            DiskStats {
                reads_completed: values[0],
                sectors_read: values[2],
                read_time_ms: values[3],
                writes_completed: values[4],
                sectors_written: values[6],
                write_time_ms: values[7],
                io_in_progress: values[8],
                io_time_ms: values[9],
            },
        );
    }

    DiskSnapshot { devices }
}

fn read_snapshot() -> io::Result<DiskSnapshot> {
    let contents = fs::read_to_string("/proc/diskstats")?;
    Ok(parse_diskstats(&contents))
}

fn calculate_usage(
    previous: &DiskStats,
    current: &DiskStats,
    elapsed: Duration,
    name: &str,
) -> DiskUsage {
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let elapsed_seconds = elapsed.as_secs_f64();

    let read_sectors = current.sectors_read.saturating_sub(previous.sectors_read);
    let write_sectors = current
        .sectors_written
        .saturating_sub(previous.sectors_written);

    let read_bytes = read_sectors.saturating_mul(512);
    let write_bytes = write_sectors.saturating_mul(512);

    let utilization_percent = if elapsed_ms > 0.0 {
        current.io_time_ms.saturating_sub(previous.io_time_ms) as f64 / elapsed_ms * 100.0
    } else {
        0.0
    };

    DiskUsage {
        name: name.to_string(),
        read_ops: current
            .reads_completed
            .saturating_sub(previous.reads_completed),
        write_ops: current
            .writes_completed
            .saturating_sub(previous.writes_completed),
        read_bytes,
        write_bytes,
        read_bytes_per_sec: if elapsed_seconds > 0.0 {
            read_bytes as f64 / elapsed_seconds
        } else {
            0.0
        },
        write_bytes_per_sec: if elapsed_seconds > 0.0 {
            write_bytes as f64 / elapsed_seconds
        } else {
            0.0
        },
        utilization_percent: utilization_percent.min(100.0),
        io_in_progress: current.io_in_progress,
    }
}

fn format_bytes(bytes: u64) -> String {
    format_rate(bytes as f64)
}

fn format_rate(bytes_per_sec: f64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes_per_sec;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    format!("{value:.2} {}", UNITS[unit])
}

fn display_filesystems(filesystems: &[FilesystemInfo]) {
    println!("\n========== Filesystem Capacity ==========");
    println!(
        "{:<20} {:<12} {:>12} {:>12} {:>8}",
        "MOUNT", "TYPE", "TOTAL", "AVAILABLE", "USED"
    );

    for filesystem in filesystems {
        let used = filesystem
            .total_bytes
            .saturating_sub(filesystem.available_bytes);
        let used_percent = if filesystem.total_bytes > 0 {
            used as f64 / filesystem.total_bytes as f64 * 100.0
        } else {
            0.0
        };

        println!(
            "{:<20} {:<12} {:>12} {:>12} {:>7.2}%",
            filesystem.mount_point,
            filesystem.filesystem_type,
            format_bytes(filesystem.total_bytes),
            format_bytes(filesystem.available_bytes),
            used_percent
        );
    }
}

fn display_disks(usages: &[DiskUsage], filesystems: &[FilesystemInfo]) {
    print!("\x1B[2J\x1B[1;1H");
    println!("========== SysTrace Disk Monitor ==========");

    if usages.is_empty() {
        println!("No block-device activity data available yet.");
    } else {
        println!(
            "{:<12} {:>8} {:>8} {:>12} {:>12} {:>9}",
            "DEVICE", "READS", "WRITES", "READ RATE", "WRITE RATE", "UTIL"
        );

        for disk in usages {
            println!(
                "{:<12} {:>8} {:>8} {:>12} {:>12} {:>8.2}%",
                disk.name,
                disk.read_ops,
                disk.write_ops,
                format_rate(disk.read_bytes_per_sec),
                format_rate(disk.write_bytes_per_sec),
                disk.utilization_percent
            );
            println!(
                "  Read: {} | Written: {} | I/O in progress: {}",
                format_bytes(disk.read_bytes),
                format_bytes(disk.write_bytes),
                disk.io_in_progress
            );
        }
    }

    display_filesystems(filesystems);
    println!("\nPress Ctrl+C to stop.");
}

pub fn monitor(interval: u64) -> io::Result<()> {
    let mut previous = read_snapshot()?;

    if previous.devices.is_empty() {
        return Err(io::Error::other("No block-device statistics found"));
    }

    println!("SysTrace Disk Monitor | Refresh: {interval}s");
    println!("Collecting initial disk I/O counters...");

    let mut previous_time = Instant::now();

    loop {
        thread::sleep(Duration::from_secs(interval));

        let current = read_snapshot()?;
        let current_time = Instant::now();
        let elapsed = current_time.duration_since(previous_time);

        let mut usages = Vec::new();

        for (name, stats) in &current.devices {
            if let Some(old_stats) = previous.devices.get(name) {
                usages.push(calculate_usage(old_stats, stats, elapsed, name));
            }
        }

        let filesystems = super::collect_filesystems();
        display_disks(&usages, &filesystems);

        previous = current;
        previous_time = current_time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_disk_statistics() {
        let input = "\
8 0 sda 100 5 2000 30 50 2 4000 60 0 90 100
8 16 sdb 10 0 500 5 20 0 1000 10 1 20 25
1 0 ram0 0 0 0 0 0 0 0 0 0 0 0";

        let snapshot = parse_diskstats(input);

        assert_eq!(snapshot.devices.len(), 2);

        let disk = &snapshot.devices["sda"];
        assert_eq!(disk.reads_completed, 100);
        assert_eq!(disk.sectors_read, 2000);
        assert_eq!(disk.writes_completed, 50);
        assert_eq!(disk.sectors_written, 4000);
        assert_eq!(disk.io_time_ms, 90);
    }

    #[test]
    fn calculates_disk_io_rates_and_utilization() {
        let previous = DiskStats::default();
        let current = DiskStats {
            reads_completed: 10,
            sectors_read: 200,
            writes_completed: 5,
            sectors_written: 100,
            io_time_ms: 500,
            ..Default::default()
        };

        let usage = calculate_usage(&previous, &current, Duration::from_secs(1), "sda");

        assert_eq!(usage.read_ops, 10);
        assert_eq!(usage.write_ops, 5);
        assert_eq!(usage.read_bytes, 102400);
        assert_eq!(usage.write_bytes, 51200);
        assert_eq!(usage.read_bytes_per_sec, 102400.0);
        assert_eq!(usage.write_bytes_per_sec, 51200.0);
        assert_eq!(usage.utilization_percent, 50.0);
    }

    #[test]
    fn handles_counter_resets_safely() {
        let previous = DiskStats {
            sectors_read: 100,
            io_time_ms: 100,
            ..Default::default()
        };

        let current = DiskStats::default();
        let usage = calculate_usage(&previous, &current, Duration::from_secs(1), "sda");

        assert_eq!(usage.read_bytes, 0);
        assert_eq!(usage.utilization_percent, 0.0);
    }

    #[test]
    fn ignores_malformed_and_virtual_devices() {
        let snapshot = parse_diskstats(
            "invalid line\n1 0 ram0 0 0 0 0 0 0 0 0 0 0 0\n8 0 sda 1 0 2 3 4 0 5 6 0 7 8",
        );

        assert_eq!(snapshot.devices.len(), 1);
        assert!(snapshot.devices.contains_key("sda"));
    }
}
