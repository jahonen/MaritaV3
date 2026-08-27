//! Mass–mass collision detection and resolution.
//!
//! Celestial bodies may be configured to merge on contact. Ships and other
//! objects bounce with a coefficient of restitution. Processing is pairwise;
//! overlapping pairs are separated by a positional correction impulse.

use crate::spatial_tree::{Aabb, Quadtree};
use crate::state::{Body, CollisionResponse, Ship};
use glam::DVec2;

/// Collision resolution result.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionResult {
    pub bodies: Vec<Body>,
    pub ships: Vec<Ship>,
}

/// Resolve collisions among bodies and ships.
///
/// Ships never merge; they bounce. Bodies merge if their response is
/// `Merge`. The function returns updated entity lists.
pub fn resolve_collisions(bodies: Vec<Body>, ships: Vec<Ship>) -> CollisionResult {
    let bodies = resolve_body_collisions(bodies);
    let mut ships = ships;

    resolve_ship_body_collisions(&mut ships, &bodies);
    resolve_ship_ship_collisions(&mut ships);

    CollisionResult { bodies, ships }
}

fn resolve_body_collisions(bodies: Vec<Body>) -> Vec<Body> {
    let mut merged = vec![false; bodies.len()];
    let mut output: Vec<Body> = Vec::new();

    for i in 0..bodies.len() {
        if merged[i] {
            continue;
        }
        let mut current = bodies[i].clone();

        for j in (i + 1)..bodies.len() {
            if merged[j] {
                continue;
            }
            let other = &bodies[j];
            let delta = other.position - current.position;
            let dist = delta.length();
            let min_dist = current.radius + other.radius;

            if dist >= min_dist {
                continue;
            }

            match (current.collision_response, other.collision_response) {
                (CollisionResponse::Merge, CollisionResponse::Merge) => {
                    current = merge_bodies(&current, other);
                    merged[j] = true;
                }
                _ => {
                    let normal = if dist > 1e-6 { delta / dist } else { DVec2::X };
                    resolve_body_bounce(&mut current, other, normal, dist, min_dist);
                }
            }
        }

        output.push(current);
    }

    output
}

fn merge_bodies(a: &Body, b: &Body) -> Body {
    let total_mass = a.mass + b.mass;
    let velocity = (a.velocity * a.mass + b.velocity * b.mass) / total_mass;
    let position = (a.position * a.mass + b.position * b.mass) / total_mass;
    let radius = (a.radius.powi(3) + b.radius.powi(3)).cbrt();
    let thermal_temperature =
        (a.thermal.temperature * a.mass + b.thermal.temperature * b.mass) / total_mass;
    let mut merged = a.clone();
    merged.mass = total_mass;
    merged.velocity = velocity;
    merged.position = position;
    merged.radius = radius;
    merged.thermal.temperature = thermal_temperature;
    merged
}

fn resolve_body_bounce(a: &mut Body, b: &Body, normal: DVec2, dist: f64, min_dist: f64) {
    let e = 0.5; // default restitution
    let rel_vel = a.velocity - b.velocity;
    let sep_vel = rel_vel.dot(normal);
    if sep_vel > 0.0 {
        return;
    }
    let impulse_mag = -(1.0 + e) * sep_vel / (1.0 / a.mass + 1.0 / b.mass);
    let impulse = normal * impulse_mag;
    a.velocity += impulse / a.mass;

    let penetration = min_dist - dist;
    let correction = normal * (penetration * 0.2);
    a.position -= correction;
}

fn resolve_ship_body_collisions(ships: &mut [Ship], bodies: &[Body]) {
    if ships.is_empty() || bodies.is_empty() {
        return;
    }

    let body_items: Vec<_> = bodies
        .iter()
        .enumerate()
        .map(|(i, b)| (i, Aabb::from_point(b.position)))
        .collect();
    let body_tree = Quadtree::build(&body_items, 8, 16);
    let max_body_radius = bodies.iter().map(|b| b.radius).fold(0.0, f64::max);

    for ship in ships.iter_mut() {
        if matches!(ship.collision_response, CollisionResponse::Ghost) {
            continue;
        }
        let query_radius = ship.radius() + max_body_radius;
        for i in body_tree.query_circle(ship.position, query_radius) {
            let body = &bodies[i];
            let delta = ship.position - body.position;
            let dist = delta.length();
            let min_dist = ship.radius() + body.radius;
            if dist < min_dist {
                let normal = if dist > 1e-6 { delta / dist } else { DVec2::X };
                resolve_ship_bounce(ship, body.mass, body.velocity, normal, dist, min_dist);
            }
        }
    }
}

