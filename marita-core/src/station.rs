//! Fixed celestial-station logic: production, energy, market broadcasts, and
//! command handling.
//!
//! Stations are anchored to bodies and do not move independently.  Their
//! bookkeeping (warehouses, production progress, market posters) is fully
//! deterministic; an external LLM-driven agent only proposes actions such as
//! posting a market message or starting a reaction.

use crate::ambient::AmbientField;
use crate::material::{default_body_composition, material_info, reaction_info, MaterialId};
use crate::state::{
    MarketMessage, MarketMessageKind, MarketPoster, ProductionLine, SensorArray, SimulationState,
    Spectrum, Station, StationCommand, ThermalState, WavelengthBin,
};
use crate::units::TICK_SIM_TIME;
use glam::DVec2;
use std::collections::HashMap;

/// Energy flux (W/m²) → total collected energy (kWh) over one tick.
fn solar_energy_kwh(irradiance_w_m2: f64, area_m2: f64, efficiency: f64) -> f64 {
    let power_w = irradiance_w_m2 * area_m2 * efficiency;
    let dt_hours = TICK_SIM_TIME / 3600.0;
    power_w * dt_hours / 1000.0
}

/// Apply station commands proposed by an external agent.
pub fn apply_station_commands(state: &mut SimulationState, commands: &[StationCommand]) {
    for cmd in commands {
        match cmd {
            StationCommand::PostMarketMessage(msg) => {
                if !msg.quantity.is_finite()
                    || !msg.price_per_unit_kwh.is_finite()
                    || msg.quantity <= 0.0
                    || msg.price_per_unit_kwh <= 0.0
                    || !state.stations.iter().any(|s| s.id == msg.station_id)
                {
                    continue;
                }
                let mut message = msg.clone();
                message.message_id = state.alloc_market_message_id();
                message.tick = state.tick;
                message.station_name = state
                    .stations
                    .iter()
                    .find(|s| s.id == message.station_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                if matches!(message.kind, MarketMessageKind::Accept) {
                    try_form_contract(state, &message);
                }
                state
                    .market_messages
                    .insert(message.message_id, message.clone());
                if !matches!(message.kind, MarketMessageKind::Reject) {
                    if let Some(station) = state
                        .stations
                        .iter_mut()
                        .find(|s| s.id == message.station_id)
                    {
                        station.active_market_posters.push(MarketPoster {
                            message,
                            remaining_ticks: msg.ttl_ticks.max(1),
                            broadcasted: false,
                        });
                    }
                }
            }
            StationCommand::StartProduction {
                station_id,
                reaction,
            } => {
                if let Some(station) = state.stations.iter_mut().find(|s| s.id == *station_id) {
                    let rxn = reaction_info(*reaction);
                    if station.tech_tier >= rxn.required_tech_tier {
                        station.production_lines.push(ProductionLine {
                            reaction: *reaction,
                            progress_ticks: 0,
                            active: true,
                        });
                    }
                }
            }
            StationCommand::SetCollectorArea {
                station_id,
                area_m2,
            } => {
                if let Some(station) = state.stations.iter_mut().find(|s| s.id == *station_id) {
                    station.solar_collector_area = area_m2.max(0.0);
                }
            }
        }
    }
}

fn try_form_contract(state: &mut SimulationState, acceptance: &MarketMessage) {
    use crate::state::{ContractStatus, TradeContract};
    let Some(offer_id) = acceptance.in_reply_to else {
        return;
    };
    let Some(offer) = state.market_messages.get(&offer_id).cloned() else {
        return;
    };
    if !matches!(
        offer.kind,
        MarketMessageKind::Offer | MarketMessageKind::Counter
    ) || offer.to_station_id != Some(acceptance.station_id)
        || acceptance.to_station_id != Some(offer.station_id)
    {
        return;
    }
    let quantity = acceptance.quantity.min(offer.quantity);
    let total = quantity * offer.price_per_unit_kwh;
    let Some(seller_idx) = state.stations.iter().position(|s| s.id == offer.station_id) else {
        return;
    };
    let Some(buyer_idx) = state
        .stations
        .iter()
        .position(|s| s.id == acceptance.station_id)
    else {
        return;
    };
    if seller_idx == buyer_idx {
        return;
    }
    let available = state.stations[seller_idx]
        .warehouses
        .get(&offer.material)
        .copied()
        .unwrap_or(0.0)
        - state.stations[seller_idx]
            .reserved_warehouses
            .get(&offer.material)
            .copied()
            .unwrap_or(0.0);
    if available < quantity || state.stations[buyer_idx].trade_credits_kwh < total {
        return;
    }
    state.stations[buyer_idx].trade_credits_kwh -= total;
    *state.stations[seller_idx]
        .reserved_warehouses
        .entry(offer.material)
        .or_insert(0.0) += quantity;
    let distance = (state.stations[seller_idx].position(&state.bodies)
        - state.stations[buyer_idx].position(&state.bodies))
    .length();
    const CARGO_SPEED_M_S: f64 = 100_000.0;
    let travel_ticks = (distance / (CARGO_SPEED_M_S * TICK_SIM_TIME))
        .ceil()
        .max(1.0) as u64;
    let id = state.next_id;
    state.next_id += 1;
    state.contracts.push(TradeContract {
        id,
        buyer_station_id: acceptance.station_id,
        seller_station_id: offer.station_id,
        material: offer.material,
        quantity,
        price_per_unit_kwh: offer.price_per_unit_kwh,
        escrow_kwh: total,
        created_tick: state.tick,
        arrival_tick: state.tick + travel_ticks,
        status: ContractStatus::InTransit,
    });
}

fn settle_contracts(state: &mut SimulationState) {
    use crate::state::ContractStatus;
    let due: Vec<usize> = state
        .contracts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.status == ContractStatus::InTransit && c.arrival_tick <= state.tick)
        .map(|(i, _)| i)
        .collect();
    for i in due {
        let contract = state.contracts[i].clone();
        let Some(seller_idx) = state
            .stations
            .iter()
            .position(|s| s.id == contract.seller_station_id)
        else {
            state.contracts[i].status = ContractStatus::Failed;
            continue;
        };
        let Some(buyer_idx) = state
            .stations
            .iter()
            .position(|s| s.id == contract.buyer_station_id)
        else {
            state.contracts[i].status = ContractStatus::Failed;
            continue;
        };
        let stock = state.stations[seller_idx]
            .warehouses
            .entry(contract.material)
            .or_insert(0.0);
        if *stock < contract.quantity {
            state.stations[buyer_idx].trade_credits_kwh += contract.escrow_kwh;
            state.contracts[i].status = ContractStatus::Failed;
            continue;
        }
        *stock -= contract.quantity;
        let reserved = state.stations[seller_idx]
            .reserved_warehouses
            .entry(contract.material)
            .or_insert(0.0);
        *reserved = (*reserved - contract.quantity).max(0.0);
        *state.stations[buyer_idx]
            .warehouses
            .entry(contract.material)
            .or_insert(0.0) += contract.quantity;
        state.stations[seller_idx].trade_credits_kwh += contract.escrow_kwh;
        state.contracts[i].status = ContractStatus::Settled;
    }
}

