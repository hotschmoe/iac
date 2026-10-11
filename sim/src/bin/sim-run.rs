// Headless fast runner: create a world at a pace, seat a mix of bots, step
// the real engine as fast as possible, print a balance report.
//
//   cargo run -p iac-sim --release --bin sim-run -- --pace season --days 7
//   cargo run -p iac-sim --release --bin sim-run -- \
//       --pace blitz --hours 1 --seat all:hands-on --seat idle --json
//
// The report goes to stdout; timing goes to stderr.

use std::fmt::Write as _;
use std::time::Instant;

use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::pace::Pace;
use iac_shared::world::home_seats;
use iac_sim::bot::{Attention, Bot, Personality, Style, DAY, DEFAULT_SESSIONS_PER_DAY, MAX_SESSIONS_PER_DAY};
use iac_sim::headless::{Headless, IdleWhy, PlayerSummary, ScoreBreakdown, SlotIdle, Why, WHY_LABELS};
use iac_sim::score::ExtraRule;
use iac_sim::script::IdlePlayer;
use serde_json::json;

const USAGE: &str = "usage: sim-run [--pace <preset|number>] [--seed <n>] [--seat <spec>]... [--sessions <n>]
               [--ticks <n> | --hours <n> | --days <n>] [--every <ticks>] [--ignore-cap] [--cap <rule>] [--json]
  --pace      world pace (default season)
  --seed      world seed (default: the server's default)
  --seat      who plays; repeatable. <personality>:<attention>[:<count>] or idle[:<count>]
                personality  turtle | miner | raider | explorer | mixed | all
                attention    check-in | hybrid | hands-on | all
              default: all:all and idle (every style once, and a player that never acts);
              a world has 24 home seats, so at most 24 players in all
  --sessions  look-ins per day of check-in and hybrid players (default 4, at most 24)
  --ticks/--hours/--days  simulated duration; a tick is one second (default --days 1)
  --every     ticks between score samples (default: a day; a tenth of a run under two days)
  --ignore-cap  bots keep flying for combat and exploration points past the score cap
                (measures what the map offers; by default they stop once the cap is banked)
  --cap       score the run, and have the bots play, under another rule for combat and
              exploration points than the game's (which is 0.25); the world itself is untouched
                <share>       counted up to that share of core, not beyond: 0.5
                soft:<share>  counted in full up to that share of core, beyond it by the
                              square root (four times the points count double)
                none          counted in full
  --json      print the full record as JSON instead of the tables";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Idle,
    Bot(Style),
}

impl Kind {
    fn style(self) -> String {
        match self {
            Kind::Idle => "idle".to_string(),
            Kind::Bot(style) => style.label(),
        }
    }
}

struct Args {
    pace: Pace,
    seed: u64,
    seats: Vec<Kind>,
    sessions: u64,
    ticks: u64,
    every: Option<u64>,
    json: bool,
    ignore_cap: bool,
    rule: ExtraRule,
}

fn parse_rule(text: &str) -> Result<ExtraRule, String> {
    let share = |text: &str| match text.parse::<f32>() {
        Ok(share) if share > 0.0 && share.is_finite() => Ok(share),
        _ => Err(format!("--cap: '{text}' is not a share of core above zero")),
    };
    match text.split_once(':') {
        None if text == "none" => Ok(ExtraRule::Uncapped),
        None => share(text).map(ExtraRule::Cap),
        Some(("soft", knee)) => share(knee).map(ExtraRule::Soft),
        Some(_) => Err(format!("--cap: '{text}' is not <share>, soft:<share> or none")),
    }
}

fn rule_text(rule: ExtraRule) -> String {
    match rule {
        ExtraRule::Cap(share) => format!("extra = min(combat + explore, {:.0}% of core)", 100.0 * share),
        ExtraRule::Soft(knee) => format!(
            "extra = combat + explore up to k = {:.0}% of core, beyond it sqrt(k * (combat + explore))",
            100.0 * knee
        ),
        ExtraRule::Uncapped => "extra = combat + explore".to_string(),
    }
}

