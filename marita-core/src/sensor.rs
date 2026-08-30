//! Detailed receiver model for ship sensor arrays.
//!
//! Each sensor has an aperture, integration time, noise floor, minimum SNR, a
//! field of view, and a set of wavelength bands it can detect. The module
//! computes what a ship can detect from the current signal arcs, taking
//! jamming (other in-band signals) into account.

use crate::ambient::AmbientField;
use crate::spatial_tree::{Aabb, Quadtree};
use crate::state::{
    normalize_angle, Body, MarketMessage, SensorArray, Ship, SignalArc, WavelengthBin,
    SPECTRUM_BINS,
};
use glam::DVec2;

/// A detection reported by a sensor array.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    /// Private authoritative source ID; stripped from unprivileged APIs.
    pub source_id: Option<u64>,
    /// Observer-scoped anonymous contact handle.
    pub contact_id: u64,
    pub wavelength_bin: WavelengthBin,
    /// Bearing to the source in world radians.
    pub bearing: f64,
    /// Distance to the source in metres.
    pub distance: f64,
    /// Received effective information strength.
    pub strength: f64,
    /// Signal-to-noise ratio used for detection.
    pub snr: f64,
    pub bearing_sigma: f64,
    pub range_sigma: f64,
    pub emission_tick: u64,
    /// Decoded market message, if this detection carried one on the Radio band.
    pub market_payload: Option<MarketMessage>,
}

pub fn luna_sensor() -> SensorArray {
    let response = crate::state::SensorSpectralResponse {
        noise_floor: [
            1.0e-20, 1.0e-18, 1.0e-8, 1.0e-8, 1.0e-10, 1.0e-12, 1.0e-14, 1.0e-8, 1.0e-12, 1.0e-12,
        ],
        ..Default::default()
    };
    SensorArray {
        local_position: DVec2::ZERO,
        bearing: 0.0,
        field_of_view: 2.0 * std::f64::consts::PI,
        bands: [true; SPECTRUM_BINS],
        aperture_area: 1.0,
        noise_floor: 1.0,
        integration_time: 1.0,
        min_snr: 0.001,
        spectral_response: Some(response),
    }
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
    // The ephemeris generator names Earth's natural satellite "Moon".
    let Some(luna) = bodies
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case("moon") || b.name.eq_ignore_ascii_case("luna"))
    else {
        return Vec::new();
    };

    let field = AmbientField::new(bodies, ships);
    let signal_items: Vec<_> = signals
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Aabb::from_circle(s.origin, s.outer_radius)))
        .collect();
    let signal_tree = Quadtree::build(&signal_items, 16, 20);

    let sensors = [luna_sensor()];

    compute_observer_detections(luna.position, 0.0, &sensors, signals, &signal_tree, &field)
}

/// Compute physically received detections independently for each station.
pub fn compute_station_detections(
    bodies: &[Body],
    ships: &[Ship],
    stations: &[crate::state::Station],
    signals: &[SignalArc],
) -> std::collections::HashMap<u64, Vec<Detection>> {
    let field = AmbientField::new(bodies, ships);
    let signal_items: Vec<_> = signals
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Aabb::from_circle(s.origin, s.outer_radius)))
        .collect();
    let signal_tree = Quadtree::build(&signal_items, 16, 20);
    stations
        .iter()
        .map(|station| {
            let detections = compute_observer_detections(
                station.position(bodies),
                0.0,
                &station.sensor_arrays,
                signals,
                &signal_tree,
                &field,
            );
            (station.id, detections)
        })
        .collect()
}

