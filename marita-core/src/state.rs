//! Core simulation state and entity definitions.
//!
//! All spatial quantities are stored in SI units (meters, seconds, radians)
//! using `glam::DVec2` for 2D vectors.

use glam::DVec2;

/// Number of wavelength bins in a `Spectrum`.
pub const SPECTRUM_BINS: usize = 10;

/// Fixed wavelength bins used for signal spectra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(usize)]
pub enum WavelengthBin {
    Radio = 0,
    Microwave = 1,
    Infrared = 2,
    Optical = 3,
    Ultraviolet = 4,
    XRay = 5,
    Gamma = 6,
    EngineThermal = 7,
    Radar = 8,
    Lidar = 9,
}

/// A per-wavelength information budget.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spectrum {
    pub bins: [f64; SPECTRUM_BINS],
}

impl Spectrum {
    pub const fn zero() -> Self {
        Self {
            bins: [0.0; SPECTRUM_BINS],
        }
    }

    pub fn total(&self) -> f64 {
        self.bins.iter().sum()
    }

    pub fn scale(&mut self, factor: f64) {
        for v in self.bins.iter_mut() {
            *v *= factor;
        }
    }

    pub fn scaled(&self, factor: f64) -> Self {
        let mut copy = *self;
        copy.scale(factor);
        copy
    }

    pub fn add(&mut self, other: &Self) {
        for (a, b) in self.bins.iter_mut().zip(other.bins.iter()) {
            *a += b;
        }
    }

    pub fn degrade(&mut self, rates: &Self, dt: f64) {
        for (v, rate) in self.bins.iter_mut().zip(rates.bins.iter()) {
            *v *= (-rate * dt).exp();
        }
    }

    /// Multiply each bin by the corresponding bin in `factors`.
    pub fn scaled_by_spectrum(&self, factors: &Self) -> Self {
        let mut out = Spectrum::zero();
        for i in 0..out.bins.len() {
            out.bins[i] = self.bins[i] * factors.bins[i];
        }
        out
    }
}

/// Thermal state for a massive body or ship.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThermalState {
    /// Temperature in Kelvin.
    pub temperature: f64,
    /// Heat capacity in J/K.
    pub heat_capacity: f64,
    /// Radiating surface area in m².
    pub surface_area: f64,
    /// Emissivity in [0, 1].
    pub emissivity: f64,
    /// Internal heat generation in watts.
    pub internal_generation: f64,
}

impl ThermalState {
    pub fn new(temperature: f64, heat_capacity: f64, surface_area: f64) -> Self {
        Self {
            temperature,
            heat_capacity,
            surface_area,
            emissivity: 0.9,
            internal_generation: 0.0,
        }
    }
}

/// How a body behaves when it collides with another massive object.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CollisionResponse {
    /// Bodies pass through each other (no collision handling).
    Ghost,
    /// Bodies merge into a single body; momentum is conserved.
    Merge,
    /// Bodies bounce with the given coefficient of restitution in [0, 1].
    Bounce { restitution: f64 },
}

/// A celestial body (point mass for physics, with radius for collisions/sensors).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Body {
    pub id: u64,
    pub name: String,
    pub mass: f64,
    pub position: DVec2,
    pub velocity: DVec2,
    pub radius: f64,
    pub collision_response: CollisionResponse,
    pub thermal: ThermalState,
    /// Fraction of incoming energy reflected per wavelength bin.
    pub albedo: Spectrum,
}

/// An engine mounted on a ship.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EngineMount {
    /// Position relative to the ship center of mass (m).
    pub local_position: DVec2,
    /// Maximum thrust in newtons.
    pub max_thrust: f64,
    /// Specific impulse in seconds.
    pub specific_impulse: f64,
    /// Maximum fuel mass flow in kg/s at full throttle.
    pub max_mass_flow: f64,
    /// Current gimbal angle relative to ship orientation (rad).
    pub gimbal: f64,
}

