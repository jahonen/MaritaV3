//! egui application that renders the Luna station detection view.

use crate::client::spawn_client;
use egui::{Color32, Painter, Pos2, Stroke, Vec2};
use marita_grpc::proto::Detection;
use marita_grpc::proto::LunaDetections;
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
        }
    }

    fn try_recv_state(&mut self) {
        let mut received = false;
        while let Ok(state) = self.state_rx.try_recv() {
            self.latest = Some(state);
            received = true;
        }
        if received {
            self.status = "Connected".into();
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
                        ui.label(*name);
                    });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(rect);
            let center = rect.center();
            let radius = rect.size().min_elem() * 0.5 - 20.0;

            draw_grid(&painter, center, radius, self.max_range_m, self.log_scale);

            if let Some(state) = &self.latest {
                draw_detections(
                    &painter,
                    center,
                    radius,
                    self.max_range_m,
                    self.log_scale,
                    &state.detections,
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

fn draw_detections(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    max_range_m: f64,
    log_scale: bool,
    detections: &[Detection],
) {
    for d in detections {
        let distance = d.distance;
        if distance <= 0.0 {
            continue;
        }
        let normalized = if log_scale {
            (distance.log10() - 1.0) / (max_range_m.log10() - 1.0)
        } else {
            distance / max_range_m
        }
        .clamp(0.0, 1.0) as f32;
        let r = radius * normalized;

        // Bearing is world radians; screen Y is flipped.
        let angle = d.bearing as f32;
        let pos = Pos2::new(center.x + r * angle.cos(), center.y - r * angle.sin());

        let color = bin_color(d.wavelength_bin as usize);
        let size = 3.0 + (d.strength.log10().max(0.0) as f32).min(6.0);
        painter.circle_filled(pos, size, color);
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
