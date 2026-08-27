use clap::Parser;
use farbus_bench::{Cli, Scenario};

#[test]
fn defaults_to_control_latency_scenario() {
    let cli = Cli::try_parse_from(["farbus-bench"]).unwrap();
    assert_eq!(cli.scenario, Scenario::ControlLatency);
}

#[test]
fn parses_bulk_throughput_scenario() {
    let cli = Cli::try_parse_from(["farbus-bench", "--scenario", "bulk-throughput"]).unwrap();
    assert_eq!(cli.scenario, Scenario::BulkThroughput);
}
