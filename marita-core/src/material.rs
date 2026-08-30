//! Simplified periodic-table-grounded material and production model.
//!
//! This is an intentionally small MVP subset.  Every material has a complexity
//! tier and an embodied energy cost derived from its processing path.  Later the
//! tier system can be replaced by a full bond-energy synthesis web; for the proof
//! of concept the tiers encode the same physical progression from abundant raw
//! elements toward scarce, highly-processed goods.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A material that can be mined, refined, traded, or consumed by stations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum MaterialId {
    // Tier 0: raw planetary resources (abundant)
    Regolith = 0,
    IronOre = 1,
    AluminumOre = 2,
    TitaniumOre = 3,
    WaterIce = 4,
    CarbonaceousOre = 5,
    SilicateOre = 6,
    RareEarthOre = 7,

    // Tier 1: basic processed materials
    Iron = 100,
    Aluminum = 101,
    Titanium = 102,
    Water = 103,
    Oxygen = 104,
    Hydrogen = 105,
    Methane = 106,
    Glass = 107,

    // Tier 2: industrial materials
    Steel = 200,
    Concrete = 201,
    Polymer = 202,
    SolarCellGradeSilicon = 203,

    // Tier 3: advanced materials
    Composite = 300,
    Semiconductor = 301,
    AdvancedAlloy = 302,

    // Tier 4: station modules (synthetic, scarce)
    HabitatModule = 400,
    RefineryModule = 401,
    SolarArrayModule = 402,
}

impl MaterialId {
    /// Complexity tier: 0 raw, 1 basic, 2 industrial, 3 advanced, 4 synthetic.
    pub fn tier(&self) -> u32 {
        let raw = *self as u32;
        raw / 100
    }

    /// Whether the material is a raw planetary resource.
    pub fn is_raw(&self) -> bool {
        self.tier() == 0
    }
}

/// Static information about a material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialInfo {
    pub id: MaterialId,
    pub name: &'static str,
    pub mass_per_unit: f64,
    /// Baseline market value in kWh-equivalent per unit.
    pub base_value_kwh: f64,
}

/// Reaction inputs and outputs for a production line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub id: ReactionId,
    pub name: &'static str,
    /// Required input materials and quantities (units per batch).
    pub inputs: HashMap<MaterialId, f64>,
    /// Output materials and quantities (units per batch).
    pub outputs: HashMap<MaterialId, f64>,
    /// Energy consumed per batch in kWh.
    pub energy_kwh: f64,
    /// Simulation ticks required to complete one batch.
    pub duration_ticks: u64,
    /// Minimum technology/complexity tier this station must support.
    pub required_tech_tier: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum ReactionId {
    SmeltIron = 0,
    SmeltAluminum = 1,
    SmeltTitanium = 2,
    ElectrolyseWater = 3,
    SabatierMethane = 4,
    MakeSteel = 5,
    MakeGlass = 6,
    MakeConcrete = 7,
    MakePolymer = 8,
    RefineSolarSilicon = 9,
    MakeComposite = 10,
    MakeSemiconductor = 11,
    MakeAdvancedAlloy = 12,
    AssembleHabitat = 13,
    AssembleRefinery = 14,
    AssembleSolarArray = 15,
}

