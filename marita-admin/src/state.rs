//! Lightweight snapshot state used by the admin viewer.

use marita_grpc::proto;

/// A body entity from the simulation.
#[derive(Clone, Debug)]
pub struct Body {
    pub id: u64,
    pub name: String,
    pub mass: f64,
    pub position: glam::DVec2,
    pub velocity: glam::DVec2,
    pub radius: f64,
}

/// A ship entity from the simulation.
#[derive(Clone, Debug)]
pub struct Ship {
    pub id: u64,
    pub name: String,
    pub dry_mass: f64,
    pub fuel_mass: f64,
    pub position: glam::DVec2,
    pub velocity: glam::DVec2,
    pub orientation: f64,
    pub angular_velocity: f64,
}

impl Ship {
    pub fn mass(&self) -> f64 {
        self.dry_mass + self.fuel_mass
    }
}

/// A signal arc from the simulation.
#[derive(Clone, Debug)]
pub struct SignalArc {
    pub id: u64,
    pub origin: glam::DVec2,
    pub direction: f64,
    pub angular_width: f64,
    pub inner_radius: f64,
    pub outer_radius: f64,
    pub source_id: Option<u64>,
    pub total_strength: f64,
}

/// Snapshot of the simulation world at one tick.
#[derive(Clone, Debug)]
pub struct ViewerState {
    pub tick: u64,
    pub sim_time: f64,
    pub bodies: Vec<Body>,
    pub ships: Vec<Ship>,
    pub signals: Vec<SignalArc>,
}

impl ViewerState {
    pub fn from_proto(proto: proto::SimulationTick) -> Self {
        Self {
            tick: proto.tick,
            sim_time: proto.sim_time,
            bodies: proto.bodies.into_iter().map(convert_body).collect(),
            ships: proto.ships.into_iter().map(convert_ship).collect(),
            signals: proto.signals.into_iter().map(convert_signal).collect(),
        }
    }

    pub fn position_of(&self, id: Option<u64>) -> Option<glam::DVec2> {
        let id = id?;
        self.bodies
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.position)
            .or_else(|| self.ships.iter().find(|s| s.id == id).map(|s| s.position))
    }
}

fn convert_body(b: proto::Body) -> Body {
    let pos = b
        .position
        .map(|v| glam::DVec2::new(v.x, v.y))
        .unwrap_or(glam::DVec2::ZERO);
    let vel = b
        .velocity
        .map(|v| glam::DVec2::new(v.x, v.y))
        .unwrap_or(glam::DVec2::ZERO);
    Body {
        id: b.id,
        name: b.name,
        mass: b.mass,
        position: pos,
        velocity: vel,
        radius: b.radius,
    }
}

fn convert_ship(s: proto::Ship) -> Ship {
    let pos = s
        .position
        .map(|v| glam::DVec2::new(v.x, v.y))
        .unwrap_or(glam::DVec2::ZERO);
    let vel = s
        .velocity
        .map(|v| glam::DVec2::new(v.x, v.y))
        .unwrap_or(glam::DVec2::ZERO);
    Ship {
        id: s.id,
        name: s.name,
        dry_mass: s.dry_mass,
        fuel_mass: s.fuel_mass,
        position: pos,
        velocity: vel,
        orientation: s.orientation,
        angular_velocity: s.angular_velocity,
    }
}

fn convert_signal(a: proto::SignalArc) -> SignalArc {
    let origin = a
        .origin
        .map(|v| glam::DVec2::new(v.x, v.y))
        .unwrap_or(glam::DVec2::ZERO);
    let total = a.spectrum.map(|s| s.bins.iter().sum()).unwrap_or(0.0);
    SignalArc {
        id: a.id,
        origin,
        direction: a.direction,
        angular_width: a.angular_width,
        inner_radius: a.inner_radius,
        outer_radius: a.outer_radius,
        source_id: a.source_id,
        total_strength: total,
    }
}
