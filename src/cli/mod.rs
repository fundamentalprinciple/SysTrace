use clap::Parser;

#[derive(Parser)]
#[command(
    name = "systrace",
    version,
    about = "A lightweight Linux system profiler"
)]
pub struct Cli;

pub fn run() {
    let _cli = Cli::parse();

    if !crate::system_metrics::is_linux() {
        eprintln!("SysTrace requires a Linux environment.");
        std::process::exit(1);
    }

    let info = crate::system_metrics::collect();
    crate::system_metrics::display(&info);
}
