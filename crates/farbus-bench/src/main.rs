use clap::Parser;
use farbus_bench::Cli;

fn main() {
    let cli = Cli::parse();
    println!("Benchmark {:?} arrives in M4.", cli.scenario);
}
