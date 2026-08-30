# MaritaV3 Components

This document lists every modular unit of functionality in the MaritaV3 space simulation engine.

## Core Physics Components

### `SimulationState`
- **Purpose:** Owns the entire simulation world: bodies, ships, signals, and the clock. Can be serialized to/from JSON checkpoints.
- **Inputs:** Initial ephemeris, ship definitions, tick commands; or a checkpoint file.
- **Outputs:** Updated state after each tick, sensor detections; JSON checkpoint files.
- **Side effects:** Reads/writes checkpoint files when asked.
- **Lifecycle:** alpha

### `Gravity`
- **Purpose:** Computes gravitational accelerations using a top-3-influencer approximation.
- **Inputs:** List of massive bodies and ships; positions and masses.
- **Outputs:** Acceleration vectors for each mass.
- **Side effects:** None.
- **Lifecycle:** alpha

### `SignalArc`
- **Purpose:** Represents an expanding arc-segment signal carrying a per-wavelength information spectrum.
- **Inputs:** Origin, direction, angular width, spectrum, degradation rates.
- **Outputs:** Expanded/clipped arcs after propagation; detectable energy at sensor locations.
- **Side effects:** Absorbed energy feeds into thermal state.
- **Lifecycle:** alpha

### `Propulsion`
- **Purpose:** Applies rocket-equation thrust and torque to rigid-body ships.
- **Inputs:** Ship state, engine configuration, throttle/gimbal commands.
- **Outputs:** Force and torque on the ship; updated fuel and mass.
- **Side effects:** None.
- **Lifecycle:** alpha

### `SensorArray`
- **Purpose:** Detects and characterizes signals at the ship location.
- **Inputs:** Ship state, sensor configuration, all signal arcs, other ship jamming.
- **Outputs:** List of detections with bearing, wavelength bin, and strength.
- **Side effects:** None.
- **Lifecycle:** alpha

### `ThermalState`
- **Purpose:** Tracks heat capacity, temperature, and blackbody emission.
- **Inputs:** Absorbed radiation, internal heat generation, surface area, emissivity.
- **Outputs:** Updated temperature and emitted IR signal arcs.
- **Side effects:** Adds new signal arcs each tick.
- **Lifecycle:** alpha

### `CollisionResolver`
- **Purpose:** Detects and resolves mass–mass contacts.
- **Inputs:** Body and ship positions, radii, collision response rules.
- **Outputs:** Updated velocities or merged bodies.
- **Side effects:** May remove/merge entities.
- **Lifecycle:** alpha

### `CircularOrbitLoader`
- **Purpose:** Fallback loader providing simplified circular heliocentric orbits for development and tests.
- **Inputs:** None (constants).
- **Outputs:** `Vec<Body>` for Sun + eight planets.
- **Side effects:** None.
- **Lifecycle:** alpha

### `JsonFileLoader`
- **Purpose:** Loads an ephemeris snapshot generated from local SPICE kernels by `scripts/generate_ephemeris.py` or from JPL Horizons by `scripts/generate_ephemeris_horizons.py`.
- **Inputs:** Path to JSON snapshot file.
- **Outputs:** `Vec<Body>` projected onto the ecliptic plane.
- **Side effects:** Reads file.
- **Lifecycle:** alpha

### `SpatialIndex`
- **Purpose:** Legacy uniform-grid spatial index for dynamic 2D entities. Replaced by `Quadtree` for signal, sensor, and collision queries.
- **Inputs:** Entity positions and radii; a query center/radius or bounding box.
- **Outputs:** Candidate entity indices for exact geometric tests.
- **Side effects:** None.
- **Lifecycle:** deprecated

### `Quadtree`
- **Purpose:** Adaptive 2D spatial index for points (bodies, ships) and regions (signal arcs). Rebuilt deterministically each tick so the hot paths scale with local density instead of total entity count.
- **Inputs:** Entity AABBs, capacity threshold, max depth.
- **Outputs:** Candidate indices for point, circle, and region queries.
- **Side effects:** None.
- **Lifecycle:** alpha

### `EphemerisLoader` (trait)
- **Purpose:** Common interface for initial-condition loaders.
- **Inputs:** Implementation-specific.
- **Outputs:** `Vec<Body>` at simulation epoch.
- **Side effects:** Depends on implementation.
- **Lifecycle:** alpha

### `TickExecutor`
- **Purpose:** Orchestrates one full simulation step.
- **Inputs:** Current `SimulationState`, commands for the tick.
- **Outputs:** Next `SimulationState`.
- **Side effects:** None.
- **Lifecycle:** alpha

### `AmbientField`
- **Purpose:** Legacy continuous radiation field and current-local heating input. It remains available while the causal observer prototype is evaluated.
- **Inputs:** Current `Body` and `Ship` slices.
- **Outputs:** Current-time irradiance and absorbed energy; legacy sensor sources when selected.
- **Side effects:** None.
- **Lifecycle:** deprecated

