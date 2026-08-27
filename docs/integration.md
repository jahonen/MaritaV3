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

## gRPC Clients

Any language with Protobuf support can connect to `MaritaEngine`. The primary client contract is:
- Stream `ShipCommand` at the tick cadence.
- Receive `SimulationTick` at 1 Hz (real time), representing 10 s of simulation time.

## Telemetry / Logging

- Server logs use `tracing` (Rust standard) at `info`/`warn`/`error` levels.
- Structured metrics may be added later under a separate observability component.
