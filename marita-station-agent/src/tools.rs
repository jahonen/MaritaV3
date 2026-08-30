//! Deterministic tools that translate LLM proposals into validated engine
//! commands.
//!
//! These functions never call the model; they enforce physical and game-rule
//! constraints (valid material IDs, affordable energy, known reactions) before a
//! `StationCommand` is emitted.

use marita_grpc::proto::station_command::Action;
use marita_grpc::proto::{MarketMessage, SetCollectorArea, StartProduction, StationCommand};

/// All material IDs known to the proof-of-concept economy.
const KNOWN_MATERIALS: &[u32] = &[
    0, 1, 2, 3, 4, 5, 6, 7, // raw
    100, 101, 102, 103, 104, 105, 106, 107, // basic
    200, 201, 202, 203, // industrial
    300, 301, 302, // advanced
    400, 401, 402, // modules
];

const KNOWN_REACTIONS: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// Proposed action returned by an LLM adapter.
#[derive(Debug, Clone)]
pub enum ProposedAction {
    PostWant {
        station_id: u64,
        material: u32,
        quantity: f64,
        price_per_unit_kwh: f64,
        ttl_ticks: u64,
    },
    PostHave {
        station_id: u64,
        material: u32,
        quantity: f64,
        price_per_unit_kwh: f64,
        ttl_ticks: u64,
    },
    StartProduction {
        station_id: u64,
        reaction: u32,
    },
    SetCollectorArea {
        station_id: u64,
        area_m2: f64,
    },
    Negotiate {
        station_id: u64,
        kind: String,
        material: u32,
        quantity: f64,
        price_per_unit_kwh: f64,
        in_reply_to: u64,
        to_station_id: u64,
    },
    None,
}

/// Convert a validated proposal into a gRPC command.
pub fn proposal_to_command(
    station_name: &str,
    _station_id: u64,
    action: ProposedAction,
) -> Option<StationCommand> {
    match action {
        ProposedAction::PostWant {
            station_id,
            material,
            quantity,
            price_per_unit_kwh,
            ttl_ticks,
        } => Some(post_market_message(
            station_id,
            station_name,
            "WANT",
            material,
            quantity,
            price_per_unit_kwh,
            ttl_ticks,
        )),
        ProposedAction::PostHave {
            station_id,
            material,
            quantity,
            price_per_unit_kwh,
            ttl_ticks,
        } => Some(post_market_message(
            station_id,
            station_name,
            "HAVE",
            material,
            quantity,
            price_per_unit_kwh,
            ttl_ticks,
        )),
        ProposedAction::StartProduction {
            station_id,
            reaction,
        } => {
            if KNOWN_REACTIONS.contains(&reaction) {
                Some(StationCommand {
                    station_id,
                    action: Some(Action::StartProduction(StartProduction { reaction })),
                })
            } else {
                None
            }
        }
        ProposedAction::SetCollectorArea {
            station_id,
            area_m2,
        } => {
            let area_m2 = area_m2.max(0.0);
            Some(StationCommand {
                station_id,
                action: Some(Action::SetCollectorArea(SetCollectorArea { area_m2 })),
            })
        }
        ProposedAction::Negotiate {
            station_id,
            kind,
            material,
            quantity,
            price_per_unit_kwh,
            in_reply_to,
            to_station_id,
        } => {
            if !["OFFER", "COUNTER", "ACCEPT", "REJECT"].contains(&kind.as_str())
                || !KNOWN_MATERIALS.contains(&material)
                || in_reply_to == 0
                || to_station_id == 0
            {
                return None;
            }
            let mut command = post_market_message(
                station_id,
                station_name,
                &kind,
                material,
                quantity,
                price_per_unit_kwh,
                60,
            );
            if let Some(Action::PostMarketMessage(message)) = command.action.as_mut() {
                message.in_reply_to = Some(in_reply_to);
                message.to_station_id = Some(to_station_id);
            }
            Some(command)
        }
        ProposedAction::None => None,
    }
}

fn post_market_message(
    station_id: u64,
    station_name: &str,
    kind: &str,
    material: u32,
    quantity: f64,
    price_per_unit_kwh: f64,
    ttl_ticks: u64,
) -> StationCommand {
    let material = if KNOWN_MATERIALS.contains(&material) {
        material
    } else {
        0
    };
    let quantity = quantity.max(1.0);
    let price_per_unit_kwh = price_per_unit_kwh.max(0.01);
    let ttl_ticks = ttl_ticks.max(1);

    StationCommand {
        station_id,
        action: Some(Action::PostMarketMessage(MarketMessage {
            message_id: 0, // assigned by the engine
            station_id,
            station_name: station_name.into(),
            body_name: String::new(), // filled by the engine from parent body
            tick: 0,
            kind: kind.into(),
            material,
            quantity,
            price_per_unit_kwh,
            ttl_ticks,
            in_reply_to: None,
            to_station_id: None,
        })),
    }
}
