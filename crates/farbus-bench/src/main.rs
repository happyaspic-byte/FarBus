use clap::Parser;
use farbus_bench::{Cli, Scenario};
use farbus_core::{
    hash_pin, make_self_signed, make_server_config, serve_session, simulated_lab_devices,
    ClientError, DeviceId, FarBusClient, ServerState, TransferType,
};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

#[tokio::main]
#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    if !cli.json {
        println!("=== FarBus Benchmark: {:?} ===", cli.scenario);
    }

    let (certs, key, server_fp) = make_self_signed("farbus.local")?;
    let acceptor = make_server_config(certs, key)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let state = Arc::new(ServerState::new(
        "farbus-bench".into(),
        server_fp,
        simulated_lab_devices(),
    ));
    let pin = state.pin.lock().await.pin.clone();

    let _server = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let _ = stream.set_nodelay(true);
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

    let mut client = FarBusClient::connect(addr, server_fp).await?;
    client.pair(&pin, server_fp).await?;
    let _ = hash_pin(&pin, server_fp);
    let device = match cli.scenario {
        Scenario::BulkThroughput => DeviceId(3),
        _ => DeviceId(1),
    };
    client.attach(device).await?;

    match cli.scenario {
        Scenario::ControlLatency => {
            let rounds = 1_000u32;
            let start = Instant::now();
            for seq in 0..rounds {
                let _ = client
                    .urb(
                        device,
                        seq,
                        0,
                        TransferType::Control,
                        vec![0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x12, 0x00],
                    )
                    .await?;
            }
            let elapsed = start.elapsed();
            let avg = elapsed.as_micros() as f64 / f64::from(rounds);
            println!("Rounds       : {rounds}");
            println!("Total time   : {elapsed:?}");
            println!("Avg URB RTT  : {avg:.2} µs ({:.3} ms)", avg / 1000.0);
            println!(
                "Throughput   : {:.0} ops/sec",
                f64::from(rounds) / elapsed.as_secs_f64()
            );
        }
        Scenario::BulkThroughput => {
            use futures::stream::{self, StreamExt, TryStreamExt};
            let chunks = 1_000u32;
            let chunk_size = 16_384u32;
            let payload = vec![0u8; chunk_size as usize];
            let start = Instant::now();
            let mut submitted = 0u64;
            stream::iter(0..chunks)
                .map(|seq| {
                    let client = client.clone();
                    let payload = payload.clone();
                    async move {
                        let _ = client
                            .urb(device, seq, 1, TransferType::Bulk, payload)
                            .await?;
                        Ok::<u32, ClientError>(1)
                    }
                })
                .buffer_unordered(cli.depth)
                .try_for_each(|n| {
                    submitted += u64::from(n);
                    futures::future::ready(Ok::<(), ClientError>(()))
                })
                .await?;
            let elapsed = start.elapsed();
            let total_mb = f64::from(chunks * chunk_size) / (1024.0 * 1024.0);
            let mb_s = total_mb / elapsed.as_secs_f64();
            assert_eq!(
                submitted,
                u64::from(chunks),
                "every submitted URB completed"
            );
            if cli.json {
                println!(
                    "{{\"scenario\":\"bulk-throughput\",\"depth\":{},\"chunks\":{},\"chunk_size\":{},\"mb\":{:.2},\"elapsed_s\":{:.4},\"mbps\":{:.2}}}",
                    cli.depth, chunks, chunk_size, total_mb, elapsed.as_secs_f64(), mb_s
                );
            } else {
                println!("Pipeline depth: {}", cli.depth);
                println!("Transferred  : {total_mb:.2} MB");
                println!("Elapsed      : {elapsed:?}");
                println!("Bulk Speed   : {mb_s:.2} MB/s");
            }
        }
        Scenario::Reconnect => {
            println!("Simulating 50 disconnect & reconnect cycles...");
            let start = Instant::now();
            for _ in 0..50 {
                client.reconnect().await?;
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
