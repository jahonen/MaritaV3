//! Strict retarded-time evaluation of persistent passive radiation.

use crate::ambient::blackbody_spectrum;
use crate::history::{EntityKey, EntityKind, ObservationHistory, ObservationSample};
use crate::radiative_profile::bundled_catalog;
use crate::state::{Body, Ship, Spectrum, Station, WavelengthBin};
use crate::units::{SPEED_OF_LIGHT, STEFAN_BOLTZMANN};
use glam::DVec2;

#[derive(Debug, Clone)]
pub struct PassiveContribution {
    pub source: EntityKey,
    pub bearing: f64,
    pub distance: f64,
    pub emission_time: f64,
    pub spectrum: Spectrum,
}

pub fn observe_bodies(
    history: &ObservationHistory,
    bodies: &[Body],
    ships: &[Ship],
    stations: &[Station],
    observer_pos: DVec2,
    observer_body_id: Option<u64>,
    observation_time: f64,
    candidate_cap: usize,
) -> Vec<PassiveContribution> {
    let Some(sun) = bodies
        .iter()
        .find(|body| body.name.eq_ignore_ascii_case("sun"))
    else {
        return Vec::new();
    };
    let sun_key = EntityKey {
        kind: EntityKind::Body,
        id: sun.id,
    };
    let mut out = Vec::new();
    for body in bodies.iter().take(candidate_cap) {
        if Some(body.id) == observer_body_id {
            continue;
        }
        let key = EntityKey {
            kind: EntityKind::Body,
            id: body.id,
        };
        let Some((sample, emission_time, distance)) =
            solve_retarded(history, key, observer_pos, observation_time)
        else {
            continue;
        };
        let transmission = path_transmission(
            history,
            bodies,
            key,
            observer_body_id,
            sample.position,
            observer_pos,
            emission_time,
            observation_time,
        );
        if transmission <= 0.0 {
            continue;
        }
        let profile = bundled_catalog().get(&body.name);
        let mut spectrum = emitted_spectrum(&sample, profile.natural_luminosity);
        spectrum.scale(transmission / (4.0 * std::f64::consts::PI * distance * distance).max(1.0));
        if spectrum.total() > 0.0 {
            out.push(PassiveContribution {
                source: key,
                bearing: bearing(observer_pos, sample.position),
                distance,
                emission_time,
                spectrum,
            });
        }

        if key != sun_key {
            if let Some(reflected) = reflected_sunlight(
                history,
                bodies,
                sun_key,
                key,
                observer_pos,
                observer_body_id,
                observation_time,
            ) {
                out.push(reflected);
            }
        }
    }
    let remaining = candidate_cap.saturating_sub(bodies.len());
    for (key, name) in ships
        .iter()
        .map(|ship| {
            (
                EntityKey {
                    kind: EntityKind::Ship,
                    id: ship.id,
                },
                ship.name.as_str(),
            )
        })
        .chain(stations.iter().map(|station| {
            (
                EntityKey {
                    kind: EntityKind::Station,
                    id: station.id,
                },
                station.name.as_str(),
            )
        }))
        .take(remaining)
    {
        let Some((sample, emission_time, distance)) =
            solve_retarded(history, key, observer_pos, observation_time)
        else {
            continue;
        };
        if distance <= sample.radius {
            continue;
        }
        let transmission = path_transmission(
            history,
            bodies,
            key,
            observer_body_id,
            sample.position,
            observer_pos,
            emission_time,
            observation_time,
        );
        if transmission <= 0.0 {
            continue;
        }
        let profile = bundled_catalog().get(name);
        let mut spectrum = emitted_spectrum(&sample, profile.natural_luminosity);
        spectrum.scale(transmission / (4.0 * std::f64::consts::PI * distance * distance).max(1.0));
        if spectrum.total() > 0.0 {
            out.push(PassiveContribution {
                source: key,
                bearing: bearing(observer_pos, sample.position),
                distance,
                emission_time,
                spectrum,
            });
        }
    }
    out
}

fn emitted_spectrum(sample: &ObservationSample, natural: [f64; 10]) -> Spectrum {
    let thermal_power =
        sample.emissivity * STEFAN_BOLTZMANN * sample.radiating_area * sample.temperature.powi(4);
    let mut spectrum = blackbody_spectrum(sample.temperature).scaled(thermal_power);
    for (value, extra) in spectrum.bins.iter_mut().zip(natural) {
        *value += extra;
    }
    spectrum
}

