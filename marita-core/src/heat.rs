//! Heat conservation and blackbody radiation for massive bodies and ships.
//!
//! Each thermal entity tracks temperature, heat capacity, surface area, and
//! emissivity. Incoming absorbed radiation and internal generation raise the
//! temperature; radiative emission lowers it.

use crate::signal::AbsorbedEnergy;
use crate::state::ThermalState;
use crate::units::STEFAN_BOLTZMANN;

/// Update a thermal state by one tick and return the radiated power.
///
/// `absorbed_energy` is the total energy absorbed from signal collisions in
/// joules; `dt` is the tick duration in seconds. The returned power is the
/// amount that should be emitted as a blackbody signal over the next tick.
pub fn update_thermal(thermal: &mut ThermalState, absorbed_energy: f64, dt: f64) -> f64 {
    let q_in = absorbed_energy + thermal.internal_generation * dt;
    let q_out = thermal.emissivity
        * STEFAN_BOLTZMANN
        * thermal.surface_area
        * thermal.temperature.powi(4)
        * dt;

    let net_energy = q_in - q_out;
    if thermal.heat_capacity > 0.0 {
        thermal.temperature += net_energy / thermal.heat_capacity;
    }

    // Ensure temperature does not go negative (numerical safety).
    if thermal.temperature < 0.0 {
        thermal.temperature = 0.0;
    }

    thermal.emissivity * STEFAN_BOLTZMANN * thermal.surface_area * thermal.temperature.powi(4)
}

/// Convenience helper to compute absorbed energy from an `AbsorbedEnergy` record.
pub fn absorbed_energy(record: &AbsorbedEnergy) -> f64 {
    record.energy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_equilibrium_reached() {
        let mut thermal = ThermalState::new(300.0, 1.0e6, 10.0);
        // Add a large amount of energy over 10 seconds.
        let power = update_thermal(&mut thermal, 1.0e12, 10.0);
        assert!(thermal.temperature > 300.0);
        assert!(power > 0.0);
    }

    #[test]
    fn no_temperature_below_zero() {
        let mut thermal = ThermalState::new(1.0, 1.0, 1.0);
        update_thermal(&mut thermal, -1.0e20, 1.0);
        assert_eq!(thermal.temperature, 0.0);
    }
}
