# MaritaV3 — Start Here

MaritaV3 is a deterministic, API-first 2D heliocentric Newtonian physics engine
written in Rust. It simulates solar-system bodies, rigid-body ships, expanding
arc-segment signals, sensors, heat emission, and an emerging station economy.
AI-operated stations collect solar energy, refine local materials, and trade
missing inputs over a light-speed communication network.

## Project Structure

```
maritav3/
├── marita-core/          # Pure physics/economy simulation (no I/O, deterministic)
├── marita-grpc/          # gRPC service wrapper
├── marita-cli/           # CLI harness
├── marita-admin/         # Local egui gods-eye viewer
├── marita-luna/          # Restricted Luna observer GUI
├── marita-station-agent/ # Autonomous AI station client
├── docs/                 # Component and service docs
└── Cargo.toml            # Workspace manifest
```

## Quick Start

See `README.md` for the project overview and dedication.

Requires Rust (installed via rustup) and protobuf (`brew install protobuf` on macOS).

```bash
# Run all tests
cargo test --workspace

# Run a short local scenario with six AI stations. Two are parked at the
# Earth-Moon L4/L5 points; the rest are on planetary surfaces.
cargo run --bin marita -- scenario --ticks 100 --stations 6 --ships 0

# Start the gRPC server with six stations and strict causal omniband observation
cargo run --bin marita -- serve --stations 6 --ships 0 \
  --observer-model causal --history-au 100 --history-budget-mb 512 \
  --addr 127.0.0.1:50051

# Launch the Luna observer GUI to watch public market broadcasts (another terminal)
cargo run --bin marita-luna -- --addr http://127.0.0.1:50051

# Launch the admin viewer (another terminal)
cargo run --bin marita-admin -- --addr http://127.0.0.1:50051

# Run the autonomous station agent against the local server (optional)
ollama pull hermes3:8b
ollama serve
cargo run --bin marita-station-agent -- --addr http://127.0.0.1:50051 --station-id 2000
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
- **Sensors:** Selectable legacy or strict retarded-time causal observation,
  per-band response, one-bounce reflected light, historical occultation,
  deterministic uncertainty, and observer-scoped anonymous contacts.
- **Heat:** Simple thermal equilibrium producing blackbody IR emission.
- **Stations:** Industrial sites anchored to body surfaces or to L4/L5
  Lagrange points of major moons. They own solar collectors, warehouses,
  production lines, and public market posters.
- **Economy:** Deterministic scarcity/surplus posting; emergent trade driven by
  local needs, not a centralized market model.
- **AI agents:** Replaceable LLM adapters (MVP: Hermes 3 8B via Ollama) that
  propose actions; the engine validates and applies all state changes.

## Key Files

- `marita-core/src/state.rs` — simulation state and entity definitions
- `marita-core/src/tick.rs` — fixed-step orchestrator
- `marita-core/src/station.rs` — station economy bookkeeping
- `marita-core/src/material.rs` — materials and synthesis reactions
- `marita-core/src/gravity.rs` — N-body gravity
- `marita-core/src/signal.rs` — signal propagation and clipping
- `marita-core/src/sensor.rs` — sensor detection model
- `marita-core/src/heat.rs` — thermal equilibrium
- `marita-core/src/propulsion.rs` — rocket thrust
- `marita-core/src/collision.rs` — mass–mass collisions
- `marita-grpc/proto/marita.proto` — gRPC API contract
- `marita-station-agent/` — autonomous AI station client
- `docs/component.md` — component documentation
- `docs/services.md` — service documentation
- `docs/integration.md` — external integrations (SPICE, clients, LLM)

## Testing

```bash
cargo test --workspace
```

Integration tests include determinism, short-term orbital stability, ship burn
validation, station market-broadcast propagation to Luna, and a 50-body +
1000-ship load test.

## Next Steps / Known Limitations

- SPICE kernel loader is stubbed as a circular-orbit fallback.
- Full-year orbital stability tests are too slow in debug; run in `--release` or
  with signal propagation disabled for long-term ephemeris validation.
- Ship spawning is currently only done at startup; runtime ship creation via gRPC
  is a future addition.
- Station economy is a proof of concept: material tiers and reactions are
  representative, not a full chemistry simulation.
- The LLM agent MVP calls Ollama locally; the adapter trait supports swapping in
  vLLM, OpenAI-compatible, or other inference backends.

## License

Proprietary — All Rights Reserved. See [LICENSE](LICENSE).