/// Static material library.
pub fn material_info(id: MaterialId) -> MaterialInfo {
    match id {
        MaterialId::Regolith => MaterialInfo {
            id,
            name: "Regolith",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.1,
        },
        MaterialId::IronOre => MaterialInfo {
            id,
            name: "Iron Ore",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.5,
        },
        MaterialId::AluminumOre => MaterialInfo {
            id,
            name: "Aluminum Ore",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.4,
        },
        MaterialId::TitaniumOre => MaterialInfo {
            id,
            name: "Titanium Ore",
            mass_per_unit: 1000.0,
            base_value_kwh: 1.0,
        },
        MaterialId::WaterIce => MaterialInfo {
            id,
            name: "Water Ice",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.2,
        },
        MaterialId::CarbonaceousOre => MaterialInfo {
            id,
            name: "Carbonaceous Ore",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.3,
        },
        MaterialId::SilicateOre => MaterialInfo {
            id,
            name: "Silicate Ore",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.2,
        },
        MaterialId::RareEarthOre => MaterialInfo {
            id,
            name: "Rare Earth Ore",
            mass_per_unit: 1000.0,
            base_value_kwh: 5.0,
        },

        MaterialId::Iron => MaterialInfo {
            id,
            name: "Iron",
            mass_per_unit: 1000.0,
            base_value_kwh: 2.0,
        },
        MaterialId::Aluminum => MaterialInfo {
            id,
            name: "Aluminum",
            mass_per_unit: 1000.0,
            base_value_kwh: 3.0,
        },
        MaterialId::Titanium => MaterialInfo {
            id,
            name: "Titanium",
            mass_per_unit: 1000.0,
            base_value_kwh: 8.0,
        },
        MaterialId::Water => MaterialInfo {
            id,
            name: "Water",
            mass_per_unit: 1000.0,
            base_value_kwh: 0.5,
        },
        MaterialId::Oxygen => MaterialInfo {
            id,
            name: "Oxygen",
            mass_per_unit: 100.0,
            base_value_kwh: 1.0,
        },
        MaterialId::Hydrogen => MaterialInfo {
            id,
            name: "Hydrogen",
            mass_per_unit: 100.0,
            base_value_kwh: 2.0,
        },
        MaterialId::Methane => MaterialInfo {
            id,
            name: "Methane",
            mass_per_unit: 100.0,
            base_value_kwh: 2.5,
        },
        MaterialId::Glass => MaterialInfo {
            id,
            name: "Glass",
            mass_per_unit: 1000.0,
            base_value_kwh: 1.5,
        },

        MaterialId::Steel => MaterialInfo {
            id,
            name: "Steel",
            mass_per_unit: 1000.0,
            base_value_kwh: 5.0,
        },
        MaterialId::Concrete => MaterialInfo {
            id,
            name: "Concrete",
            mass_per_unit: 1000.0,
            base_value_kwh: 2.0,
        },
        MaterialId::Polymer => MaterialInfo {
            id,
            name: "Polymer",
            mass_per_unit: 100.0,
            base_value_kwh: 4.0,
        },
        MaterialId::SolarCellGradeSilicon => MaterialInfo {
            id,
            name: "Solar-Grade Silicon",
            mass_per_unit: 100.0,
            base_value_kwh: 15.0,
        },

        MaterialId::Composite => MaterialInfo {
            id,
            name: "Composite",
            mass_per_unit: 100.0,
            base_value_kwh: 25.0,
        },
        MaterialId::Semiconductor => MaterialInfo {
            id,
            name: "Semiconductor",
            mass_per_unit: 10.0,
            base_value_kwh: 80.0,
        },
        MaterialId::AdvancedAlloy => MaterialInfo {
            id,
            name: "Advanced Alloy",
            mass_per_unit: 100.0,
            base_value_kwh: 40.0,
        },

        MaterialId::HabitatModule => MaterialInfo {
            id,
            name: "Habitat Module",
            mass_per_unit: 10_000.0,
            base_value_kwh: 500.0,
        },
        MaterialId::RefineryModule => MaterialInfo {
            id,
            name: "Refinery Module",
            mass_per_unit: 10_000.0,
            base_value_kwh: 600.0,
        },
        MaterialId::SolarArrayModule => MaterialInfo {
            id,
            name: "Solar Array Module",
            mass_per_unit: 5000.0,
            base_value_kwh: 400.0,
        },
    }
}