/// Run one tick of station bookkeeping: solar collection, production,
/// decay of market posters, Lagrange-point tracking, and automatic demand broadcasts.
pub fn update_stations(state: &mut SimulationState) {
    update_lagrange_offsets(state);
    settle_contracts(state);

    let ambient = AmbientField::new(&state.bodies, &state.ships);

    for station in &mut state.stations {
        let pos = station.position(&state.bodies);
        let irradiance = ambient.irradiance(pos).total();
        let mut energy_kwh = solar_energy_kwh(
            irradiance,
            station.solar_collector_area,
            station.panel_efficiency,
        );

        // Feed production lines in order.
        for line in &mut station.production_lines {
            if !line.active {
                continue;
            }
            let rxn = reaction_info(line.reaction);
            let mut can_run = energy_kwh >= rxn.energy_kwh;
            for (input, qty) in &rxn.inputs {
                if station.warehouses.get(input).copied().unwrap_or(0.0) < *qty {
                    can_run = false;
                    break;
                }
            }
            if can_run {
                // Consume energy and inputs once per tick per line.
                energy_kwh -= rxn.energy_kwh;
                for (input, qty) in &rxn.inputs {
                    if let Some(stock) = station.warehouses.get_mut(input) {
                        *stock -= qty;
                    }
                }
                line.progress_ticks += 1;
                if line.progress_ticks >= rxn.duration_ticks {
                    for (output, qty) in &rxn.outputs {
                        *station.warehouses.entry(*output).or_insert(0.0) += qty;
                    }
                    line.progress_ticks = 0;
                }
            }
        }

        // Age market posters and remove expired ones.
        for poster in &mut station.active_market_posters {
            poster.remaining_ticks = poster.remaining_ticks.saturating_sub(1);
        }
        station
            .active_market_posters
            .retain(|p| p.remaining_ticks > 0);
    }
}

