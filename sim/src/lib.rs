// iac-sim: the game rules and world state, with no I/O, no async and no wall
// clock. Time enters only as ticks; every random draw is seeded from the world
// seed, the tick or ids. Hosts (the server, a headless runner, a WebAssembly
// page) feed commands in, call `tick`, and read state out.

pub mod auth;
pub mod combat;
pub mod commands;
pub mod engine;
pub mod headless;
pub mod intel;
pub mod persist;
pub mod queue;
pub mod score;
pub mod script;
pub mod snapshot;
pub mod views;

pub use engine::GameEngine;
pub use persist::{Persist, PersistBatch};
pub use snapshot::Snapshot;
