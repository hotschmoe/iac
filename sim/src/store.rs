// A stored world: what every host keeps of a world between two runs, where
// it puts it, and when it owes it.
//
// What: one document. A header line of JSON, a newline, then the world's
// `Snapshot` as JSON:
//
//   {"format":7,"economy_version":8,"worldgen_version":3,"digest":<xxh3 of the body>,"host":{...}}
//   {"world_seed":...,"meta":{...},"tick":...,"players":{...},...}
//
// The header names what wrote the document, so one from another format,
// economy or world generator is refused before its world is parsed
// (`host::Stale`), and the digest catches one that was cut short or altered.
// `host` is what the host that wrote it keeps beside the world: the
// single-player host's commander and clock, nothing for the server. A host
// reads its own and need not know another's. The seed and the pace are the
// world's and stand in the body. Nothing else of a world is stored anywhere:
// what must survive a restart is a field of `Snapshot`.
//
// Where: a `Store`, which holds the document as bytes and knows nothing of
// what they mean. The server's is a file, a test's is memory, and in the
// browser the worker performs the same two verbs on IndexedDB, because
// WebAssembly cannot wait for it.
//
// When: `Due`. A document is the whole world as of one tick, and a host that
// stops without warning comes back as its last document says.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::host::Stale;
use crate::snapshot::Snapshot;

/// Bumped when the header or what the snapshot must hold changes.
pub const SAVE_FORMAT: u32 = 7;

#[derive(Debug, Deserialize)]
struct Stamp {
    format: u32,
    economy_version: u32,
    worldgen_version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Header<H> {
    format: u32,
    economy_version: u32,
    worldgen_version: u32,
    digest: u64,
    host: H,
}

/// Why a document is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// Another format, economy or world generator wrote it.
    Stale(Stale),
    /// It is not a whole document: what is wrong with it.
    Damaged(String),
}

impl Unread {
    /// The refusal, for a world kept as a `stored` ("save", "world file").
    /// The host adds what its player or operator can do about it.
    pub fn sentence(&self, stored: &str) -> String {
        match self {
            Unread::Stale(stale) => stale.sentence(stored),
            Unread::Damaged(how) => format!("this {stored} cannot be read: {how}"),
        }
    }
}

/// The document of `snapshot`, with what its host keeps beside it.
pub fn encode<H: Serialize>(host: &H, snapshot: &Snapshot) -> Vec<u8> {
    let body = serde_json::to_vec(snapshot).expect("a snapshot always serializes");
    let header = Header {
        format: SAVE_FORMAT,
        economy_version: snapshot.meta.economy_version,
        worldgen_version: snapshot.meta.worldgen_version,
        digest: xxh3_64(&body),
        host,
    };
    let mut document = serde_json::to_vec(&header).expect("a header always serializes");
    document.push(b'\n');
    document.extend(body);
    document
}

/// The world of a document and what its host kept beside it.
pub fn decode<H: DeserializeOwned>(document: &[u8]) -> Result<(H, Snapshot), Unread> {
    let damaged = Unread::Damaged;
    let split = document
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| damaged("it is not a world document".to_string()))?;
    let (head, body) = (&document[..split], &document[split + 1..]);
    let stamp: Stamp = serde_json::from_slice(head).map_err(|e| damaged(format!("it has no readable header ({e})")))?;
    Stale::check(stamp.format, stamp.economy_version, stamp.worldgen_version).map_err(Unread::Stale)?;
    let header: Header<H> = serde_json::from_slice(head)
        .map_err(|e| damaged(format!("its header does not hold what this host keeps beside a world ({e})")))?;
    if xxh3_64(body) != header.digest {
        return Err(damaged("its contents do not match its checksum".to_string()));
    }
    let snapshot = serde_json::from_slice(body).map_err(|e| damaged(format!("this build cannot read its world ({e})")))?;
    Ok((header.host, snapshot))
}

/// Where a world's document is kept. A place holds bytes: it never sees a
/// world, and nothing that plays a world sees a place.
pub trait Store {
    /// The document last put here; `None` for a place that holds no world.
    fn read(&mut self) -> Result<Option<Vec<u8>>, String>;

    /// Put `document` in place of the one held: afterwards the place holds
    /// all of it, and if that fails it holds the one it held before.
    fn replace(&mut self, document: &[u8]) -> Result<(), String>;
}

/// When a host owes its place a document. Times are milliseconds of
/// whatever clock the host has; this crate has none.
///
/// A document is owed a period after the last one. It is owed sooner once
/// the world has taken in something that no replay and no later document
/// brings back to whoever gave it, but never sooner than `soon` after the
/// last one, so that a burst of such things is one document and not many.
#[derive(Debug, Clone, Copy)]
pub struct Due {
    stored_at: u64,
    hurried: bool,
}

impl Due {
    /// A world whose last document was stored at `stored_at`.
    pub fn since(stored_at: u64) -> Due {
        Due { stored_at, hurried: false }
    }

    /// A world that has no document yet: one is owed at once.
    pub fn never() -> Due {
        Due { stored_at: 0, hurried: true }
    }

    /// The next document should not wait for the period.
    pub fn hurry(&mut self) {
        self.hurried = true;
    }

    /// A document as of `now` went to the place.
    pub fn stored(&mut self, now: u64) {
        *self = Due::since(now);
    }

    pub fn is_due(&self, now: u64, period: u64, soon: u64) -> bool {
        let since = now.saturating_sub(self.stored_at);
        since >= period || (self.hurried && since >= soon)
    }
}
