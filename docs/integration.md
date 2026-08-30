# MaritaV3 External Integrations

## SPICE / JPL Ephemeris

The engine loads initial state vectors for the largest solar-system bodies from local SPICE kernels. The supported workflow is:

1. Download NAIF SPICE kernels.
2. Run `scripts/generate_ephemeris.py` to produce a JSON snapshot.
3. Point the engine at the snapshot with `--ephemeris <path>`.

This avoids a native CSPICE build dependency in the Rust service.

### Required Kernels

Download from [NASA NAIF](https://naif.jpl.nasa.gov/naif/data.html) or use the helper:

```bash
# Lite set (~120 MB): Sun, Mercury, Venus, Earth, Moon
scripts/download_kernels.sh lite

# Full set (~several GB): adds major planet moons
scripts/download_kernels.sh full
```

The lite set contains:
- `de440.bsp` — planetary ephemeris.
- `pck00011.tpc` — physical constants and radii.
- `naif0012.tls` — leap seconds.

The full set adds satellite ephemeris kernels (`mar097.bsp`, `jup365.bsp`,
`sat441.bsp`, `ura111.bsp`, `nep081.bsp`, `plu055.bsp`).

### Generate a Snapshot

Use the Python virtual environment created in the repo:

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r scripts/requirements.txt  # installs spiceypy

python scripts/generate_ephemeris.py \
  --kernels-dir ./kernels \
  --epoch 2026-01-01T00:00:00Z \
  --out data/ephemeris.json
```

The script skips targets not covered by the loaded kernels (e.g., moons from a
satellite kernel that is not present).

### Use the Snapshot

```bash
# Server
cargo run --bin marita -- serve --ephemeris data/ephemeris.json

# Scenario
cargo run --bin marita -- scenario --ticks 100 --ephemeris data/ephemeris.json
```

The JSON loader projects 3D ecliptic state vectors onto the XY plane because
MaritaV3 is a 2D engine. Body masses are filled from a built-in lookup table.

### Fallback

If no SPICE kernels are available, use the built-in circular-orbit loader:

```bash
cargo run --bin marita -- serve --ephemeris circular
```

## JPL Horizons Ephemeris

If you do not want to download multi-gigabyte SPICE satellite kernels, use the
JPL Horizons web API instead:

```bash
source .venv/bin/activate
python scripts/generate_ephemeris_horizons.py \
  --epoch 2026-01-01T00:00:00Z \
  --out data/ephemeris.json
```

This produces the same snapshot format as the SPICE script and covers the Sun,
planets, major moons, and selected main-belt asteroids. Body masses and
temperatures are filled from the built-in lookup table.

## Causal Omniband Observation

The prototype can be enabled without changing the legacy default:

```bash
cargo run --bin marita -- serve --stations 6 --ships 0 \
  --observer-model causal --history-au 100 --history-budget-mb 512
```

Passive direct, thermal, reflected optical/UV, and configured natural radio signatures are evaluated from engine-private historical state. Discrete messages, radar, lidar, burns, and transients continue to use expanding `SignalArc`s. Unprivileged Luna/station APIs receive observer-scoped anonymous contacts rather than engine IDs.

Radiative body properties come from the bundled and startup-validated `marita-core/data/body-radiative-profiles.json`. Unknown names use its `default` profile. The causal history is intentionally not serialized into JSON checkpoints; after resume, observations whose retarded emission time predates available history are withheld until the light cone warms up, never replaced with current state.

## Simulation Checkpoints

The full simulation state (bodies, ships, stations, signals, clock, and next ID)
can be saved to and loaded from JSON checkpoint files. This is useful for long
runs, replays, and debugging.

```bash
# Save a checkpoint after a scenario run
cargo run --bin marita -- scenario \
  --ticks 1000 --ephemeris data/ephemeris.json --ships 1 \
  --checkpoint-out state.json

# Resume from a checkpoint
cargo run --bin marita -- scenario --ticks 100 --checkpoint-in state.json \
  --checkpoint-out state2.json

# Start the gRPC server from a checkpoint
cargo run --bin marita -- serve --checkpoint-in state.json
```

When `--checkpoint-in` is provided, ephemeris and ship-spawn options are
ignored.

## gRPC Clients

Any language with Protobuf support can connect to `MaritaEngine`. The client contracts are:
- Stream `ShipCommand` at the tick cadence to control ships.
- Stream `StationCommand` at the tick cadence to propose station actions.
- Receive `SimulationTick` at 1 Hz (real time), representing 10 s of simulation time.

Station commands are validated deterministically by the engine; clients do not own authoritative inventory, energy, or production state.

## Local LLM Agent Integration

The `marita-station-agent` crate provides a replaceable LLM adapter. The default MVP adapter connects to a local Ollama instance running Hermes 3 8B:

```bash
# Install Ollama and pull Hermes 3 8B
ollama pull hermes3:8b
ollama serve

# In another terminal, start the engine with stations
cargo run --bin marita -- serve --stations 6 --ships 0

# Run one independent process per station (repeat with IDs 2000..2005)
cargo run --bin marita-station-agent -- \
  --addr http://127.0.0.1:50051 \
  --station-id 2000 \
  --ollama-url http://localhost:11434 \
  --ollama-model hermes3:8b
```

To run without an LLM endpoint, pass `--no-llm`; a deterministic planner will propose scarcity/surplus actions and respond to physically received WANT/OFFER traffic. Every process is bound to one station and receives no global body, ship, signal, or other-station state.

## Telemetry / Logging

- Server logs use `tracing` (Rust standard) at `info`/`warn`/`error` levels.
- Structured metrics may be added later under a separate observability component.
