# MaritaV3 External Integrations

## SPICE / JPL Ephemeris

The engine loads initial state vectors for the largest solar-system bodies from SPICE kernels.

### Options (in evaluation order)

1. **`rust-spice` crate** — direct Rust bindings. Preferred if it builds on the target platform and supports the required kernel types (DE440, PCK).
2. **Python bootstrap script** — uses `spiceypy` to read kernels and write a static JSON snapshot at build/run time. Used as fallback if `rust-spice` is unavailable.
3. **Bundled JSON snapshot** — checked-in initial conditions for offline testing and reproducibility.

### Required Kernels
- `de440.bsp` — planetary ephemeris.
- `pck00011.tpc` — physical constants and radii.
- `naif0012.tls` — leap seconds.

## gRPC Clients

Any language with Protobuf support can connect to `MaritaEngine`. The primary client contract is:
- Stream `ShipCommand` at the tick cadence.
- Receive `SimulationTick` at 1 Hz (real time), representing 10 s of simulation time.

## Telemetry / Logging

- Server logs use `tracing` (Rust standard) at `info`/`warn`/`error` levels.
- Structured metrics may be added later under a separate observability component.