fn parse_seat(spec: &str) -> Result<Vec<Kind>, String> {
    let (kind, count) = spec.split_once(':').unwrap_or((spec, "1"));
    if kind == "idle" {
        let count = count.parse::<usize>().map_err(|_| format!("--seat {spec}: '{count}' is not a count"))?;
        return Ok(vec![Kind::Idle; count]);
    }
    let seats = Style::parse_seats(spec).map_err(|e| format!("--seat {e} (or idle[:<count>])"))?;
    Ok(seats.into_iter().map(Kind::Bot).collect())
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        pace: Pace::parse("season")?,
        seed: DEFAULT_WORLD_SEED,
        seats: Vec::new(),
        sessions: DEFAULT_SESSIONS_PER_DAY,
        ticks: DAY,
        every: None,
        json: false,
        ignore_cap: false,
        rule: ExtraRule::GAME,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--help" | "-h" => return Err(String::new()),
            "--json" => {
                args.json = true;
                continue;
            }
            "--ignore-cap" => {
                args.ignore_cap = true;
                continue;
            }
            _ => {}
        }
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = |what: &str| value.parse::<u64>().map_err(|_| format!("{what}: '{value}' is not a whole number"));
        match flag.as_str() {
            "--pace" => args.pace = Pace::parse(&value)?,
            "--seed" => args.seed = number("--seed")?,
            "--seat" => args.seats.extend(parse_seat(&value)?),
            "--sessions" => args.sessions = number("--sessions")?,
            "--cap" => args.rule = parse_rule(&value)?,
            "--ticks" => args.ticks = number("--ticks")?,
            "--hours" => args.ticks = number("--hours")? * 3600,
            "--days" => args.ticks = number("--days")? * DAY,
            "--every" => args.every = Some(number("--every")?),
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    if !(1..=MAX_SESSIONS_PER_DAY).contains(&args.sessions) {
        return Err(format!("--sessions must be 1 to {MAX_SESSIONS_PER_DAY}"));
    }
    if args.seats.is_empty() {
        args.seats = parse_seat("all:all")?;
        args.seats.push(Kind::Idle);
    }
    let homes = home_seats().count();
    if args.seats.len() > homes {
        return Err(format!("--seat asks for {} players; a world has {homes} home seats", args.seats.len()));
    }
    Ok(args)
}

fn seat_all(world: &mut Headless, args: &Args) -> Result<(), String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for &kind in &args.seats {
        let style = kind.style();
        let number = match seen.iter_mut().find(|(s, _)| *s == style) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen.push((style.clone(), 0));
                0
            }
        };
        let name = format!("{}-{number}", style.replace('/', "-"));
        match kind {
            Kind::Idle => world.add_player(Box::new(IdlePlayer { label: name }), style)?,
            Kind::Bot(bot_style) => {
                let bot_style = Style { sessions_per_day: args.sessions, ignore_cap: args.ignore_cap, ..bot_style };
                world.add_player(Box::new(Bot::new(name, bot_style).under(args.rule)), style)?
            }
        };
    }
    Ok(())
}

fn when(tick: u64) -> String {
    if tick.is_multiple_of(DAY) {
        format!("day {}", tick / DAY)
    } else if tick >= DAY {
        format!("day {:.2}", tick as f64 / DAY as f64)
    } else {
        format!("{} min", tick / 60)
    }
}

fn score_columns(s: &ScoreBreakdown) -> String {
    format!(
        "{:>8.1} {:>7.1} {:>7.1} {:>7.1} {:>8.1} {:>8.1} {:>8.1} {:>8.1}",
        s.econ, s.fleet, s.defence, s.core, s.combat, s.explore, s.extra, s.total
    )
}

const SCORE_HEADER: &str = "    econ   fleet defence    core   combat  explore    extra    total";

struct StyleRow {
    style: String,
    players: usize,
    score: ScoreBreakdown,
    haul: f32,
    raid_salvage: f32,
    ships_lost: f32,
    refuel_deuterium: f32,
    idle: SlotIdle,
    why: IdleWhy<f32>,
    /// Share of what the mines made that a full store turned away.
    spilled: f32,
    fleet_hours: f32,
    unpaid_hours: f32,
}

fn whys(why: &mut IdleWhy<f32>) -> impl Iterator<Item = &mut f32> {
    [&mut why.building, &mut why.research, &mut why.shipyard].into_iter().flat_map(Why::each)
}

