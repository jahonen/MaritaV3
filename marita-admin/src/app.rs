//! Main egui application state and UI logic.

use crate::client::{spawn_client, CommandHandle};
use crate::render::{
    draw_bodies, draw_orbits, draw_ships, draw_signals, draw_trails, Grid, Viewport,
};
use crate::state::{TrailHistory, ViewerState};
use marita_grpc::proto::ShipCommand;
use std::collections::HashMap;
use std::sync::mpsc;

pub struct AdminApp {
    #[allow(dead_code)]
    _runtime: tokio::runtime::Runtime,
    addr: String,
    state_rx: mpsc::Receiver<ViewerState>,
    command_handle: CommandHandle,
    latest: Option<ViewerState>,
    status: String,
    viewport: Viewport,
    show_signals: bool,
    show_orbits: bool,
    show_labels: bool,
    follow_selection: bool,
    selected_entity: Option<u64>,
    ship_controls: HashMap<u64, ShipControlState>,
    show_trails: bool,
    trail_history: TrailHistory,
    realtime_elapsed: f64,
    sim_time: f64,
    tick: u64,
}

impl AdminApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, addr: String) -> Self {
        let (state_tx, state_rx) = mpsc::channel();
        let (runtime, command_handle) = spawn_client(addr.clone(), state_tx);

        Self {
            _runtime: runtime,
            addr,
            state_rx,
            command_handle,
            latest: None,
            status: "Connecting...".into(),
            viewport: Viewport::fit_system(),
            show_signals: true,
            show_orbits: true,
            show_labels: true,
            follow_selection: false,
            selected_entity: None,
            ship_controls: HashMap::new(),
            show_trails: true,
            trail_history: TrailHistory::new(500),
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
            self.trail_history.update(&state);
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

    fn control_state(&mut self, ship_id: u64) -> &mut ShipControlState {
        self.ship_controls.entry(ship_id).or_default()
    }

    fn send_command(&self, ship_id: u64, control: &ShipControlState) {
        let emitters = control
            .emitter_states
            .iter()
            .map(|(&idx, &active)| marita_grpc::proto::EmitterCommand {
                emitter_index: idx as u64,
                active,
            })
            .collect();
        let cmd = ShipCommand {
            tick: 0,
            ship_id,
            throttle: control.throttle,
            gimbal: control.gimbal,
            emitters,
        };
        self.command_handle.push(cmd);
    }
}

#[derive(Clone, Debug, Default)]
struct ShipControlState {
    throttle: f64,
    gimbal: f64,
    /// Per-emitter active flags, keyed by emitter index.
    emitter_states: HashMap<usize, bool>,
}

impl eframe::App for AdminApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        self.try_recv_state();

        // Continuously send the current control state for the selected ship so
        // burns persist across engine ticks.
        let maybe_ship_id = self.latest.as_ref().and_then(|state| {
            state
                .ships
                .iter()
                .find(|s| Some(s.id) == self.selected_entity)
                .map(|s| s.id)
        });
        if let Some(ship_id) = maybe_ship_id {
            let control = self.control_state(ship_id).clone();
            self.send_command(ship_id, &control);
        }

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
                ui.checkbox(&mut self.show_trails, "Trails");
                ui.checkbox(&mut self.show_signals, "Show active signals");
                ui.checkbox(&mut self.show_orbits, "Orbits");
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

                let selected_ship_id = self.latest.as_ref().and_then(|state| {
                    self.selected_entity
                        .and_then(|id| state.ships.iter().find(|s| s.id == id).map(|s| s.id))
                });

                if let Some(ship_id) = selected_ship_id {
                    ui.separator();
                    ui.heading("Ship Control");
                    let control = self.control_state(ship_id).clone();

                    let mut control = control;
                    let mut changed = false;

                    ui.label("Throttle");
                    changed |= ui
                        .add(egui::Slider::new(&mut control.throttle, 0.0..=1.0))
                        .changed();

                    ui.label("Gimbal (rad)");
                    changed |= ui
                        .add(egui::Slider::new(
                            &mut control.gimbal,
                            -std::f64::consts::PI..=std::f64::consts::PI,
                        ))
                        .changed();

                    if changed {
                        self.send_command(ship_id, &control);
                    }
                    *self.control_state(ship_id) = control;
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

        egui::SidePanel::right("emitter_controls")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Emitters");
                let selected_id = self.selected_entity;
                let emitter_defaults: Vec<_> = self
                    .latest
                    .as_ref()
                    .and_then(|state| {
                        selected_id.and_then(|id| {
                            state
                                .ships
                                .iter()
                                .find(|s| s.id == id)
                                .map(|s| s.emitters.clone())
                        })
                    })
                    .unwrap_or_default();

                if let Some(ship_id) = selected_id {
                    if emitter_defaults.is_empty() {
                        ui.label("Selected ship has no emitters.");
                    } else {
                        let mut control = self.control_state(ship_id).clone();
                        let mut changed = false;

                        // Seed the control map with the ship's current emitter
                        // defaults so toggles reflect the real initial state.
                        for (idx, emitter) in emitter_defaults.iter().enumerate() {
                            let entry = control.emitter_states.entry(idx).or_insert(emitter.active);
                            let label = format!("Emitter {} ({:?})", idx, emitter.wavelength_bin);
                            changed |= ui.checkbox(entry, &label).changed();
                        }

                        if changed {
                            self.send_command(ship_id, &control);
                        }
                        *self.control_state(ship_id) = control;
                    }
                } else {
                    ui.label("Select a ship to control emitters.");
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

            if self.show_trails {
                draw_trails(&painter, &self.viewport, &self.trail_history);
            }

            if let Some(state) = &self.latest {
                if self.show_orbits {
                    draw_orbits(&painter, &self.viewport, state);
                }
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
