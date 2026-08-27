//! gRPC service for the MaritaV3 simulation engine.

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/marita.rs"));
}

pub mod server;
pub mod service;
