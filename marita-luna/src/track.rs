use marita_grpc::proto::Detection;
use std::collections::HashMap;

#[derive(Clone)]
pub struct Track {
    pub contact_id: u64,
    pub bearing: f64,
    pub distance: f64,
    pub bearing_sigma: f64,
    pub range_sigma: f64,
    pub last_tick: u64,
    pub emission_tick: u64,
    pub bands: [Option<(f64, f64)>; 10],
}

pub struct TrackStore {
    tracks: HashMap<u64, Track>,
    stale_after_ticks: u64,
}

impl TrackStore {
    pub fn new(stale_after_ticks: u64) -> Self {
        Self {
            tracks: HashMap::new(),
            stale_after_ticks,
        }
    }

    pub fn update(&mut self, tick: u64, detections: &[Detection]) {
        for detection in detections {
            if detection.contact_id == 0 {
                continue;
            }
            let track = self.tracks.entry(detection.contact_id).or_insert(Track {
                contact_id: detection.contact_id,
                bearing: detection.bearing,
                distance: detection.distance,
                bearing_sigma: detection.bearing_sigma,
                range_sigma: detection.range_sigma,
                last_tick: tick,
                emission_tick: detection.emission_tick,
                bands: [None; 10],
            });
            let alpha = 0.35;
            track.bearing += angle_delta(track.bearing, detection.bearing) * alpha;
            track.distance += (detection.distance - track.distance) * alpha;
            track.bearing_sigma = detection.bearing_sigma;
            track.range_sigma = detection.range_sigma;
            track.last_tick = tick;
            track.emission_tick = detection.emission_tick;
            if let Some(band) = track.bands.get_mut(detection.wavelength_bin as usize) {
                *band = Some((detection.strength, detection.snr));
            }
        }
        self.tracks
            .retain(|_, track| tick.saturating_sub(track.last_tick) <= self.stale_after_ticks);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }
}

fn angle_delta(from: f64, to: f64) -> f64 {
    let mut delta = to - from;
    while delta > std::f64::consts::PI {
        delta -= 2.0 * std::f64::consts::PI;
    }
    while delta < -std::f64::consts::PI {
        delta += 2.0 * std::f64::consts::PI;
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_are_updated_and_expire() {
        let mut store = TrackStore::new(5);
        let detection = Detection {
            source_id: None,
            contact_id: 42,
            wavelength_bin: 3,
            bearing: 1.0,
            distance: 100.0,
            strength: 2.0,
            snr: 3.0,
            market_payload: None,
            bearing_sigma: 0.1,
            range_sigma: 2.0,
            emission_tick: 1,
        };
        store.update(2, &[detection]);
        assert_eq!(store.iter().count(), 1);
        store.update(8, &[]);
        assert_eq!(store.iter().count(), 0);
    }
}
