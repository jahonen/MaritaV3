//! 2D rendering transforms and drawing primitives for the admin viewer.

use crate::state::{SignalArc, ViewerState};
use egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2};
use glam::DVec2;
use marita_core::units::AU;

const MIN_BODY_RADIUS_PX: f32 = 3.0;
const MIN_SHIP_SIZE_PX: f32 = 5.0;
const MAX_BODY_RADIUS_PX: f32 = 80.0;

/// Viewport maps world coordinates (meters) to screen coordinates (pixels).
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub screen_size: Vec2,
    pub screen_center: Pos2,
    /// Offset in screen pixels applied after scaling.
    pub pan: Vec2,
    /// Pixels per meter.
    pub zoom: f64,
}

impl Viewport {
    pub fn fit_system() -> Self {
        Self {
            screen_size: Vec2::splat(1.0),
            screen_center: Pos2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0 / (120.0 * AU),
        }
    }

    pub fn fit_inner_system() -> Self {
        Self {
            screen_size: Vec2::splat(1.0),
            screen_center: Pos2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0 / (15.0 * AU),
        }
    }

    pub fn world_to_screen(&self, pos: DVec2) -> Pos2 {
        let offset = Vec2::new(
            (pos.x * self.zoom) as f32,
            (-pos.y * self.zoom) as f32, // flip Y so +Y is up
        );
        self.screen_center + self.pan + offset
    }

    pub fn screen_to_world(&self, pos: Pos2) -> DVec2 {
        let offset = pos - self.screen_center - self.pan;
        DVec2::new(offset.x as f64 / self.zoom, -offset.y as f64 / self.zoom)
    }

    pub fn range_meters(&self) -> f64 {
        let min_px = self.screen_size.min_elem() as f64;
        min_px / self.zoom
    }

    pub fn center_on(&mut self, pos: DVec2) {
        let screen = self.world_to_screen(pos);
        let delta = self.screen_center - screen;
        self.pan += delta;
    }

    pub fn zoom_at(&mut self, screen_pos: Pos2, factor: f64) {
        let world_before = self.screen_to_world(screen_pos);
        self.zoom *= factor;
        let world_after = self.screen_to_world(screen_pos);
        let delta = world_before - world_after;
        let screen_delta = Vec2::new((delta.x * self.zoom) as f32, (-delta.y * self.zoom) as f32);
        self.pan += screen_delta;
    }

    /// Radius in pixels, clamped to useful visible bounds.
    pub fn visible_radius(&self, physical_radius: f64) -> f32 {
        let px = (physical_radius * self.zoom) as f32;
        px.clamp(MIN_BODY_RADIUS_PX, MAX_BODY_RADIUS_PX)
    }
}

pub fn draw_trails(painter: &Painter, viewport: &Viewport, history: &crate::state::TrailHistory) {
    for (_id, points) in history.iter() {
        if points.len() < 2 {
            continue;
        }
        let screen_points: Vec<Pos2> = points
            .iter()
            .map(|p| viewport.world_to_screen(*p))
            .collect();
        // Fade the trail from old (dim) to new (bright).
        for window in screen_points.windows(2) {
            let a = window[0];
            let b = window[1];
            let alpha = ((window.len() as f32) / (screen_points.len() as f32) * 200.0 + 20.0)
                .clamp(20.0, 220.0) as u8;
            let color = Color32::from_rgba_unmultiplied(200, 200, 200, alpha);
            painter.line_segment([a, b], Stroke::new(1.0_f32, color));
        }
    }
}

pub fn draw_bodies(
    painter: &Painter,
    viewport: &Viewport,
    state: &ViewerState,
    labels: bool,
    selected: Option<u64>,
) {
    for body in &state.bodies {
        let center = viewport.world_to_screen(body.position);
        let radius = viewport.visible_radius(body.radius);
        let color = body_color(&body.name);

        painter.circle_filled(center, radius, color);
        if selected == Some(body.id) {
            painter.circle_stroke(center, radius + 4.0, Stroke::new(2.0_f32, Color32::WHITE));
        }

        if labels {
            let label_pos = center + Vec2::new(radius + 4.0, 0.0);
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                &body.name,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }
}

pub fn draw_ships(
    painter: &Painter,
    viewport: &Viewport,
    state: &ViewerState,
    labels: bool,
    selected: Option<u64>,
) {
    for ship in &state.ships {
        let center = viewport.world_to_screen(ship.position);
        let size = MIN_SHIP_SIZE_PX.max((ship.mass().sqrt() * viewport.zoom) as f32);
        let color = if selected == Some(ship.id) {
            Color32::YELLOW
        } else {
            Color32::LIGHT_BLUE
        };

        // Draw a small arrow indicating orientation.
        let arrow = arrow_shape(center, ship.orientation as f32, size, color);
        painter.add(arrow);

        if labels {
            let label_pos = center + Vec2::new(size + 4.0, 0.0);
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                &ship.name,
                egui::FontId::proportional(12.0),
                Color32::LIGHT_BLUE,
            );
        }
    }
}

