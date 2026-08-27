//! Signal propagation, clipping, reflection, and emission.
//!
//! Signals travel as expanding arc segments (annular sectors) at the speed of
//! light. Each arc carries a per-wavelength information spectrum. When an arc
//! intersects a massive body, the occluded angular portion is either destroyed
//! (absorbed) or reflected, producing one or more new arcs.

use crate::spatial_tree::{Aabb, Quadtree};
use crate::state::{
    normalize_angle, Body, CollisionResponse, Ship, SignalArc, SimulationState, Spectrum,
    ThermalState, WavelengthBin,
};
use crate::units::{SOLAR_SYSTEM_BOUNDARY, SPEED_OF_LIGHT, STEFAN_BOLTZMANN};
use glam::DVec2;

/// Thermal energy absorbed by a body or ship from signals, per entity.
#[derive(Debug, Clone, PartialEq)]
pub struct AbsorbedEnergy {
    pub entity_id: u64,
    pub energy: f64,
    pub spectrum: Spectrum,
}

/// Result of clipping signals against masses.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipResult {
    pub remaining: Vec<SignalArc>,
    pub reflected: Vec<SignalArc>,
    pub absorbed: Vec<AbsorbedEnergy>,
}

/// Advance every signal by one tick.
///
/// Signals expand radially at `c`, their per-bin information degrades
/// exponentially, and arcs that have left the solar system or lost all
/// information are discarded.
pub fn propagate(signals: &mut Vec<SignalArc>, dt: f64) {
    let delta_radius = SPEED_OF_LIGHT * dt;
    signals.retain_mut(|arc| {
        arc.expand(delta_radius);
        arc.spectrum.degrade(&arc.degradation_rates, dt);
        arc.outer_radius <= SOLAR_SYSTEM_BOUNDARY && arc.spectrum.total() > 1e-30
    });
}

/// Clip all signals against massive bodies and ships.
pub fn clip_against_masses(signals: Vec<SignalArc>, bodies: &[Body], ships: &[Ship]) -> ClipResult {
    let mut remaining: Vec<SignalArc> = Vec::new();
    let mut reflected: Vec<SignalArc> = Vec::new();
    let mut absorbed_map: std::collections::HashMap<u64, (f64, Spectrum)> =
        std::collections::HashMap::new();

    // Build adaptive quadtrees for bodies and ships. Even for small counts the
    // cost is negligible and the code path stays uniform.
    let body_items: Vec<_> = bodies
        .iter()
        .enumerate()
        .map(|(i, b)| (i, Aabb::from_point(b.position)))
        .collect();
    let ship_items: Vec<_> = ships
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Aabb::from_point(s.position)))
        .collect();
    let body_tree = Quadtree::build(&body_items, 8, 16);
    let ship_tree = Quadtree::build(&ship_items, 8, 16);

    let max_body_radius = bodies.iter().map(|b| b.radius).fold(0.0, f64::max);
    let max_ship_radius = ships.iter().map(|s| s.radius()).fold(0.0, f64::max);

    for arc in signals {
        clip_arc(
            arc,
            bodies,
            ships,
            &body_tree,
            &ship_tree,
            max_body_radius,
            max_ship_radius,
            &mut remaining,
            &mut reflected,
            &mut absorbed_map,
        );
    }

    let absorbed = absorbed_map
        .into_iter()
        .map(|(entity_id, (energy, spectrum))| AbsorbedEnergy {
            entity_id,
            energy,
            spectrum,
        })
        .collect();

    ClipResult {
        remaining,
        reflected,
        absorbed,
    }
}

