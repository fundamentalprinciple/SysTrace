use std::fs;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
struct CpuTimes {
    user: u64,
    system: u64,
    idle: u64,
    other: u64,
}

impl CpuTimes {
    fn total(&self) -> u64 {
        self.user + self.system + self.idle + self.other
    }
}

#[derive(Debug)]
pub struct CpuUsage {
    pub name: String,
    pub user: f64,
    pub system: f64,
    pub idle: f64,
    pub other: f64,
    pub total: f64,
}

fn parse_cpu_stat(contents: &str) -> Vec<(String, CpuTimes)> {
    contents
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();

            if name != "cpu" && !name.starts_with("cpu") {
                return None;
            }

            let values: Vec<u64> = parts
                .take(8)
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;

            if values.len() < 4 {
                return None;
            }

            Some((
                name,
                CpuTimes {
                    user: values[0] + values[1],
                    system: values[2] + values[5] + values[6],
                    idle: values[3] + values[4],
                    other: values.get(7).copied().unwrap_or(0),
                },
            ))
        })
        .collect()
}

fn calculate_usage(previous: &CpuTimes, current: &CpuTimes) -> Option<CpuUsage> {
    let total_delta = current.total().checked_sub(previous.total())?;

    if total_delta == 0 {
        return None;
    }

    let user = current.user.saturating_sub(previous.user) as f64;
    let system = current.system.saturating_sub(previous.system) as f64;
    let idle = current.idle.saturating_sub(previous.idle) as f64;
    let other = current.other.saturating_sub(previous.other) as f64;

    let total = total_delta as f64;

    Some(CpuUsage {
        name: String::new(),
        user: user / total * 100.0,
        system: system / total * 100.0,
        idle: idle / total * 100.0,
        other: other / total * 100.0,
        total: (user + system + other) / total * 100.0,
    })
}

fn read_cpu_stats() -> std::io::Result<Vec<(String, CpuTimes)>> {
    let contents = fs::read_to_string("/proc/stat")?;
    Ok(parse_cpu_stat(&contents))
}

fn display_usage(usages: &[CpuUsage]) {
    println!("\n========== CPU Usage ==========");

    println!(
        "{:<8} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "CPU", "USER", "SYSTEM", "IDLE", "OTHER", "TOTAL"
    );

    for cpu in usages {
        println!(
            "{:<8} {:>8.2}% {:>8.2}% {:>8.2}% {:>8.2}% {:>8.2}%",
            cpu.name, cpu.user, cpu.system, cpu.idle, cpu.other, cpu.total
        );
    }

    println!("===============================");
}

pub fn monitor(interval: u64) -> std::io::Result<()> {
    let mut previous = read_cpu_stats()?;

    if previous.is_empty() {
        return Err(std::io::Error::other("No CPU statistics found"));
    }

    println!("SysTrace CPU Monitor | Refresh: {interval}s");
    println!("Press Ctrl+C to stop.");

    loop {
        thread::sleep(Duration::from_secs(interval));

        let current = read_cpu_stats()?;
        let mut usages = Vec::new();

        for (name, times) in &current {
            if let Some((_, old_times)) = previous.iter().find(|(old_name, _)| old_name == name) {
                if let Some(mut usage) = calculate_usage(old_times, times) {
                    usage.name = name.clone();
                    usages.push(usage);
                }
            }
        }

        display_usage(&usages);
        previous = current;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aggregate_and_per_core_stats() {
        let input = "\
cpu 100 10 50 200 5 2 3 1
cpu0 50 5 25 100 2 1 1 0
intr 1000 0 0";

        let parsed = parse_cpu_stat(input);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "cpu");
        assert_eq!(parsed[1].0, "cpu0");
        assert_eq!(parsed[0].1.user, 110);
    }

    #[test]
    fn calculates_cpu_utilization() {
        let previous = CpuTimes {
            user: 100,
            system: 50,
            idle: 800,
            other: 50,
        };

        let current = CpuTimes {
            user: 150,
            system: 100,
            idle: 900,
            other: 50,
        };

        let usage = calculate_usage(&previous, &current).unwrap();

        assert!((usage.total - 50.0).abs() < 0.01);
        assert!((usage.user - 25.0).abs() < 0.01);
        assert!((usage.system - 25.0).abs() < 0.01);
        assert!((usage.idle - 50.0).abs() < 0.01);
    }
}
