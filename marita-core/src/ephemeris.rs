//! Solar-system ephemeris loading.
//!
//! The production path loads initial state vectors from local SPICE kernels.
//! A fallback `CircularOrbitLoader` is provided so the engine can run without
//! SPICE dependencies during development and testing.

use crate::state::{Body, CollisionResponse, Spectrum, ThermalState};
use crate::units::{AU, SOLAR_MASS, SOLAR_RADIUS, SUN_EFFECTIVE_TEMPERATURE};
use glam::DVec2;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Trait for loading the initial solar-system bodies at a given epoch.
pub trait EphemerisLoader {
    fn load(&self) -> Vec<Body>;
}

#[derive(Debug, thiserror::Error)]
pub enum EphemerisError {
    #[error("failed to read ephemeris file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse ephemeris JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Loader that reads a JSON snapshot produced by `scripts/generate_ephemeris.py`.
///
/// The JSON is expected to contain 3D state vectors in a heliocentric/ecliptic
/// frame. The loader projects positions and velocities onto the ecliptic (XY)
/// plane because MaritaV3 is a 2D engine. Body masses are filled from a built-in
/// table; if a body is not in the table it receives zero mass.
pub struct JsonFileLoader {
    pub path: std::path::PathBuf,
}

impl JsonFileLoader {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn load_result(&self) -> Result<Vec<Body>, EphemerisError> {
        let text = std::fs::read_to_string(&self.path)?;
        let snapshot: EphemerisSnapshot = serde_json::from_str(&text)?;

