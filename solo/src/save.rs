// The save document: one header line of JSON, a newline, then the world's
// `Snapshot` as JSON. A host stores the bytes whole and never reads them.
//
// The header names what wrote it, so a save from another format, economy
// or world generator is refused before its snapshot is parsed, the way the
// server refuses a database from another version. The digest catches a
// document that was cut short or altered.

use iac_shared::constants::{ECONOMY_VERSION, WORLDGEN_VERSION};
use iac_shared::protocol::GameEvent;
use iac_sim::Snapshot;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::world::{CatchUp, Clock};

/// Bumped when the header or what the snapshot must hold changes.
pub const SAVE_FORMAT: u32 = 1;

#[derive(Debug, Deserialize)]
struct Stamp {
    format: u32,
    economy_version: u32,
    worldgen_version: u32,
}

/// What the host keeps of a world beside its snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    /// The player's account.
    pub commander: u64,
    pub clock: Clock,
    pub saved_at_ms: u64,
    /// The absence being caught up when the save was written.
    pub catch_up: Option<CatchUp>,
    /// What the player has yet to be told about the last absence.
    pub notices: Vec<GameEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    format: u32,
    economy_version: u32,
    worldgen_version: u32,
    digest: u64,
    #[serde(flatten)]
    bookmark: Bookmark,
}

pub fn encode(bookmark: Bookmark, snapshot: &Snapshot) -> Vec<u8> {
    let body = serde_json::to_vec(snapshot).expect("a snapshot always serializes");
    let header = Header {
        format: SAVE_FORMAT,
        economy_version: snapshot.meta.economy_version,
        worldgen_version: snapshot.meta.worldgen_version,
        digest: xxh3_64(&body),
        bookmark,
    };
    let mut document = serde_json::to_vec(&header).expect("a header always serializes");
    document.push(b'\n');
    document.extend(body);
    document
}

/// The bookmark and world of a save, or the sentence that says why this
/// build will not load it.
pub fn decode(document: &[u8]) -> Result<(Bookmark, Snapshot), String> {
    let split = document.iter().position(|&b| b == b'\n').ok_or("this save is not a save document")?;
    let (head, body) = (&document[..split], &document[split + 1..]);
    let stamp: Stamp = serde_json::from_slice(head).map_err(|e| format!("this save has no readable header ({e})"))?;
    if stamp.format != SAVE_FORMAT {
        return Err(format!(
            "this save is format {}, this build reads format {SAVE_FORMAT}; start a new game",
            stamp.format
        ));
    }
    if stamp.economy_version != ECONOMY_VERSION {
        return Err(format!(
            "this save is economy version {}, this build runs version {ECONOMY_VERSION}; start a new game",
            stamp.economy_version
        ));
    }
    if stamp.worldgen_version != WORLDGEN_VERSION {
        return Err(format!(
            "this world was generated with worldgen version {}, this build runs version {WORLDGEN_VERSION}; \
             start a new game",
            stamp.worldgen_version
        ));
    }
    let header: Header = serde_json::from_slice(head).map_err(|e| format!("this save has a damaged header ({e})"))?;
    if xxh3_64(body) != header.digest {
        return Err("this save is damaged: its contents do not match its checksum".to_string());
    }
    let snapshot: Snapshot =
        serde_json::from_slice(body).map_err(|e| format!("this build cannot read this save's world ({e})"))?;
    if !snapshot.players.contains_key(&header.bookmark.commander) {
        return Err("this save is damaged: its world does not hold its commander".to_string());
    }
    Ok((header.bookmark, snapshot))
}
