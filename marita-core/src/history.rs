//! Bounded engine-private state history for causal passive observations.

use crate::state::{SimulationState, Spectrum};
use crate::units::{AU, SPEED_OF_LIGHT, TICK_SIM_TIME};
use glam::DVec2;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    Body,
    Ship,
    Station,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityKey {
    pub kind: EntityKind,
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservationSample {
    pub key: EntityKey,
    pub position: DVec2,
    pub velocity: DVec2,
    pub orientation: f64,
    pub radius: f64,
    pub temperature: f64,
    pub radiating_area: f64,
    pub emissivity: f64,
    pub albedo: Spectrum,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryFrame {
    pub tick: u64,
    pub sim_time: f64,
    pub samples: Vec<ObservationSample>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservationHistory {
    frames: VecDeque<HistoryFrame>,
    max_frames: usize,
    budget_bytes: usize,
}

impl Default for ObservationHistory {
    fn default() -> Self {
        Self::new(100.0, 512 * 1024 * 1024)
    }
}

impl ObservationHistory {
    pub fn new(history_au: f64, budget_bytes: usize) -> Self {
        let light_frames =
            ((history_au.max(0.0) * AU) / (SPEED_OF_LIGHT * TICK_SIM_TIME)).ceil() as usize + 2;
        Self {
            frames: VecDeque::new(),
            max_frames: light_frames.max(2),
            budget_bytes,
        }
    }

    pub fn append(&mut self, state: &SimulationState) {
        let mut samples =
            Vec::with_capacity(state.bodies.len() + state.ships.len() + state.stations.len());
        samples.extend(state.bodies.iter().map(|body| ObservationSample {
            key: EntityKey {
                kind: EntityKind::Body,
                id: body.id,
            },
            position: body.position,
            velocity: body.velocity,
            orientation: 0.0,
            radius: body.radius,
            temperature: body.thermal.temperature,
            radiating_area: body.thermal.surface_area,
            emissivity: body.thermal.emissivity,
            albedo: body.albedo,
        }));
        samples.extend(state.ships.iter().map(|ship| ObservationSample {
            key: EntityKey {
                kind: EntityKind::Ship,
                id: ship.id,
            },
            position: ship.position,
            velocity: ship.velocity,
            orientation: ship.orientation,
            radius: ship.radius(),
            temperature: ship.thermal.temperature,
            radiating_area: ship.thermal.surface_area,
            emissivity: ship.thermal.emissivity,
            albedo: ship.albedo,
        }));
        samples.extend(state.stations.iter().map(|station| ObservationSample {
            key: EntityKey {
                kind: EntityKind::Station,
                id: station.id,
            },
            position: station.position(&state.bodies),
            velocity: DVec2::ZERO,
            orientation: 0.0,
            radius: station.radius(),
            temperature: station.thermal.temperature,
            radiating_area: station.thermal.surface_area,
            emissivity: station.thermal.emissivity,
            albedo: station.albedo,
        }));
        samples.sort_by_key(|sample| sample.key);
        self.frames.push_back(HistoryFrame {
            tick: state.tick,
            sim_time: state.sim_time,
            samples,
        });
        let frame_bytes = self
            .frames
            .back()
            .map(|frame| frame.samples.len() * std::mem::size_of::<ObservationSample>())
            .unwrap_or(1)
            .max(1);
        let budget_frames = (self.budget_bytes / frame_bytes).max(2);
        let limit = self.max_frames.min(budget_frames);
        while self.frames.len() > limit {
            self.frames.pop_front();
        }
    }

    pub fn sample(&self, key: EntityKey, time: f64) -> Option<ObservationSample> {
        let first = self.frames.front()?;
        let last = self.frames.back()?;
        if time < first.sim_time || time > last.sim_time {
            return None;
        }
        let span = (last.sim_time - first.sim_time).max(f64::MIN_POSITIVE);
        let approximate =
            ((time - first.sim_time) / span * (self.frames.len() - 1) as f64).ceil() as usize;
        let upper = approximate.min(self.frames.len() - 1);
        let lower = upper.saturating_sub(1);
        let a = self.find_in_frame(lower, key)?;
        let b = self.find_in_frame(upper, key)?;
        let ta = self.frames[lower].sim_time;
        let tb = self.frames[upper].sim_time;
        if tb <= ta {
            return Some(a.clone());
        }
        let f = ((time - ta) / (tb - ta)).clamp(0.0, 1.0);
        Some(ObservationSample {
            key,
            position: a.position.lerp(b.position, f),
            velocity: a.velocity.lerp(b.velocity, f),
            orientation: a.orientation + (b.orientation - a.orientation) * f,
            radius: a.radius + (b.radius - a.radius) * f,
            temperature: a.temperature + (b.temperature - a.temperature) * f,
            radiating_area: a.radiating_area + (b.radiating_area - a.radiating_area) * f,
            emissivity: a.emissivity + (b.emissivity - a.emissivity) * f,
            albedo: interpolate_spectrum(a.albedo, b.albedo, f),
        })
    }

    fn find_in_frame(&self, frame: usize, key: EntityKey) -> Option<&ObservationSample> {
        let samples = &self.frames.get(frame)?.samples;
        samples
            .binary_search_by_key(&key, |sample| sample.key)
            .ok()
            .map(|i| &samples[i])
    }

    pub fn oldest_time(&self) -> Option<f64> {
        self.frames.front().map(|f| f.sim_time)
    }
    pub fn newest_time(&self) -> Option<f64> {
        self.frames.back().map(|f| f.sim_time)
    }
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn estimated_bytes(&self) -> usize {
        self.frames
            .iter()
            .map(|f| f.samples.capacity() * std::mem::size_of::<ObservationSample>())
            .sum()
    }
}

fn interpolate_spectrum(a: Spectrum, b: Spectrum, f: f64) -> Spectrum {
    let mut out = Spectrum::zero();
    for i in 0..out.bins.len() {
        out.bins[i] = a.bins[i] + (b.bins[i] - a.bins[i]) * f;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::{CircularOrbitLoader, EphemerisLoader};

    #[test]
    fn history_interpolates_and_obeys_budget() {
        let mut state = SimulationState::new();
        state.bodies = CircularOrbitLoader.load();
        let mut history = ObservationHistory::new(100.0, 1024 * 1024);
        history.append(&state);
        state.sim_time = 10.0;
        state.bodies[0].position.x = 10.0;
        history.append(&state);
        let sample = history
            .sample(
                EntityKey {
                    kind: EntityKind::Body,
                    id: 1,
                },
                5.0,
            )
            .unwrap();
        assert!((sample.position.x - 5.0).abs() < 1e-9);
        assert!(history.estimated_bytes() <= 1024 * 1024);
    }
}