/// Recompute `surface_offset` for stations parked at L4/L5 points so they
/// stay at the equilateral triangle points of the primary-secondary system.
fn update_lagrange_offsets(state: &mut SimulationState) {
    for station in &mut state.stations {
        let Some((secondary_id, point)) = station.lagrange_point else {
            continue;
        };
        let Some(primary) = state.bodies.iter().find(|b| b.id == station.parent_body_id) else {
            continue;
        };
        let Some(secondary) = state.bodies.iter().find(|b| b.id == secondary_id) else {
            continue;
        };
        station.surface_offset = lagrange_offset(primary.position, secondary.position, point);
    }
}

/// Compute the offset from the primary body to the requested Lagrange point.
/// The L4/L5 points form an equilateral triangle with the primary and secondary.
pub fn lagrange_offset(
    primary: DVec2,
    secondary: DVec2,
    point: crate::state::LagrangePoint,
) -> DVec2 {
    let delta = secondary - primary;
    // Rotate by +/- 60° to get the equilateral triangle vertex.
    let angle = match point {
        crate::state::LagrangePoint::L4 => std::f64::consts::FRAC_PI_3,
        crate::state::LagrangePoint::L5 => -std::f64::consts::FRAC_PI_3,
    };
    let c = angle.cos();
    let s = angle.sin();
    DVec2::new(delta.x * c - delta.y * s, delta.x * s + delta.y * c)
}

/// Generate deterministic WANT/HAVE posters for stations that have no active
/// poster for a given material.  This keeps the proof-of-concept market alive
/// even when no LLM is connected.
pub fn generate_auto_posters(state: &mut SimulationState) {
    const POST_INTERVAL: u64 = 30;
    if state.tick % POST_INTERVAL != 0 {
        return;
    }

    #[derive(Clone, Copy)]
    struct PosterPlan {
        station_id: u64,
        kind: MarketMessageKind,
        material: MaterialId,
        quantity: f64,
        price_multiplier: f64,
    }

    let mut plans: Vec<PosterPlan> = Vec::new();
    for station in &state.stations {
        // Offer surplus first, then ask for scarce inputs. This ensures trade
        // opportunities are advertised even when a station also needs something.
        if let Some(material) = find_surplus_raw(station) {
            let already = station.active_market_posters.iter().any(|p| {
                p.message.kind == MarketMessageKind::Have && p.message.material == material
            });
            if !already {
                plans.push(PosterPlan {
                    station_id: station.id,
                    kind: MarketMessageKind::Have,
                    material,
                    quantity: 50.0,
                    price_multiplier: 0.8,
                });
            }
        }
        // Stations look for inputs suitable for their current tech tier; as
        // they refine and upgrade they naturally shift demand up the material
        // tree.
        let target_tier = station.tech_tier;
        if let Some(material) = find_scarce_material(station, target_tier) {
            let already = station.active_market_posters.iter().any(|p| {
                p.message.kind == MarketMessageKind::Want && p.message.material == material
            });
            if !already {
                plans.push(PosterPlan {
                    station_id: station.id,
                    kind: MarketMessageKind::Want,
                    material,
                    quantity: 10.0,
                    price_multiplier: 1.5,
                });
            }
        }
    }

    // Only post one message per station per interval to avoid swamping the
    // local radio channel and self-jamming at receivers.
    let mut posted_station_ids = std::collections::HashSet::new();
    for plan in plans {
        if !posted_station_ids.insert(plan.station_id) {
            continue;
        }
        let Some(station) = state.stations.iter().find(|s| s.id == plan.station_id) else {
            continue;
        };
        let station_name = station.name.clone();
        let body_name = state
            .bodies
            .iter()
            .find(|b| b.id == station.parent_body_id)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        let msg = MarketMessage {
            message_id: state.alloc_market_message_id(),
            station_id: plan.station_id,
            station_name,
            body_name,
            tick: state.tick,
            kind: plan.kind,
            material: plan.material,
            quantity: plan.quantity,
            price_per_unit_kwh: material_info(plan.material).base_value_kwh * plan.price_multiplier,
            ttl_ticks: 60,
            in_reply_to: None,
            to_station_id: None,
        };
        if let Some(station) = state.stations.iter_mut().find(|s| s.id == plan.station_id) {
            station.active_market_posters.push(MarketPoster {
                message: msg,
                remaining_ticks: 60,
                broadcasted: false,
            });
        }
    }
}

