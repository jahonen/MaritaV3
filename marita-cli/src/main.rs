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
        } => {
            let addr: std::net::SocketAddr = addr.parse()?;
            let mut state = SimulationState::new();
            state.bodies = load_ephemeris(&ephemeris);
            state.ships = spawn_play_ships(&state.bodies, ships);
            marita_grpc::server::run(addr, state).await?;
        }
        Commands::Scenario {
            ticks,
            ephemeris,
            ships,
        } => {
            println!("Running scenario for {ticks} ticks");
            let mut state = SimulationState::new();
            state.bodies = load_ephemeris(&ephemeris);
            state.ships = spawn_play_ships(&state.bodies, ships);
            let executor = marita_core::tick::TickExecutor::new();
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
        }
    }
    Ok(())
}

fn spawn_play_ships(
    bodies: &[marita_core::state::Body],
    count: usize,
) -> Vec<marita_core::state::Ship> {
    let earth = bodies.iter().find(|b| b.name == "Earth");
    let (base_pos, base_vel) = earth
        .map(|b| (b.position, b.velocity))
        .unwrap_or((DVec2::new(AU, 0.0), DVec2::new(0.0, 29_780.0)));

    let mut ships = Vec::new();
    for i in 0..count {
        let offset = DVec2::new(1.0e6 + i as f64 * 500.0, 0.0);
        let ship_pos = base_pos + offset;
        let orbital_v = base_vel + DVec2::new(0.0, 1_000.0 + i as f64 * 100.0);
        ships.push(default_ship(
            1000 + i as u64,
            &format!("play-ship-{i}"),
            ship_pos,
            orbital_v,
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