/// A sensor array on a ship.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SensorArray {
    pub local_position: DVec2,
    /// Center bearing relative to ship orientation (rad).
    pub bearing: f64,
    /// Total field of view (rad).
    pub field_of_view: f64,
    /// Wavelength bins this sensor can receive.
    pub bands: [bool; SPECTRUM_BINS],
    /// Effective aperture area in m².
    pub aperture_area: f64,
    /// Noise floor in information units per second.
    pub noise_floor: f64,
    /// Integration time in seconds; defaults to the tick step.
    pub integration_time: f64,
    /// Minimum SNR required for a detection.
    pub min_snr: f64,
}

/// An intentional signal emitter on a ship.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Emitter {
    pub local_position: DVec2,
    /// Center direction relative to ship orientation (rad).
    pub direction: f64,
    /// Angular width of the emitted arc (rad).
    pub angular_width: f64,
    /// Primary wavelength bin.
    pub wavelength_bin: WavelengthBin,
    /// Maximum information emitted per tick.
    pub max_info_per_tick: f64,
    /// Whether the emitter is currently firing.
    pub active: bool,
}

/// A per-tick command controlling a ship.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShipCommand {
    pub ship_id: u64,
    /// Throttle in [0, 1] applied uniformly to all engines.
    pub throttle: f64,
    /// Gimbal override for the main (first) engine in radians.
    pub gimbal: f64,
    /// Per-emitter enable flags as `(emitter_index, active)` pairs.
    pub emitter_states: Vec<(usize, bool)>,
}

impl Default for ShipCommand {
    fn default() -> Self {
        Self {
            ship_id: 0,
            throttle: 0.0,
            gimbal: 0.0,
            emitter_states: Vec::new(),
        }
    }
}

/// A ship represented as a 2D rigid body.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ship {
    pub id: u64,
    pub name: String,
    pub dry_mass: f64,
    pub fuel_mass: f64,
    pub position: DVec2,
    pub velocity: DVec2,
    pub orientation: f64,
    pub angular_velocity: f64,
    pub moment_of_inertia: f64,
    pub engine_mounts: Vec<EngineMount>,
    pub sensor_arrays: Vec<SensorArray>,
    pub emitters: Vec<Emitter>,
    pub thermal: ThermalState,
    pub collision_response: CollisionResponse,
    pub albedo: Spectrum,
}

impl Ship {
    /// Total mass including remaining fuel.
    pub fn mass(&self) -> f64 {
        self.dry_mass + self.fuel_mass
    }

    /// Approximate collision/sensor radius in meters.
    pub fn radius(&self) -> f64 {
        // Volume = mass / density assuming density ~1000 kg/m³.
        let volume = self.mass() / 1000.0;
        let r = ((3.0 * volume) / (4.0 * std::f64::consts::PI)).cbrt();
        r.max(1.0)
    }
}

/// Convenience constructor for a default play-test ship with a single main
/// engine and enough fuel for burns.
///
/// Tuned for responsive admin-viewer demonstrations: roughly 10 m/s² initial
/// acceleration and ~4 km/s Δv budget, enough to escape low Earth orbit in
/// a few tens of seconds of real time.
pub fn default_ship(id: u64, name: &str, position: DVec2, velocity: DVec2) -> Ship {
    Ship {
        id,
        name: name.into(),
        dry_mass: 500.0,
        fuel_mass: 1500.0,
        position,
        velocity,
        orientation: 0.0,
        angular_velocity: 0.0,
        moment_of_inertia: 5000.0,
        engine_mounts: vec![EngineMount {
            local_position: DVec2::ZERO,
            max_thrust: 20_000.0,
            specific_impulse: 300.0,
            max_mass_flow: 6.8,
            gimbal: 0.0,
        }],
        sensor_arrays: vec![SensorArray {
            local_position: DVec2::ZERO,
            bearing: 0.0,
            field_of_view: std::f64::consts::PI / 2.0,
            bands: [true; SPECTRUM_BINS],
            aperture_area: 0.1,
            noise_floor: 1.0,
            integration_time: 1.0,
            min_snr: 1.0,
        }],
        emitters: vec![],
        thermal: ThermalState::new(300.0, 1.0e6, 10.0),
        collision_response: CollisionResponse::Bounce { restitution: 0.5 },
        albedo: Spectrum::zero(),
    }
}

