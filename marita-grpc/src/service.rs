//! gRPC `MaritaEngine` service implementation.

use crate::proto;
use glam::DVec2;
use marita_core::material::{MaterialId, ReactionId};
use marita_core::sensor::Detection;
use marita_core::state::{
    Body, MarketMessage, MarketMessageKind, MarketPoster, ProductionLine, Ship, ShipCommand,
    SignalArc, SimulationState, Spectrum, Station, StationCommand,
};
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
    /// Ship commands submitted by clients since the last tick.
    pub pending_commands: Vec<ShipCommand>,
    /// Station commands submitted by clients since the last tick.
    pub pending_station_commands: Vec<StationCommand>,
    /// Latest physically received detections for each station.
    pub station_detections: HashMap<u64, Vec<Detection>>,
}

/// The gRPC service implementation.
pub struct MaritaEngineService {
    pub shared: Arc<Mutex<EngineState>>,
    pub command_tx: mpsc::UnboundedSender<ShipCommand>,
    pub station_command_tx: mpsc::UnboundedSender<StationCommand>,
    pub tick_rx: broadcast::Receiver<proto::SimulationTick>,
}

impl MaritaEngineService {
    pub fn new(
        shared: Arc<Mutex<EngineState>>,
        command_tx: mpsc::UnboundedSender<ShipCommand>,
        station_command_tx: mpsc::UnboundedSender<StationCommand>,
        tick_rx: broadcast::Receiver<proto::SimulationTick>,
    ) -> Self {
        Self {
            shared,
            command_tx,
            station_command_tx,
            tick_rx,
        }
    }
}

#[tonic::async_trait]
impl proto::marita_engine_server::MaritaEngine for MaritaEngineService {
    type StreamCommandsStream =
        Pin<Box<dyn Stream<Item = Result<proto::SimulationTick, Status>> + Send + Sync + 'static>>;

    type StreamStationCommandsStream =
        Pin<Box<dyn Stream<Item = Result<proto::SimulationTick, Status>> + Send + Sync + 'static>>;

    type StreamLunaViewStream =
        Pin<Box<dyn Stream<Item = Result<proto::LunaDetections, Status>> + Send + Sync + 'static>>;

    async fn stream_commands(
        &self,
        request: Request<Streaming<proto::ShipCommand>>,
    ) -> MaritaServiceResult<Self::StreamCommandsStream> {
        let mut incoming = request.into_inner();
        let command_tx = self.command_tx.clone();

        tokio::spawn(async move {
            while let Some(Ok(cmd)) = incoming.next().await {
                if let Some(c) = convert_command(&cmd) {
                    let _ = command_tx.send(c);
                }
            }
        });

        let rx = self.tick_rx.resubscribe();
        let stream = BroadcastStream::new(rx).map(|res| match res {
            Ok(tick) => Ok(tick),
            Err(_) => Err(Status::internal("tick broadcast closed")),
        });

        Ok(Response::new(Box::pin(stream)))
    }

    async fn stream_station_commands(
        &self,
        request: Request<Streaming<proto::StationCommand>>,
    ) -> MaritaServiceResult<Self::StreamStationCommandsStream> {
        let mut incoming = request.into_inner();
        let command_tx = self.station_command_tx.clone();

        tokio::spawn(async move {
            while let Some(Ok(cmd)) = incoming.next().await {
                if let Some(c) = convert_station_command(&cmd) {
                    let _ = command_tx.send(c);
                }
            }
        });

        let rx = self.tick_rx.resubscribe();
        let stream = BroadcastStream::new(rx).map(|res| match res {
            Ok(tick) => Ok(tick),
            Err(_) => Err(Status::internal("tick broadcast closed")),
        });

        Ok(Response::new(Box::pin(stream)))
    }

