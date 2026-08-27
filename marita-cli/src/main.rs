//! CLI harness for running the MaritaV3 simulation engine locally.

use clap::{Parser, Subcommand};
use marita_core::ephemeris::EphemerisLoader;

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
    },
    /// Run a scripted local scenario.
    Scenario {
        /// Number of ticks to run.
        #[arg(long, default_value = "100")]
        ticks: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { addr } => {
            let addr: std::net::SocketAddr = addr.parse()?;
            let mut state = marita_core::state::SimulationState::new();
            state.bodies = marita_core::ephemeris::CircularOrbitLoader.load();
            marita_grpc::server::run(addr, state).await?;
        }
        Commands::Scenario { ticks } => {
            println!("Running scenario for {ticks} ticks");
            let mut state = marita_core::state::SimulationState::new();
            state.bodies = marita_core::ephemeris::CircularOrbitLoader.load();
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
