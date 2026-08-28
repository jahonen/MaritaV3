//! Fixed-tick simulation orchestrator.
//!
//! A single tick advances the simulation by `TICK_SIM_TIME` seconds. It applies
//! ship commands, integrates gravity and rigid-body dynamics, propagates and
//! clips signals, updates heat, emits new signals, and runs sensors.

use crate::ambient::AmbientField;
use crate::collision::resolve_collisions;
use crate::gravity::compute_accelerations;
use crate::heat::update_thermal;
use crate::propulsion::{compute_thrust, EngineSignature};
use crate::sensor::{compute_all_detections, Detection};
use crate::signal::{clip_against_masses, cull_signals_past_sensors, emit_signals, propagate};
use crate::state::{Body, Ship, ShipCommand, SignalArc, SimulationState, Spectrum, WavelengthBin};
use crate::units::{SOLAR_SYSTEM_BOUNDARY, TICK_SIM_TIME};
use glam::DVec2;

/// Output of one simulation tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TickOutput {
    pub state: SimulationState,
    pub detections: Vec<Vec<Detection>>,
}

/// Executes a single simulation tick.
#[derive(Debug, Clone)]
pub struct TickExecutor {
    pub max_signals: usize,
}

impl Default for TickExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl TickExecutor {
    pub fn new() -> Self {
        Self {
            max_signals: 50_000,
        }
    }

    pub fn with_max_signals(mut self, max_signals: usize) -> Self {
        self.max_signals = max_signals;
        self
    }

