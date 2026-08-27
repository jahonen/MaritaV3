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

## `MaritaCli`

A command-line harness for local runs, replays, and scripted scenarios. Not a network service.

- **Inputs:** Subcommands (`run`, `replay`, `benchmark`).
- **Outputs:** Console output, optionally JSON logs.
- **Side effects:** Spawns the gRPC server, writes logs.

## `MaritaAdmin`

A local GUI viewer built with `egui`/`eframe` that consumes the `MaritaEngine` gRPC stream.

- **Inputs:** `MaritaEngine::StreamCommands` (empty command stream) or `GetState` snapshots.
- **Outputs:** Real-time 2D visualization with pan/zoom, labels, grid, and entity selection.
- **Side effects:** Connects to the gRPC server over the network; renders GUI.
