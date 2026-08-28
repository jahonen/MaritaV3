//! Detailed receiver model for ship sensor arrays.
//!
//! Each sensor has an aperture, integration time, noise floor, minimum SNR, a
//! field of view, and a set of wavelength bands it can detect. The module
//! computes what a ship can detect from the current signal arcs, taking
//! jamming (other in-band signals) into account.

use crate::ambient::AmbientField;
use crate::spatial_tree::{Aabb, Quadtree};
use crate::state::{
    normalize_angle, Body, SensorArray, Ship, SignalArc, WavelengthBin, SPECTRUM_BINS,
};
use glam::DVec2;

/// A detection reported by a sensor array.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub source_id: Option<u64>,
    pub wavelength_bin: WavelengthBin,
    /// Bearing to the source in world radians.
    pub bearing: f64,
    /// Distance to the source in metres.
    pub distance: f64,
    /// Received effective information strength.
    pub strength: f64,
    /// Signal-to-noise ratio used for detection.
    pub snr: f64,
}

/// Compute all detections for all ships.
/// Compute detections for a fixed omnidirectional sensor at Luna, if Luna
/// exists in the system. Returns an empty vector if there is no body named
/// "Luna".
pub fn compute_luna_detections(
    bodies: &[Body],
    ships: &[Ship],
    signals: &[SignalArc],
) -> Vec<Detection> {
    let Some(luna) = bodies.iter().find(|b| b.name.eq_ignore_ascii_case("luna")) else {
        return Vec::new();
    };

    let field = AmbientField::new(bodies, ships);
    let signal_items: Vec<_> = signals
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Aabb::from_circle(s.origin, s.outer_radius)))
        .collect();
    let signal_tree = Quadtree::build(&signal_items, 16, 20);

    let sensors = vec![SensorArray {
        local_position: DVec2::ZERO,
        bearing: 0.0,
        field_of_view: 2.0 * std::f64::consts::PI,
        bands: [true; SPECTRUM_BINS],
        aperture_area: 1.0,
        noise_floor: 1.0,
        integration_time: 1.0,
        min_snr: 1.0,
    }];

    compute_observer_detections(luna.position, 0.0, &sensors, signals, &signal_tree, &field)
}

pub fn compute_all_detections(
    bodies: &[Body],
    ships: &[Ship],
    signals: &[SignalArc],
) -> Vec<Vec<Detection>> {
    if ships.is_empty() {
        return Vec::new();
    }

    let field = AmbientField::new(bodies, ships);

    let signal_items: Vec<_> = signals
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Aabb::from_circle(s.origin, s.outer_radius)))
        .collect();
    let signal_tree = Quadtree::build(&signal_items, 16, 20);

    ships
        .iter()
        .map(|ship| {
            compute_observer_detections(
                ship.position,
                ship.orientation,
                &ship.sensor_arrays,
                signals,
                &signal_tree,
                &field,
            )
        })
        .collect()
}

/// Compute all detections for a set of sensors located at `observer_pos` and
/// oriented by `observer_orientation`. This is used both for ships and for
/// fixed observer stations such as Luna.
pub fn compute_observer_detections(
    observer_pos: DVec2,
    observer_orientation: f64,
    sensors: &[SensorArray],
    signals: &[SignalArc],
    signal_tree: &Quadtree,
    field: &AmbientField,
) -> Vec<Detection> {
    let mut detections = Vec::new();
    for sensor in sensors {
        compute_sensor_detections(
            &mut detections,
            observer_pos,
            observer_orientation,
            sensor,
            signals,
            signal_tree,
            field,
        );
    }
    detections
}

