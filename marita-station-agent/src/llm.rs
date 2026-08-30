//! Replaceable LLM adapters for station decision-making.
//!
//! The default MVP adapter talks to Hermes 3 8B running locally through the
//! Ollama API.  A deterministic fallback is provided so the agent can run
//! without an LLM endpoint and still produce sensible actions.

use crate::tools::ProposedAction;
use async_trait::async_trait;
use serde::Deserialize;

/// Snapshot of one station as seen by the agent.
#[derive(Debug, Clone)]
pub struct StationSnapshot {
    pub id: u64,
    pub name: String,
    pub tech_tier: u32,
    pub solar_collector_area: f64,
    pub trade_credits_kwh: f64,
    pub warehouses: Vec<(u32, f64)>,
    pub active_posters: Vec<PosterSnapshot>,
    pub received_messages: Vec<PosterSnapshot>,
}

#[derive(Debug, Clone)]
pub struct PosterSnapshot {
    pub message_id: u64,
    pub station_id: u64,
    pub kind: String,
    pub material: u32,
    pub quantity: f64,
    pub price: f64,
}

/// An LLM adapter proposes an action for a single station.
#[async_trait]
pub trait LlmAdapter: Send + Sync {
    async fn decide(&self, station: &StationSnapshot) -> anyhow::Result<ProposedAction>;
}

/// Deterministic fallback that posts a WANT for the lowest-stock material one
/// tier above the station's current tech level, and a HAVE for the largest raw
/// surplus.  This mirrors the engine's auto-poster logic and keeps the agent
/// useful even when no LLM is reachable.
pub struct DeterministicLlm;

impl DeterministicLlm {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LlmAdapter for DeterministicLlm {
    async fn decide(&self, station: &StationSnapshot) -> anyhow::Result<ProposedAction> {
        // Respond only to messages that physically reached this station.
        if let Some(offer) = station.received_messages.iter().find(|m| {
            m.kind == "OFFER"
                && stock(station, m.material) < m.quantity
                && station.trade_credits_kwh >= m.quantity * m.price
        }) {
            return Ok(ProposedAction::Negotiate {
                station_id: station.id,
                kind: "ACCEPT".into(),
                material: offer.material,
                quantity: offer.quantity,
                price_per_unit_kwh: offer.price,
                in_reply_to: offer.message_id,
                to_station_id: offer.station_id,
            });
        }
        if let Some(want) = station
            .received_messages
            .iter()
            .find(|m| m.kind == "WANT" && stock(station, m.material) >= m.quantity)
        {
            return Ok(ProposedAction::Negotiate {
                station_id: station.id,
                kind: "OFFER".into(),
                material: want.material,
                quantity: want.quantity,
                price_per_unit_kwh: want.price,
                in_reply_to: want.message_id,
                to_station_id: want.station_id,
            });
        }

        let next_tier = station.tech_tier + 1;
        let candidates: Vec<u32> = match next_tier {
            2 => vec![0, 1, 2, 3, 4, 5, 6, 7],
            3 => vec![100, 101, 102, 103, 104, 105, 106, 107],
            4 => vec![200, 201, 202, 203],
            5 => vec![300, 301, 302],
            _ => vec![0, 1, 2, 3, 4, 5, 6, 7],
        };

        if next_tier <= 5 {
            if let Some(material) = find_scarce_material(station, &candidates) {
                return Ok(ProposedAction::PostWant {
                    station_id: station.id,
                    material,
                    quantity: 10.0,
                    price_per_unit_kwh: base_value(material) * 1.5,
                    ttl_ticks: 60,
                });
            }
        }

        if let Some(material) = find_surplus_raw(station) {
            return Ok(ProposedAction::PostHave {
                station_id: station.id,
                material,
                quantity: 50.0,
                price_per_unit_kwh: base_value(material) * 0.8,
                ttl_ticks: 60,
            });
        }

        Ok(ProposedAction::None)
    }
}

fn find_scarce_material(station: &StationSnapshot, candidates: &[u32]) -> Option<u32> {
    candidates
        .iter()
        .copied()
        .min_by(|a, b| {
            let sa = stock(station, *a);
            let sb = stock(station, *b);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|m| stock(station, *m) < 5.0)
}

fn find_surplus_raw(station: &StationSnapshot) -> Option<u32> {
    station
        .warehouses
        .iter()
        .filter(|(m, v)| *m < 100 && *v >= 100.0)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(m, _)| *m)
}

fn stock(station: &StationSnapshot, material: u32) -> f64 {
    station
        .warehouses
        .iter()
        .find(|(m, _)| *m == material)
        .map(|(_, v)| *v)
        .unwrap_or(0.0)
}

fn base_value(material: u32) -> f64 {
    match material {
        0 => 0.1,
        1 | 2 | 4 | 6 => 0.5,
        3 => 1.0,
        5 => 0.3,
        7 => 5.0,
        100 => 2.0,
        101 => 3.0,
        102 => 8.0,
        103 => 0.5,
        104 => 1.0,
        105 => 2.0,
        106 => 2.5,
        107 => 1.5,
        200 => 5.0,
        201 => 2.0,
        202 => 4.0,
        203 => 15.0,
        300 => 25.0,
        301 => 80.0,
        302 => 40.0,
        400 => 500.0,
        401 => 600.0,
        402 => 400.0,
        _ => 1.0,
    }
}

/// Hermes 3 8B via Ollama.  The adapter asks the model to return a short JSON
/// object describing the action.  If the model fails or returns malformed JSON,
/// the deterministic fallback logic is used instead.
pub struct HermesOllamaLlm {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl HermesOllamaLlm {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmAdapter for HermesOllamaLlm {
    async fn decide(&self, station: &StationSnapshot) -> anyhow::Result<ProposedAction> {
        let prompt = build_prompt(station);
        let url = format!("{}/api/chat", self.base_url);

        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": prompt}
            ],
            "stream": false,
            "format": "json",
            "options": {
                "temperature": 0.2,
                "num_predict": 256
            }
        });

