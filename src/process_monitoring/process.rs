use std::collections::HashMap;
use std::fs;
use std::io;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
struct ProcessInfo {
    pid: u32,
    name: String,
    state: char,
    parent_pid: u32,
    cpu_ticks: u64,
    resident_bytes: u64,
    command: String,
}

#[derive(Clone, Copy)]
enum SortBy {
    Cpu,
    Memory,
}

fn parse_sort(value: &str) -> io::Result<SortBy> {
    match value {
        "cpu" => Ok(SortBy::Cpu),
        "memory" => Ok(SortBy::Memory),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "sort must be either 'cpu' or 'memory'",
        )),
    }
}

fn total_cpu_ticks() -> io::Result<u64> {
    let contents = fs::read_to_string("/proc/stat")?;
    let line = contents
        .lines()
        .next()
        .ok_or_else(|| io::Error::other("missing aggregate CPU statistics"))?;

    line.split_whitespace()
        .skip(1)
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        })
        .try_fold(0_u64, |total, value| {
            value.map(|ticks| total.saturating_add(ticks))
        })
}

fn total_memory_bytes() -> io::Result<u64> {
    let contents = fs::read_to_string("/proc/meminfo")?;
    let line = contents
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .ok_or_else(|| io::Error::other("MemTotal not found in /proc/meminfo"))?;

    let kib = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::other("invalid MemTotal value"))?
        .parse::<u64>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    Ok(kib.saturating_mul(1024))
}

fn read_process(pid: u32, page_size: u64) -> Option<ProcessInfo> {
    let directory = format!("/proc/{pid}");
    let stat = fs::read_to_string(format!("{directory}/stat")).ok()?;

    // The comm field is parenthesized and may itself contain spaces or ')'.
    let closing_paren = stat.rfind(')')?;
    let name = stat.get(stat.find('(')? + 1..closing_paren)?.to_string();
    let fields: Vec<&str> = stat.get(closing_paren + 1..)?.split_whitespace().collect();

    let state = fields.first()?.chars().next()?;
    let parent_pid = fields.get(1)?.parse().ok()?;
    let user_ticks: u64 = fields.get(11)?.parse().ok()?;
    let system_ticks: u64 = fields.get(12)?.parse().ok()?;

    let statm = fs::read_to_string(format!("{directory}/statm")).ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;

    let command_line = fs::read(format!("{directory}/cmdline"))
        .ok()
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .replace('\0', " ")
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    let executable = fs::read_link(format!("{directory}/exe"))
        .ok()
        .map(|path| path.to_string_lossy().into_owned());

    let command = if !command_line.is_empty() {
        command_line
    } else {
        executable.unwrap_or_else(|| name.clone())
    };

    Some(ProcessInfo {
        pid,
        name,
        state,
        parent_pid,
        cpu_ticks: user_ticks.saturating_add(system_ticks),
        resident_bytes: resident_pages.saturating_mul(page_size),
        command,
    })
}

fn collect_processes(page_size: u64) -> HashMap<u32, ProcessInfo> {
    let mut processes = HashMap::new();

    let Ok(entries) = fs::read_dir("/proc") else {
        return processes;
    };

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };

        if let Some(process) = read_process(pid, page_size) {
            processes.insert(pid, process);
        }
    }

    processes
}

fn matches_filter(process: &ProcessInfo, filter: Option<&str>) -> bool {
    filter.is_none_or(|filter| process.name.to_lowercase().contains(&filter.to_lowercase()))
}

fn display_processes(
    previous: &HashMap<u32, ProcessInfo>,
    current: &HashMap<u32, ProcessInfo>,
    total_tick_delta: u64,
    total_memory: u64,
    cpu_count: usize,
    sort_by: SortBy,
    filter: Option<&str>,
) {
    let mut rows: Vec<(&ProcessInfo, f64, f64)> = current
        .values()
        .filter(|process| matches_filter(process, filter))
        .map(|process| {
            let cpu_percent = previous
                .get(&process.pid)
                .map(|old| {
                    let delta = process.cpu_ticks.saturating_sub(old.cpu_ticks);
                    if total_tick_delta == 0 {
                        0.0
                    } else {
                        delta as f64 / total_tick_delta as f64 * cpu_count as f64 * 100.0
                    }
                })
                .unwrap_or(0.0);

            let memory_percent = if total_memory == 0 {
                0.0
            } else {
                process.resident_bytes as f64 / total_memory as f64 * 100.0
            };

            (process, cpu_percent, memory_percent)
        })
        .collect();

    match sort_by {
        SortBy::Cpu => rows.sort_by(|a, b| b.1.total_cmp(&a.1)),
        SortBy::Memory => rows.sort_by(|a, b| b.2.total_cmp(&a.2)),
    }

    println!(
        "{:<8} {:<22} {:<6} {:>7} {:>7} {:<8} COMMAND",
        "PID", "NAME", "STATE", "CPU%", "MEM%", "PPID"
    );

    for (process, cpu, memory) in rows.iter().take(30) {
        let command: String = process.command.chars().take(65).collect();

        println!(
            "{:<8} {:<22} {:<6} {:>6.2}% {:>6.2}% {:<8} {}",
            process.pid,
            process.name.chars().take(21).collect::<String>(),
            process.state,
            cpu,
            memory,
            process.parent_pid,
            command
        );
    }

    println!(
        "\nProcesses: {} | Showing: {} | Sorted by: {}",
        current.len(),
        rows.len().min(30),
        match sort_by {
            SortBy::Cpu => "CPU",
            SortBy::Memory => "memory",
        }
    );
}

pub fn monitor(interval: u64, sort: &str, filter: Option<&str>) -> io::Result<()> {
    let sort_by = parse_sort(sort)?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };

    if page_size <= 0 {
        return Err(io::Error::other("could not determine system page size"));
    }

    let page_size = page_size as u64;
    let total_memory = total_memory_bytes()?;
    let cpu_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);

    let mut previous = collect_processes(page_size);
    let mut previous_total_ticks = total_cpu_ticks()?;

    loop {
        thread::sleep(Duration::from_secs(interval));

        let current = collect_processes(page_size);
        let current_total_ticks = total_cpu_ticks()?;
        let total_tick_delta = current_total_ticks.saturating_sub(previous_total_ticks);

        print!("\x1B[2J\x1B[H");
        println!("SysTrace Process Monitor | Refresh: {interval}s");
        println!("Press Ctrl+C to stop\n");

        display_processes(
            &previous,
            &current,
            total_tick_delta,
            total_memory,
            cpu_count,
            sort_by,
            filter,
        );

        previous = current;
        previous_total_ticks = current_total_ticks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sort_modes() {
        assert!(matches!(parse_sort("cpu"), Ok(SortBy::Cpu)));
        assert!(matches!(parse_sort("memory"), Ok(SortBy::Memory)));
        assert!(parse_sort("invalid").is_err());
    }

    #[test]
    fn process_filter_is_case_insensitive() {
        let process = ProcessInfo {
            pid: 1,
            name: "PythonWorker".to_string(),
            state: 'S',
            parent_pid: 0,
            cpu_ticks: 0,
            resident_bytes: 0,
            command: "python worker.py".to_string(),
        };

        assert!(matches_filter(&process, Some("python")));
        assert!(matches_filter(&process, Some("WORKER")));
        assert!(!matches_filter(&process, Some("nginx")));
        assert!(matches_filter(&process, None));
    }
}
