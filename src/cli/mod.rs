use clap::Parser;

#[derive(Parser)]
#[command(
    name = "systrace",
    version,
    about = "A lightweight Linux system profiler"
)]
pub struct Cli {
    /// Enable continuous CPU monitoring
    #[arg(long)]
    cpu: bool,

    /// Enable continuous memory monitoring
    #[arg(long)]
    memory: bool,

    /// Enable continuous disk and filesystem monitoring
    #[arg(long)]
    disk: bool,

    /// CPU monitoring refresh interval in seconds
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..))]
    interval: u64,
}

pub fn run() {
    let cli = Cli::parse();

    if !crate::system_metrics::is_linux() {
        eprintln!("SysTrace requires a Linux environment.");
        std::process::exit(1);
    }

    if (cli.cpu as u8 + cli.memory as u8 + cli.disk as u8) > 1 {
        eprintln!("Choose only one of --cpu, --memory, or --disk.");
        std::process::exit(1);
    }

    if cli.disk {
        if let Err(error) = crate::system_metrics::disk::monitor(cli.interval) {
            eprintln!("Disk monitoring failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if cli.memory {
        if let Err(error) = crate::system_metrics::memory::monitor(cli.interval) {
            eprintln!("Memory monitoring failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if cli.cpu {
        if let Err(error) = crate::system_metrics::cpu::monitor(cli.interval) {
            eprintln!("CPU monitoring failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    let info = crate::system_metrics::collect();
    crate::system_metrics::display(&info);
}