pub fn draw_signals(painter: &Painter, viewport: &Viewport, state: &ViewerState) {
    for arc in &state.signals {
        draw_signal_arc(painter, viewport, arc);
    }
}

fn draw_signal_arc(painter: &Painter, viewport: &Viewport, arc: &SignalArc) {
    let origin = viewport.world_to_screen(arc.origin);
    let inner_px = (arc.inner_radius * viewport.zoom) as f32;
    let outer_px = (arc.outer_radius * viewport.zoom) as f32;

    // Skip if the ring is thinner than a pixel or the whole arc is off-screen.
    if outer_px < 1.0 || outer_px - inner_px < 0.5 {
        return;
    }

    let color = signal_color(arc.total_strength);
    let segments = ((arc.angular_width * 32.0).ceil() as usize).max(8);
    let half = arc.angular_width as f32 / 2.0;
    let center = arc.direction as f32;
    let start = center - half;

    let mut outer_points: Vec<Pos2> = Vec::with_capacity(segments + 1);
    let mut inner_points: Vec<Pos2> = Vec::with_capacity(segments + 1);

    for i in 0..=segments {
        let angle = start + half * 2.0 * (i as f32 / segments as f32);
        let (s, c) = angle.sin_cos();
        let dx = Vec2::new(c, -s); // flip Y
        outer_points.push(origin + dx * outer_px);
        inner_points.push(origin + dx * inner_px.max(0.0));
    }

    // Build a closed polygon from outer arc + reversed inner arc.
    let mut points = outer_points.clone();
    points.extend(inner_points.iter().rev());

    painter.add(Shape::Path(egui::epaint::PathShape {
        points,
        fill: color,
        stroke: egui::epaint::PathStroke::NONE,
        closed: true,
    }));
}

pub struct Grid;

impl Grid {
    pub fn draw(painter: &Painter, viewport: &Viewport) {
        let step = Self::nice_step(viewport.range_meters());
        if step <= 0.0 {
            return;
        }

        let rect = painter.clip_rect();
        let bottom_left = viewport.screen_to_world(rect.left_bottom());
        let top_right = viewport.screen_to_world(rect.right_top());

        let x_start = (bottom_left.x / step).floor() as i64;
        let x_end = (top_right.x / step).ceil() as i64;
        let y_start = (bottom_left.y / step).floor() as i64;
        let y_end = (top_right.y / step).ceil() as i64;

        let stroke = Stroke::new(1.0_f32, Color32::from_gray(40));

        for i in x_start..=x_end {
            let x = i as f64 * step;
            let a = viewport.world_to_screen(DVec2::new(x, bottom_left.y));
            let b = viewport.world_to_screen(DVec2::new(x, top_right.y));
            painter.line_segment([a, b], stroke);
        }

        for i in y_start..=y_end {
            let y = i as f64 * step;
            let a = viewport.world_to_screen(DVec2::new(bottom_left.x, y));
            let b = viewport.world_to_screen(DVec2::new(top_right.x, y));
            painter.line_segment([a, b], stroke);
        }
    }

    fn nice_step(range: f64) -> f64 {
        // Choose a grid spacing that is a round number and gives ~5-15 lines.
        let rough = range / 10.0;
        let exp = rough.log10().floor();
        let base = 10.0_f64.powf(exp);
        let normalized = rough / base;
        let factor = if normalized < 1.5 {
            1.0
        } else if normalized < 3.5 {
            2.0
        } else if normalized < 7.5 {
            5.0
        } else {
            10.0
        };
        factor * base
    }
}

fn arrow_shape(center: Pos2, orientation: f32, size: f32, color: Color32) -> Shape {
    // orientation is radians, 0 = +X, positive counter-clockwise in world space.
    let (s, c) = orientation.sin_cos();
    let forward = Vec2::new(c, -s); // flip Y
    let left = Vec2::new(-forward.y, forward.x) * 0.4;

    let tip = center + forward * size;
    let base = center - forward * size * 0.5;
    let p1 = base + left * size;
    let p2 = base - left * size;

    Shape::convex_polygon(vec![tip, p1, p2], color, Stroke::NONE)
}

fn body_color(name: &str) -> Color32 {
    match name.to_ascii_lowercase().as_str() {
        "sun" => Color32::YELLOW,
        "mercury" => Color32::from_rgb(169, 169, 169),
        "venus" => Color32::from_rgb(217, 186, 140),
        "earth" => Color32::from_rgb(70, 130, 180),
        "mars" => Color32::from_rgb(188, 39, 50),
        "jupiter" => Color32::from_rgb(183, 143, 101),
        "saturn" => Color32::from_rgb(217, 188, 129),
        "uranus" => Color32::from_rgb(144, 205, 210),
        "neptune" => Color32::from_rgb(62, 84, 232),
        _ => Color32::GRAY,
    }
}

fn signal_color(strength: f64) -> Color32 {
    // Map a wide range of signal strengths to a faint-to-visible green alpha.
    let log = strength.max(1.0).log10();
    let alpha = (log * 4.0).clamp(4.0, 96.0) as u8;
    Color32::from_rgba_unmultiplied(0, 255, 100, alpha)
}