    /// Run one tick of the simulation.
    pub fn step(&self, state: &mut SimulationState, commands: &[ShipCommand]) -> TickOutput {
        let dt = TICK_SIM_TIME;

        // 1. Apply per-tick commands to ships.
        apply_commands(state, commands);

        // 2. Compute thrust forces/torques and engine signatures.
        let mut thrust_forces: Vec<DVec2> = vec![DVec2::ZERO; state.ships.len()];
        let mut thrust_torques: Vec<f64> = vec![0.0; state.ships.len()];
        let mut fuel_consumed: Vec<f64> = vec![0.0; state.ships.len()];
        let mut engine_signatures: Vec<Option<EngineSignature>> = Vec::new();
        for (i, ship) in state.ships.iter().enumerate() {
            let cmd = commands
                .iter()
                .find(|c| c.ship_id == ship.id)
                .cloned()
                .unwrap_or_default();
            let result = compute_thrust(ship, &cmd, dt);
            thrust_forces[i] = result.force;
            thrust_torques[i] = result.torque;
            fuel_consumed[i] = result.fuel_consumed;
            engine_signatures.push(result.signature);
        }

        // 3. Compute gravity at current positions.
        let acc_current = compute_accelerations(&state.bodies, &state.ships);

        // 4. Half gravity kick.
        apply_body_accel(&mut state.bodies, &acc_current.body_accelerations, dt * 0.5);
        apply_ship_accel(&mut state.ships, &acc_current.ship_accelerations, dt * 0.5);

        // 5. Drift positions.
        drift_bodies(&mut state.bodies, dt);
        drift_ships(&mut state.ships, dt);

        // 6. Apply thrust impulse at midpoint.
        for (i, ship) in state.ships.iter_mut().enumerate() {
            ship.fuel_mass = (ship.fuel_mass - fuel_consumed[i]).max(0.0);
            let mass = ship.mass();
            if mass > 0.0 {
                ship.velocity += thrust_forces[i] / mass * dt;
                let moment = ship.moment_of_inertia;
                if moment > 0.0 {
                    ship.angular_velocity += thrust_torques[i] / moment * dt;
                }
            }
        }

        // 7. Recompute gravity at new positions and finish with half kick.
        let acc_new = compute_accelerations(&state.bodies, &state.ships);
        apply_body_accel(&mut state.bodies, &acc_new.body_accelerations, dt * 0.5);
        apply_ship_accel(&mut state.ships, &acc_new.ship_accelerations, dt * 0.5);

        // 8. Update ship orientations.
        for ship in &mut state.ships {
            ship.orientation =
                crate::state::normalize_angle(ship.orientation + ship.angular_velocity * dt);
        }

        // 9. Resolve mass-mass collisions.
        let collision_result = resolve_collisions(
            std::mem::take(&mut state.bodies),
            std::mem::take(&mut state.ships),
        );
        state.bodies = collision_result.bodies;
        state.ships = collision_result.ships;

        // 10. Build the continuous ambient radiation field from the Sun and warm
        // bodies. This is used for heating and sensor background.
        let ambient = AmbientField::new(&state.bodies, &state.ships);

        // 11. Propagate existing active signals.
        propagate(&mut state.signals, dt);

        // 12. Clip active signals against masses.
        let clip_result = clip_against_masses(
            std::mem::take(&mut state.signals),
            &state.bodies,
            &state.ships,
        );
        state.signals = clip_result.remaining;
        state.signals.extend(clip_result.reflected);

        // 13. Update thermal states from ambient irradiance plus any energy
        // absorbed from active signal arcs.
        let mut absorbed_by_entity: std::collections::HashMap<u64, f64> =
            std::collections::HashMap::new();
        for absorbed in &clip_result.absorbed {
            *absorbed_by_entity.entry(absorbed.entity_id).or_default() += absorbed.energy;
        }

        for body in &mut state.bodies {
            let ambient_energy =
                ambient.absorbed_energy(body.position, body.radius, body.thermal.emissivity, dt);
            let arc_energy = absorbed_by_entity.get(&body.id).copied().unwrap_or(0.0);
            update_thermal(&mut body.thermal, ambient_energy + arc_energy, dt);
        }
        for ship in &mut state.ships {
            let ambient_energy =
                ambient.absorbed_energy(ship.position, ship.radius(), ship.thermal.emissivity, dt);
            let arc_energy = absorbed_by_entity.get(&ship.id).copied().unwrap_or(0.0);
            update_thermal(&mut ship.thermal, ambient_energy + arc_energy, dt);
        }

        // 14. Discard active arcs that have already swept past every known
        // sensor and cannot be detected in the future.
        state.signals = cull_signals_past_sensors(
            std::mem::take(&mut state.signals),
            &state.ships,
            SOLAR_SYSTEM_BOUNDARY,
        );

        // 15. Emit new active signals: intentional emitters and engine signatures.
        let mut next_id = state.next_id;
        let mut emitted = emit_signals(state, dt, &mut next_id);
        // Add engine signatures as arcs.
        for sig in engine_signatures.into_iter().flatten() {
            // Degrade engine signature heavily in vacuum so it does not persist.
            let mut rates = Spectrum::zero();
            rates.bins[WavelengthBin::EngineThermal as usize] = 1.0;
            rates.bins[WavelengthBin::Infrared as usize] = 0.5;
            let mut arc = SignalArc::new(
                next_id,
                sig.origin,
                sig.direction,
                sig.angular_width,
                sig.spectrum,
            );
            next_id += 1;
            arc.degradation_rates = rates;
            emitted.push(arc);
        }
        state.next_id = next_id;
        state.signals.extend(emitted);

        // 16. Cap total signals to prevent unbounded memory growth.
        if state.signals.len() > self.max_signals {
            // Keep the newest (highest id) arcs, which are the most recently emitted.
            state.signals.sort_by_key(|a| a.id);
            let drain_count = state.signals.len() - self.max_signals;
            state.signals.drain(0..drain_count);
        }

        // 17. Run sensors against the ambient field plus active arcs.
        let detections = compute_all_detections(&state.bodies, &state.ships, &state.signals);

        // 18. Advance time.
        state.tick += 1;
        state.sim_time += dt;

        TickOutput {
            state: state.clone(),
            detections,
        }
    }
}

fn apply_commands(state: &mut SimulationState, commands: &[ShipCommand]) {
    for ship in &mut state.ships {
        if let Some(cmd) = commands.iter().find(|c| c.ship_id == ship.id) {
            for (idx, active) in &cmd.emitter_states {
                if let Some(emitter) = ship.emitters.get_mut(*idx) {
                    emitter.active = *active;
                }
            }
        }
    }
}

