use std::collections::HashMap;
use std::fs;
use std::io;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq)]
struct MemorySnapshot {
    total_kb: u64,
    available_kb: u64,
    used_kb: u64,
    cached_kb: u64,
    buffers_kb: u64,
    swap_total_kb: u64,
    swap_free_kb: u64,
}

impl MemorySnapshot {
    fn from_meminfo(contents: &str) -> io::Result<Self> {
        let mut fields = HashMap::new();

        for line in contents.lines() {
            let mut parts = line.split_whitespace();

            let Some(key) = parts.next() else {
                continue;
            };

            let Some(value) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
                continue;
            };

            fields.insert(key.trim_end_matches(':').to_string(), value);
        }

        let get = |key: &str| fields.get(key).copied();

        let total_kb = get("MemTotal")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing MemTotal"))?;

        let available_kb = get("MemAvailable")
            .or_else(|| get("MemFree"))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Missing MemAvailable and MemFree",
                )
            })?
            .min(total_kb);

        let swap_total_kb = get("SwapTotal").unwrap_or(0);
        let swap_free_kb = get("SwapFree").unwrap_or(0).min(swap_total_kb);

        Ok(Self {
            total_kb,
            available_kb,
            used_kb: total_kb.saturating_sub(available_kb),
            cached_kb: get("Cached").unwrap_or(0),
            buffers_kb: get("Buffers").unwrap_or(0),
            swap_total_kb,
            swap_free_kb,
        })
    }

    fn swap_used_kb(&self) -> u64 {
        self.swap_total_kb.saturating_sub(self.swap_free_kb)
    }

    fn memory_percent(&self) -> f64 {
        percentage(self.used_kb, self.total_kb)
    }

    fn swap_percent(&self) -> f64 {
        percentage(self.swap_used_kb(), self.swap_total_kb)
    }
}

fn percentage(value: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        value as f64 / total as f64 * 100.0
    }
}

fn format_memory(kb: u64) -> String {
    let bytes = kb as f64 * 1024.0;
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    format!("{value:.2} {}", UNITS[unit])
}

fn read_snapshot() -> io::Result<MemorySnapshot> {
    let contents = fs::read_to_string("/proc/meminfo")?;
    MemorySnapshot::from_meminfo(&contents)
}

fn display(snapshot: &MemorySnapshot) {
    println!("========== SysTrace Memory Monitor ==========");
    println!(
        "RAM Usage:      {} / {} ({:.2}%)",
        format_memory(snapshot.used_kb),
        format_memory(snapshot.total_kb),
        snapshot.memory_percent()
    );
    println!("Available:      {}", format_memory(snapshot.available_kb));
    println!("Cached:         {}", format_memory(snapshot.cached_kb));
    println!("Buffers:        {}", format_memory(snapshot.buffers_kb));
    println!(
        "Swap Usage:     {} / {} ({:.2}%)",
        format_memory(snapshot.swap_used_kb()),
        format_memory(snapshot.swap_total_kb),
        snapshot.swap_percent()
    );
    println!("=============================================\n");
}

pub fn monitor(interval: u64) -> io::Result<()> {
    loop {
        let snapshot = read_snapshot()?;
        print!("\x1B[2J\x1B[1;1H");
        display(&snapshot);
        thread::sleep(Duration::from_secs(interval));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
MemTotal:       1000000 kB
MemFree:         200000 kB
MemAvailable:    400000 kB
Buffers:          10000 kB
Cached:          150000 kB
SwapTotal:       500000 kB
SwapFree:        300000 kB
";

    #[test]
    fn calculates_memory_and_swap_usage() {
        let memory = MemorySnapshot::from_meminfo(SAMPLE).unwrap();

        assert_eq!(memory.used_kb, 600000);
        assert_eq!(memory.swap_used_kb(), 200000);
        assert_eq!(memory.memory_percent(), 60.0);
        assert_eq!(memory.swap_percent(), 40.0);
    }

    #[test]
    fn handles_missing_optional_fields() {
        let memory = MemorySnapshot::from_meminfo("MemTotal: 1000 kB\nMemFree: 250 kB\n").unwrap();

        assert_eq!(memory.available_kb, 250);
        assert_eq!(memory.used_kb, 750);
        assert_eq!(memory.cached_kb, 0);
        assert_eq!(memory.swap_total_kb, 0);
    }

    #[test]
    fn clamps_inconsistent_values() {
        let memory = MemorySnapshot::from_meminfo(
            "MemTotal: 1000 kB\nMemAvailable: 2000 kB\nSwapTotal: 100 kB\nSwapFree: 200 kB\n",
        )
        .unwrap();

        assert_eq!(memory.available_kb, 1000);
        assert_eq!(memory.used_kb, 0);
        assert_eq!(memory.swap_free_kb, 100);
    }

    #[test]
    fn rejects_missing_required_fields() {
        assert!(MemorySnapshot::from_meminfo("MemFree: 100 kB\n").is_err());
    }

    #[test]
    fn handles_zero_total_percentage() {
        assert_eq!(percentage(100, 0), 0.0);
    }
}
