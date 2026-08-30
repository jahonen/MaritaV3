//! egui application that renders the Luna station detection view.

use crate::client::spawn_client;
use crate::track::TrackStore;
use egui::{Color32, Painter, Pos2, Stroke, Vec2};
use marita_grpc::proto::LunaDetections;
use std::collections::VecDeque;
use std::sync::mpsc;

/// Wavelength bin names, matching `marita_core::state::WavelengthBin`.
const BIN_NAMES: &[&str] = &[
    "Radio",
    "Microwave",
    "Infrared",
    "Optical",
    "UV",
    "XRay",
    "Gamma",
    "EngineThermal",
    "Radar",
    "Lidar",
];

/// A market message observed at Luna, kept for display even after the
/// originating signal arc has passed.
#[derive(Clone)]
struct DisplayMarketMessage {
    tick: u64,
    station_name: String,
    body_name: String,
    kind: String,
    material: String,
    quantity: f64,
    price: f64,
}

pub struct LunaApp {
    #[allow(dead_code)]
    _runtime: tokio::runtime::Runtime,
    state_rx: mpsc::Receiver<LunaDetections>,
    latest: Option<LunaDetections>,
    status: String,
    realtime_elapsed: f64,
    /// Maximum displayed range in metres.
    max_range_m: f64,
    /// If true, use a logarithmic radial scale.
    log_scale: bool,
    /// Market messages decoded from radio detections at Luna.
    market_feed: VecDeque<DisplayMarketMessage>,
    tracks: TrackStore,
    enabled_bands: [bool; 10],
}

impl LunaApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, addr: String) -> Self {
        let (state_tx, state_rx) = mpsc::channel();
        let runtime = spawn_client(addr, state_tx);

        Self {
            _runtime: runtime,
            state_rx,
            latest: None,
            status: "Connecting...".into(),
            realtime_elapsed: 0.0,
            max_range_m: 1.5e11, // ~1 AU default
            log_scale: false,
            market_feed: VecDeque::new(),
            tracks: TrackStore::new(30),
            enabled_bands: [true; 10],
        }
    }

    fn try_recv_state(&mut self) {
        let mut received = false;
        while let Ok(state) = self.state_rx.try_recv() {
            self.tracks.update(state.tick, &state.detections);
            self.latest = Some(state.clone());
            self.update_market_feed(&state);
            received = true;
        }
        if received {
            self.status = "Connected".into();
        }
    }

    fn update_market_feed(&mut self, state: &LunaDetections) {
        for d in &state.detections {
            if let Some(msg) = &d.market_payload {
                let entry = DisplayMarketMessage {
                    tick: msg.tick,
                    station_name: msg.station_name.clone(),
                    body_name: msg.body_name.clone(),
                    kind: msg.kind.clone(),
                    material: material_name(msg.material),
                    quantity: msg.quantity,
                    price: msg.price_per_unit_kwh,
                };
                // Avoid duplicates from repeated signal arrivals.
                if !self.market_feed.iter().any(|e| {
                    e.tick == entry.tick
                        && e.station_name == entry.station_name
                        && e.material == entry.material
                }) {
                    self.market_feed.push_front(entry);
                }
            }
        }
        // Keep only the most recent 64 messages to avoid unbounded growth.
        while self.market_feed.len() > 64 {
            self.market_feed.pop_back();
        }
    }
}