fn reflected_sunlight(
    history: &ObservationHistory,
    bodies: &[Body],
    sun_key: EntityKey,
    reflector_key: EntityKey,
    observer_pos: DVec2,
    observer_body_id: Option<u64>,
    observation_time: f64,
) -> Option<PassiveContribution> {
    let (reflector, reflection_time, observer_distance) =
        solve_retarded(history, reflector_key, observer_pos, observation_time)?;
    let (sun, source_time, source_distance) =
        solve_retarded(history, sun_key, reflector.position, reflection_time)?;
    let incoming = path_transmission(
        history,
        bodies,
        sun_key,
        Some(reflector_key.id),
        sun.position,
        reflector.position,
        source_time,
        reflection_time,
    );
    let outgoing = path_transmission(
        history,
        bodies,
        reflector_key,
        observer_body_id,
        reflector.position,
        observer_pos,
        reflection_time,
        observation_time,
    );
    if incoming <= 0.0 || outgoing <= 0.0 {
        return None;
    }
    let sun_power = emitted_spectrum(&sun, [0.0; 10]);
    let incident = sun_power
        .scaled(1.0 / (4.0 * std::f64::consts::PI * source_distance * source_distance).max(1.0));
    let to_sun = (sun.position - reflector.position).normalize_or_zero();
    let to_observer = (observer_pos - reflector.position).normalize_or_zero();
    let alpha = to_sun.dot(to_observer).clamp(-1.0, 1.0).acos();
    let phase = (alpha.sin() + (std::f64::consts::PI - alpha) * alpha.cos()) / std::f64::consts::PI;
    let intercepted_area = std::f64::consts::PI * reflector.radius * reflector.radius;
    let mut reflected = incident
        .scaled(intercepted_area * phase.max(0.0) * incoming * outgoing)
        .scaled_by_spectrum(&reflector.albedo);
    reflected
        .scale(1.0 / (4.0 * std::f64::consts::PI * observer_distance * observer_distance).max(1.0));
    // Passive reflection is modeled only in physically reflected bins.
    reflected.bins[WavelengthBin::EngineThermal as usize] = 0.0;
    reflected.bins[WavelengthBin::Radar as usize] = 0.0;
    reflected.bins[WavelengthBin::Lidar as usize] = 0.0;
    if reflected.total() <= 0.0 {
        return None;
    }
    Some(PassiveContribution {
        source: reflector_key,
        bearing: bearing(observer_pos, reflector.position),
        distance: observer_distance,
        emission_time: source_time,
        spectrum: reflected,
    })
}

pub fn solve_retarded(
    history: &ObservationHistory,
    key: EntityKey,
    observer_pos: DVec2,
    observation_time: f64,
) -> Option<(ObservationSample, f64, f64)> {
    let latest = history.sample(key, history.newest_time()?)?;
    let mut emission_time =
        observation_time - (latest.position - observer_pos).length() / SPEED_OF_LIGHT;
    for _ in 0..3 {
        let sample = history.sample(key, emission_time)?;
        emission_time =
            observation_time - (sample.position - observer_pos).length() / SPEED_OF_LIGHT;
    }
    let sample = history.sample(key, emission_time)?;
    let distance = (sample.position - observer_pos).length();
    Some((sample, emission_time, distance))
}

