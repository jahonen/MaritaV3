//! Continuous ambient radiation fields for sunlight and thermal emission.
//!
//! The engine's discrete `SignalArc`s are reserved for active/intentional
//! emissions (radar, laser, engine signatures) and their reflections. The Sun
//! and every warm body contribute a continuous irradiance field that is used
//! for body heating and sensor background without creating thousands of
//! expanding shells.

use crate::state::{Body, Ship, Spectrum, WavelengthBin};
use crate::units::STEFAN_BOLTZMANN;
use glam::DVec2;

/// A radiating body used to compute continuous irradiance.
#[derive(Debug, Clone, Copy)]
struct RadiatingBody {
    id: u64,
    position: DVec2,
    radius: f64,
    temperature: f64,
    emissivity: f64,
}

/// Continuous radiation field from the Sun and warm bodies.
#[derive(Debug, Clone)]
pub struct AmbientField {
    solar_source: Option<RadiatingBody>,
    thermal_sources: Vec<RadiatingBody>,
    /// Bodies that can fully occlude the Sun. Includes all massive bodies
    /// except the Sun itself.
    shadow_casters: Vec<RadiatingBody>,
}

impl AmbientField {
    pub fn empty() -> Self {
        Self {
            solar_source: None,
            thermal_sources: Vec::new(),
            shadow_casters: Vec::new(),
        }
    }

    /// Build the field from the current massive bodies and ships.
    pub fn new(bodies: &[Body], ships: &[Ship]) -> Self {
        let solar_source = bodies
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case("sun"))
            .map(|b| RadiatingBody {
                id: b.id,
                position: b.position,
                radius: b.radius,
                temperature: b.thermal.temperature,
                emissivity: b.thermal.emissivity,
            });

        let mut thermal_sources = Vec::new();
        let mut shadow_casters = Vec::new();
        for body in bodies {
            // Skip the Sun from the thermal list; its output is represented by
            // the dedicated solar field. Including it here would double-count its
            // dominant optical contribution.
            if body.name.eq_ignore_ascii_case("sun") {
                continue;
            }
            if body.thermal.temperature > 0.0 {
                thermal_sources.push(RadiatingBody {
                    id: body.id,
                    position: body.position,
                    radius: body.radius,
                    temperature: body.thermal.temperature,
                    emissivity: body.thermal.emissivity,
                });
            }
            // Any massive body can cast a shadow on a more distant one.
            shadow_casters.push(RadiatingBody {
                id: body.id,
                position: body.position,
                radius: body.radius,
                temperature: body.thermal.temperature,
                emissivity: body.thermal.emissivity,
            });
        }
        for ship in ships {
            if ship.thermal.temperature > 0.0 {
                thermal_sources.push(RadiatingBody {
                    id: ship.id,
                    position: ship.position,
                    radius: ship.radius(),
                    temperature: ship.thermal.temperature,
                    emissivity: ship.thermal.emissivity,
                });
            }
        }