fn clip_arc(
    arc: SignalArc,
    bodies: &[Body],
    ships: &[Ship],
    body_tree: &Quadtree,
    ship_tree: &Quadtree,
    max_body_radius: f64,
    max_ship_radius: f64,
    remaining: &mut Vec<SignalArc>,
    reflected: &mut Vec<SignalArc>,
    absorbed_map: &mut std::collections::HashMap<u64, (f64, Spectrum)>,
) {
    // Build list of occluders as (entity_id, position, radius, response, albedo).
    let mut occluders: Vec<(u64, DVec2, f64, CollisionResponse, Spectrum)> = Vec::new();

    // Skip the body/ship that emitted this arc so thermal and intentional
    // emitters are not immediately occluded by their own source.
    let source_id = arc.source_id;

    let max_entity_radius = max_body_radius.max(max_ship_radius);
    let query_radius = arc.outer_radius + max_entity_radius;
    for i in body_tree.query_circle(arc.origin, query_radius) {
        let body = &bodies[i];
        if source_id == Some(body.id) {
            continue;
        }
        if !matches!(body.collision_response, CollisionResponse::Ghost) {
            let dist_sq = (body.position - arc.origin).length_squared();
            if dist_sq < (arc.outer_radius + body.radius) * (arc.outer_radius + body.radius) {
                occluders.push((
                    body.id,
                    body.position,
                    body.radius,
                    body.collision_response,
                    body.albedo,
                ));
            }
        }
    }
    for i in ship_tree.query_circle(arc.origin, query_radius) {
        let ship = &ships[i];
        if source_id == Some(ship.id) {
            continue;
        }
        if !matches!(ship.collision_response, CollisionResponse::Ghost) {
            let dist_sq = (ship.position - arc.origin).length_squared();
            if dist_sq < (arc.outer_radius + ship.radius()) * (arc.outer_radius + ship.radius()) {
                occluders.push((
                    ship.id,
                    ship.position,
                    ship.radius(),
                    ship.collision_response,
                    ship.albedo,
                ));
            }
        }
    }

    if occluders.is_empty() {
        remaining.push(arc);
        return;
    }

    // Compute occluded angular intervals.
    let mut occluded_intervals: Vec<(f64, f64)> = Vec::new();
    for (_, pos, radius, _, _) in &occluders {
        if let Some((start, end)) = angular_occlusion(arc.origin, *pos, *radius) {
            occluded_intervals.push((start, end));
        }
    }

    // Simplify by unioning intervals and capping to arc range.
    let arc_start = arc.direction - arc.angular_width / 2.0;
    let arc_end = arc.direction + arc.angular_width / 2.0;
    let occluded = union_intervals(&occluded_intervals, arc_start, arc_end);

    if occluded.is_empty() {
        remaining.push(arc);
        return;
    }

    let mut next_arc_id = arc.id; // dummy; real IDs allocated later
                                  // Remaining visible portions.
    let visible = subtract_intervals(arc_start, arc_end, &occluded);
    for (start, end) in visible {
        let mut sub = arc.clone();
        sub.direction = (start + end) / 2.0;
        sub.angular_width = end - start;
        next_arc_id += 1;
        sub.id = next_arc_id;
        remaining.push(sub);
    }

    // Occluded portions: absorb/reflect.
    let total_arc_span = arc.angular_width;
    for (start, end) in occluded {
        let frac = (end - start) / total_arc_span;
        let occluded_spectrum = arc.spectrum.scaled(frac);

        // Find the nearest occluder in this direction for material properties.
        let mid = (start + end) / 2.0;
        let dir = DVec2::new(mid.cos(), mid.sin());
        let mut nearest: Option<&(u64, DVec2, f64, CollisionResponse, Spectrum)> = None;
        let mut nearest_dist = f64::INFINITY;
        for occ in &occluders {
            let hit = ray_circle_intersection(arc.origin, dir, occ.1, occ.2);
            if let Some((d, _)) = hit {
                if d < nearest_dist {
                    nearest_dist = d;
                    nearest = Some(occ);
                }
            }
        }

        if let Some((id, pos, radius, _, albedo)) = nearest {
            // Absorbed portion = whatever is not reflected.
            let reflected_spectrum = occluded_spectrum.scaled_by_spectrum(albedo);
            // component-wise: absorbed = occluded - reflected
            let mut absorbed_spectrum = occluded_spectrum;
            for i in 0..absorbed_spectrum.bins.len() {
                absorbed_spectrum.bins[i] -= reflected_spectrum.bins[i];
            }

            let entry = absorbed_map.entry(*id).or_insert((0.0, Spectrum::zero()));
            entry.0 += absorbed_spectrum.total();
            entry.1.add(&absorbed_spectrum);

            // Spawn reflected arc if any reflected energy remains.
            if reflected_spectrum.total() > 1e-30 && arc.generation < 3 {
                let normal = (arc.origin - *pos).normalize();
                let incident = -dir;
                let reflected_dir = reflect_vector(incident, normal);
                let mut reflected_arc = arc.clone();
                reflected_arc.id = next_arc_id + 1;
                reflected_arc.origin = *pos + normal * (*radius + 1.0);
                reflected_arc.direction = reflected_dir.angle_to(DVec2::X);
                reflected_arc.angular_width = end - start;
                reflected_arc.spectrum = reflected_spectrum;
                reflected_arc.generation += 1;
                reflected.push(reflected_arc);
            }
        }
    }
}