/// Static reaction library.
pub fn reaction_info(id: ReactionId) -> Reaction {
    let mut inputs = HashMap::new();
    let mut outputs = HashMap::new();
    let (energy, duration, tier) = match id {
        ReactionId::SmeltIron => {
            inputs.insert(MaterialId::IronOre, 1.0);
            outputs.insert(MaterialId::Iron, 0.6);
            outputs.insert(MaterialId::SilicateOre, 0.3);
            (2.0, 2, 1)
        }
        ReactionId::SmeltAluminum => {
            inputs.insert(MaterialId::AluminumOre, 1.0);
            outputs.insert(MaterialId::Aluminum, 0.5);
            outputs.insert(MaterialId::SilicateOre, 0.4);
            (4.0, 3, 1)
        }
        ReactionId::SmeltTitanium => {
            inputs.insert(MaterialId::TitaniumOre, 1.0);
            outputs.insert(MaterialId::Titanium, 0.4);
            outputs.insert(MaterialId::SilicateOre, 0.5);
            (8.0, 4, 1)
        }
        ReactionId::ElectrolyseWater => {
            inputs.insert(MaterialId::WaterIce, 1.0);
            outputs.insert(MaterialId::Oxygen, 0.9);
            outputs.insert(MaterialId::Hydrogen, 1.8);
            (5.0, 2, 1)
        }
        ReactionId::SabatierMethane => {
            inputs.insert(MaterialId::CarbonaceousOre, 0.5);
            inputs.insert(MaterialId::Hydrogen, 4.0);
            outputs.insert(MaterialId::Methane, 1.0);
            outputs.insert(MaterialId::Water, 0.5);
            (3.0, 2, 1)
        }
        ReactionId::MakeSteel => {
            inputs.insert(MaterialId::Iron, 0.9);
            inputs.insert(MaterialId::CarbonaceousOre, 0.1);
            outputs.insert(MaterialId::Steel, 1.0);
            (3.0, 3, 2)
        }
        ReactionId::MakeGlass => {
            inputs.insert(MaterialId::SilicateOre, 1.0);
            outputs.insert(MaterialId::Glass, 0.8);
            (2.0, 2, 2)
        }
        ReactionId::MakeConcrete => {
            inputs.insert(MaterialId::Regolith, 0.7);
            inputs.insert(MaterialId::SilicateOre, 0.2);
            inputs.insert(MaterialId::Water, 0.1);
            outputs.insert(MaterialId::Concrete, 1.0);
            (1.0, 2, 2)
        }
        ReactionId::MakePolymer => {
            inputs.insert(MaterialId::CarbonaceousOre, 0.4);
            inputs.insert(MaterialId::Hydrogen, 0.3);
            inputs.insert(MaterialId::Oxygen, 0.3);
            outputs.insert(MaterialId::Polymer, 1.0);
            (6.0, 3, 2)
        }
        ReactionId::RefineSolarSilicon => {
            inputs.insert(MaterialId::SilicateOre, 2.0);
            inputs.insert(MaterialId::RareEarthOre, 0.1);
            outputs.insert(MaterialId::SolarCellGradeSilicon, 1.0);
            (20.0, 6, 2)
        }
        ReactionId::MakeComposite => {
            inputs.insert(MaterialId::Polymer, 0.4);
            inputs.insert(MaterialId::Aluminum, 0.3);
            inputs.insert(MaterialId::Glass, 0.2);
            inputs.insert(MaterialId::Titanium, 0.1);
            outputs.insert(MaterialId::Composite, 1.0);
            (12.0, 4, 3)
        }
        ReactionId::MakeSemiconductor => {
            inputs.insert(MaterialId::SolarCellGradeSilicon, 1.0);
            inputs.insert(MaterialId::RareEarthOre, 0.2);
            outputs.insert(MaterialId::Semiconductor, 1.0);
            (30.0, 6, 3)
        }
        ReactionId::MakeAdvancedAlloy => {
            inputs.insert(MaterialId::Steel, 0.6);
            inputs.insert(MaterialId::Titanium, 0.3);
            inputs.insert(MaterialId::RareEarthOre, 0.1);
            outputs.insert(MaterialId::AdvancedAlloy, 1.0);
            (18.0, 5, 3)
        }
        ReactionId::AssembleHabitat => {
            inputs.insert(MaterialId::Steel, 5.0);
            inputs.insert(MaterialId::Concrete, 10.0);
            inputs.insert(MaterialId::Glass, 2.0);
            inputs.insert(MaterialId::Semiconductor, 1.0);
            outputs.insert(MaterialId::HabitatModule, 1.0);
            (50.0, 10, 4)
        }
        ReactionId::AssembleRefinery => {
            inputs.insert(MaterialId::AdvancedAlloy, 2.0);
            inputs.insert(MaterialId::Semiconductor, 2.0);
            inputs.insert(MaterialId::Steel, 5.0);
            outputs.insert(MaterialId::RefineryModule, 1.0);
            (60.0, 10, 4)
        }
        ReactionId::AssembleSolarArray => {
            inputs.insert(MaterialId::SolarCellGradeSilicon, 5.0);
            inputs.insert(MaterialId::Composite, 3.0);
            inputs.insert(MaterialId::Aluminum, 2.0);
            outputs.insert(MaterialId::SolarArrayModule, 1.0);
            (40.0, 8, 4)
        }
    };
    Reaction {
        id,
        name: reaction_name(id),
        inputs,
        outputs,
        energy_kwh: energy,
        duration_ticks: duration,
        required_tech_tier: tier,
    }
}