        Self {
            solar_source,
            thermal_sources,
            shadow_casters,
        }
    }

    /// Check whether the Sun is fully visible from `point`.
    ///
    /// A body fully shadows the Sun when its apparent angular radius from `point`
    /// is at least the Sun's apparent angular radius plus the angular separation
    /// between the body and the Sun. This is a simple full-shadow test; partial
    /// penumbras are ignored.
    fn is_sun_visible(&self, point: DVec2) -> bool {
        let Some(sun) = self.solar_source else {
            return false;
        };
        let to_sun = sun.position - point;
        let sun_dist = to_sun.length();
        if sun_dist <= 0.0 {
            return false;
        }
        if sun_dist <= sun.radius {
            return true;
        }

        for caster in &self.shadow_casters {
            let to_caster = caster.position - point;
            let caster_dist = to_caster.length();
            if caster_dist <= caster.radius {
                // The point is inside or on the surface of the body. Don't
                // let a body shadow its own interior; this keeps surface
                // heating and virtual station sensors working.
                continue;
            }
            if caster_dist >= sun_dist {
                // Body is not between the point and the Sun.
                continue;
            }

            let dot = to_caster.dot(to_sun);
            let cos_sep = (dot / (caster_dist * sun_dist)).clamp(-1.0, 1.0);

            let sin_caster = (caster.radius / caster_dist).min(1.0);
            let sin_sun = (sun.radius / sun_dist).min(1.0);
            let cos_caster = (1.0 - sin_caster * sin_caster).max(0.0).sqrt();
            let cos_sun = (1.0 - sin_sun * sin_sun).max(0.0).sqrt();
            // cos(alpha_caster - alpha_sun)
            let cos_caster_minus_sun = cos_caster * cos_sun + sin_caster * sin_sun;

            // Shadow when alpha_caster >= alpha_sun + sep, i.e.
            // cos(alpha_caster - alpha_sun) <= cos(sep).
            if cos_caster_minus_sun <= cos_sep {
                return false;
            }
        }

        true
    }

    /// Solar irradiance (W/m^2) at `point`.
    pub fn solar_irradiance(&self, point: DVec2) -> Spectrum {
        let Some(sun) = self.solar_source else {
            return Spectrum::zero();
        };
        let delta = sun.position - point;
        let dist_sq = delta.length_squared();
        if dist_sq <= 0.0 {
            return Spectrum::zero();
        }
        if !self.is_sun_visible(point) {
            return Spectrum::zero();
        }
        // Total emitted power = ε σ 4π r^2 T^4.  The 4π cancels with the
        // spherical spreading 1/(4π d^2), leaving ε σ r^2 T^4 / d^2.
        let total_power =
            sun.emissivity * STEFAN_BOLTZMANN * sun.radius * sun.radius * sun.temperature.powi(4);
        let irradiance = total_power / dist_sq;
        blackbody_spectrum(sun.temperature).scaled(irradiance)
    }

    /// Direction to the Sun at `point` (world radians), or `None` if there is no
    /// Sun or the point is at the Sun's center.
    pub fn sun_direction(&self, point: DVec2) -> Option<f64> {
        let sun = self.solar_source?;
        let delta = sun.position - point;
        if delta.length_squared() <= 0.0 {
            return None;
        }
        Some(delta.y.atan2(delta.x))
    }

    /// Thermal irradiance (W/m^2) at `point` from all warm non-Sun bodies.
    pub fn thermal_irradiance(&self, point: DVec2) -> Spectrum {
        let mut total = Spectrum::zero();
        for source in &self.thermal_sources {
            let delta = source.position - point;
            let dist_sq = delta.length_squared();
            if dist_sq <= 0.0 {
                continue;
            }
            let power = source.emissivity
                * STEFAN_BOLTZMANN
                * source.radius
                * source.radius
                * source.temperature.powi(4);
            let irradiance = power / dist_sq;
            total.add(&blackbody_spectrum(source.temperature).scaled(irradiance));
        }
        total
    }

    /// Total ambient irradiance at `point`.
    pub fn irradiance(&self, point: DVec2) -> Spectrum {
        let mut total = self.solar_irradiance(point);
        total.add(&self.thermal_irradiance(point));
        total
    }

    /// Ambient energy absorbed by a spherical entity over `dt` seconds.
    ///
    /// The cross-sectional area of a sphere is `π r²`; the absorbing fraction is
    /// folded into the entity's emissivity/absorptivity approximation.
    pub fn absorbed_energy(&self, position: DVec2, radius: f64, emissivity: f64, dt: f64) -> f64 {
        let area = std::f64::consts::PI * radius * radius;
        let irradiance = self.irradiance(position).total();
        emissivity * irradiance * area * dt
    }

    /// Iterate over all ambient sources a sensor might want to consider.
    ///
    /// The returned iterator yields the source direction, distance, and per-bin
    /// irradiance. The caller is responsible for field-of-view filtering.
    pub fn sensor_sources(&self, point: DVec2) -> impl Iterator<Item = AmbientSource> + '_ {
        let solar = self.solar_source.and_then(move |sun| {
            if !self.is_sun_visible(point) {
                return None;
            }
            let delta = sun.position - point;
            Some(AmbientSource {
                id: Some(sun.id),
                direction: if delta.length_squared() > 0.0 {
                    delta.y.atan2(delta.x)
                } else {
                    0.0
                },
                distance: delta.length(),
                spectrum: self.solar_irradiance(point),
            })
        });

        let thermal = self.thermal_sources.iter().map(move |src| {
            let delta = src.position - point;
            let dist_sq = delta.length_squared();
            let power = src.emissivity
                * STEFAN_BOLTZMANN
                * src.radius
                * src.radius
                * src.temperature.powi(4);
            let irradiance = if dist_sq > 0.0 { power / dist_sq } else { 0.0 };
            AmbientSource {
                id: Some(src.id),
                direction: if dist_sq > 0.0 {
                    delta.y.atan2(delta.x)
                } else {
                    0.0
                },
                distance: delta.length(),
                spectrum: blackbody_spectrum(src.temperature).scaled(irradiance),
            }
        });

        solar.into_iter().chain(thermal)
    }
}

