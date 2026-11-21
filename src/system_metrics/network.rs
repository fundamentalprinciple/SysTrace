use std::collections::HashMap;
use std::fs;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
struct InterfaceStats {
    rx_bytes: u64,
    rx_packets: u64,
    tx_bytes: u64,
    tx_packets: u64,
}

type Snapshot = HashMap<String, InterfaceStats>;

#[derive(Debug)]
struct InterfaceUsage {
    rx_bytes: u64,
    rx_packets: u64,
    tx_bytes: u64,
    tx_packets: u64,
    rx_rate: f64,
    tx_rate: f64,
}

fn parse_net_dev(content: &str) -> Snapshot {
    content
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (name, stats) = line.split_once(':')?;
            let values: Vec<u64> = stats
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;

            if values.len() < 16 {
                return None;
            }

            Some((
                name.trim().to_string(),
                InterfaceStats {
                    rx_bytes: values[0],
                    rx_packets: values[1],
                    tx_bytes: values[8],
                    tx_packets: values[9],
                },
            ))
        })
        .collect()
}

fn read_snapshot() -> io::Result<Snapshot> {
    let content = fs::read_to_string("/proc/net/dev")?;
    Ok(parse_net_dev(&content))
}

fn calculate_usage(
    previous: &InterfaceStats,
    current: &InterfaceStats,
    elapsed: f64,
) -> InterfaceUsage {
    let elapsed = elapsed.max(f64::EPSILON);
    let rx_bytes = current.rx_bytes.saturating_sub(previous.rx_bytes);
    let tx_bytes = current.tx_bytes.saturating_sub(previous.tx_bytes);

    InterfaceUsage {
        rx_bytes,
        rx_packets: current.rx_packets.saturating_sub(previous.rx_packets),
        tx_bytes,
        tx_packets: current.tx_packets.saturating_sub(previous.tx_packets),
        rx_rate: rx_bytes as f64 / elapsed,
        tx_rate: tx_bytes as f64 / elapsed,
    }
}

fn format_rate(bytes_per_second: f64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];

    let mut value = bytes_per_second;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    format!("{value:.2} {}", UNITS[unit])
}

pub fn monitor(
    interval: u64,
    interface_filter: Option<&str>,
    include_loopback: bool,
) -> io::Result<()> {
    let mut previous = read_snapshot()?;

    if let Some(name) = interface_filter {
        if !previous.contains_key(name) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Network interface '{name}' was not found"),
            ));
        }
    }

    println!("SysTrace Network Monitor | Refresh: {interval}s");
    println!("Monitoring network interfaces...");

    loop {
        let start = Instant::now();
        thread::sleep(Duration::from_secs(interval));
        let current = read_snapshot()?;
        let elapsed = start.elapsed().as_secs_f64();

        println!("\n========== Network Traffic Monitor ==========");
        println!(
            "{:<16} {:>12} {:>12} {:>14} {:>14}",
            "INTERFACE", "RX PACKETS", "TX PACKETS", "RX RATE", "TX RATE"
        );

        let mut names: Vec<_> = current.keys().collect();
        names.sort();

        for name in names {
            if !include_loopback && name == "lo" {
                continue;
            }

            if interface_filter.is_some_and(|filter| filter != name) {
                continue;
            }

            let stats = &current[name];
            let Some(old) = previous.get(name) else {
                continue;
            };

            let usage = calculate_usage(old, stats, elapsed);

            println!(
                "{:<16} {:>12} {:>12} {:>14} {:>14}",
                name,
                usage.rx_packets,
                usage.tx_packets,
                format_rate(usage.rx_rate),
                format_rate(usage.tx_rate),
            );
            println!(
                "  Received: {} bytes | Sent: {} bytes",
                usage.rx_bytes, usage.tx_bytes
            );
        }

        println!("\nPress Ctrl+C to stop.");
        previous = current;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Inter-| Receive | Transmit
 face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed
    lo: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0
  eth0: 5000 50 0 0 0 0 0 5 9000 90 0 0 0 0 0 0
";

    #[test]
    fn parses_interface_statistics() {
        let snapshot = parse_net_dev(SAMPLE);
        let eth0 = &snapshot["eth0"];

        assert_eq!(eth0.rx_bytes, 5000);
        assert_eq!(eth0.rx_packets, 50);
        assert_eq!(eth0.tx_bytes, 9000);
        assert_eq!(eth0.tx_packets, 90);
    }

    #[test]
    fn calculates_network_throughput() {
        let previous = InterfaceStats {
            rx_bytes: 1000,
            rx_packets: 10,
            tx_bytes: 2000,
            tx_packets: 20,
        };
        let current = InterfaceStats {
            rx_bytes: 3000,
            rx_packets: 30,
            tx_bytes: 6000,
            tx_packets: 60,
        };

        let usage = calculate_usage(&previous, &current, 2.0);

        assert_eq!(usage.rx_bytes, 2000);
        assert_eq!(usage.tx_bytes, 4000);
        assert_eq!(usage.rx_packets, 20);
        assert_eq!(usage.tx_packets, 40);
        assert_eq!(usage.rx_rate, 1000.0);
        assert_eq!(usage.tx_rate, 2000.0);
    }

    #[test]
    fn handles_counter_resets() {
        let previous = InterfaceStats {
            rx_bytes: 5000,
            rx_packets: 50,
            tx_bytes: 9000,
            tx_packets: 90,
        };
        let current = InterfaceStats {
            rx_bytes: 100,
            rx_packets: 1,
            tx_bytes: 200,
            tx_packets: 2,
        };

        let usage = calculate_usage(&previous, &current, 1.0);

        assert_eq!(usage.rx_bytes, 0);
        assert_eq!(usage.tx_bytes, 0);
        assert_eq!(usage.rx_packets, 0);
        assert_eq!(usage.tx_packets, 0);
    }

    #[test]
    fn parses_multiple_interfaces() {
        let snapshot = parse_net_dev(SAMPLE);

        assert_eq!(snapshot.len(), 2);
        assert!(snapshot.contains_key("lo"));
        assert!(snapshot.contains_key("eth0"));
    }
}