    async fn stream_luna_view(
        &self,
        _request: Request<proto::LunaViewRequest>,
    ) -> MaritaServiceResult<Self::StreamLunaViewStream> {
        let rx = self.tick_rx.resubscribe();
        let stream = BroadcastStream::new(rx).map(|res| match res {
            Ok(tick) => Ok(proto::LunaDetections {
                tick: tick.tick,
                sim_time: tick.sim_time,
                detections: tick.luna_detections,
            }),
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

    async fn get_station_view(
        &self,
        request: Request<proto::StationViewRequest>,
    ) -> MaritaServiceResult<proto::StationView> {
        let station_id = request.into_inner().station_id;
        let guard = self.shared.lock().await;
        let station = guard
            .state
            .stations
            .iter()
            .find(|s| s.id == station_id)
            .ok_or_else(|| Status::not_found("station not found"))?;
        let contracts = guard
            .state
            .contracts
            .iter()
            .filter(|c| c.buyer_station_id == station_id || c.seller_station_id == station_id)
            .map(convert_contract)
            .collect();
        Ok(Response::new(proto::StationView {
            tick: guard.state.tick,
            sim_time: guard.state.sim_time,
            station: Some(convert_station(station)),
            received_messages: guard
                .station_detections
                .get(&station_id)
                .into_iter()
                .flatten()
                .filter(|d| {
                    d.market_payload
                        .as_ref()
                        .map(|m| m.to_station_id.is_none() || m.to_station_id == Some(station_id))
                        .unwrap_or(false)
                })
                .map(|d| convert_observer_detection(d, station_id))
                .collect(),
            contracts,
        }))
    }

    async fn submit_station_command(
        &self,
        request: Request<proto::StationCommand>,
    ) -> MaritaServiceResult<proto::CommandResult> {
        let cmd = request.into_inner();
        let guard = self.shared.lock().await;
        if !guard.state.stations.iter().any(|s| s.id == cmd.station_id) {
            return Ok(Response::new(proto::CommandResult {
                accepted: false,
                reason: "unknown station".into(),
            }));
        }
        if let Some(proto::station_command::Action::PostMarketMessage(message)) = &cmd.action {
            if message.station_id != cmd.station_id {
                return Ok(Response::new(proto::CommandResult {
                    accepted: false,
                    reason: "station identity mismatch".into(),
                }));
            }
            if let Some(reply_id) = message.in_reply_to {
                let was_received = guard
                    .station_detections
                    .get(&cmd.station_id)
                    .into_iter()
                    .flatten()
                    .any(|d| {
                        d.market_payload
                            .as_ref()
                            .map(|m| m.message_id == reply_id)
                            .unwrap_or(false)
                    });
                if !was_received {
                    return Ok(Response::new(proto::CommandResult {
                        accepted: false,
                        reason: "reply references a message not received by this station".into(),
                    }));
                }
            }
        }
        drop(guard);
        let Some(command) = convert_station_command(&cmd) else {
            return Ok(Response::new(proto::CommandResult {
                accepted: false,
                reason: "invalid command".into(),
            }));
        };
        self.station_command_tx
            .send(command)
            .map_err(|_| Status::unavailable("engine command queue closed"))?;
        Ok(Response::new(proto::CommandResult {
            accepted: true,
            reason: String::new(),
        }))
    }
}

/// Background tick loop. Runs every real-time second, executes one simulation
/// tick, and broadcasts the result to all connected clients.
pub async fn tick_loop(
    shared: Arc<Mutex<EngineState>>,
    mut command_rx: mpsc::UnboundedReceiver<ShipCommand>,
    mut station_command_rx: mpsc::UnboundedReceiver<StationCommand>,
    tick_tx: broadcast::Sender<proto::SimulationTick>,
    max_signals: usize,
    observer_config: marita_core::observer::ObserverConfig,
) {
    let executor = TickExecutor::new()
        .with_max_signals(max_signals)
        .with_observer_config(observer_config);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let mut commands = Vec::new();
        while let Ok(cmd) = command_rx.try_recv() {
            commands.push(cmd);
        }

        let mut station_commands = Vec::new();
        while let Ok(cmd) = station_command_rx.try_recv() {
            station_commands.push(cmd);
        }

        let proto_tick = {
            let mut guard = shared.lock().await;
            let output = executor.step(&mut guard.state, &commands, &station_commands);
            guard.station_detections = output.station_detections.clone();
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

fn convert_station_command(cmd: &proto::StationCommand) -> Option<StationCommand> {
    use proto::station_command::Action;
    let action = cmd.action.as_ref()?;
    match action {
        Action::PostMarketMessage(msg) => {
            let message = convert_market_message(msg)?;
            Some(StationCommand::PostMarketMessage(message))
        }
        Action::StartProduction(sp) => {
            let reaction = convert_reaction_id(sp.reaction)?;
            Some(StationCommand::StartProduction {
                station_id: cmd.station_id,
                reaction,
            })
        }
        Action::SetCollectorArea(sc) => Some(StationCommand::SetCollectorArea {
            station_id: cmd.station_id,
            area_m2: sc.area_m2,
        }),
    }
}

fn convert_state(state: &SimulationState) -> proto::SimulationState {
    proto::SimulationState {
        tick: state.tick,
        sim_time: state.sim_time,
        bodies: state.bodies.iter().map(convert_body).collect(),
        ships: state.ships.iter().map(convert_ship).collect(),
        signals: state.signals.iter().map(convert_signal).collect(),
        stations: state.stations.iter().map(convert_station).collect(),
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
        luna_detections: output
            .luna_detections
            .iter()
            .map(|d| convert_observer_detection(d, u64::MAX))
            .collect(),
        stations: output.state.stations.iter().map(convert_station).collect(),
        station_detections: output
            .station_detections
            .iter()
            .map(|(station_id, detections)| proto::StationDetections {
                station_id: *station_id,
                detections: detections
                    .iter()
                    .map(|d| convert_observer_detection(d, *station_id))
                    .collect(),
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
        emitters: ship
            .emitters
            .iter()
            .map(|e| proto::Emitter {
                wavelength_bin: e.wavelength_bin as u32,
                angular_width: e.angular_width,
                active: e.active,
            })
            .collect(),
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

fn convert_station(station: &Station) -> proto::Station {
    proto::Station {
        id: station.id,
        name: station.name.clone(),
        parent_body_id: station.parent_body_id,
        position: Some(convert_vec2(&station.surface_offset)),
        solar_collector_area: station.solar_collector_area,
        panel_efficiency: station.panel_efficiency,
        warehouses: station
            .warehouses
            .iter()
            .map(|(mat, qty)| proto::WarehouseEntry {
                material: *mat as u32,
                quantity: *qty,
            })
            .collect(),
        production_lines: station
            .production_lines
            .iter()
            .map(convert_production_line)
            .collect(),
        active_market_posters: station
            .active_market_posters
            .iter()
            .map(convert_market_poster)
            .collect(),
        tech_tier: station.tech_tier,
        modules: station
            .modules
            .iter()
            .map(|(mat, qty)| proto::WarehouseEntry {
                material: *mat as u32,
                quantity: *qty,
            })
            .collect(),
        emitters: station
            .emitters
            .iter()
            .map(|e| proto::Emitter {
                wavelength_bin: e.wavelength_bin as u32,
                angular_width: e.angular_width,
                active: e.active,
            })
            .collect(),
        trade_credits_kwh: station.trade_credits_kwh,
        reserved_warehouses: station
            .reserved_warehouses
            .iter()
            .map(|(mat, qty)| proto::WarehouseEntry {
                material: *mat as u32,
                quantity: *qty,
            })
            .collect(),
    }
}

fn convert_contract(contract: &marita_core::state::TradeContract) -> proto::TradeContract {
    proto::TradeContract {
        id: contract.id,
        buyer_station_id: contract.buyer_station_id,
        seller_station_id: contract.seller_station_id,
        material: contract.material as u32,
        quantity: contract.quantity,
        price_per_unit_kwh: contract.price_per_unit_kwh,
        escrow_kwh: contract.escrow_kwh,
        created_tick: contract.created_tick,
        arrival_tick: contract.arrival_tick,
        status: format!("{:?}", contract.status).to_uppercase(),
    }
}

fn convert_production_line(line: &ProductionLine) -> proto::ProductionLine {
    proto::ProductionLine {
        reaction: line.reaction as u32,
        progress_ticks: line.progress_ticks,
        active: line.active,
    }
}

fn convert_market_poster(poster: &MarketPoster) -> proto::MarketPoster {
    proto::MarketPoster {
        message: Some(convert_market_message_to_proto(&poster.message)),
        remaining_ticks: poster.remaining_ticks,
    }
}

fn convert_market_message_to_proto(msg: &MarketMessage) -> proto::MarketMessage {
    proto::MarketMessage {
        message_id: msg.message_id,
        station_id: msg.station_id,
        station_name: msg.station_name.clone(),
        body_name: msg.body_name.clone(),
        tick: msg.tick,
        kind: match msg.kind {
            MarketMessageKind::Want => "WANT".into(),
            MarketMessageKind::Have => "HAVE".into(),
            MarketMessageKind::Offer => "OFFER".into(),
            MarketMessageKind::Counter => "COUNTER".into(),
            MarketMessageKind::Accept => "ACCEPT".into(),
            MarketMessageKind::Reject => "REJECT".into(),
        },
        material: msg.material as u32,
        quantity: msg.quantity,
        price_per_unit_kwh: msg.price_per_unit_kwh,
        ttl_ticks: msg.ttl_ticks,
        in_reply_to: msg.in_reply_to,
        to_station_id: msg.to_station_id,
    }
}

fn convert_market_message(msg: &proto::MarketMessage) -> Option<MarketMessage> {
    Some(MarketMessage {
        message_id: msg.message_id,
        station_id: msg.station_id,
        station_name: msg.station_name.clone(),
        body_name: msg.body_name.clone(),
        tick: msg.tick,
        kind: match msg.kind.as_str() {
            "WANT" => MarketMessageKind::Want,
            "HAVE" => MarketMessageKind::Have,
            "OFFER" => MarketMessageKind::Offer,
            "COUNTER" => MarketMessageKind::Counter,
            "ACCEPT" => MarketMessageKind::Accept,
            "REJECT" => MarketMessageKind::Reject,
            _ => return None,
        },
        material: convert_material_id(msg.material)?,
        quantity: msg.quantity,
        price_per_unit_kwh: msg.price_per_unit_kwh,
        ttl_ticks: msg.ttl_ticks,
        in_reply_to: msg.in_reply_to,
        to_station_id: msg.to_station_id,
    })
}

fn convert_material_id(id: u32) -> Option<MaterialId> {
    use MaterialId::*;
    Some(match id {
        0 => Regolith,
        1 => IronOre,
        2 => AluminumOre,
        3 => TitaniumOre,
        4 => WaterIce,
        5 => CarbonaceousOre,
        6 => SilicateOre,
        7 => RareEarthOre,
        100 => Iron,
        101 => Aluminum,
        102 => Titanium,
        103 => Water,
        104 => Oxygen,
        105 => Hydrogen,
        106 => Methane,
        107 => Glass,
        200 => Steel,
        201 => Concrete,
        202 => Polymer,
        203 => SolarCellGradeSilicon,
        300 => Composite,
        301 => Semiconductor,
        302 => AdvancedAlloy,
        400 => HabitatModule,
        401 => RefineryModule,
        402 => SolarArrayModule,
        _ => return None,
    })
}

fn convert_reaction_id(id: u32) -> Option<ReactionId> {
    use ReactionId::*;
    Some(match id {
        0 => SmeltIron,
        1 => SmeltAluminum,
        2 => SmeltTitanium,
        3 => ElectrolyseWater,
        4 => SabatierMethane,
        5 => MakeSteel,
        6 => MakeGlass,
        7 => MakeConcrete,
        8 => MakePolymer,
        9 => RefineSolarSilicon,
        10 => MakeComposite,
        11 => MakeSemiconductor,
        12 => MakeAdvancedAlloy,
        13 => AssembleHabitat,
        14 => AssembleRefinery,
        15 => AssembleSolarArray,
        _ => return None,
    })
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
        distance: d.distance,
        strength: d.strength,
        snr: d.snr,
        market_payload: d
            .market_payload
            .as_ref()
            .map(convert_market_message_to_proto),
        contact_id: d.contact_id,
        bearing_sigma: d.bearing_sigma,
        range_sigma: d.range_sigma,
        emission_tick: d.emission_tick,
    }
}

fn convert_observer_detection(d: &Detection, observer_scope: u64) -> proto::Detection {
    let mut detection = convert_detection(d);
    detection.source_id = None;
    detection.contact_id = scoped_contact_id(observer_scope, d.contact_id);
    detection
}

fn scoped_contact_id(scope: u64, contact: u64) -> u64 {
    let mut value = scope ^ contact.rotate_left(29) ^ 0x9e3779b97f4a7c15;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn convert_vec2(v: &DVec2) -> proto::Vec2 {
    proto::Vec2 { x: v.x, y: v.y }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marita_core::state::WavelengthBin;

    #[test]
    fn unprivileged_detection_hides_authoritative_source_id() {
        let detection = Detection {
            source_id: Some(42),
            contact_id: 7,
            wavelength_bin: WavelengthBin::Optical,
            bearing: 1.0,
            distance: 2.0,
            strength: 3.0,
            snr: 4.0,
            bearing_sigma: 0.1,
            range_sigma: 0.2,
            emission_tick: 5,
            market_payload: None,
        };
        let converted = convert_observer_detection(&detection, 99);
        assert_eq!(converted.source_id, None);
        assert_ne!(converted.contact_id, 0);
        assert_ne!(converted.contact_id, 7);
    }
}
