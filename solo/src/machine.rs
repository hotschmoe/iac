// The host's one door. A request is a line of JSON naming an operation,
// then a newline and a body where the operation takes one; the reply is a
// line of JSON, then a newline and its body:
//
//   request                           body in            body out
//   describe                          -                  -
//   new_game {name, pace, bots,       -                  -
//             entropy}
//   load                              save document      -
//   save                              -                  save document
//   abandon                           -                  -
//   open | close                      -                  -
//   frame                             one client frame   server frames
//   advance {max_ticks}               -                  server frames
//   take_command                      -                  server frames
//
// Every request carries `now_ms`, the wall clock where the host runs: this
// crate has none of its own. Frames are the wire protocol's JSON, one per
// line, exactly what a WebSocket to the server would carry. The reply line
// says how the world stands (`world`), what may be chosen for a new one
// (`options`, for `describe`), and what went wrong (`error`, `refused`).

use iac_shared::pace::{Pace, PRESETS};
use iac_shared::protocol::ServerMessage;
use iac_shared::world::home_seats;
use iac_sim::auth::{hash_token, token_text};
use iac_sim::bot::{Attention, Personality, Style};
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::world::{Status, World, CATCH_UP_LIMIT};

#[derive(Debug, Deserialize)]
struct Request {
    now_ms: u64,
    #[serde(flatten)]
    op: Op,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    Describe,
    NewGame(NewGame),
    Load,
    Save,
    Abandon,
    Open,
    Close,
    Frame,
    Advance { max_ticks: u64 },
    TakeCommand,
}

#[derive(Debug, Deserialize)]
struct NewGame {
    name: String,
    /// A preset name or a multiplier, as the server's `--pace`.
    pace: String,
    /// Seats as the server's `--bot`: `<personality>:<attention>[:<count>]`.
    bots: Vec<String>,
    /// Randomness from where the host runs: the world seed and the account
    /// tokens are drawn from it.
    entropy: String,
}

#[derive(Debug, Default, Serialize)]
struct Reply {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Why the save that was offered is not loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    refused: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Options>,
    world: Option<Status>,
}

#[derive(Debug, Serialize)]
struct Options {
    presets: Vec<PresetOption>,
    personalities: Vec<&'static str>,
    attentions: Vec<&'static str>,
    max_bots: usize,
    catch_up_limit: u64,
}

#[derive(Debug, Serialize)]
struct PresetOption {
    name: &'static str,
    pace: f64,
    first_cruiser_s: u64,
    endgame_s: u64,
}

#[derive(Default)]
pub struct Machine {
    world: Option<World>,
    refused: Option<String>,
}

impl Machine {
    pub fn call(&mut self, request: &[u8]) -> Vec<u8> {
        let (head, body) = match request.iter().position(|&b| b == b'\n') {
            Some(at) => (&request[..at], &request[at + 1..]),
            None => (request, &[][..]),
        };
        let mut out = Vec::new();
        let mut reply = Reply::default();
        match serde_json::from_slice::<Request>(head) {
            Ok(request) => {
                if let Err(error) = self.run(request.op, request.now_ms, body, &mut reply, &mut out) {
                    reply.error = Some(error);
                }
                reply.refused = self.refused.clone();
                reply.world = self.world.as_ref().map(|world| world.status(request.now_ms));
            }
            Err(e) => reply.error = Some(format!("unreadable request: {e}")),
        }
        let mut bytes = serde_json::to_vec(&reply).expect("a reply always serializes");
        bytes.push(b'\n');
        bytes.extend(out);
        bytes
    }

    fn run(&mut self, op: Op, now_ms: u64, body: &[u8], reply: &mut Reply, out: &mut Vec<u8>) -> Result<(), String> {
        let mut frames: Vec<ServerMessage> = Vec::new();
        match op {
            Op::Describe => reply.options = Some(options()),
            Op::NewGame(config) => {
                self.world = Some(new_game(&config, now_ms)?);
                self.refused = None;
            }
            Op::Load => match World::load(body) {
                Ok(world) => {
                    self.world = Some(world);
                    self.refused = None;
                }
                Err(refusal) => {
                    self.world = None;
                    self.refused = Some(refusal);
                }
            },
            Op::Save => out.extend(self.world()?.save(now_ms)),
            Op::Abandon => {
                self.world = None;
                self.refused = None;
            }
            Op::Open => self.world()?.open(),
            Op::Close => self.world()?.close(),
            Op::Frame => self.world()?.receive(body, &mut frames)?,
            Op::Advance { max_ticks } => self.world()?.advance(now_ms, max_ticks, &mut frames)?,
            Op::TakeCommand => self.world()?.take_command(now_ms, &mut frames),
        }
        for frame in &frames {
            serde_json::to_writer(&mut *out, frame).expect("a server message always serializes");
            out.push(b'\n');
        }
        Ok(())
    }

    fn world(&mut self) -> Result<&mut World, String> {
        self.world.as_mut().ok_or_else(|| "no world is loaded".to_string())
    }
}

fn options() -> Options {
    Options {
        presets: PRESETS
            .iter()
            .map(|p| PresetOption {
                name: p.name,
                pace: p.pace,
                first_cruiser_s: p.first_cruiser_s,
                endgame_s: p.endgame_s,
            })
            .collect(),
        personalities: Personality::ALL.iter().map(|p| p.label()).collect(),
        attentions: Attention::ALL.iter().map(|a| a.label()).collect(),
        max_bots: home_seats().count() - 1,
        catch_up_limit: CATCH_UP_LIMIT,
    }
}

fn new_game(config: &NewGame, now_ms: u64) -> Result<World, String> {
    let pace = Pace::parse(&config.pace)?;
    let mut bots = Vec::new();
    for spec in &config.bots {
        bots.extend(Style::parse_seats(spec)?);
    }
    if config.entropy.len() < 32 {
        return Err("a new game needs at least 32 characters of entropy".to_string());
    }
    let seed = xxh3_64(config.entropy.as_bytes());
    let mut minted = 0u32;
    let mint = || {
        minted += 1;
        token_text(&hash_token(&format!("{}/{minted}", config.entropy)))
    };
    World::create(seed, pace, config.name.trim(), &bots, now_ms, mint)
}