pub fn compute_causal_observer_detections(
    observer_id: u64,
    observer_pos: DVec2,
    observer_orientation: f64,
    sensors: &[SensorArray],
    signals: &[SignalArc],
    history: &crate::history::ObservationHistory,
    bodies: &[Body],
    ships: &[Ship],
    stations: &[crate::state::Station],
    observer_body_id: Option<u64>,
    observation_time: f64,
    candidate_cap: usize,
) -> Vec<Detection> {
    if sensors.is_empty() {
        return Vec::new();
    }
    let signal_items: Vec<_> = signals
        .iter()
        .enumerate()
        .map(|(i, signal)| (i, Aabb::from_circle(signal.origin, signal.outer_radius)))
        .collect();
    let signal_tree = Quadtree::build(&signal_items, 16, 20);
    let mut detections = compute_observer_detections(
        observer_pos,
        observer_orientation,
        sensors,
        signals,
        &signal_tree,
        &AmbientField::empty(),
    );
    let window = (observation_time
        / sensors
            .first()
            .map(|s| s.integration_time)
            .unwrap_or(1.0)
            .max(1.0)) as u64;
    let observer_noise_id = observer_id
        ^ sensors
            .first()
            .and_then(|s| s.spectral_response)
            .map(|r| r.noise_seed)
            .unwrap_or(0);
    for detection in &mut detections {
        detection.contact_id = anonymous_contact_id(observer_id, detection.source_id.unwrap_or(0));
        detection.emission_tick =
            (observation_time - detection.distance / crate::units::SPEED_OF_LIGHT).max(0.0) as u64
                / crate::units::TICK_SIM_TIME as u64;
        apply_measurement_noise(detection, observer_noise_id, window);
    }
    let contributions = crate::passive_radiation::observe_bodies(
        history,
        bodies,
        ships,
        stations,
        observer_pos,
        observer_body_id,
        observation_time,
        candidate_cap,
    );
    for sensor in sensors {
        let sensor_bearing = normalize_angle(observer_orientation + sensor.bearing);
        let half_fov = sensor.field_of_view / 2.0;
        for contribution in &contributions {
            let raw_diff = (contribution.bearing - sensor_bearing).abs();
            let bearing_diff = raw_diff.min(2.0 * std::f64::consts::PI - raw_diff);
            if bearing_diff > half_fov {
                continue;
            }
            for i in 0..SPECTRUM_BINS {
                if !sensor.bands[i] {
                    continue;
                }
                let response = sensor.spectral_response.unwrap_or_default();
                let power = contribution.spectrum.bins[i]
                    * sensor.aperture_area
                    * sensor.integration_time
                    * response.efficiency[i];
                if power <= 0.0 {
                    continue;
                }
                let angular_resolution = response.angular_resolution;
                let local_jamming: f64 = contributions
                    .iter()
                    .filter(|other| other.source != contribution.source)
                    .filter(|other| {
                        let delta = (other.bearing - contribution.bearing).abs();
                        delta.min(2.0 * std::f64::consts::PI - delta) <= angular_resolution
                    })
                    .map(|other| {
                        other.spectrum.bins[i]
                            * sensor.aperture_area
                            * sensor.integration_time
                            * response.efficiency[i]
                    })
                    .sum();
                let noise = response.noise_floor[i] + local_jamming;
                let snr = power / noise.max(f64::MIN_POSITIVE);
                if snr >= sensor.min_snr {
                    let mut detection = Detection {
                        source_id: Some(contribution.source.id),
                        contact_id: anonymous_contact_id(observer_id, contribution.source.id),
                        wavelength_bin: wavelength_bin(i),
                        bearing: contribution.bearing,
                        distance: contribution.distance,
                        strength: power,
                        snr,
                        bearing_sigma: response.angular_resolution,
                        range_sigma: contribution.distance * response.range_resolution_fraction,
                        emission_tick: (contribution.emission_time.max(0.0)
                            / crate::units::TICK_SIM_TIME)
                            as u64,
                        market_payload: None,
                    };
                    apply_measurement_noise(&mut detection, observer_id, window);
                    detections.push(detection);
                }
            }
        }
    }
    detections
}

fn anonymous_contact_id(observer_id: u64, source_id: u64) -> u64 {
    mix64(observer_id ^ source_id.rotate_left(29) ^ 0x9e3779b97f4a7c15)
}

fn apply_measurement_noise(detection: &mut Detection, observer_id: u64, window: u64) {
    let seed = observer_id
        ^ detection.source_id.unwrap_or(0).rotate_left(17)
        ^ (detection.wavelength_bin as u64).rotate_left(41)
        ^ window;
    let bearing_error = unit_noise(seed) * detection.bearing_sigma;
    let range_error = unit_noise(seed ^ 0xa0761d6478bd642f) * detection.range_sigma;
    detection.bearing = normalize_angle(detection.bearing + bearing_error);
    detection.distance = (detection.distance + range_error).max(0.0);
}