impl eframe::App for LunaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        self.try_recv_state();
        self.realtime_elapsed += ctx.input(|i| i.stable_dt) as f64;

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("MaritaV3 Luna Station");
                ui.separator();
                ui.label(&self.status);
                if let Some(state) = &self.latest {
                    ui.separator();
                    ui.label(format!("Tick: {}", state.tick));
                    ui.label(format!("Detections: {}", state.detections.len()));
                }
                ui.separator();
                ui.label(format!("RT: {:.1} s", self.realtime_elapsed));
            });
        });

        egui::SidePanel::left("controls")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Range");
                ui.add(
                    egui::Slider::new(&mut self.max_range_m, 1e9..=1e13)
                        .logarithmic(true)
                        .text("max distance (m)"),
                );
                ui.checkbox(&mut self.log_scale, "Logarithmic radial scale");

                ui.separator();
                ui.heading("Legend");
                for (idx, name) in BIN_NAMES.iter().enumerate() {
                    let color = bin_color(idx);
                    ui.horizontal(|ui| {
                        ui.painter_at(ui.max_rect()).rect_filled(
                            egui::Rect::from_min_size(ui.cursor().min, Vec2::new(12.0, 12.0)),
                            0.0,
                            color,
                        );
                        ui.checkbox(&mut self.enabled_bands[idx], *name);
                    });
                }
                ui.separator();
                ui.label(format!("Active tracks: {}", self.tracks.iter().count()));
            });

        egui::SidePanel::right("market_feed")
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Public Market Channel");
                ui.label("Radio broadcasts received at Luna");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for msg in &self.market_feed {
                        ui.group(|ui| {
                            ui.label(format!(
                                "{} from {} ({})",
                                msg.kind, msg.station_name, msg.body_name
                            ));
                            ui.label(format!(
                                "{} x {:.1} @ {:.2} kWh/u",
                                msg.material, msg.quantity, msg.price
                            ));
                            ui.label(format!("tick {}", msg.tick));
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(rect);
            let center = rect.center();
            let radius = rect.size().min_elem() * 0.5 - 20.0;

            draw_grid(&painter, center, radius, self.max_range_m, self.log_scale);

            if let Some(state) = &self.latest {
                draw_tracks(
                    &painter,
                    center,
                    radius,
                    self.max_range_m,
                    self.log_scale,
                    state.tick,
                    &self.tracks,
                    &self.enabled_bands,
                );
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn draw_grid(painter: &Painter, center: Pos2, radius: f32, max_range_m: f64, log_scale: bool) {
    let stroke = Stroke::new(1.0_f32, Color32::from_gray(50));
    let text_color = Color32::from_gray(180);

    // Outer range circle.
    painter.circle_stroke(center, radius, stroke);

    // Inner range rings.
    for i in 1..=4 {
        let frac = i as f32 / 5.0;
        let r = radius * frac;
        painter.circle_stroke(center, r, stroke);

        let range_label = if log_scale {
            let log_min = max_range_m.log10();
            let value = 10f64.powf(log_min * frac as f64);
            format_distance(value)
        } else {
            format_distance(max_range_m * frac as f64)
        };
        painter.text(
            Pos2::new(center.x - r, center.y),
            egui::Align2::RIGHT_CENTER,
            range_label,
            egui::FontId::proportional(10.0),
            text_color,
        );
    }

    // Cardinal direction lines.
    painter.line_segment(
        [center, Pos2::new(center.x, center.y - radius)],
        Stroke::new(1.0_f32, Color32::from_gray(40)),
    );
    painter.line_segment(
        [center, Pos2::new(center.x + radius, center.y)],
        Stroke::new(1.0_f32, Color32::from_gray(40)),
    );
    painter.line_segment(
        [center, Pos2::new(center.x, center.y + radius)],
        Stroke::new(1.0_f32, Color32::from_gray(40)),
    );
    painter.line_segment(
        [center, Pos2::new(center.x - radius, center.y)],
        Stroke::new(1.0_f32, Color32::from_gray(40)),
    );

    // Center label.
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        "Luna",
        egui::FontId::proportional(14.0),
        text_color,
    );
}

fn draw_tracks(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    max_range_m: f64,
    log_scale: bool,
    tick: u64,
    tracks: &TrackStore,
    enabled_bands: &[bool; 10],
) {
    for track in tracks.iter() {
        if track.distance <= 0.0 {
            continue;
        }
        let Some((band, (strength, _))) = track
            .bands
            .iter()
            .enumerate()
            .filter(|(band, value)| enabled_bands[*band] && value.is_some())
            .filter_map(|(band, value)| value.map(|sample| (band, sample)))
            .max_by(|a, b| {
                a.1 .0
                    .partial_cmp(&b.1 .0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            continue;
        };
        let normalized = if log_scale {
            (track.distance.log10() - 1.0) / (max_range_m.log10() - 1.0)
        } else {
            track.distance / max_range_m
        }
        .clamp(0.0, 1.0) as f32;
        let r = radius * normalized;
        let angle = track.bearing as f32;
        let pos = Pos2::new(center.x + r * angle.cos(), center.y - r * angle.sin());
        let age = tick.saturating_sub(track.last_tick);
        let color = if age > 5 {
            bin_color(band).gamma_multiply(0.45)
        } else {
            bin_color(band)
        };
        let size = 3.0 + (strength.log10().max(0.0) as f32).min(6.0);
        painter.circle_filled(pos, size, color);
        painter.text(
            pos + Vec2::new(size + 2.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!(
                "#{:x} t-{}",
                track.contact_id,
                tick.saturating_sub(track.emission_tick)
            ),
            egui::FontId::proportional(9.0),
            color,
        );
    }
}

fn bin_color(bin: usize) -> Color32 {
    match bin % 10 {
        0 => Color32::from_rgb(150, 75, 0),    // Radio
        1 => Color32::from_rgb(200, 100, 50),  // Microwave
        2 => Color32::RED,                     // Infrared
        3 => Color32::YELLOW,                  // Optical
        4 => Color32::from_rgb(150, 0, 255),   // UV
        5 => Color32::from_rgb(0, 200, 200),   // XRay
        6 => Color32::from_rgb(255, 255, 255), // Gamma
        7 => Color32::from_rgb(255, 100, 100), // EngineThermal
        8 => Color32::GREEN,                   // Radar
        9 => Color32::from_rgb(100, 255, 100), // Lidar
        _ => Color32::GRAY,
    }
}

fn format_distance(m: f64) -> String {
    if m >= 1.496e11 {
        format!("{:.2} AU", m / 1.496e11)
    } else if m >= 1e9 {
        format!("{:.0} Mm", m / 1e6)
    } else if m >= 1e6 {
        format!("{:.0} km", m / 1e3)
    } else {
        format!("{:.0} m", m)
    }
}

fn material_name(id: u32) -> String {
    match id {
        0 => "Regolith",
        1 => "Iron Ore",
        2 => "Aluminum Ore",
        3 => "Titanium Ore",
        4 => "Water Ice",
        5 => "Carbonaceous Ore",
        6 => "Silicate Ore",
        7 => "Rare Earth Ore",
        100 => "Iron",
        101 => "Aluminum",
        102 => "Titanium",
        103 => "Water",
        104 => "Oxygen",
        105 => "Hydrogen",
        106 => "Methane",
        107 => "Glass",
        200 => "Steel",
        201 => "Concrete",
        202 => "Polymer",
        203 => "Solar Silicon",
        300 => "Composite",
        301 => "Semiconductor",
        302 => "Advanced Alloy",
        400 => "Habitat Module",
        401 => "Refinery Module",
        402 => "Solar Array Module",
        _ => "Unknown",
    }
    .into()
}
