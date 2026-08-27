//! gRPC client that streams simulation ticks into the viewer.

use crate::state::ViewerState;
use marita_grpc::proto::marita_engine_client::MaritaEngineClient;
use marita_grpc::proto::ShipCommand;
use std::sync::mpsc;
use std::time::Duration;

/// Spawn a background tokio runtime and start streaming from `addr`.
///
/// Returns the runtime so it is kept alive as long as the app lives.
pub fn spawn_client(addr: String, state_tx: mpsc::Sender<ViewerState>) -> tokio::runtime::Runtime {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.spawn(client_loop(addr, state_tx));
    rt
}

async fn client_loop(addr: String, state_tx: mpsc::Sender<ViewerState>) {
    loop {
        match connect_and_stream(addr.clone(), state_tx.clone()).await {
            Ok(()) => {
                eprintln!("stream ended; reconnecting in 1 s");
            }
            Err(e) => {
                eprintln!("client error: {e}; reconnecting in 1 s");
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn connect_and_stream(
    addr: String,
    state_tx: mpsc::Sender<ViewerState>,
) -> anyhow::Result<()> {
    let mut client = MaritaEngineClient::connect(addr).await?;

    // We do not need to send commands from the admin viewer, so provide an
    // empty command stream. The server still broadcasts ticks to us.
    let commands = futures::stream::iter(std::iter::empty::<ShipCommand>());
    let mut stream = client.stream_commands(commands).await?.into_inner();

    while let Some(tick) = stream.message().await? {
        let state = ViewerState::from_proto(tick);
        if state_tx.send(state).is_err() {
            break;
        }
    }

    Ok(())
}