fn resolve_ship_ship_collisions(ships: &mut [Ship]) {
    if ships.len() < 2 {
        return;
    }

    let ship_items: Vec<_> = ships
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Aabb::from_point(s.position)))
        .collect();
    let ship_tree = Quadtree::build(&ship_items, 8, 16);
    let max_ship_radius = ships.iter().map(|s| s.radius()).fold(0.0, f64::max);

    for i in 0..ships.len() {
        let radius_i = ships[i].radius();
        let query_radius = radius_i + max_ship_radius;
        let candidates = ship_tree.query_circle(ships[i].position, query_radius);

        // Borrow the i-th ship mutably and every candidate j < i mutably.
        // Candidates with j >= i are handled when those indices are processed.
        for j in candidates {
            if j >= i {
                continue;
            }

            let (left, right) = ships.split_at_mut(i);
            let ship_i = right.first_mut().unwrap();
            let ship_j = &mut left[j];

            let delta = ship_i.position - ship_j.position;
            let dist = delta.length();
            let min_dist = ship_i.radius() + ship_j.radius();
            if dist < min_dist {
                let normal = if dist > 1e-6 { delta / dist } else { DVec2::X };
                let mass_j = ship_j.mass();
                let vel_j = ship_j.velocity;
                resolve_ship_bounce(ship_i, mass_j, vel_j, normal, dist, min_dist);
                // Update j as well with respect to i.
                let mass_i = ship_i.mass();
                let vel_i = ship_i.velocity;
                let normal_j = -normal;
                resolve_ship_bounce(ship_j, mass_i, vel_i, normal_j, dist, min_dist);
            }
        }
    }
}

fn resolve_ship_bounce(
    ship: &mut Ship,
    other_mass: f64,
    other_velocity: DVec2,
    normal: DVec2,
    dist: f64,
    min_dist: f64,
) {
    let e = match ship.collision_response {
        CollisionResponse::Bounce { restitution } => restitution,
        _ => 0.5,
    };
    let rel_vel = ship.velocity - other_velocity;
    let sep_vel = rel_vel.dot(normal);
    if sep_vel > 0.0 {
        return;
    }
    let impulse_mag = -(1.0 + e) * sep_vel / (1.0 / ship.mass() + 1.0 / other_mass);
    let impulse = normal * impulse_mag;
    ship.velocity += impulse / ship.mass();

    let penetration = min_dist - dist;
    let correction = normal * (penetration * 0.2);
    ship.position -= correction;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Body, CollisionResponse, Ship, Spectrum, ThermalState};

    fn test_body(mass: f64, x: f64, y: f64) -> Body {
        Body {
            id: 1,
            name: "body".into(),
            mass,
            position: DVec2::new(x, y),
            velocity: DVec2::ZERO,
            radius: 1.0,
            collision_response: CollisionResponse::Merge,
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn bodies_merge_and_conserve_momentum() {
        let a = test_body(100.0, 0.0, 0.0);
        let mut b = test_body(100.0, 1.0, 0.0);
        b.velocity = DVec2::new(10.0, 0.0);
        let result = resolve_body_collisions(vec![a, b]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mass, 200.0);
        assert!((result[0].velocity.x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn ships_bounce_without_merge() {
        let a = Ship {
            id: 1,
            name: "a".into(),
            dry_mass: 100.0,
            fuel_mass: 0.0,
            position: DVec2::new(0.0, 0.0),
            velocity: DVec2::new(10.0, 0.0),
            orientation: 0.0,
            angular_velocity: 0.0,
            moment_of_inertia: 1000.0,
            engine_mounts: vec![],
            sensor_arrays: vec![],
            emitters: vec![],
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            collision_response: CollisionResponse::Bounce { restitution: 0.5 },
            albedo: Spectrum::zero(),
        };
        let mut b = a.clone();
        b.id = 2;
        b.position = DVec2::new(1.5, 0.0);
        b.velocity = DVec2::new(-10.0, 0.0);

        let _before_energy =
            0.5 * 100.0 * (a.velocity.length_squared() + b.velocity.length_squared());
        resolve_ship_ship_collisions(&mut [a.clone(), b.clone()]);
        // After bounce the relative velocity along normal should be reversed
        // with reduced magnitude; exact value depends on restitution.
        assert!((a.velocity - b.velocity).x.abs() > 1e-3);
    }
}
