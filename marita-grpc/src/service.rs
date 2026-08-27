//! gRPC `MaritaEngine` service implementation.

use crate::proto;
use glam::DVec2;
use marita_core::sensor::Detection;
use marita_core::state::{Body, Ship, ShipCommand, SignalArc, SimulationState, Spectrum};
use marita_core::tick::{TickExecutor, TickOutput};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

pub type MaritaServiceResult<T> = Result<Response<T>, Status>;

/// Shared state of the running engine.
pub struct EngineState {
    pub state: SimulationState,
    /// Commands submitted by clients since the last tick.
    pub pending_commands: Vec<ShipCommand>,
}

/// The gRPC service implementation.
pub struct MaritaEngineService {
    pub shared: Arc<Mutex<EngineState>>,
    pub command_tx: mpsc::UnboundedSender<ShipCommand>,
    pub tick_rx: broadcast::Receiver<proto::SimulationTick>,
}

impl MaritaEngineService {
    pub fn new(
        shared: Arc<Mutex<EngineState>>,
        command_tx: mpsc::UnboundedSender<ShipCommand>,
        tick_rx: broadcast::Receiver<proto::SimulationTick>,
    ) -> Self {
        Self {
            shared,
            command_tx,
            tick_rx,
        }
    }
}

#[tonic::async_trait]
impl proto::marita_engine_server::MaritaEngine for MaritaEngineService {
    type StreamCommandsStream =
        Pin<Box<dyn Stream<Item = Result<proto::SimulationTick, Status>> + Send + Sync + 'static>>;

    async fn stream_commands(
        &self,
        request: Request<Streaming<proto::ShipCommand>>,
    ) -> MaritaServiceResult<Self::StreamCommandsStream> {
        let mut incoming = request.into_inner();
        let command_tx = self.command_tx.clone();

        // Forward client commands into the command channel.
        tokio::spawn(async move {
            while let Some(Ok(cmd)) = incoming.next().await {
                if let Some(c) = convert_command(&cmd) {
                    let _ = command_tx.send(c);
                }
            }
        });

        // Subscribe to tick broadcasts and stream them back.
        let rx = self.tick_rx.resubscribe();
        let stream = BroadcastStream::new(rx).map(|res| match res {
            Ok(tick) => Ok(tick),
            Err(_) => Err(Status::internal("tick broadcast closed")),
        });

        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_state(
        &self,
        _request: Request<proto::GetStateRequest>,
    ) -> MaritaServiceResult<proto::SimulationState> {
        let guard = self.shared.lock().await;
        let proto_state = convert_state(&guard.state);
        Ok(Response::new(proto_state))
    }
}

/// Background tick loop. Runs every real-time second, executes one simulation
/// tick, and broadcasts the result to all connected clients.
pub async fn tick_loop(
    shared: Arc<Mutex<EngineState>>,
    mut command_rx: mpsc::UnboundedReceiver<ShipCommand>,
    tick_tx: broadcast::Sender<proto::SimulationTick>,
) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let mut commands = Vec::new();
        while let Ok(cmd) = command_rx.try_recv() {
            commands.push(cmd);
        }

        let proto_tick = {
            let mut guard = shared.lock().await;
            let output = TickExecutor::new().step(&mut guard.state, &commands);
            convert_tick_output(&output)
        };

        let _ = tick_tx.send(proto_tick);
    }
}

fn convert_command(cmd: &proto::ShipCommand) -> Option<ShipCommand> {
    Some(ShipCommand {
        ship_id: cmd.ship_id,
        throttle: cmd.throttle,
        gimbal: cmd.gimbal,
        emitter_states: cmd
            .emitters
            .iter()
            .map(|e| (e.emitter_index as usize, e.active))
            .collect(),
    })
}

fn convert_state(state: &SimulationState) -> proto::SimulationState {
    proto::SimulationState {
        tick: state.tick,
        sim_time: state.sim_time,
        bodies: state.bodies.iter().map(convert_body).collect(),
        ships: state.ships.iter().map(convert_ship).collect(),
        signals: state.signals.iter().map(convert_signal).collect(),
    }
}

fn convert_tick_output(output: &TickOutput) -> proto::SimulationTick {
    let mut ship_detections: HashMap<u64, Vec<proto::Detection>> = HashMap::new();
    for (ship_idx, detections) in output.detections.iter().enumerate() {
        let ship_id = output.state.ships.get(ship_idx).map(|s| s.id).unwrap_or(0);
        let converted = detections.iter().map(convert_detection).collect();
        ship_detections.insert(ship_id, converted);
    }

    proto::SimulationTick {
        tick: output.state.tick,
        sim_time: output.state.sim_time,
        bodies: output.state.bodies.iter().map(convert_body).collect(),
        ships: output.state.ships.iter().map(convert_ship).collect(),
        signals: output.state.signals.iter().map(convert_signal).collect(),
        ship_detections: ship_detections
            .into_iter()
            .map(|(ship_id, detections)| proto::ShipDetections {
                ship_id,
                detections,
            })
            .collect(),
    }
}

fn convert_body(body: &Body) -> proto::Body {
    proto::Body {
        id: body.id,
        name: body.name.clone(),
        mass: body.mass,
        position: Some(convert_vec2(&body.position)),
        velocity: Some(convert_vec2(&body.velocity)),
        radius: body.radius,
    }
}

fn convert_ship(ship: &Ship) -> proto::Ship {
    proto::Ship {
        id: ship.id,
        name: ship.name.clone(),
        dry_mass: ship.dry_mass,
        fuel_mass: ship.fuel_mass,
        position: Some(convert_vec2(&ship.position)),
        velocity: Some(convert_vec2(&ship.velocity)),
        orientation: ship.orientation,
        angular_velocity: ship.angular_velocity,
    }
}

fn convert_signal(arc: &SignalArc) -> proto::SignalArc {
    proto::SignalArc {
        id: arc.id,
        origin: Some(convert_vec2(&arc.origin)),
        direction: arc.direction,
        angular_width: arc.angular_width,
        inner_radius: arc.inner_radius,
        outer_radius: arc.outer_radius,
        spectrum: Some(convert_spectrum(&arc.spectrum)),
        source_id: arc.source_id,
        generation: arc.generation,
    }
}

fn convert_spectrum(s: &Spectrum) -> proto::Spectrum {
    proto::Spectrum {
        bins: s.bins.to_vec(),
    }
}

fn convert_detection(d: &Detection) -> proto::Detection {
    proto::Detection {
        source_id: d.source_id,
        wavelength_bin: d.wavelength_bin as u32,
        bearing: d.bearing,
        strength: d.strength,
        snr: d.snr,
    }
}

fn convert_vec2(v: &DVec2) -> proto::Vec2 {
    proto::Vec2 { x: v.x, y: v.y }
}