fn find_scarce_material(station: &Station, target_tier: u32) -> Option<MaterialId> {
    use MaterialId::*;
    let candidates: &[MaterialId] = match target_tier {
        1 => &[IronOre, AluminumOre, TitaniumOre, WaterIce, CarbonaceousOre],
        2 => &[
            Iron,
            Aluminum,
            Titanium,
            Water,
            Hydrogen,
            Methane,
            SilicateOre,
            RareEarthOre,
        ],
        3 => &[Steel, Glass, Polymer, SolarCellGradeSilicon],
        4 => &[Composite, Semiconductor, AdvancedAlloy],
        _ => &[],
    };
    candidates
        .iter()
        .copied()
        .min_by(|a, b| {
            let sa = station.warehouses.get(a).copied().unwrap_or(0.0);
            let sb = station.warehouses.get(b).copied().unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|m| station.warehouses.get(m).copied().unwrap_or(0.0) < 5.0)
}

fn find_surplus_raw(station: &Station) -> Option<MaterialId> {
    station
        .warehouses
        .iter()
        .filter(|(k, v)| k.is_raw() && **v >= 100.0)
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(k, _)| *k)
}

/// Emit one market-broadcast signal arc per active poster. Posters are only
/// broadcast once to avoid flooding the radio channel and self-jamming at
/// receivers.
pub fn emit_market_broadcasts(
    state: &mut SimulationState,
    next_id: &mut u64,
) -> Vec<crate::state::SignalArc> {
    let mut emitted = Vec::new();
    // Snapshot positions first so we do not borrow `bodies` mutably while
    // iterating stations.
    let positions: std::collections::HashMap<u64, DVec2> = state
        .stations
        .iter()
        .map(|s| (s.id, s.position(&state.bodies)))
        .collect();

    for station in &mut state.stations {
        let &pos = positions
            .get(&station.id)
            .unwrap_or(&station.surface_offset);
        for poster in &mut station.active_market_posters {
            if poster.broadcasted {
                continue;
            }
            poster.broadcasted = true;
            let mut spectrum = Spectrum::zero();
            // Market chatter rides the radio band with enough information budget
            // to remain detectable across interplanetary distances.
            spectrum.bins[WavelengthBin::Radio as usize] = 1.0e18;
            let mut arc = crate::state::SignalArc::new(
                *next_id,
                pos,
                0.0,
                2.0 * std::f64::consts::PI,
                spectrum,
            );
            // A broadcast emitted over one tick is a shell c * dt thick so that
            // observers at different distances can detect it in the same tick.
            arc.outer_radius = crate::units::SPEED_OF_LIGHT * crate::units::TICK_SIM_TIME;
            arc.source_id = Some(station.id);
            arc.market_payload = Some(poster.message.clone());
            // Market broadcasts degrade slowly so they cross the solar system.
            let mut rates = Spectrum::zero();
            rates.bins[WavelengthBin::Radio as usize] = 0.001;
            arc.degradation_rates = rates;
            *next_id += 1;
            emitted.push(arc);
        }
    }
    emitted
}

/// Build a default station anchored to a body.
pub fn default_station(id: u64, name: &str, parent_body_id: u64, surface_offset: DVec2) -> Station {
    Station {
        id,
        name: name.into(),
        parent_body_id,
        surface_offset,
        lagrange_point: None,
        solar_collector_area: 1000.0,
        panel_efficiency: 0.25,
        warehouses: HashMap::new(),
        trade_credits_kwh: 100_000.0,
        reserved_warehouses: HashMap::new(),
        production_lines: Vec::new(),
        active_market_posters: Vec::new(),
        tech_tier: 1,
        modules: HashMap::new(),
        emitters: vec![],
        sensor_arrays: vec![SensorArray {
            local_position: DVec2::ZERO,
            bearing: 0.0,
            field_of_view: 2.0 * std::f64::consts::PI,
            bands: [true; 10],
            aperture_area: 10.0,
            noise_floor: 0.1,
            integration_time: 1.0,
            min_snr: 0.5,
            spectral_response: None,
        }],
        thermal: ThermalState::new(300.0, 1.0e6, 100.0),
        albedo: Spectrum::zero(),
    }
}

