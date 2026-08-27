//! CLI harness for running the MaritaV3 simulation engine locally.

use clap::{Parser, Subcommand};
use glam::DVec2;
use marita_core::ephemeris::{CircularOrbitLoader, EphemerisLoader, JsonFileLoader};
use marita_core::state::{default_ship, SimulationState};
use marita_core::units::AU;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "marita")]
#[command(about = "MaritaV3 space simulation engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the gRPC server.
    Serve {
        /// Bind address.
        #[arg(long, default_value = "127.0.0.1:50051")]
        addr: String,

        /// Ephemeris source: `circular` or path to a JSON snapshot.
        #[arg(long, default_value = "circular")]
        ephemeris: String,

        /// Number of default play-test ships to spawn near Earth.
        #[arg(long, default_value = "1")]
        ships: usize,

        /// Maximum number of signal arcs to keep in memory.
        #[arg(long, default_value = "50000")]
        max_signals: usize,

        /// Load an initial state from a checkpoint file instead of ephemeris.
        #[arg(long, value_name = "PATH")]
        checkpoint_in: Option<PathBuf>,
    },
    /// Run a scripted local scenario.
    Scenario {
        /// Number of ticks to run.
        #[arg(long, default_value = "100")]
        ticks: u64,

        /// Ephemeris source: `circular` or path to a JSON snapshot.
        #[arg(long, default_value = "circular")]
        ephemeris: String,

        /// Number of default play-test ships to spawn near Earth.
        #[arg(long, default_value = "1")]
        ships: usize,

        /// Maximum number of signal arcs to keep in memory.
        #[arg(long, default_value = "50000")]
        max_signals: usize,

        /// Load an initial state from a checkpoint file instead of ephemeris.
        #[arg(long, value_name = "PATH")]
        checkpoint_in: Option<PathBuf>,

        /// Save the final state to a checkpoint file after running.
        #[arg(long, value_name = "PATH")]
        checkpoint_out: Option<PathBuf>,
    },
    /// Benchmark a scripted local scenario and report performance.
    Benchmark {
        /// Number of ticks to run.
        #[arg(long, default_value = "100")]
        ticks: u64,

        /// Ephemeris source: `circular` or path to a JSON snapshot.
        #[arg(long, default_value = "circular")]
        ephemeris: String,

        /// Number of default play-test ships to spawn near Earth.
        #[arg(long, default_value = "1")]
        ships: usize,

        /// Maximum number of signal arcs to keep in memory.
        #[arg(long, default_value = "50000")]
        max_signals: usize,

        /// Load an initial state from a checkpoint file instead of ephemeris.
        #[arg(long, value_name = "PATH")]
        checkpoint_in: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve {
            addr,
            ephemeris,
            ships,
            max_signals,
            checkpoint_in,
        } => {
            let addr: std::net::SocketAddr = addr.parse()?;
            let state = if let Some(path) = checkpoint_in {
                println!("Loading checkpoint from {}", path.display());
                SimulationState::load(path).map_err(|e| anyhow::anyhow!(e))?
            } else {
                let mut s = SimulationState::new();
                s.bodies = load_ephemeris(&ephemeris);
                s.ships = spawn_play_ships(&s.bodies, ships);
                s
            };
            marita_grpc::server::run(addr, state, max_signals).await?;
        }
        Commands::Scenario {
            ticks,
            ephemeris,
            ships,
            max_signals,
            checkpoint_in,
            checkpoint_out,
        } => {
            println!("Running scenario for {ticks} ticks");
            let mut state = if let Some(path) = checkpoint_in {
                println!("Loading checkpoint from {}", path.display());
                SimulationState::load(path).map_err(|e| anyhow::anyhow!(e))?
            } else {
                let mut s = SimulationState::new();
                s.bodies = load_ephemeris(&ephemeris);
                s.ships = spawn_play_ships(&s.bodies, ships);
                s
            };
            let executor = marita_core::tick::TickExecutor::new().with_max_signals(max_signals);
            for _ in 0..ticks {
                let output = executor.step(&mut state, &[]);
                println!(
                    "tick={} time={:.1}s ships={} signals={}",
                    output.state.tick,
                    output.state.sim_time,
                    output.state.ships.len(),
                    output.state.signals.len()
                );
            }
            if let Some(path) = checkpoint_out {
                println!("Saving checkpoint to {}", path.display());
                state.save(path).map_err(|e| anyhow::anyhow!(e))?;
            }
        }
        Commands::Benchmark {
            ticks,
            ephemeris,
            ships,
            max_signals,
            checkpoint_in,
        } => {
            println!("Benchmarking {ticks} ticks");
            let mut state = if let Some(path) = checkpoint_in {
                println!("Loading checkpoint from {}", path.display());
                SimulationState::load(path).map_err(|e| anyhow::anyhow!(e))?
            } else {
                let mut s = SimulationState::new();
                s.bodies = load_ephemeris(&ephemeris);
                s.ships = spawn_play_ships(&s.bodies, ships);
                s
            };
            let executor = marita_core::tick::TickExecutor::new().with_max_signals(max_signals);
            let start = std::time::Instant::now();
            for _ in 0..ticks {
                executor.step(&mut state, &[]);
            }
            let elapsed = start.elapsed();
            let elapsed_secs = elapsed.as_secs_f64();
            let ticks_per_sec = ticks as f64 / elapsed_secs;
            println!(
                "Finished {ticks} ticks in {:.3}s ({:.2} ticks/s, {:.3} ms/tick)",
                elapsed_secs,
                ticks_per_sec,
                elapsed_secs * 1000.0 / ticks as f64
            );
            println!(
                "Final state: bodies={} ships={} signals={}",
                state.bodies.len(),
                state.ships.len(),
                state.signals.len()
            );
        }
    }
    Ok(())
}