fn compute_sensor_detections(
    detections: &mut Vec<Detection>,
    observer_pos: DVec2,
    observer_orientation: f64,
    sensor: &SensorArray,
    signals: &[SignalArc],
    signal_tree: &Quadtree,
    field: &AmbientField,
) {
    let sensor_pos = observer_pos + rotate_vector(sensor.local_position, observer_orientation);
    let sensor_bearing = normalize_angle(observer_orientation + sensor.bearing);
    let half_fov = sensor.field_of_view / 2.0;

    // First pass: compute received power per bin at the observer location from
    // every arc, binned by wavelength.
    let mut received: [f64; SPECTRUM_BINS] = [0.0; SPECTRUM_BINS];
    let mut per_arc: Vec<(usize, [f64; SPECTRUM_BINS], f64, f64)> = Vec::new();

    // Ambient field sources (Sunlight + thermal) are continuous, not arcs.
    // Add any that are within the sensor field of view as detections and
    // accumulate their power into the jamming floor.
    for source in field.sensor_sources(sensor_pos) {
        let raw_diff = (source.direction - sensor_bearing).abs();
        let bearing_diff = raw_diff.min(2.0 * std::f64::consts::PI - raw_diff);
        if bearing_diff > half_fov {
            continue;
        }
        for i in 0..SPECTRUM_BINS {
            if !sensor.bands[i] {
                continue;
            }
            let power = source.spectrum.bins[i] * sensor.aperture_area * sensor.integration_time;
            if power <= 0.0 {
                continue;
            }
            let noise = sensor.noise_floor + received[i];
            if noise > 0.0 {
                let snr = power / noise;
                if snr >= sensor.min_snr {
                    detections.push(Detection {
                        source_id: source.id,
                        wavelength_bin: unsafe { std::mem::transmute::<usize, WavelengthBin>(i) },
                        bearing: source.direction,
                        distance: source.distance,
                        strength: power,
                        snr,
                    });
                }
            }
            received[i] += power;
        }
    }

    let sensor_aabb = Aabb::from_circle(sensor_pos, 1.0);
    let candidate_indices = signal_tree.query_region(sensor_aabb);
    for arc_idx in candidate_indices {
        let arc = &signals[arc_idx];
        let delta = sensor_pos - arc.origin;
        let r = delta.length();
        if r < arc.inner_radius || r > arc.outer_radius {
            continue;
        }
        let bearing = normalize_angle(delta.y.atan2(delta.x));
        // Compute smallest angular distance, wrapping around the circle.
        let raw_diff = (bearing - sensor_bearing).abs();
        let bearing_diff = raw_diff.min(2.0 * std::f64::consts::PI - raw_diff);
        if bearing_diff > half_fov {
            continue;
        }

        // Directional gain: simplified top-hat within FOV.
        let gain = 1.0;
        // Arc angular density at this radius.
        let arc_length = r * arc.angular_width;
        if arc_length <= 0.0 {
            continue;
        }
        let mut arc_received = [0.0; SPECTRUM_BINS];
        for i in 0..SPECTRUM_BINS {
            if !sensor.bands[i] {
                continue;
            }
            // Information density along the wavefront.
            let density = arc.spectrum.bins[i] / arc_length;
            let power = density * sensor.aperture_area * sensor.integration_time * gain;
            arc_received[i] = power.max(0.0);
            received[i] += power.max(0.0);
        }
        per_arc.push((arc_idx, arc_received, bearing, r));
    }

    // Second pass: determine which arcs rise above the noise + jamming floor.
    for (arc_idx, arc_received, bearing, distance) in per_arc {
        let arc = &signals[arc_idx];
        for i in 0..SPECTRUM_BINS {
            if !sensor.bands[i] || arc_received[i] <= 0.0 {
                continue;
            }
            // Jamming = all other received power in this bin.
            let jamming = received[i] - arc_received[i];
            let noise = sensor.noise_floor + jamming;
            if noise <= 0.0 {
                continue;
            }
            let snr = arc_received[i] / noise;
            if snr >= sensor.min_snr {
                detections.push(Detection {
                    source_id: arc.source_id,
                    wavelength_bin: unsafe { std::mem::transmute::<usize, WavelengthBin>(i) },
                    bearing,
                    distance,
                    strength: arc_received[i],
                    snr,
                });
            }
        }
    }
}

fn rotate_vector(v: DVec2, angle: f64) -> DVec2 {
    let (s, c) = angle.sin_cos();
    DVec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CollisionResponse, SensorArray, Ship, Spectrum, ThermalState};

    fn test_ship_with_sensor() -> Ship {
        Ship {
            id: 1,
            name: "observer".into(),
            dry_mass: 1000.0,
            fuel_mass: 0.0,
            position: DVec2::new(1.0e6, 0.0),
            velocity: DVec2::ZERO,
            orientation: 0.0,
            angular_velocity: 0.0,
            moment_of_inertia: 1000.0,
            engine_mounts: vec![],
            sensor_arrays: vec![SensorArray {
                local_position: DVec2::ZERO,
                bearing: std::f64::consts::PI,
                field_of_view: std::f64::consts::PI / 2.0,
                bands: [true; SPECTRUM_BINS],
                aperture_area: 1.0,
                noise_floor: 1.0,
                integration_time: 1.0,
                min_snr: 1.0,
            }],
            emitters: vec![],
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            collision_response: CollisionResponse::Ghost,
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn sensor_detects_nearby_emitter() {
        let ship = test_ship_with_sensor();
        let mut arc = SignalArc::new(
            1,
            DVec2::new(2.0e6, 0.0),
            std::f64::consts::PI,
            0.1,
            Spectrum::zero(),
        );
        arc.spectrum.bins[WavelengthBin::Radar as usize] = 1.0e12;
        // Manually place arc wavefront over the ship.
        arc.inner_radius = 0.9e6;
        arc.outer_radius = 1.1e6;

        let signal_items = vec![(0usize, Aabb::from_circle(arc.origin, arc.outer_radius))];
        let signal_tree = Quadtree::build(&signal_items, 4, 8);
        let field = AmbientField::new(&[], &[]);
        let detections = compute_observer_detections(
            ship.position,
            ship.orientation,
            &ship.sensor_arrays,
            &[arc],
            &signal_tree,
            &field,
        );
        assert!(!detections.is_empty());
        assert!(detections
            .iter()
            .any(|d| d.wavelength_bin == WavelengthBin::Radar));
    }

    #[test]
    fn sensor_ignores_out_of_fov_signal() {
        let mut ship = test_ship_with_sensor();
        ship.position = DVec2::new(0.0, 1.0e6);
        let mut arc = SignalArc::new(
            1,
            DVec2::new(0.0, 2.0e6),
            -std::f64::consts::PI / 2.0,
            0.1,
            Spectrum::zero(),
        );
        arc.spectrum.bins[WavelengthBin::Radar as usize] = 1.0e12;
        arc.inner_radius = 0.9e6;
        arc.outer_radius = 1.1e6;

        let signal_items = vec![(0usize, Aabb::from_circle(arc.origin, arc.outer_radius))];
        let signal_tree = Quadtree::build(&signal_items, 4, 8);
        let field = AmbientField::new(&[], &[]);
        let detections = compute_observer_detections(
            ship.position,
            ship.orientation,
            &ship.sensor_arrays,
            &[arc],
            &signal_tree,
            &field,
        );
        // Sensor points at +X, source is at +Y, outside FOV.
        assert!(detections.is_empty());
    }
}