/// An expanding arc-segment signal.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SignalArc {
    pub id: u64,
    /// Arc origin in world space.
    pub origin: DVec2,
    /// Center direction of the arc (rad).
    pub direction: f64,
    /// Total angular width of the arc (rad).
    pub angular_width: f64,
    /// Inner radius of the arc (m).
    pub inner_radius: f64,
    /// Outer radius of the arc (m).
    pub outer_radius: f64,
    /// Per-wavelength information content.
    pub spectrum: Spectrum,
    /// Per-wavelength exponential degradation rate (1/s).
    pub degradation_rates: Spectrum,
    /// Source entity id, if any.
    pub source_id: Option<u64>,
    /// Reflection generation depth.
    pub generation: u32,
}

impl SignalArc {
    /// Convenience constructor for a fresh arc.
    pub fn new(
        id: u64,
        origin: DVec2,
        direction: f64,
        angular_width: f64,
        spectrum: Spectrum,
    ) -> Self {
        Self {
            id,
            origin,
            direction: normalize_angle(direction),
            angular_width,
            inner_radius: 0.0,
            outer_radius: 0.0,
            spectrum,
            degradation_rates: Spectrum::zero(),
            source_id: None,
            generation: 0,
        }
    }

    /// Advance the wavefront by the given radial distance.
    pub fn expand(&mut self, delta_radius: f64) {
        self.inner_radius += delta_radius;
        self.outer_radius += delta_radius;
    }
}

/// The complete world state at one tick.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SimulationState {
    pub tick: u64,
    pub sim_time: f64,
    pub bodies: Vec<Body>,
    pub ships: Vec<Ship>,
    pub signals: Vec<SignalArc>,
    pub next_id: u64,
}

impl Default for SimulationState {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationState {
    pub fn new() -> Self {
        Self {
            tick: 0,
            sim_time: 0.0,
            bodies: Vec::new(),
            ships: Vec::new(),
            signals: Vec::new(),
            next_id: 1,
        }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Serialize the state to a JSON file.
    pub fn save(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file = std::fs::File::create(path)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        serde_json::to_writer_pretty(file, self)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(())
    }

    /// Deserialize the state from a JSON file.
    pub fn load(
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let file = std::fs::File::open(path)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        let state = serde_json::from_reader(file)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(state)
    }
}

/// Normalize an angle to the range (-π, π].
pub fn normalize_angle(a: f64) -> f64 {
    let mut a = a;
    while a > std::f64::consts::PI {
        a -= 2.0 * std::f64::consts::PI;
    }
    while a < -std::f64::consts::PI {
        a += 2.0 * std::f64::consts::PI;
    }
    a
}

/// Shortest signed angle difference from `from` to `to` in [-π, π].
pub fn angle_difference(from: f64, to: f64) -> f64 {
    normalize_angle(to - from)
}

/// Compute the world-space forward vector from a 2D orientation angle.
pub fn heading_vector(orientation: f64) -> DVec2 {
    DVec2::new(orientation.cos(), orientation.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_total_and_scale() {
        let mut s = Spectrum::zero();
        s.bins[0] = 10.0;
        s.bins[3] = 20.0;
        assert_eq!(s.total(), 30.0);
        let scaled = s.scaled(0.5);
        assert_eq!(scaled.total(), 15.0);
    }

    #[test]
    fn angle_normalization() {
        assert!((normalize_angle(3.0 * std::f64::consts::PI) - std::f64::consts::PI).abs() < 1e-12);
        assert!(
            (normalize_angle(-3.0 * std::f64::consts::PI) + std::f64::consts::PI).abs() < 1e-12
        );
    }

    #[test]
    fn checkpoint_roundtrip() {
        let mut state = SimulationState::new();
        state.tick = 42;
        let tmp = std::env::temp_dir().join("marita_checkpoint_roundtrip.json");
        state.save(&tmp).expect("save checkpoint");
        let loaded = SimulationState::load(&tmp).expect("load checkpoint");
        assert_eq!(loaded.tick, state.tick);
        assert_eq!(loaded.bodies, state.bodies);
        assert_eq!(loaded.ships, state.ships);
        assert_eq!(loaded.signals, state.signals);
        assert_eq!(loaded.next_id, state.next_id);
    }
}