fn apply_body_accel(bodies: &mut [Body], accelerations: &[DVec2], dt: f64) {
    for (body, acc) in bodies.iter_mut().zip(accelerations.iter()) {
        body.velocity += *acc * dt;
    }
}

fn apply_ship_accel(ships: &mut [Ship], accelerations: &[DVec2], dt: f64) {
    for (ship, acc) in ships.iter_mut().zip(accelerations.iter()) {
        ship.velocity += *acc * dt;
    }
}

fn drift_bodies(bodies: &mut [Body], dt: f64) {
    for body in bodies.iter_mut() {
        body.position += body.velocity * dt;
    }
}

fn drift_ships(ships: &mut [Ship], dt: f64) {
    for ship in ships.iter_mut() {
        ship.position += ship.velocity * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::{CircularOrbitLoader, EphemerisLoader};
    use crate::state::{
        CollisionResponse, EngineMount, Ship, Spectrum, ThermalState, WavelengthBin,
    };

    fn make_ship(id: u64, pos: DVec2) -> Ship {
        Ship {
            id,
            name: "ship".into(),
            dry_mass: 1000.0,
            fuel_mass: 500.0,
            position: pos,
            velocity: DVec2::ZERO,
            orientation: 0.0,
            angular_velocity: 0.0,
            moment_of_inertia: 1000.0,
            engine_mounts: vec![EngineMount {
                local_position: DVec2::ZERO,
                max_thrust: 0.0,
                specific_impulse: 300.0,
                max_mass_flow: 0.0,
                gimbal: 0.0,
            }],
            sensor_arrays: vec![],
            emitters: vec![],
            thermal: ThermalState::new(300.0, 1.0e6, 10.0),
            collision_response: CollisionResponse::Ghost,
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn earth_orbit_is_stable_for_one_tick() {
        let mut state = SimulationState::new();
        state.bodies = CircularOrbitLoader.load();
        let executor = TickExecutor::new();
        let out = executor.step(&mut state, &[]);
        assert_eq!(out.state.tick, 1);
        assert!(out.state.bodies.iter().any(|b| b.name == "Earth"));
    }

    #[test]
    fn ship_burn_consumes_fuel_and_changes_velocity() {
        let mut state = SimulationState::new();
        state
            .ships
            .push(make_ship(1, DVec2::new(crate::units::AU, 0.0)));
        // Override engine to produce thrust.
        state.ships[0].engine_mounts[0].max_thrust = 1000.0;
        state.ships[0].engine_mounts[0].max_mass_flow = 0.34;
        state.ships[0].engine_mounts[0].specific_impulse = 300.0;

        let cmd = ShipCommand {
            ship_id: 1,
            throttle: 1.0,
            gimbal: 0.0,
            emitter_states: vec![],
        };

        let before_v = state.ships[0].velocity;
        let before_fuel = state.ships[0].fuel_mass;
        let executor = TickExecutor::new();
        executor.step(&mut state, &[cmd]);

        assert!(state.ships[0].fuel_mass < before_fuel);
        assert!((state.ships[0].velocity - before_v).length() > 1e-3);
    }

    #[test]
    fn thrust_emits_engine_signature_signal() {
        let mut state = SimulationState::new();
        state
            .ships
            .push(make_ship(1, DVec2::new(crate::units::AU, 0.0)));
        state.ships[0].engine_mounts[0].max_thrust = 1000.0;
        state.ships[0].engine_mounts[0].max_mass_flow = 0.34;
        state.ships[0].engine_mounts[0].specific_impulse = 300.0;

        let cmd = ShipCommand {
            ship_id: 1,
            throttle: 1.0,
            gimbal: 0.0,
            emitter_states: vec![],
        };

        let executor = TickExecutor::new();
        let output = executor.step(&mut state, &[cmd]);

        assert!(
            !output.state.signals.is_empty(),
            "expected at least one signal from engine plume"
        );
        let has_engine_thermal = output
            .state
            .signals
            .iter()
            .any(|arc| arc.spectrum.bins[WavelengthBin::EngineThermal as usize] > 0.0);
        assert!(has_engine_thermal, "expected an engine-thermal signal arc");
    }
}