        let response = match self.client.post(&url).json(&request_body).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "LLM endpoint unreachable for station {}: {}; falling back to deterministic logic",
                    station.name, e
                );
                return DeterministicLlm::new().decide(station).await;
            }
        };

        let body: OllamaChatResponse = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to parse LLM response for {}: {e}", station.name);
                return DeterministicLlm::new().decide(station).await;
            }
        };

        if let Some(action) = parse_action(&body.message.content, station.id) {
            return Ok(action);
        }

        eprintln!(
            "Could not parse LLM action for {}: {}; using deterministic fallback",
            station.name, body.message.content
        );
        DeterministicLlm::new().decide(station).await
    }
}

const SYSTEM_PROMPT: &str = r#"You are the autonomous AI commander of an operational space station.
You are responsible for the station's survival, industrial growth, energy security, logistics, and trade.
You have no privileged world state: use only your local telemetry and radio messages that have physically reached your station. Treat every remote station, ship, and contact simply as an independent counterparty; never speculate whether it is controlled by a person or software. Communications and deliveries are delayed by physical distance.
You must choose ONE action per turn for your station.

Valid actions (return exactly one JSON object):
- Post a WANT: {"action":"PostWant","material":1,"quantity":10.0,"price":2.0}
- Post a HAVE: {"action":"PostHave","material":0,"quantity":50.0,"price":0.3}
- Offer goods in reply to a WANT: {"action":"Offer","material":1,"quantity":10.0,"price":2.0,"in_reply_to":42,"to_station_id":2001}
- Counter an offer: {"action":"Counter","material":1,"quantity":10.0,"price":1.8,"in_reply_to":43,"to_station_id":2002}
- Accept an offer: {"action":"Accept","material":1,"quantity":10.0,"price":2.0,"in_reply_to":43,"to_station_id":2002}
- Reject an offer: {"action":"Reject","material":1,"quantity":10.0,"price":2.0,"in_reply_to":43,"to_station_id":2002}
- Start production: {"action":"StartProduction","reaction":5}
- Expand solar array: {"action":"SetCollectorArea","area_m2":2000.0}
- Do nothing: {"action":"None"}

