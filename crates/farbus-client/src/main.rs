use clap::Parser;
use farbus_client::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Discover => println!("No servers found. Discovery arrives in M1."),
        Command::Pair { fingerprint } => {
            println!("Pairing with {fingerprint}. Secure PIN input arrives in M1.");
        }
        Command::Devices { fingerprint } => {
            println!("Listing devices on {fingerprint}. Linux export arrives in M2.");
        }
        Command::Attach {
            fingerprint,
            device_id,
        } => {
            println!(
                "Attaching device {device_id} on {fingerprint}. Windows attach arrives in M3."
            );
        }
        Command::Detach {
            fingerprint,
            device_id,
        } => {
            println!(
                "Detaching device {device_id} on {fingerprint}. Windows attach arrives in M3."
            );
        }
    }
}