fn styles(players: &[PlayerSummary]) -> Vec<StyleRow> {
    let mut rows: Vec<StyleRow> = Vec::new();
    for p in players {
        let at = rows.iter().position(|r| r.style == p.style).unwrap_or_else(|| {
            rows.push(StyleRow {
                style: p.style.clone(),
                players: 0,
                score: ScoreBreakdown::default(),
                haul: 0.0,
                raid_salvage: 0.0,
                ships_lost: 0.0,
                refuel_deuterium: 0.0,
                idle: SlotIdle::default(),
                why: IdleWhy::default(),
                spilled: 0.0,
                fleet_hours: 0.0,
                unpaid_hours: 0.0,
            });
            rows.len() - 1
        });
        let row = &mut rows[at];
        row.players += 1;
        let (sum, s) = (&mut row.score, &p.score);
        sum.econ += s.econ;
        sum.fleet += s.fleet;
        sum.defence += s.defence;
        sum.core += s.core;
        sum.combat += s.combat;
        sum.explore += s.explore;
        sum.extra += s.extra;
        sum.total += s.total;
        row.haul += p.field.haul();
        row.raid_salvage += p.field.raid_salvage;
        row.ships_lost += p.field.ships_lost as f32;
        row.refuel_deuterium += p.field.refuel_deuterium;
        row.idle.building += p.idle.building;
        row.idle.research += p.idle.research;
        row.idle.shipyard += p.idle.shipyard;
        let mut why = p.idle_why.shares();
        for (sum, share) in whys(&mut row.why).zip(whys(&mut why)) {
            *sum += *share;
        }
        if p.mined > 0.0 {
            row.spilled += p.spilled / p.mined;
        }
        row.fleet_hours += p.field.fleet_hours;
        row.unpaid_hours += p.field.unpaid_hours;
    }
    for row in &mut rows {
        let n = row.players as f32;
        let s = &mut row.score;
        for value in [
            &mut s.econ, &mut s.fleet, &mut s.defence, &mut s.core, &mut s.combat, &mut s.explore, &mut s.extra,
            &mut s.total, &mut row.haul, &mut row.raid_salvage, &mut row.ships_lost, &mut row.refuel_deuterium,
            &mut row.idle.building, &mut row.idle.research, &mut row.idle.shipyard, &mut row.spilled,
            &mut row.fleet_hours, &mut row.unpaid_hours,
        ] {
            *value /= n;
        }
        for share in whys(&mut row.why) {
            *share /= n;
        }
    }
    rows
}

fn percent(value: f32, of: Option<f32>) -> String {
    match of {
        Some(of) if of > 0.0 => format!("{:>5.0}%", 100.0 * value / of),
        _ => "     -".to_string(),
    }
}

/// Final scores by style, with the two comparisons the balance targets ask
/// for: against the turtle with the same attention, and against the same
/// personality flown hands-on.
fn style_table(players: &[PlayerSummary]) -> String {
    let rows = styles(players);
    let total_of = |style: String| rows.iter().find(|r| r.style == style).map(|r| r.score.total);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<20} {:>2} {SCORE_HEADER} {:>7} {:>7} {:>6} {:>8} {:>7} {:>7} {:>6} {:>9} {:>11}",
        "style", "n", "haul", "raidsal", "lost", "refuel", "spilled", "field h", "unpaid", "vs turtle", "vs hands-on"
    );
    for row in &rows {
        let (personality, attention) = row.style.split_once('/').unwrap_or((row.style.as_str(), ""));
        let turtle = total_of(format!("{}/{attention}", Personality::Turtle.label()));
        let hands_on = total_of(format!("{personality}/{}", Attention::HandsOn.label()));
        let _ = writeln!(
            out,
            "{:<20} {:>2} {} {:>7.1} {:>7.1} {:>6.1} {:>8.0} {:>6.1}% {:>7.0} {:>6} {:>9} {:>11}",
            row.style,
            row.players,
            score_columns(&row.score),
            row.haul,
            row.raid_salvage,
            row.ships_lost,
            row.refuel_deuterium,
            100.0 * row.spilled,
            row.fleet_hours,
            percent(row.unpaid_hours, Some(row.fleet_hours)),
            percent(row.score.total, turtle),
            percent(row.score.total, hands_on),
        );
    }
    out
}