/// Compute the angular interval from `origin` that is blocked by a circle.
fn angular_occlusion(origin: DVec2, center: DVec2, radius: f64) -> Option<(f64, f64)> {
    let delta = center - origin;
    let d2 = delta.length_squared();
    let r2 = radius * radius;
    if d2 <= r2 {
        // Origin is inside the circle; treat as full 360 occlusion.
        return Some((-std::f64::consts::PI, std::f64::consts::PI));
    }
    let d = d2.sqrt();
    let half_angle = (radius / d).asin();
    let center_angle = delta.angle_to(DVec2::X);
    let start = normalize_angle(center_angle - half_angle);
    let end = normalize_angle(center_angle + half_angle);
    Some((start, end))
}

/// Compute reflection of `incident` vector around `normal`.
fn reflect_vector(incident: DVec2, normal: DVec2) -> DVec2 {
    incident - 2.0 * incident.dot(normal) * normal
}

/// Intersect a ray `origin + t * dir` with a circle; returns near/far distances.
fn ray_circle_intersection(
    origin: DVec2,
    dir: DVec2,
    center: DVec2,
    radius: f64,
) -> Option<(f64, f64)> {
    let oc = origin - center;
    let a = dir.length_squared();
    let b = 2.0 * oc.dot(dir);
    let c = oc.length_squared() - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let sqrt = discriminant.sqrt();
    let t1 = (-b - sqrt) / (2.0 * a);
    let t2 = (-b + sqrt) / (2.0 * a);
    if t2 < 0.0 {
        return None;
    }
    Some((t1.max(0.0), t2))
}

/// Union a set of angular intervals, clamped to [range_start, range_end].
fn union_intervals(intervals: &[(f64, f64)], range_start: f64, range_end: f64) -> Vec<(f64, f64)> {
    if intervals.is_empty() {
        return Vec::new();
    }
    let range_start = normalize_angle(range_start);
    let range_end = normalize_angle(range_end);

    let mut points: Vec<(f64, bool)> = Vec::new();
    for (s, e) in intervals {
        // Handle wrap-around by splitting at PI boundary if necessary.
        let s = *s;
        let e = *e;
        if s > e {
            // Interval wraps across -PI/PI boundary.
            add_clipped_interval(s, std::f64::consts::PI, range_start, range_end, &mut points);
            add_clipped_interval(
                -std::f64::consts::PI,
                e,
                range_start,
                range_end,
                &mut points,
            );
        } else {
            add_clipped_interval(s, e, range_start, range_end, &mut points);
        }
    }

    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut depth = 0;
    let mut unioned: Vec<(f64, f64)> = Vec::new();
    let mut current_start: Option<f64> = None;
    for (angle, entering) in points {
        if entering {
            depth += 1;
            if depth == 1 {
                current_start = Some(angle);
            }
        } else {
            depth -= 1;
            if depth == 0 {
                if let Some(start) = current_start {
                    unioned.push((start, angle));
                    current_start = None;
                }
            }
        }
    }
    unioned
}

