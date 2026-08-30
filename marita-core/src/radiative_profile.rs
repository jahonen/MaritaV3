//! Validated data-driven radiative properties for natural bodies.

use crate::state::{Spectrum, SPECTRUM_BINS};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
pub struct BodyRadiativeProfile {
    pub name: String,
    pub temperature_k: f64,
    pub emissivity: f64,
    pub albedo: [f64; SPECTRUM_BINS],
    pub natural_luminosity: [f64; SPECTRUM_BINS],
    pub internal_generation_w: f64,
}

impl BodyRadiativeProfile {
    pub fn albedo_spectrum(&self) -> Spectrum {
        Spectrum { bins: self.albedo }
    }

    fn validate(&self) -> Result<(), String> {
        if !self.temperature_k.is_finite() || self.temperature_k < 0.0 {
            return Err(format!("{} has invalid temperature", self.name));
        }
        if !self.emissivity.is_finite() || !(0.0..=1.0).contains(&self.emissivity) {
            return Err(format!("{} has invalid emissivity", self.name));
        }
        if self
            .albedo
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(format!("{} has invalid albedo", self.name));
        }
        if self
            .natural_luminosity
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
            || !self.internal_generation_w.is_finite()
            || self.internal_generation_w < 0.0
        {
            return Err(format!("{} has invalid luminosity", self.name));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    profiles: Vec<BodyRadiativeProfile>,
}

#[derive(Debug)]
pub struct RadiativeProfileCatalog {
    profiles: HashMap<String, BodyRadiativeProfile>,
}

impl RadiativeProfileCatalog {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let file: CatalogFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut profiles = HashMap::new();
        for profile in file.profiles {
            profile.validate()?;
            profiles.insert(profile.name.to_ascii_lowercase(), profile);
        }
        if !profiles.contains_key("default") {
            return Err("radiative profile catalog has no default profile".into());
        }
        Ok(Self { profiles })
    }

    pub fn get(&self, name: &str) -> &BodyRadiativeProfile {
        self.profiles
            .get(&name.to_ascii_lowercase())
            .unwrap_or_else(|| &self.profiles["default"])
    }
}

pub fn bundled_catalog() -> &'static RadiativeProfileCatalog {
    static CATALOG: OnceLock<RadiativeProfileCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        RadiativeProfileCatalog::from_json(include_str!("../data/body-radiative-profiles.json"))
            .expect("bundled radiative profile catalog must be valid")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_profiles_are_valid_and_have_fallback() {
        let catalog = bundled_catalog();
        assert_eq!(catalog.get("Earth").temperature_k, 288.0);
        assert_eq!(catalog.get("unknown").name, "default");
    }

    #[test]
    fn invalid_albedo_is_rejected() {
        let json = r#"{"profiles":[{"name":"default","temperature_k":1,"emissivity":1,"albedo":[2,0,0,0,0,0,0,0,0,0],"natural_luminosity":[0,0,0,0,0,0,0,0,0,0],"internal_generation_w":0}]}"#;
        assert!(RadiativeProfileCatalog::from_json(json).is_err());
    }
}