/// Why each kind of slot stood empty, by style: every reason as a percent of
/// the slot's idle time.
fn why_table(players: &[PlayerSummary]) -> String {
    let labels: String = WHY_LABELS.iter().map(|label| format!(" {label:>8}")).collect();
    let mut out = String::new();
    let _ = writeln!(out, "{:<20} {:<9} {:>5}{labels}", "style", "slot", "idle");
    for mut row in styles(players) {
        let slots = [
            ("building", row.idle.building, &mut row.why.building),
            ("lab", row.idle.research, &mut row.why.research),
            ("shipyard", row.idle.shipyard, &mut row.why.shipyard),
        ];
        for (slot, idle, why) in slots {
            let shares: String = why.each().into_iter().map(|share| format!(" {:>8.0}", 100.0 * *share)).collect();
            let style = if slot == "building" { row.style.as_str() } else { "" };
            let _ = writeln!(out, "{style:<20} {slot:<9} {:>4.0}%{shares}", 100.0 * idle);
        }
    }
    out
}

fn sample_table(players: &[PlayerSummary]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{:<22} {:>8} {SCORE_HEADER} {:>7} {:>6} {:>6}", "player", "at", "haul", "charts", "kills");
    for p in players {
        for s in &p.samples {
            let _ = writeln!(
                out,
                "{:<22} {:>8} {} {:>7.1} {:>6} {:>6}",
                p.name,
                when(s.tick),
                score_columns(&s.score),
                s.field.haul(),
                s.field.sectors_charted,
                s.field.kills,
            );
        }
    }
    out
}

#[expect(clippy::disallowed_methods, reason = "the runner measures its own wall-clock speed; the engine never reads the clock")]
fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("sim-run: {msg}");
            }
            eprintln!("{USAGE}");
            std::process::exit(if msg.is_empty() { 0 } else { 2 });
        }
    };

    let mut world = Headless::new(args.seed, args.pace);
    world.score_under(args.rule);
    if let Err(msg) = seat_all(&mut world, &args) {
        eprintln!("sim-run: {msg}");
        std::process::exit(2);
    }
    let every = args.every.unwrap_or(if args.ticks >= 2 * DAY { DAY } else { (args.ticks / 10).max(1) });

    let started = Instant::now();
    world.run_sampled(args.ticks, every).expect("simulation failed");
    let wall = started.elapsed().as_secs_f64();
    let tps = args.ticks as f64 / wall;

    let players = world.summaries();
    let hash = format!("{:016x}", world.engine.snapshot().hash());
    if args.json {
        let report = json!({
            "pace": args.pace.value(),
            "seed": args.seed,
            "ticks": args.ticks,
            "sample_every": every,
            "sessions_per_day": args.sessions,
            "score_rule": rule_text(args.rule),
            "snapshot_hash": hash,
            "players": players,
        });
        println!("{}", serde_json::to_string_pretty(&report).expect("report serializes"));
    } else {
        println!(
            "pace x{} seed {} ticks {} ({:.2} days) sessions/day {} snapshot {hash}",
            args.pace.value(),
            args.seed,
            args.ticks,
            args.ticks as f64 / DAY as f64,
            args.sessions,
        );
        println!("score = core + extra; core = econ + fleet + defence; {}", rule_text(args.rule));
        println!("haul and raidsal: resources from the field and from repelled raids, in the points they buy");
        println!("refuel: deuterium paid to fill fleet tanks");
        println!("spilled: share of what the mines made that a full store turned away");
        println!("field h: hours away from home, summed over fleets; unpaid: the part of them flown while");
        println!("  one more combat or exploration point would not have counted\n");
        println!("{}", sample_table(&players));
        println!("{}", style_table(&players));
        println!("idle: share of the run the slot stood empty (building slots counted one by one)");
        println!("why it stood empty, in percent of its idle time:");
        println!("  short     an order waits for resources (yard: the planner wants a unit the stockpile does not cover)");
        println!("  blocked   an order waits for a prerequisite, or the planner's next choice needs something not built");
        println!("  unbuilt   the lab or the yard is not built");
        println!("  unspent   nothing ordered, and the stockpile covers what the planner wants");
        println!("  poor      nothing ordered, and the stockpile does not cover what the planner wants");
        println!("  declined  nothing ordered: the planner's rule says not yet, though the stockpile covers a level");
        println!("  not yet   nothing ordered: the planner's rule says not yet, and the stockpile covers no level");
        println!("  done      nothing left to order\n");
        println!("{}", why_table(&players));
    }
    let turns: u64 = players.iter().map(|p| p.turns).sum();
    eprintln!(
        "{} players, {} ticks in {:.2} s: {:.0} ticks/s, {} bot turns",
        world.players(),
        args.ticks,
        wall,
        tps,
        turns,
    );
}
