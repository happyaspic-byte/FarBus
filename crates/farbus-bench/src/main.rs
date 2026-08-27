use clap::Parser;
use farbus_bench::{Cli, Scenario};
use farbus_core::{
    make_pinned_client_config, make_self_signed, make_server_config, read_message, serve_session,
    simulated_lab_devices, write_message, DeviceId, Identity, Message, ServerState,
};
use farbus_protocol::{TransferType, UrbSubmit, VERSION};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    println!("=== FarBus Benchmark: {:?} ===", cli.scenario);

    let (certs, key, server_fp) = make_self_signed("farbus.bench")?;
    let acceptor = make_server_config(certs, key)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let state = Arc::new(ServerState::new(
        "farbus-bench".into(),
        server_fp,
        simulated_lab_devices(),
    ));

    let _server = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(stream).await {
                        let _ = serve_session(&mut tls, state).await;
                    }
                });
            }
        }
    });

    let connector = make_pinned_client_config(server_fp)?;
    let stream = TcpStream::connect(addr).await?;
    let server_name = rustls::pki_types::ServerName::try_from("farbus.bench")?;
    let mut client = connector.connect(server_name, stream).await?;

    let id = Identity::generate();
    write_message(
        &mut client,
        &Message::Hello(farbus_protocol::Hello {
            protocol_min: VERSION,
            protocol_max: VERSION,
            fingerprint: *id.fingerprint.as_bytes(),
            hostname: "bench-client".into(),
        }),
    )
    .await?;
    let _ = read_message(&mut client).await?;

    match cli.scenario {
        Scenario::ControlLatency => {
            let rounds = 1_000;
            let start = Instant::now();
            for seq in 0..rounds {
                write_message(
                    &mut client,
                    &Message::UrbSubmit(UrbSubmit {
                        seq,
                        device_id: DeviceId(1),
                        endpoint: 0,
                        transfer: TransferType::Control,
                        data: vec![0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00],
                    }),
                )
                .await?;
                let _ = read_message(&mut client).await?;
            }
            let elapsed = start.elapsed();
            let avg = elapsed.as_micros() as f64 / rounds as f64;
            println!("Rounds       : {rounds}");
            println!("Total time   : {elapsed:?}");
            println!("Avg URB RTT  : {avg:.2} µs ({:.3} ms)", avg / 1000.0);
            println!(
                "Throughput   : {:.0} ops/sec",
                rounds as f64 / elapsed.as_secs_f64()
            );
        }
        Scenario::BulkThroughput => {
            let chunks = 2_000;
            let chunk_size = 512;
            let start = Instant::now();
            for seq in 0..chunks {
                write_message(
                    &mut client,
                    &Message::UrbSubmit(UrbSubmit {
                        seq,
                        device_id: DeviceId(3),
                        endpoint: 0x81,
                        transfer: TransferType::Bulk,
                        data: vec![0u8; 64],
                    }),
                )
                .await?;
                let _ = read_message(&mut client).await?;
            }
            let elapsed = start.elapsed();
            let total_mb = (chunks * chunk_size) as f64 / (1024.0 * 1024.0);
            let mb_s = total_mb / elapsed.as_secs_f64();
            println!("Transferred  : {total_mb:.2} MB");
            println!("Elapsed      : {elapsed:?}");
            println!("Bulk Speed   : {mb_s:.2} MB/s");
        }
        Scenario::Reconnect => {
            println!("Simulating 50 disconnect & reconnect cycles...");
            let start = Instant::now();
            for _ in 0..50 {
                let stream = TcpStream::connect(addr).await?;
                let server_name = rustls::pki_types::ServerName::try_from("farbus.bench")?;
                let mut c = connector.connect(server_name, stream).await?;
                write_message(
                    &mut c,
                    &Message::Hello(farbus_protocol::Hello {
                        protocol_min: VERSION,
                        protocol_max: VERSION,
                        fingerprint: *id.fingerprint.as_bytes(),
                        hostname: "bench".into(),
                    }),
                )
                .await?;
                let _ = read_message(&mut c).await?;
            }
            let elapsed = start.elapsed();
            println!(
                "50 Reconnects: {elapsed:?} (avg {:.2} ms/cycle)",
                elapsed.as_millis() as f64 / 50.0
            );
        }
    }

    Ok(())
}
