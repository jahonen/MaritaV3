//! Physical constants and unit helpers used throughout the engine.
//!
//! All internal simulation state is stored in SI units (meters, kilograms,
//! seconds, radians). This module provides the constants and common
//! conversions so the rest of the code does not hard-code magic numbers.

use glam::DVec2;

/// Speed of light in vacuum (m/s).
pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;

/// Newtonian gravitational constant (m³ kg⁻¹ s⁻²).
pub const GRAVITATIONAL_CONSTANT: f64 = 6.674_30e-11;

/// Stefan–Boltzmann constant (W m⁻² K⁻⁴).
pub const STEFAN_BOLTZMANN: f64 = 5.670_374_419e-8;

/// Standard gravity used for Isp conversions (m/s²).
pub const STANDARD_GRAVITY: f64 = 9.806_65;

/// One astronomical unit in meters.
pub const AU: f64 = 149_597_870_700.0;

/// Rough heliopause distance used as the simulation boundary (m).
pub const SOLAR_SYSTEM_BOUNDARY: f64 = 100.0 * AU;

/// Solar mass (kg).
pub const SOLAR_MASS: f64 = 1.988_47e30;

/// Solar radius (m).
pub const SOLAR_RADIUS: f64 = 6.957e8;

/// Effective blackbody temperature of the Sun (K).
pub const SUN_EFFECTIVE_TEMPERATURE: f64 = 5_772.0;

/// Default fixed simulation time step (s).
///
/// The engine runs one tick per real-time second with 10× time propagation,
/// so each tick advances the simulation by 10 seconds.
pub const TICK_SIM_TIME: f64 = 10.0;

/// Softening length used to avoid singularities near massive bodies (m).
pub const GRAVITY_SOFTENING: f64 = 1_000_000.0;

/// Helper: zero 2D vector.
pub const fn zero2() -> DVec2 {
    DVec2::ZERO
}
