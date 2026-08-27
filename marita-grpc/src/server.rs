//! gRPC server setup.

use crate::proto::marita_engine_server::MaritaEngineServer;
use crate::service::{tick_loop, EngineState, MaritaEngineService};
use marita_core::state::SimulationState;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tonic::transport::Server;

/// Run the gRPC server bound to `addr`.
pub async fn run(
    addr: SocketAddr,
    initial_state: SimulationState,
    max_signals: usize,
) -> anyhow::Result<()> {
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (tick_tx, tick_rx) = broadcast::channel(16);

    let shared = Arc::new(Mutex::new(EngineState {
        state: initial_state,
        pending_commands: Vec::new(),
    }));

    // Spawn the background tick loop.
    let tick_shared = Arc::clone(&shared);
    tokio::spawn(tick_loop(tick_shared, command_rx, tick_tx, max_signals));

    let service = MaritaEngineService::new(Arc::clone(&shared), command_tx, tick_rx);

    println!("MaritaV3 gRPC server listening on {addr}");
    Server::builder()
        .add_service(MaritaEngineServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