        let mut bodies = Vec::new();
        for entry in snapshot.bodies {
            let mass = known_mass(entry.id, &entry.name);
            bodies.push(make_body(
                entry.id as u64,
                &entry.name,
                mass,
                entry.radius,
                DVec2::new(entry.position.x, entry.position.y),
                DVec2::new(entry.velocity.x, entry.velocity.y),
                effective_temperature(&entry.name),
            ));
        }
        Ok(bodies)
    }
}

impl EphemerisLoader for JsonFileLoader {
    fn load(&self) -> Vec<Body> {
        self.load_result().unwrap_or_else(|e| {
            eprintln!(
                "failed to load ephemeris from {}: {e}; using circular fallback",
                self.path.display()
            );
            CircularOrbitLoader.load()
        })
    }
}

#[derive(Debug, Deserialize)]
struct EphemerisSnapshot {
    #[allow(dead_code)]
    epoch: String,
    #[allow(dead_code)]
    frame: String,
    #[allow(dead_code)]
    observer: String,
    bodies: Vec<BodySnapshot>,
}

#[derive(Debug, Deserialize)]
struct BodySnapshot {
    id: i64,
    name: String,
    #[allow(dead_code)]
    mass: f64,
    position: Vec3Snapshot,
    velocity: Vec3Snapshot,
    radius: f64,
}

#[derive(Debug, Deserialize)]
struct Vec3Snapshot {
    x: f64,
    y: f64,
    #[allow(dead_code)]
    z: f64,
}

fn known_mass(_id: i64, name: &str) -> f64 {
    // Approximate masses in kg for major bodies. SPICE does not provide masses,
    // so we use a hard-coded lookup table.
    let table: HashMap<&str, f64> = [
        ("Sun", 1.98847e30),
        ("Mercury", 3.3011e23),
        ("Venus", 4.8675e24),
        ("Earth", 5.9723e24),
        ("Moon", 7.3477e22),
        ("Mars", 6.4171e23),
        ("Phobos", 1.0659e16),
        ("Deimos", 1.4762e15),
        ("Jupiter", 1.8982e27),
        ("Io", 8.9319e22),
        ("Europa", 4.7998e22),
        ("Ganymede", 1.4819e23),
        ("Callisto", 1.0759e23),
        ("Saturn", 5.6834e26),
        ("Mimas", 3.7493e19),
        ("Enceladus", 1.0802e20),
        ("Tethys", 6.1745e20),
        ("Dione", 1.0955e21),
        ("Rhea", 2.3065e21),
        ("Titan", 1.3452e23),
        ("Hyperion", 5.62e18),
        ("Iapetus", 1.8056e21),
        ("Phoebe", 8.292e18),
        ("Uranus", 8.6810e25),
        ("Miranda", 6.59e19),
        ("Ariel", 1.353e21),
        ("Umbriel", 1.172e21),
        ("Titania", 3.527e21),
        ("Oberon", 3.014e21),
        ("Neptune", 1.02413e26),
        ("Triton", 2.14e22),
        ("Nereid", 3.1e19),
        ("Naiad", 1.9e17),
        ("Thalassa", 3.5e17),
        ("Despina", 2.1e18),
        ("Galatea", 2.12e18),
        ("Larissa", 4.95e18),
        ("Proteus", 4.4e19),
        ("Pluto", 1.303e22),
        ("Charon", 1.586e21),
        ("Ceres", 9.3835e20),
        ("Pallas", 2.11e20),
        ("Vesta", 2.5908e20),
        ("Hygiea", 8.32e19),
        ("Psyche", 2.29e19),
        ("Davida", 3.66e19),
        ("Interamnia", 3.5e19),
        ("Europa", 4.7998e22),         // Jupiter moon; keep first as canonical
        ("Europa (asteroid)", 3.2e19), // asteroid 52 Europa
        ("Sylvia", 1.478e19),
    ]
    .iter()
    .copied()
    .collect();

    table.get(name).copied().unwrap_or(0.0)
}

fn effective_temperature(name: &str) -> f64 {
    match name {
        "Sun" => SUN_EFFECTIVE_TEMPERATURE,
        "Mercury" => 440.0,
        "Venus" => 737.0,
        "Earth" => 288.0,
        "Moon" => 250.0,
        "Mars" => 210.0,
        "Jupiter" => 165.0,
        "Saturn" => 134.0,
        "Uranus" => 76.0,
        "Neptune" => 72.0,
        _ => 100.0,
    }
}

/// Simplified circular-orbit loader for development and unit tests.
///
/// Provides the Sun and the eight planets at roughly correct semi-major axes,
/// masses, and radii. Orbits are circular and coplanar, which is sufficient
/// for integration tests and load tests but not scientifically accurate.
pub struct CircularOrbitLoader;

impl EphemerisLoader for CircularOrbitLoader {
    fn load(&self) -> Vec<Body> {
        let mut bodies = Vec::new();

        // Sun
        bodies.push(make_body(
            1,
            "Sun",
            SOLAR_MASS,
            SOLAR_RADIUS,
            DVec2::ZERO,
            DVec2::ZERO,
            SUN_EFFECTIVE_TEMPERATURE,
        ));

        // Planet data: (id, name, mass kg, radius m, semi-major axis AU)
        let planets = vec![
            (2, "Mercury", 3.3011e23, 2.4397e6, 0.387),
            (3, "Venus", 4.8675e24, 6.0518e6, 0.723),
            (4, "Earth", 5.9723e24, 6.371e6, 1.0),
            (5, "Mars", 6.4171e23, 3.3895e6, 1.524),
            (6, "Jupiter", 1.8982e27, 6.9911e7, 5.204),
            (7, "Saturn", 5.6834e26, 5.8232e7, 9.582),
            (8, "Uranus", 8.6810e25, 2.5362e7, 19.20),
            (9, "Neptune", 1.02413e26, 2.4622e7, 30.05),
        ];

        for (id, name, mass, radius, a) in planets {
            let r = a * AU;
            // Circular orbit velocity: sqrt(GM/r)
            let v = (crate::units::GRAVITATIONAL_CONSTANT * SOLAR_MASS / r).sqrt();
            bodies.push(make_body(
                id,
                name,
                mass,
                radius,
                DVec2::new(r, 0.0),
                DVec2::new(0.0, v),
                0.0,
            ));
        }

        bodies
    }
}

fn make_body(
    id: u64,
    name: &str,
    mass: f64,
    radius: f64,
    position: DVec2,
    velocity: DVec2,
    temperature: f64,
) -> Body {
    let mut albedo = Spectrum::zero();
    // Optical albedo is non-zero for most bodies.
    albedo.bins[crate::state::WavelengthBin::Optical as usize] = 0.3;

    Body {
        id,
        name: name.into(),
        mass,
        position,
        velocity,
        radius,
        collision_response: CollisionResponse::Merge,
        thermal: ThermalState::new(
            temperature,
            mass * 1000.0,
            4.0 * std::f64::consts::PI * radius * radius,
        ),
        albedo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_loader_provides_sun_and_planets() {
        let loader = CircularOrbitLoader;
        let bodies = loader.load();
        assert!(bodies.iter().any(|b| b.name == "Sun"));
        assert!(bodies.iter().any(|b| b.name == "Earth"));
    }

    #[test]
    fn earth_circular_velocity_matches_expected() {
        let loader = CircularOrbitLoader;
        let bodies = loader.load();
        let earth = bodies.iter().find(|b| b.name == "Earth").unwrap();
        let expected_v = (crate::units::GRAVITATIONAL_CONSTANT * SOLAR_MASS / AU).sqrt();
        assert!((earth.velocity.y - expected_v).abs() < 1.0);
    }
}
