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
    assert_eq!(cli.depth, 64);
    assert!(!cli.json);
}

#[test]
fn parses_pipeline_depth_and_json_output() {
    let cli = Cli::try_parse_from([
        "farbus-bench",
        "--scenario",
        "bulk-throughput",
        "--depth",
        "32",
        "--json",
    ])
    .unwrap();
    assert_eq!(cli.depth, 32);
    assert!(cli.json);
}

#[test]
fn rejects_zero_pipeline_depth() {
    assert!(Cli::try_parse_from([
        "farbus-bench",
        "--scenario",
        "bulk-throughput",
        "--depth",
        "0",
    ])
    .is_err());
}
