// iac-server: what a world needs to be played over a network and kept on
// disk, and nothing of the game or of hosting it (`iac_sim::host`). The
// binary (`main.rs`) is the clock and the command line around these.

pub mod auth;
pub mod database;
pub mod network;
