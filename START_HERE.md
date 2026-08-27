# MaritaV3 — Start Here

MaritaV3 is a deterministic, API-first 2D heliocentric Newtonian physics engine
written in Rust. It simulates solar-system bodies, rigid-body ships, expanding
arc-segment signals, sensors, and heat emission.

## Project Structure

```
maritav3/
├── marita-core/   # Pure physics simulation (no I/O, deterministic)
├── marita-grpc/   # gRPC service wrapper
├── marita-cli/    # CLI harness
├── marita-admin/  # Local egui gods-eye viewer
├── docs/          # Component and service docs
└── Cargo.toml     # Workspace manifest
```

## Quick Start

See `README.md` for the project overview and dedication.

Requires Rust (installed via rustup) and protobuf (`brew install protobuf` on macOS).

```bash
# Run all tests
cargo test --workspace

# Run a short local scenario
cargo run --bin marita -- scenario --ticks 100

# Start the gRPC server (in one terminal)
cargo run --bin marita -- serve --addr 127.0.0.1:50051

# Launch the admin viewer (in another terminal)
cargo run --bin marita-admin -- --addr http://127.0.0.1:50051
```

## Architecture

- **Tick model:** 1 real-time second = 1 engine tick = 10 simulation seconds.
- **Celestial bodies:** Loaded from a simplified circular-orbit ephemeris by
  default. SPICE kernel loading is documented as a follow-up integration.
- **Ships:** Full 2D rigid bodies with rocket-equation thrust, fuel consumption,
  and engine-mount torque.
- **Gravity:** Top-3-influencer approximation for O(N log N) performance.
- **Signals:** Arc segments expanding at `c` with per-wavelength information
  budgets; support absorption, reflection, and occlusion.
- **Sensors:** Detailed receiver model with aperture, FOV, noise floor, and
  jamming.
- **Heat:** Simple thermal equilibrium producing blackbody IR emission.

## Key Files

- `marita-core/src/state.rs` — simulation state and entity definitions
- `marita-core/src/tick.rs` — fixed-step orchestrator
- `marita-core/src/gravity.rs` — N-body gravity
- `marita-core/src/signal.rs` — signal propagation and clipping
- `marita-core/src/sensor.rs` — sensor detection model
- `marita-core/src/heat.rs` — thermal equilibrium
- `marita-core/src/propulsion.rs` — rocket thrust
- `marita-core/src/collision.rs` — mass–mass collisions
- `marita-grpc/proto/marita.proto` — gRPC API contract
- `docs/component.md` — component documentation
- `docs/services.md` — service documentation
- `docs/integration.md` — external integrations (SPICE, clients)

## Testing

```bash
cargo test --workspace
```

Integration tests include determinism, short-term orbital stability, ship burn
validation, and a 50-body + 1000-ship load test.

## Next Steps / Known Limitations

- SPICE kernel loader is stubbed as a circular-orbit fallback.
- Full-year orbital stability tests are too slow in debug; run in `--release` or
  with signal propagation disabled for long-term ephemeris validation.
- Ship spawning is currently only done at startup (e.g., in the CLI scenario and
  load test); runtime ship creation via gRPC is a future addition.

## License

Proprietary — All Rights Reserved. See [LICENSE](LICENSE).
