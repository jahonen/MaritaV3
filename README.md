# MaritaV3

A deterministic, API-first 2D heliocentric Newtonian physics engine written in
Rust.

MaritaV3 simulates the propagation of solar-system bodies, rigid-body ships,
expanding arc-segment signals, heat emission, and detailed ship sensors. It is
built for real-time use: one real-time second advances the simulation by ten
simulated seconds.

This project is dedicated to my grandmother, **Marita**, whose unwavering
belief in the imagination of kids made everything feel possible.

## Capabilities

- **Gravity:** top-3 influencer Newtonian gravity with a symplectic split-step
  integrator.
- **Ships:** full 2D rigid bodies with rocket-equation thrust, fuel consumption,
  and engine-mount torque.
- **Signals:** arc-segment waves expanding at the speed of light, carrying
  per-wavelength information spectra; support absorption, reflection,
  occlusion, and solar-system boundary culling.
- **Signal sources:** Sun illumination, blackbody thermal emission, engine
  exhaust signatures, and intentional ship emitters (radar, lidar, radio,
  laser).
- **Sensors:** detailed receiver model with aperture, field of view, noise
  floor, SNR, and jamming from other in-band signals.
- **Heat:** simple thermal equilibrium producing blackbody IR emission.
- **Collisions:** configurable per body type — celestial bodies merge, ships
  bounce.
- **API:** gRPC service with bidirectional command streaming and full-state
  snapshots.
- **Ephemeris:** local SPICE kernel support via `scripts/generate_ephemeris.py`
  and `scripts/download_kernels.sh`, a JPL Horizons fallback via
  `scripts/generate_ephemeris_horizons.py`, plus a circular-orbit fallback for
  development.
- **Checkpoints:** full simulation state can be saved to and loaded from JSON
  checkpoint files.
- **Admin Viewer:** local `egui`/`eframe` gods-eye visualizer that connects to
  the running gRPC server. It handles extreme scales (AU down to meters) with
  zoom/pan, labels, grid, and entity selection.

## Quick Start

Requires Rust (installed via rustup) and protobuf (`brew install protobuf` on
macOS).

```bash
# Run all tests
cargo test --workspace

# Run a short local scenario (circular ephemeris fallback)
cargo run --bin marita -- scenario --ticks 100

# Start the gRPC server with real SPICE ephemeris
cargo run --bin marita -- serve --ephemeris data/ephemeris.json --addr 127.0.0.1:50051

# Launch the admin viewer (in another terminal)
cargo run --bin marita-admin -- --addr http://127.0.0.1:50051
```

## Project Structure

```
maritav3/
├── marita-core/    # Pure physics simulation (deterministic, no I/O)
├── marita-grpc/    # gRPC service wrapper
├── marita-cli/     # CLI harness
├── marita-admin/   # Local egui gods-eye viewer
├── docs/           # Component and service documentation
└── START_HERE.md   # Contributor onboarding guide
```

## License

Proprietary — All Rights Reserved. See [LICENSE](LICENSE).

No part of this work may be copied, modified, distributed, or used without the
express prior written permission of the copyright holder.
