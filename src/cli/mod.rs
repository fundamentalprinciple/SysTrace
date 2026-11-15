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
    println!("SysTrace initialized.");
}
