// Where the server keeps its world: one file, and the thread that writes it.
//
// The file is the document of `iac_sim::store`, whole, and nothing else of a
// world is on disk. It is never edited: a new document is written beside it,
// synced, and renamed over it, so the file is always one whole document, the
// last or the one before. `<file>.lock` is held for as long as the server
// runs, so two servers never write one world.
//
// The tick copies the world out (`GameEngine::snapshot`) and hands the copy
// to the `Writer`; encoding and the disk are the writer thread's, so a slow
// disk delays documents and never a tick. What a server that is killed loses
// is what happened since the last document it wrote.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use log::{error, info, warn};
use serde::{Deserialize, Serialize};

use iac_sim::store::{self, Store};
use iac_sim::Snapshot;

/// What the server keeps of a world beside its snapshot: nothing. It reads
/// a document whatever another host kept there.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Beside {}

/// A world's file, held for this process alone.
pub struct Files {
    path: PathBuf,
    _lock: File,
}

impl Files {
    /// The place at `path`, or why this process may not have it.
    pub fn open(path: &Path) -> Result<Files, String> {
        let beside = sibling(path, "lock");
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&beside)
            .map_err(|e| format!("cannot open {}: {e}", beside.display()))?;
        match lock.try_lock() {
            Ok(()) => Ok(Files { path: path.to_path_buf(), _lock: lock }),
            Err(fs::TryLockError::WouldBlock) => Err(format!(
                "another server is running on {} (it holds {}); two servers cannot write one world",
                path.display(),
                beside.display(),
            )),
            Err(fs::TryLockError::Error(e)) => Err(format!("cannot lock {}: {e}", beside.display())),
        }
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

impl Store for Files {
    fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
        match fs::read(&self.path) {
            Ok(document) => Ok(Some(document)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read {}: {e}", self.path.display())),
        }
    }

    fn replace(&mut self, document: &[u8]) -> Result<(), String> {
        let next = sibling(&self.path, "tmp");
        let folder = match self.path.parent() {
            Some(folder) if !folder.as_os_str().is_empty() => folder,
            _ => Path::new("."),
        };
        let written = (|| {
            let mut file = File::create(&next)?;
            file.write_all(document)?;
            file.sync_all()?;
            fs::rename(&next, &self.path)?;
            File::open(folder)?.sync_all()
        })();
        written.map_err(|e| format!("cannot write {}: {e}", self.path.display()))
    }
}

enum Job {
    /// A world as copied out of the engine, and how long the copy took.
    Document(Box<Snapshot>, Duration),
    Settle(mpsc::SyncSender<Result<(), String>>),
}

const SLOW_WRITE: Duration = Duration::from_secs(1);

/// The thread that turns copies of the world into documents in a place.
/// Copies that queue up behind a slow write are not all written: only the
/// newest is, since a document replaces the one before.
pub struct Writer {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn spawn(place: impl Store + Send + 'static) -> Writer {
        let (jobs, queue) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("iac-store".into())
            .spawn(move || write_documents(place, queue))
            .expect("spawn the store thread");
        Writer { jobs: Some(jobs), thread: Some(thread) }
    }

    /// Queue a document of `world`. Never waits.
    pub fn hand(&self, world: Snapshot, copied: Duration) {
        let gone = self.jobs.as_ref().is_none_or(|jobs| jobs.send(Job::Document(Box::new(world), copied)).is_err());
        if gone {
            error!("the store thread is gone; the world is no longer being stored");
        }
    }

    /// Wait until everything handed over is in the place, and say whether
    /// the last document got there.
    pub fn settle(&self) -> Result<(), String> {
        let gone = || "the store thread is gone".to_string();
        let (done, outcome) = mpsc::sync_channel(1);
        self.jobs.as_ref().ok_or_else(gone)?.send(Job::Settle(done)).map_err(|_| gone())?;
        outcome.recv().map_err(|_| gone())?
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn write_documents(mut place: impl Store, queue: mpsc::Receiver<Job>) {
    let mut outcome = Ok(());
    while let Ok(first) = queue.recv() {
        let (mut newest, mut waiting, mut skipped) = (None, Vec::new(), 0usize);
        for job in std::iter::once(first).chain(queue.try_iter()) {
            match job {
                Job::Document(world, copied) => skipped += usize::from(newest.replace((world, copied)).is_some()),
                Job::Settle(done) => waiting.push(done),
            }
        }
        if let Some((world, copied)) = newest {
            let started = Instant::now();
            let document = store::encode(&Beside {}, &world);
            let encoded = started.elapsed();
            outcome = place.replace(&document);
            let written = started.elapsed() - encoded;
            match &outcome {
                Ok(()) => info!(
                    "World stored at tick {}: {} bytes, copied in {copied:?} on the tick, encoded in {encoded:?}, written in {written:?}",
                    world.tick,
                    document.len(),
                ),
                Err(e) => error!("The world at tick {} was not stored: {e}", world.tick),
            }
            if written > SLOW_WRITE {
                warn!("Slow disk: a document of {} bytes took {written:?} to write", document.len());
            }
            if skipped > 0 {
                warn!("{skipped} older copies of the world were not written: the disk is slower than the documents are due");
            }
        }
        for done in waiting {
            let _ = done.send(outcome.clone());
        }
    }
}
