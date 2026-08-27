//! gRPC client that streams simulation ticks into the viewer and forwards
//! outgoing ship commands to the engine.

use crate::state::ViewerState;
use marita_grpc::proto::marita_engine_client::MaritaEngineClient;
use marita_grpc::proto::ShipCommand;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub type CommandQueue = Arc<Mutex<VecDeque<ShipCommand>>>;

/// Handle returned by [`spawn_client`] for pushing commands from the UI.
pub struct CommandHandle {
    queue: CommandQueue,
}

impl CommandHandle {
    pub fn push(&self, command: ShipCommand) {
        self.queue
            .lock()
            .expect("command queue lock")
            .push_back(command);
    }
}

/// Spawn a background tokio runtime, connect to `addr`, and start streaming.
///
/// Returns the runtime (kept alive by the app) and a command handle for outgoing
/// ship commands.
pub fn spawn_client(
    addr: String,
    state_tx: mpsc::Sender<ViewerState>,
) -> (tokio::runtime::Runtime, CommandHandle) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let queue: CommandQueue = Arc::new(Mutex::new(VecDeque::new()));
    rt.spawn(client_loop(addr, state_tx, queue.clone()));
    let handle = CommandHandle { queue };
    (rt, handle)
}

async fn client_loop(addr: String, state_tx: mpsc::Sender<ViewerState>, queue: CommandQueue) {
    loop {
        match connect_and_stream(addr.clone(), state_tx.clone(), queue.clone()).await {
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
    queue: CommandQueue,
) -> anyhow::Result<()> {
    let mut client = MaritaEngineClient::connect(addr).await?;

    // Forward commands from the shared queue into the gRPC command stream.
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<ShipCommand>();
    tokio::spawn(command_forwarder(queue, cmd_tx));

    let commands = tokio_stream::wrappers::UnboundedReceiverStream::new(cmd_rx);
    let mut stream = client.stream_commands(commands).await?.into_inner();

    while let Some(tick) = stream.message().await? {
        let state = ViewerState::from_proto(tick);
        if state_tx.send(state).is_err() {
            break;
        }
    }

    Ok(())
}

async fn command_forwarder(
    queue: CommandQueue,
    tx: tokio::sync::mpsc::UnboundedSender<ShipCommand>,
) {
    loop {
        // Drain the queue each iteration so commands do not pile up.
        {
            let mut q = queue.lock().expect("command queue lock");
            while let Some(cmd) = q.pop_front() {
                if tx.send(cmd).is_err() {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
