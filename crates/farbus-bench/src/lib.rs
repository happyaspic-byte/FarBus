use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Scenario {
    ControlLatency,
    BulkThroughput,
    Reconnect,
}

#[derive(Parser, Debug)]
#[command(name = "farbus-bench", about = "FarBus benchmark harness")]
pub struct Cli {
    #[arg(long, value_enum, default_value_t = Scenario::ControlLatency)]
    pub scenario: Scenario,
}