fn spawn_play_ships(
    bodies: &[marita_core::state::Body],
    count: usize,
) -> Vec<marita_core::state::Ship> {
    let earth = bodies.iter().find(|b| b.name == "Earth");
    let (base_pos, base_vel, earth_mass) = earth
        .map(|b| (b.position, b.velocity, b.mass))
        .unwrap_or((DVec2::new(AU, 0.0), DVec2::new(0.0, 29_780.0), 5.9723e24));

    // Place ships in a low Earth orbit (altitude ~ 630 km, r = 7.0e6 m).
    let orbital_radius = 7.0e6;
    let orbital_speed =
        (marita_core::units::GRAVITATIONAL_CONSTANT * earth_mass / orbital_radius).sqrt();

    let mut ships = Vec::new();
    for i in 0..count {
        // Spread ships around Earth at evenly spaced true anomalies.
        let angle = (i as f64 / count.max(1) as f64) * 2.0 * std::f64::consts::PI;
        let offset = DVec2::new(angle.cos(), angle.sin()) * orbital_radius;
        let ship_pos = base_pos + offset;
        // Velocity is Earth's heliocentric velocity plus tangential LEO velocity.
        let tangent = DVec2::new(-angle.sin(), angle.cos());
        let ship_vel = base_vel + tangent * orbital_speed;
        ships.push(default_ship(
            1000 + i as u64,
            &format!("play-ship-{i}"),
            ship_pos,
            ship_vel,
        ));
    }
    ships
}

fn load_ephemeris(spec: &str) -> Vec<marita_core::state::Body> {
    if spec.eq_ignore_ascii_case("circular") {
        CircularOrbitLoader.load()
    } else {
        let path = PathBuf::from(spec);
        if !path.exists() {
            eprintln!(
                "ephemeris file {} not found; falling back to circular orbits",
                path.display()
            );
            return CircularOrbitLoader.load();
        }
        JsonFileLoader::new(&path).load()
    }
}
