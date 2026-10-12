// iac-server: what a world needs to be played over a network and kept in a
// file, and nothing of the game, of hosting it (`iac_sim::host`) or of what
// a stored world is (`iac_sim::store`). The binary (`main.rs`) is the clock
// and the command line around these.

pub mod auth;
pub mod network;
pub mod store;
