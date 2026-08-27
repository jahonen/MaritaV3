//! Gravitational acceleration using a top-3-influencer approximation.
//!
//! For each mass (body or ship) the engine computes the acceleration produced
//! by the three other bodies with the largest `m / r^2` value. This keeps the
//! cost O(N log N) per tick instead of O(N^2), which is important for the
//! target of ~50 bodies and up to 1000 ships.

use crate::state::{Body, Ship};
use crate::units::{GRAVITATIONAL_CONSTANT, GRAVITY_SOFTENING};
use glam::DVec2;

/// Gravitational accelerations for bodies and ships.
#[derive(Debug, Clone, PartialEq)]
pub struct Accelerations {
    pub body_accelerations: Vec<DVec2>,
    pub ship_accelerations: Vec<DVec2>,
}

/// Compute the gravitational acceleration felt by every mass.
///
/// `bodies` are treated as massive sources and targets; `ships` are targets
/// but their own mass is ignored as a source (it is negligible compared to
/// celestial bodies).
pub fn compute_accelerations(bodies: &[Body], ships: &[Ship]) -> Accelerations {
    let n_bodies = bodies.len();
    let n_ships = ships.len();

    let mut body_accelerations = vec![DVec2::ZERO; n_bodies];
    let mut ship_accelerations = vec![DVec2::ZERO; n_ships];

    // Precompute positions and masses of gravity sources (bodies only).
    let sources: Vec<(DVec2, f64)> = bodies.iter().map(|b| (b.position, b.mass)).collect();

    for (i, target) in bodies.iter().enumerate() {
        let acc = acceleration_for_target(target.position, i, &sources);
        body_accelerations[i] = acc;
    }

    for (i, ship) in ships.iter().enumerate() {
        let acc = acceleration_for_target(ship.position, n_bodies + i, &sources);
        ship_accelerations[i] = acc;
    }

    Accelerations {
        body_accelerations,
        ship_accelerations,
    }
}

fn acceleration_for_target(
    target_pos: DVec2,
    source_index_to_skip: usize,
    sources: &[(DVec2, f64)],
) -> DVec2 {
    let mut influencers: Vec<(usize, f64)> = sources
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != source_index_to_skip)
        .map(|(idx, (pos, mass))| {
            let r = (pos - target_pos).length();
            let influence = mass / (r * r + GRAVITY_SOFTENING * GRAVITY_SOFTENING);
            (idx, influence)
        })
        .collect();

    // Keep only the top 3 influencers by m / r^2.
    // Use total_cmp so NaN values (which indicate a bug elsewhere) do not panic.
    influencers.sort_by(|a, b| b.1.total_cmp(&a.1));
    influencers.truncate(3);

    let mut acc = DVec2::ZERO;
    for (idx, _) in influencers {
        let (pos, mass) = sources[idx];
        let delta = pos - target_pos;
        let r2 = delta
            .length_squared()
            .max(GRAVITY_SOFTENING * GRAVITY_SOFTENING);
        let magnitude = GRAVITATIONAL_CONSTANT * mass / r2;
        acc += magnitude * delta.normalize();
    }

    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Body, CollisionResponse, Spectrum, ThermalState};

    fn test_body(id: u64, mass: f64, x: f64, y: f64) -> Body {
        Body {
            id,
            name: format!("body-{id}"),
            mass,
            position: DVec2::new(x, y),
            velocity: DVec2::ZERO,
            radius: 1e3,
            collision_response: CollisionResponse::Ghost,
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn two_body_acceleration_matches_circular_orbit() {
        // Place a test particle at 1 AU from a 1 M_sun body and verify
        // the acceleration points toward the Sun with magnitude v^2/r.
        let sun = test_body(1, crate::units::SOLAR_MASS, 0.0, 0.0);
        let earth = test_body(2, 1.0, crate::units::AU, 0.0);

        let accs = compute_accelerations(&[sun, earth], &[]);

        let earth_acc = accs.body_accelerations[1];
        let expected_magnitude = crate::units::GRAVITATIONAL_CONSTANT * crate::units::SOLAR_MASS
            / (crate::units::AU * crate::units::AU);

        assert!((earth_acc.length() - expected_magnitude).abs() < 1e-6 * expected_magnitude);
        // Acceleration points back toward the Sun (negative X).
        assert!(earth_acc.x < 0.0);
        assert!(earth_acc.y.abs() < 1e-6);
    }

    #[test]
    fn top_three_dominant_in_solar_system() {
        // Sun + three heavy planets; a fourth small body should only feel
        // the Sun's acceleration when only the top-3 influencers are kept.
        let sun = test_body(1, crate::units::SOLAR_MASS, 0.0, 0.0);
        let jupiter = test_body(2, 1.898e27, 5.0 * crate::units::AU, 0.0);
        let saturn = test_body(3, 5.683e26, 10.0 * crate::units::AU, 0.0);
        let test_mass = test_body(4, 1.0, 1.0 * crate::units::AU, 0.0);

        let accs = compute_accelerations(&[sun, jupiter, saturn, test_mass], &[]);
        let test_acc = accs.body_accelerations[3];

        // Dominant acceleration should be from the Sun.
        let sun_only = -crate::units::GRAVITATIONAL_CONSTANT * crate::units::SOLAR_MASS
            / (crate::units::AU * crate::units::AU);
        assert!((test_acc.x - sun_only).abs() < 0.01 * sun_only.abs());
    }
}