fn add_clipped_interval(
    s: f64,
    e: f64,
    range_start: f64,
    range_end: f64,
    points: &mut Vec<(f64, bool)>,
) {
    let start = s.max(range_start);
    let end = e.min(range_end);
    if start < end {
        points.push((start, true));
        points.push((end, false));
    }
}

/// Subtract a set of intervals from [a, b], returning the remaining pieces.
fn subtract_intervals(a: f64, b: f64, remove: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if remove.is_empty() {
        return vec![(a, b)];
    }
    let mut result: Vec<(f64, f64)> = Vec::new();
    let mut cursor = a;
    let mut sorted = remove.to_vec();
    sorted.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    for (s, e) in sorted {
        if s > cursor {
            result.push((cursor, s));
        }
        cursor = cursor.max(e);
        if cursor >= b {
            break;
        }
    }
    if cursor < b {
        result.push((cursor, b));
    }
    result
}

/// Emit new signal arcs from thermal blackbody, intentional emitters, and the
/// Sun. The returned arcs are at generation 0 with `outer_radius == 0` so they
/// will expand on the next propagation step.
pub fn emit_signals(state: &SimulationState, _dt: f64, next_id: &mut u64) -> Vec<SignalArc> {
    let mut emitted: Vec<SignalArc> = Vec::new();

    // Sun emits omnidirectional optical arcs every tick.
    // TODO: scale power to solar luminosity; for MVP use a representative value.
    if let Some(sun) = state
        .bodies
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case("sun"))
    {
        let mut spectrum = Spectrum::zero();
        spectrum.bins[WavelengthBin::Optical as usize] = 1.0e25;
        spectrum.bins[WavelengthBin::Ultraviolet as usize] = 1.0e23;
        let mut arc = SignalArc::new(
            *next_id,
            sun.position,
            0.0,
            2.0 * std::f64::consts::PI,
            spectrum,
        );
        arc.source_id = Some(sun.id);
        // Sunlight does not degrade in vacuum; removed at solar-system boundary.
        arc.degradation_rates = Spectrum::zero();
        *next_id += 1;
        emitted.push(arc);
    }

    // Thermal emission from all bodies and ships.
    for body in &state.bodies {
        if let Some(arc) = thermal_arc(body.id, body.position, &body.thermal, next_id) {
            emitted.push(arc);
        }
    }
    for ship in &state.ships {
        if let Some(arc) = thermal_arc(ship.id, ship.position, &ship.thermal, next_id) {
            emitted.push(arc);
        }
    }

    // Intentional ship emitters. Emitter `active` flags are updated from
    // `ShipCommand` by the tick executor before this function is called.
    for ship in &state.ships {
        for emitter in &ship.emitters {
            if !emitter.active {
                continue;
            }
            let world_dir = crate::state::heading_vector(ship.orientation + emitter.direction);
            let world_pos = ship.position + rotate_vector(emitter.local_position, ship.orientation);
            let mut spectrum = Spectrum::zero();
            spectrum.bins[emitter.wavelength_bin as usize] = emitter.max_info_per_tick;
            let mut arc = SignalArc::new(
                *next_id,
                world_pos,
                world_dir.angle_to(DVec2::X),
                emitter.angular_width,
                spectrum,
            );
            arc.source_id = Some(ship.id);
            *next_id += 1;
            emitted.push(arc);
        }
    }

    emitted
}

