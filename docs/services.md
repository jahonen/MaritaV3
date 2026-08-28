# MaritaV3 Services

This document describes every callable/deployable service in MaritaV3.

## `MaritaEngine` (gRPC)

A single service exposing the simulation engine to clients over the network.

### RPCs

#### `StreamCommands`
- **Type:** Bidirectional streaming.
- **Inputs:** Client sends a stream of `ShipCommand` messages (throttle, gimbal, emitter toggles).
- **Outputs:** Server returns a stream of `SimulationTick` snapshots.
- **Side effects:** Updates ship controls in real time; broadcasts world state.

#### `GetState`
- **Type:** Unary request/response.
- **Inputs:** `GetStateRequest` with optional tick number.
- **Outputs:** Full `SimulationState` snapshot.
- **Side effects:** None (read-only).

#### `StreamLunaView`
- **Type:** Server-side streaming.
- **Inputs:** `LunaViewRequest` (empty).
- **Outputs:** Stream of `LunaDetections` messages: tick, sim time, and the list of `Detection` events from an omnidirectional sensor at Luna.
- **Side effects:** None (read-only). Provides a restricted, ship-client-compatible view that contains only bearings, distances, wavelengths, and strengths — no absolute coordinates.

## `MaritaCli`

A command-line harness for local scenarios and the gRPC server. Not a network service on its own.

### `marita serve`
- **Inputs:** `--addr`, `--ephemeris`, `--ships`, `--checkpoint-in <path>`.
- **Outputs:** gRPC server listening on `--addr`.
- **Side effects:** Loads ephemeris or checkpoint, spawns the gRPC server.

### `marita scenario`
- **Inputs:** `--ticks`, `--ephemeris`, `--ships`, `--max-signals`, `--checkpoint-in <path>`, `--checkpoint-out <path>`.
- **Outputs:** Console tick log; optional JSON checkpoint file.
- **Side effects:** Writes checkpoint file if `--checkpoint-out` is given.

### `marita benchmark`
- **Inputs:** same as `scenario` except `--checkpoint-out`.
- **Outputs:** Total wall time, ticks per second, milliseconds per tick, and final entity/signal counts.
- **Side effects:** None.

If `--checkpoint-in` is provided, the ephemeris and ship spawn options are ignored and the run starts from the saved state.

## `MaritaAdmin`

A local GUI viewer built with `egui`/`eframe` that consumes the `MaritaEngine` gRPC stream.

- **Inputs:** `MaritaEngine::StreamCommands` (empty command stream) or `GetState` snapshots.
- **Outputs:** Real-time 2D visualization with pan/zoom, labels, grid, orbit lines, signal toggle, ship controls, and entity selection.
- **Side effects:** Connects to the gRPC server over the network; renders GUI.

## `MaritaLuna`

A separate `egui`/`eframe` observer client that shows the infosphere from Luna's perspective. It uses only the restricted detection feed and never receives absolute coordinates.

- **Inputs:** `MaritaEngine::StreamLunaView`.
- **Outputs:** Polar plot of detections around Luna, coloured by wavelength bin, with radial distance and logarithmic-scale options.
- **Side effects:** Connects to the gRPC server over the network; renders GUI.