/// Seed a station's warehouse with an initial stock of raw materials from its
/// parent body's surface composition.
pub fn seed_station_warehouse(station: &mut Station, body_name: &str) {
    let composition = default_body_composition(body_name);
    let total: f64 = composition.values().sum();
    if total <= 0.0 {
        return;
    }
    for (material, abundance) in composition {
        let units = (abundance / total) * 10_000.0;
        *station.warehouses.entry(material).or_insert(0.0) += units;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Body, CollisionResponse, LagrangePoint, SimulationState, ThermalState};
    use crate::units::AU;
    use glam::DVec2;

    #[test]
    fn lagrange_point_tracks_secondary_body() {
        let mut state = SimulationState::new();
        state.bodies.push(Body {
            id: 1,
            name: "Earth".into(),
            mass: 5.972e24,
            position: DVec2::new(AU, 0.0),
            velocity: DVec2::new(0.0, 29_780.0),
            radius: 6.371e6,
            collision_response: CollisionResponse::Merge,
            thermal: ThermalState::new(
                288.0,
                1.0e20,
                4.0 * std::f64::consts::PI * 6.371e6 * 6.371e6,
            ),
            albedo: Spectrum::zero(),
        });
        state.bodies.push(Body {
            id: 2,
            name: "Moon".into(),
            mass: 7.3477e22,
            position: DVec2::new(AU, 3.844e8),
            velocity: DVec2::ZERO,
            radius: 1.737e6,
            collision_response: CollisionResponse::Merge,
            thermal: ThermalState::new(
                250.0,
                1.0e18,
                4.0 * std::f64::consts::PI * 1.737e6 * 1.737e6,
            ),
            albedo: Spectrum::zero(),
        });

        state.stations.push(Station {
            id: 100,
            name: "L4 Station".into(),
            parent_body_id: 1,
            surface_offset: DVec2::ZERO,
            lagrange_point: Some((2, LagrangePoint::L4)),
            solar_collector_area: 1000.0,
            panel_efficiency: 0.25,
            warehouses: HashMap::new(),
            trade_credits_kwh: 100_000.0,
            reserved_warehouses: HashMap::new(),
            production_lines: Vec::new(),
            active_market_posters: Vec::new(),
            tech_tier: 1,
            modules: HashMap::new(),
            emitters: Vec::new(),
            sensor_arrays: Vec::new(),
            thermal: ThermalState::new(300.0, 1.0e6, 10.0),
            albedo: Spectrum::zero(),
        });

        update_stations(&mut state);

        let station = &state.stations[0];
        let earth = &state.bodies[0];
        let moon = &state.bodies[1];
        let pos = station.position(&state.bodies);

        // L4 forms an equilateral triangle: distances to Earth and Moon are equal.
        let d_earth = (pos - earth.position).length();
        let d_moon = (pos - moon.position).length();
        assert!(
            (d_earth - d_moon).abs() < 1.0,
            "L4 station should be equidistant from Earth and Moon"
        );
    }

    #[test]
    fn accepted_offer_reserves_transports_and_settles() {
        let mut state = SimulationState::new();
        let mut seller = default_station(1, "Seller", 0, DVec2::ZERO);
        let buyer = default_station(2, "Buyer", 0, DVec2::new(100_000.0, 0.0));
        seller.warehouses.insert(MaterialId::IronOre, 100.0);
        state.stations = vec![seller, buyer];
        let offer = MarketMessage {
            message_id: 0,
            station_id: 1,
            station_name: String::new(),
            body_name: String::new(),
            tick: 0,
            kind: MarketMessageKind::Offer,
            material: MaterialId::IronOre,
            quantity: 10.0,
            price_per_unit_kwh: 2.0,
            ttl_ticks: 60,
            in_reply_to: Some(99),
            to_station_id: Some(2),
        };
        apply_station_commands(&mut state, &[StationCommand::PostMarketMessage(offer)]);
        let offer_id = *state.market_messages.keys().next().unwrap();
        let accept = MarketMessage {
            message_id: 0,
            station_id: 2,
            station_name: String::new(),
            body_name: String::new(),
            tick: 0,
            kind: MarketMessageKind::Accept,
            material: MaterialId::IronOre,
            quantity: 10.0,
            price_per_unit_kwh: 2.0,
            ttl_ticks: 60,
            in_reply_to: Some(offer_id),
            to_station_id: Some(1),
        };
        apply_station_commands(&mut state, &[StationCommand::PostMarketMessage(accept)]);
        assert_eq!(state.contracts.len(), 1);
        assert_eq!(
            state.stations[0].reserved_warehouses[&MaterialId::IronOre],
            10.0
        );
        state.tick = state.contracts[0].arrival_tick;
        update_stations(&mut state);
        assert_eq!(
            state.contracts[0].status,
            crate::state::ContractStatus::Settled
        );
        assert_eq!(state.stations[0].warehouses[&MaterialId::IronOre], 90.0);
        assert_eq!(state.stations[1].warehouses[&MaterialId::IronOre], 10.0);
        assert_eq!(state.stations[0].trade_credits_kwh, 100_020.0);
    }
}
