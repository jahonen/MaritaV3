//! Local admin viewer for the MaritaV3 space simulation engine.
//!
//! Connects to a running `marita serve` instance over gRPC and renders a
//! god's-eye view of the solar system, ships, and signals.

mod app;
mod client;
mod render;
mod state;

use app::AdminApp;

#[derive(Debug, Clone)]
struct Args {
    addr: String,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            addr: "http://127.0.0.1:50051".into(),
        }
    }
}

fn main() -> eframe::Result {
    let args = parse_args();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 960.0])
            .with_title("MaritaV3 Admin Viewer"),
        ..Default::default()
    };

    eframe::run_native(
        "MaritaV3 Admin Viewer",
        options,
        Box::new(|cc| Ok(Box::new(AdminApp::new(cc, args.addr)))),
    )
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--addr" => {
                if let Some(v) = iter.next() {
                    args.addr = v;
                }
            }
            _ => {}
        }
    }
    args
}
