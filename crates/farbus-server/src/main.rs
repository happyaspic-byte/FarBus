use clap::Parser;
use farbus_server::Cli;

fn main() {
    let cli = Cli::parse();
    println!(
        "FarBus M0 foundation ready. Secure listener on {} arrives in M1.",
        cli.listen
    );
}