fn path_transmission(
    history: &ObservationHistory,
    bodies: &[Body],
    source_key: EntityKey,
    destination_body_id: Option<u64>,
    source_pos: DVec2,
    destination_pos: DVec2,
    start_time: f64,
    end_time: f64,
) -> f64 {
    let segment = destination_pos - source_pos;
    let source_distance = segment.length();
    if source_distance <= 0.0 {
        return 1.0;
    }
    let source_radius = history
        .sample(source_key, start_time)
        .map(|s| s.radius)
        .unwrap_or(0.0);
    let source_angular_radius = (source_radius / source_distance).min(1.0).asin();
    let source_direction = (source_pos - destination_pos).normalize_or_zero();
    let mut transmission: f64 = 1.0;
    for body in bodies {
        if (source_key.kind == EntityKind::Body && body.id == source_key.id)
            || Some(body.id) == destination_body_id
        {
            continue;
        }
        let key = EntityKey {
            kind: EntityKind::Body,
            id: body.id,
        };
        let midpoint_time = (start_time + end_time) * 0.5;
        let Some(midpoint) = history.sample(key, midpoint_time) else {
            continue;
        };
        let fraction = ((midpoint.position - source_pos).dot(segment) / segment.length_squared())
            .clamp(0.0, 1.0);
        if !(0.0..1.0).contains(&fraction) {
            continue;
        }
        let crossing_time = start_time + fraction * (end_time - start_time);
        let Some(crossing) = history.sample(key, crossing_time) else {
            continue;
        };
        let to_occluder = crossing.position - destination_pos;
        let occluder_distance = to_occluder.length();
        if occluder_distance >= source_distance || occluder_distance <= crossing.radius {
            continue;
        }
        let occluder_angular_radius = (crossing.radius / occluder_distance).min(1.0).asin();
        let separation = source_direction
            .dot(to_occluder.normalize_or_zero())
            .clamp(-1.0, 1.0)
            .acos();
        let blocked =
            circle_overlap_fraction(source_angular_radius, occluder_angular_radius, separation);
        transmission *= 1.0 - blocked;
        if transmission <= 0.0 {
            return 0.0;
        }
    }
    transmission.clamp(0.0, 1.0)
}

fn circle_overlap_fraction(source_radius: f64, occluder_radius: f64, separation: f64) -> f64 {
    if source_radius <= 0.0 {
        return if separation <= occluder_radius {
            1.0
        } else {
            0.0
        };
    }
    if separation >= source_radius + occluder_radius {
        return 0.0;
    }
    if occluder_radius >= separation + source_radius {
        return 1.0;
    }
    if source_radius >= separation + occluder_radius {
        return (occluder_radius * occluder_radius / (source_radius * source_radius))
            .clamp(0.0, 1.0);
    }
    let d = separation.max(f64::MIN_POSITIVE);
    let a = source_radius;
    let b = occluder_radius;
    let alpha = ((d * d + a * a - b * b) / (2.0 * d * a))
        .clamp(-1.0, 1.0)
        .acos();
    let beta = ((d * d + b * b - a * a) / (2.0 * d * b))
        .clamp(-1.0, 1.0)
        .acos();
    let area = a * a * alpha + b * b * beta
        - 0.5
            * ((-d + a + b) * (d + a - b) * (d - a + b) * (d + a + b))
                .max(0.0)
                .sqrt();
    (area / (std::f64::consts::PI * a * a)).clamp(0.0, 1.0)
}

fn bearing(observer: DVec2, source: DVec2) -> f64 {
    let delta = source - observer;
    delta.y.atan2(delta.x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::{CircularOrbitLoader, EphemerisLoader};
    use crate::state::SimulationState;

    #[test]
    fn partial_disc_overlap_is_fractional() {
        let overlap = circle_overlap_fraction(1.0, 1.0, 1.0);
        assert!(overlap > 0.0 && overlap < 1.0);
        assert_eq!(circle_overlap_fraction(1.0, 1.0, 3.0), 0.0);
        assert_eq!(circle_overlap_fraction(1.0, 2.0, 0.0), 1.0);
    }

    #[test]
    fn observation_is_withheld_when_light_cone_predates_history() {
        let mut state = SimulationState::new();
        state.bodies = CircularOrbitLoader.load();
        let mut history = ObservationHistory::default();
        history.append(&state);
        state.sim_time = 10.0;
        history.append(&state);
        let key = EntityKey {
            kind: EntityKind::Body,
            id: 1,
        };
        assert!(
            solve_retarded(&history, key, DVec2::new(SPEED_OF_LIGHT * 100.0, 0.0), 10.0,).is_none()
        );
    }

    #[test]
    fn retarded_solution_uses_historical_position() {
        let mut state = SimulationState::new();
        state.bodies = CircularOrbitLoader.load();
        let mut history = ObservationHistory::default();
        history.append(&state);
        state.sim_time = 10.0;
        state.bodies[0].position.x = 1_000.0;
        history.append(&state);
        let key = EntityKey {
            kind: EntityKind::Body,
            id: 1,
        };
        let result =
            solve_retarded(&history, key, DVec2::new(SPEED_OF_LIGHT * 5.0, 0.0), 10.0).unwrap();
        assert!(result.1 < 10.0);
        assert!(result.0.position.x < 1_000.0);
    }
}