Use material IDs: 0=Regolith,1=IronOre,2=AluminumOre,3=TitaniumOre,4=WaterIce,5=CarbonaceousOre,6=SilicateOre,7=RareEarthOre,
100=Iron,101=Aluminum,102=Titanium,103=Water,104=Oxygen,105=Hydrogen,106=Methane,107=Glass,
200=Steel,201=Concrete,202=Polymer,203=SolarSilicon,
300=Composite,301=Semiconductor,302=AdvancedAlloy,
400=HabitatModule,401=RefineryModule,402=SolarArrayModule.

Prefer scarcity-driven trade: post WANT for materials needed to reach the next tech tier, post HAVE for abundant raw materials.
Prices are in kWh-equivalent per unit."#;

fn build_prompt(station: &StationSnapshot) -> String {
    let mut lines = vec![format!(
        "Station: {} (id={}, tech_tier={}, collector={} m2)",
        station.name, station.id, station.tech_tier, station.solar_collector_area
    )];
    lines.push("Warehouse:".into());
    for (mat, qty) in &station.warehouses {
        lines.push(format!("  {mat}: {qty:.1}"));
    }
    lines.push("Active market posters:".into());
    for poster in &station.active_posters {
        lines.push(format!(
            "  {} {} x {:.1} @ {:.2}",
            poster.kind, poster.material, poster.quantity, poster.price
        ));
    }
    lines.push("Radio messages physically received this tick:".into());
    for msg in &station.received_messages {
        lines.push(format!(
            "  id={} from={} {} material={} x {:.1} @ {:.2}",
            msg.message_id, msg.station_id, msg.kind, msg.material, msg.quantity, msg.price
        ));
    }
    lines.push("Choose one action JSON:".into());
    lines.join("\n")
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(Deserialize)]
struct LlmAction {
    action: String,
    #[serde(default)]
    material: u32,
    #[serde(default)]
    quantity: f64,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    reaction: u32,
    #[serde(default)]
    area_m2: f64,
    #[serde(default)]
    in_reply_to: u64,
    #[serde(default)]
    to_station_id: u64,
}

fn parse_action(content: &str, station_id: u64) -> Option<ProposedAction> {
    // Some models wrap the JSON in markdown; strip it.
    let trimmed = content
        .trim()
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .or_else(|| {
            content
                .strip_prefix("```")
                .and_then(|s| s.strip_suffix("```"))
        })
        .unwrap_or(content)
        .trim();

    let action: LlmAction = serde_json::from_str(trimmed).ok()?;
    Some(match action.action.as_str() {
        "PostWant" => ProposedAction::PostWant {
            station_id,
            material: action.material,
            quantity: action.quantity.max(1.0),
            price_per_unit_kwh: action.price.max(0.01),
            ttl_ticks: 60,
        },
        "PostHave" => ProposedAction::PostHave {
            station_id,
            material: action.material,
            quantity: action.quantity.max(1.0),
            price_per_unit_kwh: action.price.max(0.01),
            ttl_ticks: 60,
        },
        "StartProduction" => ProposedAction::StartProduction {
            station_id,
            reaction: action.reaction,
        },
        "SetCollectorArea" => ProposedAction::SetCollectorArea {
            station_id,
            area_m2: action.area_m2.max(0.0),
        },
        "Offer" | "Counter" | "Accept" | "Reject" => ProposedAction::Negotiate {
            station_id,
            kind: action.action.to_uppercase(),
            material: action.material,
            quantity: action.quantity.max(1.0),
            price_per_unit_kwh: action.price.max(0.01),
            in_reply_to: action.in_reply_to,
            to_station_id: action.to_station_id,
        },
        _ => ProposedAction::None,
    })
}