fn reaction_name(id: ReactionId) -> &'static str {
    match id {
        ReactionId::SmeltIron => "Smelt Iron",
        ReactionId::SmeltAluminum => "Smelt Aluminum",
        ReactionId::SmeltTitanium => "Smelt Titanium",
        ReactionId::ElectrolyseWater => "Electrolyse Water",
        ReactionId::SabatierMethane => "Sabatier Methane",
        ReactionId::MakeSteel => "Make Steel",
        ReactionId::MakeGlass => "Make Glass",
        ReactionId::MakeConcrete => "Make Concrete",
        ReactionId::MakePolymer => "Make Polymer",
        ReactionId::RefineSolarSilicon => "Refine Solar Silicon",
        ReactionId::MakeComposite => "Make Composite",
        ReactionId::MakeSemiconductor => "Make Semiconductor",
        ReactionId::MakeAdvancedAlloy => "Make Advanced Alloy",
        ReactionId::AssembleHabitat => "Assemble Habitat Module",
        ReactionId::AssembleRefinery => "Assemble Refinery Module",
        ReactionId::AssembleSolarArray => "Assemble Solar Array Module",
    }
}

/// Default planetary surface composition used to seed station raw-material
/// mines.  Values are relative abundance; the engine normalises them per body.
pub fn default_body_composition(body_name: &str) -> HashMap<MaterialId, f64> {
    let mut map: HashMap<MaterialId, f64> = HashMap::new();
    match body_name {
        "Mercury" => {
            map.insert(MaterialId::Regolith, 40.0);
            map.insert(MaterialId::IronOre, 35.0);
            map.insert(MaterialId::SilicateOre, 20.0);
            map.insert(MaterialId::TitaniumOre, 10.0);
            map.insert(MaterialId::RareEarthOre, 3.0);
        }
        "Venus" => {
            map.insert(MaterialId::Regolith, 50.0);
            map.insert(MaterialId::SilicateOre, 30.0);
            map.insert(MaterialId::IronOre, 15.0);
            map.insert(MaterialId::AluminumOre, 10.0);
            map.insert(MaterialId::RareEarthOre, 2.0);
        }
        "Earth" => {
            map.insert(MaterialId::Regolith, 30.0);
            map.insert(MaterialId::SilicateOre, 25.0);
            map.insert(MaterialId::IronOre, 20.0);
            map.insert(MaterialId::AluminumOre, 15.0);
            map.insert(MaterialId::WaterIce, 10.0);
            map.insert(MaterialId::RareEarthOre, 3.0);
        }
        "Moon" | "Luna" => {
            map.insert(MaterialId::Regolith, 60.0);
            map.insert(MaterialId::SilicateOre, 20.0);
            map.insert(MaterialId::AluminumOre, 10.0);
            map.insert(MaterialId::TitaniumOre, 5.0);
            map.insert(MaterialId::RareEarthOre, 2.0);
        }
        "Mars" => {
            map.insert(MaterialId::Regolith, 50.0);
            map.insert(MaterialId::IronOre, 25.0);
            map.insert(MaterialId::SilicateOre, 15.0);
            map.insert(MaterialId::WaterIce, 8.0);
            map.insert(MaterialId::AluminumOre, 5.0);
            map.insert(MaterialId::RareEarthOre, 2.0);
        }
        "Ceres" => {
            map.insert(MaterialId::WaterIce, 40.0);
            map.insert(MaterialId::CarbonaceousOre, 30.0);
            map.insert(MaterialId::SilicateOre, 20.0);
            map.insert(MaterialId::IronOre, 10.0);
            map.insert(MaterialId::RareEarthOre, 4.0);
        }
        "Jupiter" => {
            // Stations around gas giants rely on moon-mining; for the POC we treat
            // the orbit as a carbon/hydrogen rich site with moon-like silicates.
            map.insert(MaterialId::Hydrogen, 50.0);
            map.insert(MaterialId::CarbonaceousOre, 25.0);
            map.insert(MaterialId::SilicateOre, 15.0);
            map.insert(MaterialId::WaterIce, 10.0);
            map.insert(MaterialId::RareEarthOre, 1.0);
        }
        _ => {
            map.insert(MaterialId::Regolith, 50.0);
            map.insert(MaterialId::SilicateOre, 30.0);
            map.insert(MaterialId::IronOre, 15.0);
            map.insert(MaterialId::WaterIce, 5.0);
        }
    }
    map
}
