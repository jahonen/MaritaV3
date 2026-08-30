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

#### `StreamStationCommands`
- **Type:** Bidirectional streaming.
- **Inputs:** Client sends a stream of `StationCommand` messages (post market message, start production, set collector area).
- **Outputs:** Server returns a stream of `SimulationTick` snapshots.
- **Side effects:** Updates station controls in real time; broadcasts world state.

#### `GetState`
- **Type:** Unary request/response.
- **Inputs:** `GetStateRequest` with optional tick number.
- **Outputs:** Full `SimulationState` snapshot including `Station`s and market posters.
- **Side effects:** None (read-only).

#### `StreamLunaView`
- **Type:** Server-side streaming.
- **Inputs:** `LunaViewRequest` (empty).
- **Outputs:** Causally delayed anonymous contacts with wavelength, noisy bearing/range, uncertainty, SNR, emission tick, and decoded Radio payload when available.
- **Side effects:** None (read-only). Authoritative source IDs and absolute remote coordinates are stripped at the service boundary.

## `MaritaCli`

A command-line harness for local scenarios and the gRPC server. Not a network service on its own.

### `marita serve`
- **Inputs:** `--addr`, `--ephemeris`, `--ships`, `--stations`, `--max-signals`, `--observer-model legacy|causal`, `--history-au`, `--history-budget-mb`, `--checkpoint-in <path>`.
- **Outputs:** gRPC server listening on `--addr`.
- **Side effects:** Loads ephemeris or checkpoint, spawns ships/stations, starts the gRPC server.

### `marita scenario`
- **Inputs:** `--ticks`, `--ephemeris`, `--ships`, `--stations`, `--max-signals`, observer/history flags, `--checkpoint-in <path>`, `--checkpoint-out <path>`.
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
- **Outputs:** Polar plot of detections around Luna, coloured by wavelength bin, with radial distance and logarithmic-scale options. Also renders a public market-channel panel with decoded `MarketMessage` payloads from Radio-band detections.
- **Side effects:** Connects to the gRPC server over the network; renders GUI.

## `MaritaStationAgent`

An autonomous AI client that operates one or more stations. The agent architecture separates concerns:

- **LLM adapter** proposes high-level actions (post WANT/HAVE, start production, expand solar array).
- **Deterministic tools** validate proposals and turn them into `StationCommand`s.
- **Marita engine** performs all bookkeeping (warehouses, production, signal propagation).

### Station-local API

- **Lifecycle:** alpha
- **`GetStationView(station_id)` input:** One station ID.
- **Output:** Only that station's local inventory, production, credits, reservations, own contracts, and market messages physically decoded by its receivers. It does not expose bodies, ships, signals, or other stations' private state.
- **`SubmitStationCommand(command)` input:** One command for the same station identity.
- **Output:** Acceptance or a deterministic rejection reason.
- **Side effects:** Queues a validated proposal for the next engine tick. Negotiation replies are rejected unless their referenced message was physically received by that station.

## Phase 1 trade settlement

- **Lifecycle:** alpha
- **Input:** A directed `ACCEPT` replying to a received `OFFER` or `COUNTER`.
- **Output:** An engine-owned trade contract with buyer, seller, material, quantity, escrow, and arrival tick.
- **Side effects:** Buyer credits enter escrow, seller goods are reserved, delivery delay is computed from physical distance at the MVP cargo speed, and goods/credits settle atomically on arrival. Failed delivery refunds escrow.

## `marita-station-agent`
- **Lifecycle:** alpha
- **Inputs:** `--addr`, required station selection via `--station-id`, optional `--ollama-url`, optional `--ollama-model`, `--no-llm`.
- **Outputs:** One proposed action per decision cycle through `SubmitStationCommand`.
- **Side effects:** Each process commands exactly one station and polls only `GetStationView`; optionally calls a local Ollama instance running Hermes 3 8B. The commander prompt discloses no simulation-wide state and never characterizes remote contacts as people or AIs.

### Dependencies
- `reqwest` (0.12): used only by the Hermes 3 Ollama adapter for local HTTP calls. Replaceable; the deterministic adapter has no external network dependency.
