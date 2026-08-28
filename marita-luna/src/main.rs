//! Luna station observer client for the MaritaV3 space simulation engine.
//!
//! Connects to a running `marita serve` instance over gRPC and visualises the
//! infosphere from Luna's perspective using only bearing/distance detections,
//! never absolute server coordinates.

mod app;
mod client;

use app::LunaApp;

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
            .with_inner_size([960.0, 960.0])
            .with_title("MaritaV3 Luna Station"),
        ..Default::default()
    };

    eframe::run_native(
        "MaritaV3 Luna Station",
        options,
        Box::new(|cc| Ok(Box::new(LunaApp::new(cc, args.addr)))),
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
