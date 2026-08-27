//! Ship propulsion using the rocket equation.
//!
//! Thrust is computed from fuel mass flow and specific impulse. Engines are
//! mounted at arbitrary local positions, so a single engine can produce both
//! linear force and torque.

use crate::state::{Ship, ShipCommand, Spectrum, WavelengthBin};
use crate::units::STANDARD_GRAVITY;
use glam::DVec2;

/// Force, torque, fuel use, and engine signature produced by a thrust step.
#[derive(Debug, Clone, PartialEq)]
pub struct ThrustResult {
    /// Total world-space force in newtons.
    pub force: DVec2,
    /// Total torque in newton-meters.
    pub torque: f64,
    /// Fuel mass consumed in this step (kg).
    pub fuel_consumed: f64,
    /// Optional engine-exhaust signature to emit as a signal arc.
    pub signature: Option<EngineSignature>,
}

/// Description of the thermal/signal plume produced by firing engines.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineSignature {
    pub origin: DVec2,
    pub direction: f64,
    pub angular_width: f64,
    pub spectrum: Spectrum,
}

/// Compute thrust effects for a ship given a command.
///
/// The command's `throttle` is clamped to [0, 1] and applied uniformly to all
/// engines. The command's `gimbal` overrides the main (first) engine gimbal;
/// additional engines keep their configured gimbal.
pub fn compute_thrust(ship: &Ship, command: &ShipCommand, dt: f64) -> ThrustResult {
    let mut total_force = DVec2::ZERO;
    let mut total_torque = 0.0;
    let mut fuel_consumed = 0.0;
    let mut any_firing = false;

    let _ship_heading = crate::state::heading_vector(ship.orientation);

    for (idx, engine) in ship.engine_mounts.iter().enumerate() {
        let throttle = command.throttle.clamp(0.0, 1.0);
        if throttle <= 0.0 {
            continue;
        }

        let gimbal = if idx == 0 {
            command.gimbal
        } else {
            engine.gimbal
        };

        let mass_flow = throttle * engine.max_mass_flow;
        let thrust_magnitude = engine.specific_impulse * STANDARD_GRAVITY * mass_flow;
        let local_dir = crate::state::heading_vector(gimbal);
        let world_dir = rotate_vector(local_dir, ship.orientation);
        let force = world_dir * thrust_magnitude;

        // Engine mount position in world space, rotated by ship orientation.
        let mount_world = ship.position + rotate_vector(engine.local_position, ship.orientation);
        let lever = mount_world - ship.position;
        let torque = lever.perp_dot(force);

        total_force += force;
        total_torque += torque;
        fuel_consumed += mass_flow * dt;
        any_firing = true;
    }

    // Clamp fuel consumption to available fuel.
    let fuel_consumed = fuel_consumed.min(ship.fuel_mass);

    let signature = if any_firing && fuel_consumed > 0.0 {
        // Engine exhaust is roughly opposite the net thrust direction.
        let exhaust_dir = if total_force.length_squared() > 0.0 {
            (-total_force).angle_to(DVec2::X)
        } else {
            ship.orientation + std::f64::consts::PI
        };
        let mut spectrum = Spectrum::zero();
        // Engine thermal signature dominates.
        spectrum.bins[WavelengthBin::EngineThermal as usize] = fuel_consumed * 1.0e7;
        spectrum.bins[WavelengthBin::Infrared as usize] = fuel_consumed * 5.0e6;
        Some(EngineSignature {
            origin: ship.position,
            direction: exhaust_dir,
            angular_width: 0.5, // ~28 deg plume
            spectrum,
        })
    } else {
        None
    };

    ThrustResult {
        force: total_force,
        torque: total_torque,
        fuel_consumed,
        signature,
    }
}

fn rotate_vector(v: DVec2, angle: f64) -> DVec2 {
    let (s, c) = angle.sin_cos();
    DVec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CollisionResponse, EngineMount, Ship, ThermalState};

    fn test_ship_with_engine(local_pos: DVec2) -> Ship {
        Ship {
            id: 1,
            name: "test".into(),
            dry_mass: 1000.0,
            fuel_mass: 1000.0,
            position: DVec2::ZERO,
            velocity: DVec2::ZERO,
            orientation: 0.0,
            angular_velocity: 0.0,
            moment_of_inertia: 10000.0,
            engine_mounts: vec![EngineMount {
                local_position: local_pos,
                max_thrust: 1000.0,
                specific_impulse: 300.0,
                max_mass_flow: 0.34,
                gimbal: 0.0,
            }],
            sensor_arrays: vec![],
            emitters: vec![],
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            collision_response: CollisionResponse::Ghost,
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn centered_engine_produces_no_torque() {
        let ship = test_ship_with_engine(DVec2::ZERO);
        let cmd = ShipCommand {
            ship_id: 1,
            throttle: 1.0,
            gimbal: 0.0,
            emitter_states: vec![],
        };
        let result = compute_thrust(&ship, &cmd, 1.0);
        assert!(result.force.x > 0.0);
        assert!(result.force.y.abs() < 1e-9);
        assert!(result.torque.abs() < 1e-9);
    }

    #[test]
    fn off_center_engine_produces_torque() {
        let ship = test_ship_with_engine(DVec2::new(0.0, 5.0));
        let cmd = ShipCommand {
            ship_id: 1,
            throttle: 1.0,
            gimbal: 0.0,
            emitter_states: vec![],
        };
        let result = compute_thrust(&ship, &cmd, 1.0);
        assert!(result.force.x > 0.0);
        assert!(result.torque.abs() > 1e-6);
    }

    #[test]
    fn fuel_consumption_clamped() {
        let mut ship = test_ship_with_engine(DVec2::ZERO);
        ship.fuel_mass = 0.01;
        let cmd = ShipCommand {
            ship_id: 1,
            throttle: 1.0,
            gimbal: 0.0,
            emitter_states: vec![],
        };
        let result = compute_thrust(&ship, &cmd, 10.0);
        assert_eq!(result.fuel_consumed, 0.01);
    }
}
