//! gRPC client that streams Luna-only detections into the viewer.

use marita_grpc::proto::marita_engine_client::MaritaEngineClient;
use marita_grpc::proto::LunaDetections;
use std::sync::mpsc;
use std::time::Duration;

/// Spawn a background tokio runtime and connect to `addr` to stream Luna
/// detections. Returns the runtime so it stays alive for the app's lifetime.
pub fn spawn_client(
    addr: String,
    state_tx: mpsc::Sender<LunaDetections>,
) -> tokio::runtime::Runtime {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.spawn(client_loop(addr, state_tx));
    rt
}

async fn client_loop(addr: String, state_tx: mpsc::Sender<LunaDetections>) {
    loop {
        match connect_and_stream(addr.clone(), state_tx.clone()).await {
            Ok(()) => {
                eprintln!("Luna stream ended; reconnecting in 1 s");
            }
            Err(e) => {
                eprintln!("Luna client error: {e}; reconnecting in 1 s");
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn connect_and_stream(
    addr: String,
    state_tx: mpsc::Sender<LunaDetections>,
) -> anyhow::Result<()> {
    let mut client = MaritaEngineClient::connect(addr).await?;
    let mut stream = client
        .stream_luna_view(marita_grpc::proto::LunaViewRequest {})
        .await?
        .into_inner();

    while let Some(detections) = stream.message().await? {
        if state_tx.send(detections).is_err() {
            break;
        }
    }

    Ok(())
}
