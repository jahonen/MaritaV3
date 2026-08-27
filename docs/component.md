# MaritaV3 Components

This document lists every modular unit of functionality in the MaritaV3 space simulation engine.

## Core Physics Components

### `SimulationState`
- **Purpose:** Owns the entire simulation world: bodies, ships, signals, and the clock.
- **Inputs:** Initial ephemeris, ship definitions, tick commands.
- **Outputs:** Updated state after each tick, sensor detections.
- **Side effects:** None (pure in-memory structure).
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

### `EphemerisLoader`
- **Purpose:** Loads initial state vectors for solar-system bodies from SPICE kernels.
- **Inputs:** Local SPICE kernel files or fallback JSON snapshot.
- **Outputs:** `Vec<Body>` at simulation epoch.
- **Side effects:** Reads files.
- **Lifecycle:** alpha

### `TickExecutor`
- **Purpose:** Orchestrates one full simulation step.
- **Inputs:** Current `SimulationState`, commands for the tick.
- **Outputs:** Next `SimulationState`.
- **Side effects:** None.
- **Lifecycle:** alpha