fn rotate_vector(v: DVec2, angle: f64) -> DVec2 {
    let (s, c) = angle.sin_cos();
    DVec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

fn thermal_arc(
    id: u64,
    pos: DVec2,
    thermal: &ThermalState,
    next_id: &mut u64,
) -> Option<SignalArc> {
    if thermal.temperature <= 0.0 {
        return None;
    }
    let power =
        thermal.emissivity * STEFAN_BOLTZMANN * thermal.surface_area * thermal.temperature.powi(4);
    if power <= 0.0 {
        return None;
    }
    let mut spectrum = Spectrum::zero();
    // Simplified: most energy in IR, some in optical for very hot bodies.
    spectrum.bins[WavelengthBin::Infrared as usize] = power;
    if thermal.temperature > 3000.0 {
        spectrum.bins[WavelengthBin::Optical as usize] = power * 0.01;
    }
    let mut arc = SignalArc::new(*next_id, pos, 0.0, 2.0 * std::f64::consts::PI, spectrum);
    arc.source_id = Some(id);
    // Thermal emission is continuous; treat each emitted pulse as fresh and
    // fade it so old pulses do not accumulate indefinitely.
    arc.degradation_rates.bins[WavelengthBin::Infrared as usize] = 1.0;
    arc.degradation_rates.bins[WavelengthBin::Optical as usize] = 1.0;
    *next_id += 1;
    Some(arc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Body, CollisionResponse, ThermalState};

    fn test_body(id: u64, x: f64, y: f64, radius: f64) -> Body {
        Body {
            id,
            name: format!("body-{id}"),
            mass: 1e20,
            position: DVec2::new(x, y),
            velocity: DVec2::ZERO,
            radius,
            collision_response: CollisionResponse::Bounce { restitution: 0.5 },
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn signal_expands_at_speed_of_light() {
        let mut arc = SignalArc::new(1, DVec2::ZERO, 0.0, 0.1, Spectrum::zero());
        arc.spectrum.bins[0] = 1.0;
        let mut signals = vec![arc];
        propagate(&mut signals, 1.0);
        assert_eq!(signals[0].outer_radius, SPEED_OF_LIGHT * 1.0);
        assert_eq!(signals[0].inner_radius, SPEED_OF_LIGHT * 1.0);
    }

    #[test]
    fn signal_removed_when_empty() {
        let mut arc = SignalArc::new(1, DVec2::ZERO, 0.0, 0.1, Spectrum::zero());
        arc.degradation_rates.bins[0] = 1e9;
        let mut signals = vec![arc];
        propagate(&mut signals, 10.0);
        assert!(signals.is_empty());
    }

    #[test]
    fn clip_arc_behind_body() {
        let arc = SignalArc::new(
            1,
            DVec2::ZERO,
            0.0,
            std::f64::consts::PI / 2.0,
            Spectrum::zero(),
        );
        let mut s = arc;
        s.spectrum.bins[WavelengthBin::Optical as usize] = 100.0;
        s.outer_radius = 1.0e10;

        let body = test_body(10, 1.0e9, 0.0, 5.0e8);
        let result = clip_against_masses(vec![s], &[body], &[]);

        // Some energy should be absorbed by the body.
        assert!(!result.absorbed.is_empty());
        assert!(!result.remaining.is_empty());
    }

    #[test]
    fn sun_emits_optical_signal() {
        let mut state = SimulationState::new();
        state.bodies.push(Body {
            id: 1,
            name: "Sun".into(),
            mass: crate::units::SOLAR_MASS,
            position: DVec2::ZERO,
            velocity: DVec2::ZERO,
            radius: crate::units::SOLAR_RADIUS,
            collision_response: CollisionResponse::Ghost,
            thermal: ThermalState::new(5772.0, 1.0, 1.0),
            albedo: Spectrum::zero(),
        });
        let mut next_id = 2u64;
        let emitted = emit_signals(&mut state, 1.0, &mut next_id);
        assert!(!emitted.is_empty());
        let sun_arc = emitted.iter().find(|a| a.source_id == Some(1)).unwrap();
        assert!(sun_arc.spectrum.bins[WavelengthBin::Optical as usize] > 0.0);
    }
}
