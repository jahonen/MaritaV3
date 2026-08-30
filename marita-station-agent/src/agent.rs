//! Unprivileged single-station agent runtime.
//!
//! Each process is bound to one station ID and sees only its local station
//! snapshot, its own contracts, and messages physically received by its radios.

use crate::llm::{LlmAdapter, PosterSnapshot, StationSnapshot};
use crate::tools::{proposal_to_command, ProposedAction};
use marita_grpc::proto::marita_engine_client::MaritaEngineClient;
use marita_grpc::proto::StationViewRequest;
use std::sync::Arc;
use std::time::Duration;

pub struct StationAgent {
    addr: String,
    station_id: u64,
    llm: Arc<dyn LlmAdapter + Send + Sync>,
}

impl StationAgent {
    pub fn new(addr: String, station_id: u64, llm: Arc<dyn LlmAdapter + Send + Sync>) -> Self {
        Self {
            addr,
            station_id,
            llm,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let mut client = MaritaEngineClient::connect(self.addr.clone()).await?;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        println!("Station agent process bound to station {}", self.station_id);

        loop {
            interval.tick().await;
            let view = client
                .get_station_view(StationViewRequest {
                    station_id: self.station_id,
                })
                .await?
                .into_inner();
            let Some(station) = view.station.as_ref() else {
                continue;
            };
            let snapshot = build_snapshot(station, &view.received_messages);
            let action = match self.llm.decide(&snapshot).await {
                Ok(action) => action,
                Err(error) => {
                    eprintln!("Commander decision failed for {}: {error}", station.name);
                    ProposedAction::None
                }
            };
            if let Some(command) = proposal_to_command(&station.name, station.id, action) {
                let result = client.submit_station_command(command).await?.into_inner();
                if !result.accepted {
                    eprintln!(
                        "Engine rejected command for {}: {}",
                        station.name, result.reason
                    );
                }
            }
        }
    }
}

fn build_snapshot(
    station: &marita_grpc::proto::Station,
    received: &[marita_grpc::proto::Detection],
) -> StationSnapshot {
    StationSnapshot {
        id: station.id,
        name: station.name.clone(),
        tech_tier: station.tech_tier,
        solar_collector_area: station.solar_collector_area,
        trade_credits_kwh: station.trade_credits_kwh,
        warehouses: station
            .warehouses
            .iter()
            .map(|e| (e.material, e.quantity))
            .collect(),
        active_posters: station
            .active_market_posters
            .iter()
            .filter_map(|p| {
                let msg = p.message.as_ref()?;
                Some(PosterSnapshot {
                    message_id: msg.message_id,
                    station_id: msg.station_id,
                    kind: msg.kind.clone(),
                    material: msg.material,
                    quantity: msg.quantity,
                    price: msg.price_per_unit_kwh,
                })
            })
            .collect(),
        received_messages: received
            .iter()
            .filter_map(|d| {
                let msg = d.market_payload.as_ref()?;
                Some(PosterSnapshot {
                    message_id: msg.message_id,
                    station_id: msg.station_id,
                    kind: msg.kind.clone(),
                    material: msg.material,
                    quantity: msg.quantity,
                    price: msg.price_per_unit_kwh,
                })
            })
            .collect(),
    }
}