### `RadiativeProfileCatalog`
- **Purpose:** Validates the bundled JSON catalog of temperatures, emissivity, per-band albedo, internal heat, and natural omniband luminosity.
- **Inputs:** Embedded `body-radiative-profiles.json` and body name.
- **Outputs:** A validated body profile or conservative default profile.
- **Side effects:** Initializes body thermal and radiative properties during ephemeris loading.
- **Lifecycle:** alpha

### `ObservationHistory`
- **Purpose:** Engine-private bounded state history supporting strict retarded-time observations over up to 100 AU.
- **Inputs:** Finalized body, ship, and station state once per tick plus range/memory configuration.
- **Outputs:** Interpolated historical observation samples.
- **Side effects:** Retains a memory-bounded ring; history is intentionally omitted from JSON checkpoints and warms after resume.
- **Lifecycle:** alpha

### `PassiveRadiationObserver`
- **Purpose:** Backward-evaluates direct Planck-spectrum thermal/natural emission and one-bounce Lambertian sunlight using historical source, reflector, and occluder state.
- **Inputs:** Observer pose/time, sensors, body catalog, observation history, active arcs, and candidate cap.
- **Outputs:** Causally delayed per-band detections.
- **Side effects:** None; missing history suppresses a detection rather than substituting current state.
- **Lifecycle:** alpha

### `AnonymousContact`
- **Purpose:** Converts private source associations into observer-scoped contact handles with deterministic per-window bearing/range error.
- **Inputs:** Private source key, observer scope, integration window, and sensor resolution.
- **Outputs:** Stable anonymous contact ID, uncertain measurements, and emission tick.
- **Side effects:** None.
- **Lifecycle:** alpha

## Station Economy Components

### `Station`
- **Purpose:** Industrial site anchored to a celestial body or to an L4/L5 Lagrange point of a two-body system. Owns solar collectors, warehouses, production lines, market posters, trade credits, and reserved inventory.
- **Inputs:** Initial parent body, surface offset (or Lagrange point), tech tier, seed composition; `StationCommand` messages.
- **Outputs:** Processed materials, market broadcast arcs, updated warehouse balances.
- **Side effects:** Adds `SignalArc` market payloads to the simulation when posters are active; recomputes Lagrange offsets each tick.
- **Lifecycle:** alpha

### `TradeContract`
- **Purpose:** Authoritative Phase 1 agreement and cargo-delay record formed from an accepted directed offer.
- **Inputs:** Valid OFFER/COUNTER and ACCEPT message chain, seller inventory, buyer credits, station distance.
- **Outputs:** Contract status and arrival tick exposed only to the two counterparties.
- **Side effects:** Reserves seller inventory, escrows buyer credits, then transfers goods and credits atomically after deterministic transit.
- **Lifecycle:** alpha

### `MaterialLibrary`
- **Purpose:** Defines materials across four complexity tiers and their base values. Provides body-specific surface-composition tables used to seed station inventories.
- **Inputs:** Material/reaction identifiers.
- **Outputs:** Static `MaterialInfo`, `Reaction` definitions, default body composition maps.
- **Side effects:** None.
- **Lifecycle:** alpha

### `SynthesisPlanner` (deterministic tools)
- **Purpose:** Validates LLM proposals and converts them into engine-safe `StationCommand`s. Enforces known material/reaction IDs and non-negative quantities.
- **Inputs:** A `ProposedAction` from an LLM adapter.
- **Outputs:** An optional `StationCommand` for the engine.
- **Side effects:** None.
- **Lifecycle:** alpha

### `StationEconomy`
- **Purpose:** Runs station bookkeeping each tick: solar collection, production-line progress, market-poster expiry, and automatic WANT/HAVE generation when no AI agent is connected.
- **Inputs:** Current `SimulationState` and `StationCommand`s.
- **Outputs:** Updated station warehouses and active market posters; new market-broadcast `SignalArc`s.
- **Side effects:** Modifies station state; emits broadcast arcs.
- **Lifecycle:** alpha

## Admin/Observer Viewer Components

### `AdminApp`
- **Purpose:** Local egui/eframe gods-eye visualization of a running engine.
- **Inputs:** gRPC tick stream from `MaritaEngine`.
- **Outputs:** Rendered 2D view of bodies, ships, signals; user selection and view controls.
- **Side effects:** Network connection to gRPC server; GUI rendering.
- **Lifecycle:** alpha

### `Viewport`
- **Purpose:** Maps simulation world coordinates (meters) to screen pixels with pan/zoom.
- **Inputs:** World positions, zoom level, screen size.
- **Outputs:** Screen positions and visible sizes that handle AU-to-meter scale ranges.
- **Side effects:** None.
- **Lifecycle:** alpha

### `LunaApp`
- **Purpose:** Observer client showing Luna's infosphere without absolute remote state. Maintains anonymous tracks, band filters, retarded-age labels, and decoded Radio market traffic.
- **Inputs:** `StreamLunaView` gRPC detections with anonymous contact and uncertainty fields.
- **Outputs:** Persistent polar track plot and scrolling public market-channel panel.
- **Side effects:** Network connection to gRPC server; GUI rendering.
- **Lifecycle:** beta
