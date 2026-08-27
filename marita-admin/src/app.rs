//! Main egui application state and UI logic.

use crate::client::spawn_client;
use crate::render::{draw_bodies, draw_ships, draw_signals, Grid, Viewport};
use crate::state::ViewerState;
use std::sync::mpsc;

pub struct AdminApp {
    #[allow(dead_code)]
    _runtime: tokio::runtime::Runtime,
    addr: String,
    state_rx: mpsc::Receiver<ViewerState>,
    latest: Option<ViewerState>,
    status: String,
    viewport: Viewport,
    show_signals: bool,
    show_labels: bool,
    follow_selection: bool,
    selected_entity: Option<u64>,
    realtime_elapsed: f64,
    sim_time: f64,
    tick: u64,
}

impl AdminApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, addr: String) -> Self {
        let (state_tx, state_rx) = mpsc::channel();
        let runtime = spawn_client(addr.clone(), state_tx);

        Self {
            _runtime: runtime,
            addr,
            state_rx,
            latest: None,
            status: "Connecting...".into(),
            viewport: Viewport::fit_system(),
            show_signals: true,
            show_labels: true,
            follow_selection: false,
            selected_entity: None,
            realtime_elapsed: 0.0,
            sim_time: 0.0,
            tick: 0,
        }
    }

    fn try_recv_state(&mut self) {
        let mut received = false;
        while let Ok(state) = self.state_rx.try_recv() {
            self.sim_time = state.sim_time;
            self.tick = state.tick;
            self.latest = Some(state);
            received = true;
        }
        if received {
            self.status = format!("Connected to {}", self.addr);
        }
    }

    fn nearest_entity(&self, screen_pos: egui::Pos2, state: &ViewerState) -> Option<u64> {
        let click_world = self.viewport.screen_to_world(screen_pos);
        let pick_radius = 12.0 / self.viewport.zoom;

        let mut best: Option<(u64, f64)> = None;
        for body in &state.bodies {
            let d = (body.position - click_world).length();
            if d < pick_radius {
                if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((body.id, d));
                }
            }
        }
        for ship in &state.ships {
            let d = (ship.position - click_world).length();
            if d < pick_radius {
                if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((ship.id, d));
                }
            }
        }
        best.map(|(id, _)| id)
    }
}

impl eframe::App for AdminApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.try_recv_state();
        self.realtime_elapsed += ctx.input(|i| i.stable_dt) as f64;

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("MaritaV3 Admin Viewer");
                ui.separator();
                ui.label(&self.status);
                ui.separator();
                ui.label(format!("Tick: {}", self.tick));
                ui.label(format!("Sim time: {:.1} s", self.sim_time));
                ui.label(format!("RT: {:.1} s", self.realtime_elapsed));
            });
        });

        egui::SidePanel::left("controls")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("View");
                ui.checkbox(&mut self.show_signals, "Show signals");
                ui.checkbox(&mut self.show_labels, "Labels");
                ui.checkbox(&mut self.follow_selection, "Follow selected");

                ui.separator();
                ui.heading("Scale");
                ui.label(format!("Zoom: {:.3e} px/m", self.viewport.zoom));
                ui.label(format!("Range: {:.2e} m", self.viewport.range_meters()));

                if ui.button("Fit Solar System").clicked() {
                    self.viewport = Viewport::fit_system();
                }
                if ui.button("Fit Inner System").clicked() {
                    self.viewport = Viewport::fit_inner_system();
                }

                ui.separator();
                ui.heading("Selection");
                if let Some(id) = self.selected_entity {
                    ui.label(format!("Entity: {}", id));
                } else {
                    ui.label("None");
                }

                if let Some(state) = &self.latest {
                    ui.separator();
                    ui.heading("Entities");
                    ui.label(format!("Bodies: {}", state.bodies.len()));
                    ui.label(format!("Ships: {}", state.ships.len()));
                    ui.label(format!("Signals: {}", state.signals.len()));

                    egui::ScrollArea::vertical()
                        .max_height(400.0)
                        .show(ui, |ui| {
                            for body in &state.bodies {
                                let selected = self.selected_entity == Some(body.id);
                                let label =
                                    egui::RichText::new(format!("{} ({})", body.name, body.id));
                                let response = ui.selectable_label(selected, label);
                                if response.clicked() {
                                    self.selected_entity = Some(body.id);
                                }
                            }
                            for ship in &state.ships {
                                let selected = self.selected_entity == Some(ship.id);
                                let label =
                                    egui::RichText::new(format!("{} ({})", ship.name, ship.id));
                                let response = ui.selectable_label(selected, label);
                                if response.clicked() {
                                    self.selected_entity = Some(ship.id);
                                }
                            }
                        });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            self.viewport.screen_size = rect.size();
            self.viewport.screen_center = rect.center();

            // Pan/zoom/click input.
            let response = ui.allocate_rect(rect, egui::Sense::drag() | egui::Sense::click());
            if response.dragged() {
                let delta = response.drag_delta();
                self.viewport.pan.x += delta.x;
                self.viewport.pan.y += delta.y;
            }
            if let Some(hover_pos) = response.hover_pos() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let factor = (scroll / 120.0).exp2() as f64;
                    self.viewport.zoom_at(hover_pos, factor);
                }
            }
            if response.clicked() {
                if let (Some(cursor), Some(state)) =
                    (response.interact_pointer_pos(), self.latest.as_ref())
                {
                    self.selected_entity = self.nearest_entity(cursor, state);
                }
            }

            if self.follow_selection {
                if let Some(state) = &self.latest {
                    if let Some(pos) = state.position_of(self.selected_entity) {
                        self.viewport.center_on(pos);
                    }
                }
            }

            let painter = ui.painter_at(rect);
            Grid::draw(&painter, &self.viewport);

            if let Some(state) = &self.latest {
                draw_bodies(
                    &painter,
                    &self.viewport,
                    state,
                    self.show_labels,
                    self.selected_entity,
                );
                draw_ships(
                    &painter,
                    &self.viewport,
                    state,
                    self.show_labels,
                    self.selected_entity,
                );
                if self.show_signals {
                    draw_signals(&painter, &self.viewport, state);
                }
            }
        });

        // Keep repainting so the stream updates are visible.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
