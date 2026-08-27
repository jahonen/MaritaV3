//! Integration tests for the MaritaV3 engine.

use glam::DVec2;
use marita_core::ephemeris::{CircularOrbitLoader, EphemerisLoader};
use marita_core::state::{
    Body, CollisionResponse, EngineMount, SensorArray, Ship, SimulationState, Spectrum,
    ThermalState,
};
use marita_core::tick::TickExecutor;
use marita_core::units::AU;

/// Helper: create a default ship for tests.
fn default_ship(id: u64, position: DVec2) -> Ship {
    Ship {
        id,
        name: format!("ship-{id}"),
        dry_mass: 1000.0,
        fuel_mass: 0.0,
        position,
        velocity: DVec2::ZERO,
        orientation: 0.0,
        angular_velocity: 0.0,
        moment_of_inertia: 10000.0,
        engine_mounts: vec![],
        sensor_arrays: vec![],
        emitters: vec![],
        thermal: ThermalState::new(300.0, 1.0e6, 10.0),
        collision_response: CollisionResponse::Ghost,
        albedo: Spectrum::zero(),
    }
}

#[test]
fn simulation_is_deterministic() {
    let mut state_a = SimulationState::new();
    state_a.bodies = CircularOrbitLoader.load();
    state_a
        .ships
        .push(default_ship(1000, DVec2::new(AU * 1.01, 0.0)));

    let mut state_b = state_a.clone();

    let executor = TickExecutor::new();
    for _ in 0..100 {
        executor.step(&mut state_a, &[]);
        executor.step(&mut state_b, &[]);
    }

    assert_eq!(state_a, state_b, "state should be deterministic");
}

fn make_sun_earth_system() -> Vec<Body> {
    CircularOrbitLoader
        .load()
        .into_iter()
        .filter(|b| b.name == "Sun" || b.name == "Earth")
        .collect()
}

#[test]
fn earth_orbit_period_is_stable() {
    let mut state = SimulationState::new();
    state.bodies = make_sun_earth_system();

    let earth_idx = state
        .bodies
        .iter()
        .position(|b| b.name == "Earth")
        .expect("Earth should exist");
    let start = state.bodies[earth_idx].position;

    let executor = TickExecutor::new();
    // Short-run stability check. A full-year test is planned but too slow in
    // debug because of unoptimized signal propagation.
    let ticks = 1000;
    for _ in 0..ticks {
        executor.step(&mut state, &[]);
    }

    let end = state.bodies[earth_idx].position;
    let error = (end - start).length();
    // Over ~2.7 hours Earth should move only a fraction of its orbit.
    assert!(
        error < 0.01 * AU,
        "Earth position drifted by {error:.3e} m after {ticks} ticks"
    );
}

#[test]
fn load_test_fifty_bodies_and_one_thousand_ships() {
    let mut state = SimulationState::new();
    state.bodies = CircularOrbitLoader.load();

    for i in 0..1000 {
        let angle = (i as f64) * 2.0 * std::f64::consts::PI / 1000.0;
        let r = AU * (0.5 + 0.5 * (i as f64) / 1000.0);
        let pos = DVec2::new(r * angle.cos(), r * angle.sin());
        state.ships.push(default_ship(1000 + i as u64, pos));
    }

    let executor = TickExecutor::new();
    let start = std::time::Instant::now();
    for _ in 0..10 {
        executor.step(&mut state, &[]);
    }
    let elapsed = start.elapsed();

    println!(
        "Load test: 50 bodies + 1000 ships, 10 ticks took {:?}",
        elapsed
    );

    // Target: less than 1 second real time per tick.
    assert!(
        elapsed.as_secs_f64() < 10.0,
        "tick loop too slow: {:?} for 10 ticks",
        elapsed
    );
}

#[test]
fn ship_burn_changes_orbital_energy() {
    let mut state = SimulationState::new();
    state.bodies = CircularOrbitLoader.load();

    // Place a ship in circular Earth orbit with enough fuel for a burn.
    let earth_idx = state.bodies.iter().position(|b| b.name == "Earth").unwrap();
    let earth_pos = state.bodies[earth_idx].position;
    let earth_v = state.bodies[earth_idx].velocity;

    let ship = Ship {
        id: 1000,
        name: "burner".into(),
        dry_mass: 1000.0,
        fuel_mass: 1000.0,
        position: earth_pos + DVec2::new(1.0e6, 0.0),
        velocity: earth_v + DVec2::new(0.0, 1.0e3),
        orientation: std::f64::consts::FRAC_PI_2, // thrust in +Y
        angular_velocity: 0.0,
        moment_of_inertia: 10000.0,
        engine_mounts: vec![EngineMount {
            local_position: DVec2::ZERO,
            max_thrust: 1.0e6,
            specific_impulse: 450.0,
            max_mass_flow: 2.27,
            gimbal: 0.0,
        }],
        sensor_arrays: vec![SensorArray {
            local_position: DVec2::ZERO,
            bearing: 0.0,
            field_of_view: std::f64::consts::PI,
            bands: [true; 10],
            aperture_area: 1.0,
            noise_floor: 1.0,
            integration_time: 10.0,
            min_snr: 1.0,
        }],
        emitters: vec![],
        thermal: ThermalState::new(300.0, 1.0e6, 10.0),
        collision_response: CollisionResponse::Ghost,
        albedo: Spectrum::zero(),
    };
    state.ships.push(ship);

    let before_energy = orbital_energy(&state.ships[0], &state.bodies[earth_idx]);

    let cmd = marita_core::state::ShipCommand {
        ship_id: 1000,
        throttle: 1.0,
        gimbal: 0.0,
        emitter_states: vec![],
    };

    let executor = TickExecutor::new();
    executor.step(&mut state, &[cmd]);

    let after_energy = orbital_energy(&state.ships[0], &state.bodies[earth_idx]);
    assert!(
        (after_energy - before_energy).abs() > 1.0e3,
        "burn did not meaningfully change orbital energy"
    );
}

fn orbital_energy(ship: &Ship, planet: &Body) -> f64 {
    let r = (ship.position - planet.position).length();
    let v2 = ship.velocity.length_squared();
    0.5 * ship.mass() * v2
        - marita_core::units::GRAVITATIONAL_CONSTANT * planet.mass * ship.mass() / r
}
