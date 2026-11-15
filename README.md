# SysTrace

A lightweight Linux CLI system profiler built with Rust.

## Features

- CPU utilization monitoring
- Memory usage monitoring
- Disk and filesystem statistics
- Network traffic monitoring
- Process monitoring
- Resource threshold alerts
- JSON diagnostic reports

## Requirements

- Linux kernel 5.4 or newer (initial support target)
- Rust 1.85 or newer
- Cargo

## Installation

```bash
git clone https://github.com/fundamentalprinciple/SysTrace.git
cd SysTrace
cargo build --release
```

## Development

```bash
cargo run
cargo check
cargo fmt --check
cargo test
```

## Project Structure

```text
src/
├── main.rs
├── cli/
├── system_metrics/
├── process_monitoring/
└── reports/
```

## License

MIT