/// A single ambient radiation source for sensor processing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientSource {
    /// Source entity id, if the source is a tracked body or ship.
    pub id: Option<u64>,
    pub direction: f64,
    pub distance: f64,
    pub spectrum: Spectrum,
}

/// Return a normalized ten-bin Planck approximation for a blackbody.
/// Physical bands receive energy according to spectral radiance sampled over
/// logarithmic wavelength intervals; engine-signature/radar/lidar bins remain
/// reserved for intentional emissions.
pub fn blackbody_spectrum(temperature: f64) -> Spectrum {
    let mut spectrum = Spectrum::zero();
    if temperature <= 0.0 || !temperature.is_finite() {
        return spectrum;
    }
    // Representative wavelength and logarithmic interval width in metres.
    let bins = [
        (WavelengthBin::Radio as usize, 1.0, 4.0_f64),
        (WavelengthBin::Microwave as usize, 1e-2, 4.0),
        (WavelengthBin::Infrared as usize, 1e-5, 6.0),
        (WavelengthBin::Optical as usize, 5e-7, 1.4),
        (WavelengthBin::Ultraviolet as usize, 1e-7, 2.0),
        (WavelengthBin::XRay as usize, 1e-10, 6.0),
        (WavelengthBin::Gamma as usize, 1e-12, 4.0),
    ];
    const H: f64 = 6.626_070_15e-34;
    const C: f64 = 299_792_458.0;
    const K: f64 = 1.380_649e-23;
    let mut total = 0.0;
    for (index, wavelength, log_width) in bins {
        let exponent = H * C / (wavelength * K * temperature);
        let value = if exponent > 700.0 {
            0.0
        } else {
            (1.0 / wavelength.powi(4)) / exponent.exp_m1() * log_width
        };
        spectrum.bins[index] = value;
        total += value;
    }
    if total > 0.0 {
        spectrum.scale(1.0 / total);
    }
    spectrum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Body, CollisionResponse, Spectrum, ThermalState};

    fn test_body(name: &str, temp: f64, radius: f64) -> Body {
        Body {
            id: 1,
            name: name.into(),
            mass: 1.0,
            position: DVec2::ZERO,
            velocity: DVec2::ZERO,
            radius,
            collision_response: CollisionResponse::Merge,
            thermal: ThermalState::new(temp, 1.0e20, 4.0 * std::f64::consts::PI * radius * radius),
            albedo: Spectrum::zero(),
        }
    }

    #[test]
    fn solar_constant_at_earth_orbit() {
        let mut sun = test_body("Sun", 5778.0, 6.9634e8);
        let mut earth = test_body("Earth", 288.0, 6.371e6);
        sun.position = DVec2::ZERO;
        let earth_pos = DVec2::new(1.496e11, 0.0);
        earth.position = earth_pos;
        let earth_radius = earth.radius;
        let field = AmbientField::new(&[sun, earth], &[]);
        // Evaluate just above Earth's Sun-facing surface so the point is outside
        // the body and the Sun is visible.
        let surface = DVec2::new(earth_pos.x - earth_radius - 1e3, 0.0);
        let irrad = field.solar_irradiance(surface);
        // Within an order of magnitude of ~1361 W/m^2.
        assert!(irrad.total() > 100.0 && irrad.total() < 1e6);
    }

    #[test]
    fn earth_shadows_sunlight_behind_it() {
        let mut sun = test_body("Sun", 5778.0, 6.9634e8);
        let mut earth = test_body("Earth", 288.0, 6.371e6);
        sun.position = DVec2::ZERO;
        earth.position = DVec2::new(1.496e11, 0.0);
        let earth_position = earth.position;
        let earth_radius = earth.radius;
        let field = AmbientField::new(&[sun, earth], &[]);

        // Point on the far side of Earth from the Sun.
        let shadow_point = DVec2::new(earth_position.x + earth_radius + 1e6, 0.0);
        let irrad = field.solar_irradiance(shadow_point);
        assert!(irrad.total() < 1.0);
    }

    #[test]
    fn solar_blackbody_spans_optical_and_ultraviolet() {
        let spectrum = blackbody_spectrum(5778.0);
        assert!(spectrum.bins[WavelengthBin::Optical as usize] > 0.0);
        assert!(spectrum.bins[WavelengthBin::Ultraviolet as usize] > 0.0);
        assert!((spectrum.total() - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn thermal_field_peaks_in_infrared() {
        let earth = test_body("Earth", 288.0, 6.371e6);
        let field = AmbientField::new(&[earth], &[]);
        let point = DVec2::new(1e9, 0.0);
        let irrad = field.thermal_irradiance(point);
        let max_bin = irrad
            .bins
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(max_bin, WavelengthBin::Infrared as usize);
    }
}
