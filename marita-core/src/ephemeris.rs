//! Solar-system ephemeris loading.
//!
//! The production path loads initial state vectors from local SPICE kernels.
//! A fallback `CircularOrbitLoader` is provided so the engine can run without
//! SPICE dependencies during development and testing.

use crate::state::{Body, CollisionResponse, Spectrum, ThermalState};
use crate::units::{AU, SOLAR_MASS, SOLAR_RADIUS, SUN_EFFECTIVE_TEMPERATURE};
use glam::DVec2;

/// Trait for loading the initial solar-system bodies at a given epoch.
pub trait EphemerisLoader {
    fn load(&self) -> Vec<Body>;
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