fn unit_noise(seed: u64) -> f64 {
    let value = mix64(seed) >> 11;
    (value as f64 / ((1_u64 << 53) as f64)) * 2.0 - 1.0
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn wavelength_bin(index: usize) -> WavelengthBin {
    const BINS: [WavelengthBin; SPECTRUM_BINS] = [
        WavelengthBin::Radio,
        WavelengthBin::Microwave,
        WavelengthBin::Infrared,
        WavelengthBin::Optical,
        WavelengthBin::Ultraviolet,
        WavelengthBin::XRay,
        WavelengthBin::Gamma,
        WavelengthBin::EngineThermal,
        WavelengthBin::Radar,
        WavelengthBin::Lidar,
    ];
    BINS[index]
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
                        contact_id: source.id.unwrap_or(0),
                        wavelength_bin: unsafe { std::mem::transmute::<usize, WavelengthBin>(i) },
                        bearing: source.direction,
                        distance: source.distance,
                        strength: power,
                        snr,
                        bearing_sigma: 0.0,
                        range_sigma: 0.0,
                        emission_tick: 0,
                        market_payload: None,
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
                let market_payload = if i == WavelengthBin::Radio as usize {
                    arc.market_payload.clone()
                } else {
                    None
                };
                detections.push(Detection {
                    source_id: arc.source_id,
                    contact_id: arc.source_id.unwrap_or(0),
                    wavelength_bin: wavelength_bin(i),
                    bearing,
                    distance,
                    strength: arc_received[i],
                    snr,
                    bearing_sigma: 0.0,
                    range_sigma: 0.0,
                    emission_tick: 0,
                    market_payload,
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
    use crate::state::{Body, CollisionResponse, SensorArray, Ship, Spectrum, ThermalState};

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
                spectral_response: None,
            }],
            emitters: vec![],
            thermal: ThermalState::new(0.0, 1.0, 1.0),
            collision_response: CollisionResponse::Ghost,
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn deterministic_noise_varies_by_integration_window() {
        let base = Detection {
            source_id: Some(7),
            contact_id: 1,
            wavelength_bin: WavelengthBin::Optical,
            bearing: 1.0,
            distance: 1.0e9,
            strength: 1.0,
            snr: 2.0,
            bearing_sigma: 0.01,
            range_sigma: 1.0e6,
            emission_tick: 0,
            market_payload: None,
        };
        let mut first = base.clone();
        let mut replay = base.clone();
        let mut next = base;
        apply_measurement_noise(&mut first, 5, 10);
        apply_measurement_noise(&mut replay, 5, 10);
        apply_measurement_noise(&mut next, 5, 11);
        assert_eq!(first, replay);
        assert_ne!(first.bearing, next.bearing);
        assert_ne!(anonymous_contact_id(1, 7), anonymous_contact_id(2, 7));
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

    #[test]
    fn luna_sensor_detects_sun_and_earth() {
        let au = 1.496e11;
        let sun = Body {
            id: 1,
            name: "Sun".into(),
            mass: 1.989e30,
            position: DVec2::ZERO,
            velocity: DVec2::ZERO,
            radius: 6.957e8,
            collision_response: CollisionResponse::Ghost,
            thermal: ThermalState::new(5778.0, 1.0, 1.0),
            albedo: Spectrum::zero(),
        };
        let earth = Body {
            id: 2,
            name: "Earth".into(),
            mass: 5.972e24,
            position: DVec2::new(au, 0.0),
            velocity: DVec2::ZERO,
            radius: 6.371e6,
            collision_response: CollisionResponse::Merge,
            thermal: ThermalState::new(
                288.0,
                1.0e20,
                4.0 * std::f64::consts::PI * 6.371e6 * 6.371e6,
            ),
            albedo: Spectrum::zero(),
        };
        // Moon is far enough from the Earth-Sun line that the Sun is visible.
        let moon = Body {
            id: 3,
            name: "Moon".into(),
            mass: 7.3477e22,
            position: DVec2::new(0.0, au),
            velocity: DVec2::ZERO,
            radius: 1.737e6,
            collision_response: CollisionResponse::Merge,
            thermal: ThermalState::new(
                250.0,
                1.0e18,
                4.0 * std::f64::consts::PI * 1.737e6 * 1.737e6,
            ),
            albedo: Spectrum::zero(),
        };

        let detections = compute_luna_detections(&[sun, earth, moon], &[], &[]);
        assert!(!detections.is_empty(), "Luna should detect ambient sources");
        let sun_detected = detections.iter().any(|d| d.source_id == Some(1));
        assert!(sun_detected, "Luna should detect the Sun");
    }
}
