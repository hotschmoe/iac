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

use iac_shared::constants::{ShipClass, DEFAULT_WORLD_SEED, RAID_ROUNDS};
use iac_shared::pace::Pace;
use iac_shared::world::home_seats;
use iac_sim::bot::{Attention, Bot, Personality, Style, DAY, DEFAULT_SESSIONS_PER_DAY, MAX_SESSIONS_PER_DAY};
use iac_sim::headless::{
    FieldTally, Headless, IdleWhy, OutpostOrder, PlayerSummary, ScoreBreakdown, SlotIdle, Why, WHY_LABELS,
};
use iac_sim::score::ExtraRule;
use iac_sim::script::IdlePlayer;
use serde_json::json;

const USAGE: &str = "usage: sim-run [--pace <preset|number>] [--seed <n>] [--seat <spec>]... [--sessions <n>]
               [--ticks <n> | --hours <n> | --days <n>] [--every <ticks>] [--ignore-cap] [--cap <rule>]
               [--meter <share>]... [--outpost <ring>:<grade>:<level>[:<repairs>]] [--json]
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
  --meter     also report what a daily allowance of <share> of core would have counted;
              repeatable. A meter fills with the points earned and drains by one allowance
              a day; a point counts in full while it stands under one allowance, by the
              square root beyond. The bots do not play to it: pair it with --cap none
  --outpost   stand an outpost of <level> (1 to 3) for every bot at the start of day 3, on the
              sector of <ring> (9 to 20) whose ore is nearest <grade> (0 to 12) among those free,
              three hexes from the next. Nothing is founded or paid, and no bot supplies it,
              defends it or hauls from it: it measures the extractor, the raids and the store.
                <level>+   the next level is under construction from that tick
                <repairs>  how many repairs its store is stocked for (default 1)
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
    meters: Vec<f32>,
    outpost: Option<OutpostOrder>,
}

/// The tick `--outpost` stands its outposts: the start of day 3.
const OUTPOST_TICK: u64 = 2 * DAY;

fn parse_outpost(text: &str) -> Result<OutpostOrder, String> {
    let wrong = || format!("--outpost: '{text}' is not <ring>:<grade>:<level>[:<repairs>]");
    let mut parts = text.split(':');
    let (Some(ring), Some(grade), Some(level)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(wrong());
    };
    let (level, building) = match level.strip_suffix('+') {
        Some(level) => (level, true),
        None => (level, false),
    };
    let repairs = match (parts.next(), parts.next()) {
        (None, _) => 1,
        (Some(repairs), None) => repairs.parse().map_err(|_| wrong())?,
        _ => return Err(wrong()),
    };
    let grade = grade.parse::<f32>().ok().filter(|grade| grade.is_finite()).ok_or_else(wrong)?;
    Ok(OutpostOrder {
        ring: ring.parse().map_err(|_| wrong())?,
        grade,
        level: level.parse().map_err(|_| wrong())?,
        building,
        repairs,
    })
}

fn parse_share(flag: &str, text: &str) -> Result<f32, String> {
    match text.parse::<f32>() {
        Ok(share) if share > 0.0 && share.is_finite() => Ok(share),
        _ => Err(format!("{flag}: '{text}' is not a share of core above zero")),
    }
}

fn parse_rule(text: &str) -> Result<ExtraRule, String> {
    match text.split_once(':') {
        None if text == "none" => Ok(ExtraRule::Uncapped),
        None => parse_share("--cap", text).map(ExtraRule::Cap),
        Some(("soft", knee)) => parse_share("--cap", knee).map(ExtraRule::Soft),
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
        meters: Vec::new(),
        outpost: None,
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
            "--meter" => args.meters.push(parse_share("--meter", &value)?),
            "--outpost" => args.outpost = Some(parse_outpost(&value)?),
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
    if args.outpost.is_some() && args.ticks <= OUTPOST_TICK {
        return Err("--outpost stands its outposts at the start of day 3; the run ends before that".to_string());
    }
    Ok(args)
}

/// Seat everyone asked for; returns the player ids of the bots.
fn seat_all(world: &mut Headless, args: &Args) -> Result<Vec<u64>, String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut bots = Vec::new();
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
            Kind::Idle => {
                world.add_player(Box::new(IdlePlayer { label: name }), style)?;
            }
            Kind::Bot(bot_style) => {
                let bot_style = Style { sessions_per_day: args.sessions, ignore_cap: args.ignore_cap, ..bot_style };
                bots.push(world.add_player(Box::new(Bot::new(name, bot_style).under(args.rule)), style)?);
            }
        }
    }
    Ok(bots)
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
    /// Fights left by a jump under fire.
    fled: f32,
    refuel_deuterium: f64,
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
                fled: 0.0,
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
        sum.outpost += s.outpost;
        sum.core += s.core;
        sum.combat += s.combat;
        sum.explore += s.explore;
        sum.extra += s.extra;
        sum.total += s.total;
        row.haul += p.field.haul();
        row.raid_salvage += p.field.raid_salvage;
        row.ships_lost += p.field.ships_lost as f32;
        row.fled += p.field.fights_fled as f32;
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
            &mut s.econ, &mut s.fleet, &mut s.defence, &mut s.outpost, &mut s.core, &mut s.combat, &mut s.explore,
            &mut s.extra, &mut s.total, &mut row.haul, &mut row.raid_salvage, &mut row.ships_lost, &mut row.fled,
            &mut row.idle.building, &mut row.idle.research, &mut row.idle.shipyard, &mut row.spilled,
            &mut row.fleet_hours, &mut row.unpaid_hours,
        ] {
            *value /= n;
        }
        row.refuel_deuterium /= f64::from(n);
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

/// How each player's raids on the homeworld ended and what they cost.
fn raid_table(players: &[PlayerSummary]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<22} {:>5} {:>9} {:>9} {:>4} {:>9} {:>6} {:>6} {:>6} {:>7} {:>8} {:>7} {:>8} {:>8} {:>8}",
        "raids", "all", "destroyed", "outlasted", "lost", "unopposed", "rounds", "ships", "sats", "rebuilt", "defences",
        "rebuilt", "cost", "taken", "wreckage",
    );
    let mut sum = FieldTally::default();
    let mut line = |name: &str, f: &FieldTally| {
        let _ = writeln!(
            out,
            "{:<22} {:>5} {:>9} {:>9} {:>4} {:>9} {:>6} {:>6} {:>6} {:>7} {:>8} {:>7} {:>8.1} {:>8.1} {:>8.1}",
            name,
            f.raids_repelled + f.raids_lost,
            f.raids_destroyed,
            f.raids_repelled - f.raids_destroyed,
            f.raids_lost,
            f.raids_unopposed,
            f.raid_rounds,
            f.raid_ships_lost,
            f.raid_satellites_lost,
            f.raid_satellites_rebuilt,
            f.raid_defences_lost,
            f.raid_defences_rebuilt,
            f.raid_losses,
            f.raided,
            f.raid_salvage,
        );
    };
    for p in players {
        let f = &p.field;
        line(&p.name, f);
        sum.raids_repelled += f.raids_repelled;
        sum.raids_destroyed += f.raids_destroyed;
        sum.raids_lost += f.raids_lost;
        sum.raids_unopposed += f.raids_unopposed;
        sum.raid_rounds += f.raid_rounds;
        sum.raid_ships_lost += f.raid_ships_lost;
        sum.raid_satellites_lost += f.raid_satellites_lost;
        sum.raid_satellites_rebuilt += f.raid_satellites_rebuilt;
        sum.raid_defences_lost += f.raid_defences_lost;
        sum.raid_defences_rebuilt += f.raid_defences_rebuilt;
        sum.raid_losses += f.raid_losses;
        sum.raided += f.raided;
        sum.raid_salvage += f.raid_salvage;
    }
    line("all", &sum);
    out
}

/// Ships each player lost, by class, in the field and in raids, with the
/// fuel its tanks took at the dock and from holds.
fn loss_table(players: &[PlayerSummary]) -> String {
    let classes: Vec<ShipClass> =
        ShipClass::ALL.into_iter().filter(|c| players.iter().any(|p| p.field.lost_by_class.contains_key(c))).collect();
    let mut out = String::new();
    let _ = write!(out, "{:<22} {:>9} {:>9}", "lost", "refuel", "refined");
    for class in &classes {
        let name: String = class.label().chars().filter(|c| *c != ' ').take(9).collect();
        let _ = write!(out, " {name:>9}");
    }
    let _ = writeln!(out);
    for p in players {
        let f = &p.field;
        let _ = write!(out, "{:<22} {:>9.0} {:>9.0}", p.name, f.refuel_deuterium, f.refined_deuterium);
        for class in &classes {
            let _ = write!(out, " {:>9}", f.lost_by_class.get(class).copied().unwrap_or(0));
        }
        let _ = writeln!(out);
    }
    out
}

/// What each player's outpost scored, made and lost to raids; None when no
/// player has had one, which is every world bots play alone.
fn outpost_table(players: &[PlayerSummary]) -> Option<String> {
    let touched = |p: &&PlayerSummary| p.outpost.is_some() || p.field.outpost_ticks > 0 || p.field.outpost_hauled > 0.0;
    let rows: Vec<&PlayerSummary> = players.iter().filter(touched).collect();
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<22} {:>9} {:>5} {:>3} {:>6} {:>7} {:>7} {:>6} {:>9} {:>8} {:>8} {:>8} {:>8} {:>7} {:>6} {:>8} {:>4} {:>4} {:>8} {:>7} {:>6}  state",
        "outposts", "at", "grade", "lvl", "points", "rate/h", "home/h", "share", "potential", "yield", "spilled",
        "raided", "repairs", "hauled", "kept", "repelled", "lost", "open", "breached", "waiting", "build",
    );
    for p in rows {
        let f = &p.field;
        let stood = f.outpost_ticks.max(1) as f64;
        let part = |ticks: u64| 100.0 * ticks as f64 / stood;
        let site = match &p.outpost {
            Some(o) => format!(
                "{:>9} {:>5.1} {:>3} {:>6.2} {:>7.0} {:>7.0} {:>5.1}%",
                o.sector.to_string(),
                o.grade,
                o.level,
                p.score.outpost,
                o.rate_per_hour,
                o.home_per_hour,
                100.0 * o.rate_per_hour / o.home_per_hour.max(1.0),
            ),
            None => format!("{:>9} {:>5} {:>3} {:>6} {:>7} {:>7} {:>6}", "gone", "-", "-", "-", "-", "-", "-"),
        };
        let state = match &p.outpost {
            Some(o) if o.waiting => "breached, waiting for materials",
            Some(o) if o.breached => "breached",
            Some(_) => "in service",
            None => "",
        };
        let _ = writeln!(
            out,
            "{:<22} {site} {:>9.1} {:>8.1} {:>8.1} {:>8.1} {:>8.1} {:>7.1} {:>6} {:>8} {:>4} {:>4} {:>7.1}% {:>6.1}% {:>5.1}%  {state}",
            p.name,
            f.outpost_potential,
            f.outpost_yield,
            f.outpost_spilled,
            f.outpost_raided,
            f.outpost_repairs,
            f.outpost_hauled,
            percent(f.outpost_yield - f.outpost_raided - f.outpost_repairs, Some(f.outpost_potential)),
            f.outpost_raids_repelled,
            f.outpost_raids_lost,
            f.outpost_raids_on_breach,
            part(f.outpost_ticks_breached),
            part(f.outpost_ticks_waiting),
            part(f.outpost_ticks_behind_build),
        );
    }
    Some(out)
}

/// The outposts `--outpost` stood, as they were on that tick: where, and
/// what level 1 there pays back in against its owner's next Metal Mine level.
fn stood_table(stood: &[PlayerSummary]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<22} {:>9} {:>4} {:>11} {:>5} {:>9} {:>9} {:>11} {:>12}",
        "stood", "at", "ring", "ore m/c/d", "grade", "payback h", "next mine", "its payback", "against mine"
    );
    for p in stood {
        let Some(o) = &p.outpost else { continue };
        let [metal, crystal, deuterium] = o.ore.map(|density| density.harvest_multiplier());
        let _ = writeln!(
            out,
            "{:<22} {:>9} {:>4} {:>11} {:>5.1} {:>9.0} {:>9} {:>11.0} {:>12}",
            p.name,
            o.sector.to_string(),
            o.ring,
            format!("{metal}/{crystal}/{deuterium}"),
            o.grade,
            o.payback_hours,
            o.next_mine_level,
            o.mine_payback_hours,
            percent(o.payback_hours, Some(o.mine_payback_hours)),
        );
    }
    out
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
        "{:<20} {:>2} {SCORE_HEADER} {:>7} {:>7} {:>6} {:>6} {:>8} {:>7} {:>7} {:>6} {:>9} {:>11}",
        "style", "n", "haul", "raidsal", "lost", "fled", "refuel", "spilled", "field h", "unpaid", "vs turtle",
        "vs hands-on"
    );
    for row in &rows {
        let (personality, attention) = row.style.split_once('/').unwrap_or((row.style.as_str(), ""));
        let turtle = total_of(format!("{}/{attention}", Personality::Turtle.label()));
        let hands_on = total_of(format!("{personality}/{}", Attention::HandsOn.label()));
        let _ = writeln!(
            out,
            "{:<20} {:>2} {} {:>7.1} {:>7.1} {:>6.1} {:>6.1} {:>8.0} {:>6.1}% {:>7.0} {:>6} {:>9} {:>11}",
            row.style,
            row.players,
            score_columns(&row.score),
            row.haul,
            row.raid_salvage,
            row.ships_lost,
            row.fled,
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

/// The run by style as meter `at` would have scored it.
fn meter_table(players: &[PlayerSummary], at: usize) -> String {
    struct Row {
        style: String,
        players: f32,
        core: f32,
        bonus: f32,
        fleet_hours: f32,
        unpaid_hours: f32,
    }
    let mut rows: Vec<Row> = Vec::new();
    for p in players {
        let Some(meter) = p.meters.get(at) else { continue };
        let row = match rows.iter_mut().find(|r| r.style == p.style) {
            Some(row) => row,
            None => {
                rows.push(Row { style: p.style.clone(), players: 0.0, core: 0.0, bonus: 0.0, fleet_hours: 0.0, unpaid_hours: 0.0 });
                rows.last_mut().expect("a row was pushed")
            }
        };
        row.players += 1.0;
        row.core += p.score.core;
        row.bonus += meter.bonus;
        row.fleet_hours += p.field.fleet_hours;
        row.unpaid_hours += meter.unpaid_hours;
    }
    let total_of = |style: String| rows.iter().find(|r| r.style == style).map(|r| (r.core + r.bonus) / r.players);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<20} {:>8} {:>8} {:>8} {:>6} {:>9} {:>11}",
        "style", "core", "bonus", "total", "unpaid", "vs turtle", "vs hands-on"
    );
    for row in &rows {
        let (personality, attention) = row.style.split_once('/').unwrap_or((row.style.as_str(), ""));
        let total = (row.core + row.bonus) / row.players;
        let _ = writeln!(
            out,
            "{:<20} {:>8.1} {:>8.1} {:>8.1} {:>6} {:>9} {:>11}",
            row.style,
            row.core / row.players,
            row.bonus / row.players,
            total,
            percent(row.unpaid_hours, Some(row.fleet_hours)),
            percent(total, total_of(format!("{}/{attention}", Personality::Turtle.label()))),
            percent(total, total_of(format!("{personality}/{}", Attention::HandsOn.label()))),
        );
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
    for &share in &args.meters {
        world.meter(share);
    }
    let bots = seat_all(&mut world, &args).unwrap_or_else(|msg| {
        eprintln!("sim-run: {msg}");
        std::process::exit(2);
    });
    let every = args.every.unwrap_or(if args.ticks >= 2 * DAY { DAY } else { (args.ticks / 10).max(1) });

    let started = Instant::now();
    let mut stood = Vec::new();
    if let Some(order) = args.outpost {
        world.run_sampled(OUTPOST_TICK, every).expect("simulation failed");
        if let Err(msg) = world.install_outposts(&bots, order) {
            eprintln!("sim-run: --outpost: {msg}");
            std::process::exit(2);
        }
        stood = world.summaries().into_iter().filter(|p| p.outpost.is_some()).collect();
    }
    world.run_sampled(args.ticks - world.engine.current_tick(), every).expect("simulation failed");
    let wall = started.elapsed().as_secs_f64();
    let tps = args.ticks as f64 / wall;

    let players = world.summaries();
    let hash = format!("{:016x}", world.engine.snapshot().hash());
    if args.json {
        let stood: Vec<_> = stood.iter().map(|p| json!({ "name": p.name, "outpost": p.outpost })).collect();
        let report = json!({
            "pace": args.pace.value(),
            "seed": args.seed,
            "ticks": args.ticks,
            "sample_every": every,
            "sessions_per_day": args.sessions,
            "score_rule": rule_text(args.rule),
            "snapshot_hash": hash,
            "outposts_stood": stood,
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
        println!("raids on the homeworld, each fought as a battle of at most {RAID_ROUNDS} rounds:");
        println!("  destroyed: no raider was left. outlasted: a defender stood when the rounds were over. lost: no defender");
        println!("    was left; unopposed is the part of lost that found nothing in its way and was not fought");
        println!("  rounds: rounds fought, the outpost's raids included. ships: ships of fleets destroyed in raids, at either dock");
        println!("  sats, defences: Solar Satellites and defences destroyed. rebuilt, after each: those of them rebuilt at once");
        println!("  cost: the ships, satellites and defences that did not come back, at the weight of what they cost");
        println!("  taken: what lost raids took from the stockpile. wreckage: what was scooped at home. Both in the points they buy\n");
        println!("{}", raid_table(&players));
        println!("ships lost by class, in fights and raids; refuel: deuterium paid at the dock for fuel; refined: deuterium");
        println!("  turned into fuel from holds\n");
        println!("{}", loss_table(&players));
        if let Some(order) = args.outpost {
            println!(
                "--outpost: at the start of day 3 every bot was given an outpost of level {}{} in ring {}, on the free",
                order.level,
                if order.building { format!(" with level {} under construction", order.level + 1) } else { String::new() },
                order.ring,
            );
            println!(
                "  site nearest grade {}, its store stocked for {} repair{}. Nobody founded, supplied, defended or hauled.",
                order.grade,
                order.repairs,
                if order.repairs == 1 { "" } else { "s" },
            );
            println!("  ore m/c/d: what its metal, crystal and deuterium ore count for; they sum to the grade");
            println!("  payback h: game hours at pace 1 in which level 1 there makes its cost, uninterrupted; next mine and");
            println!("  its payback: the same for the Metal Mine level its owner's homeworld would build next that day\n");
            println!("{}", stood_table(&stood));
        }
        if let Some(table) = outpost_table(&players) {
            println!("outposts: core also counts the finished levels of a player's outpost (points)");
            println!("  rate/h, home/h, share: units an hour its extractor makes in service, its owner's mines make, and the first");
            println!("    as a share of the second, all as the run ended");
            println!("  potential, yield, spilled, raided, repairs, hauled: in the points they buy. Potential is the rate of every");
            println!("    tick it stood; yield what reached the store; spilled what a full store turned away; raided what lost");
            println!("    raids took; repairs what it paid for them; hauled what load took from the store");
            println!("  kept: yield less raided and repairs, as a share of potential");
            println!("  repelled, lost, open: raids on it; open is the part of lost that found it breached already");
            println!("  breached: share of the time it stood. waiting: the part with no repair running because the store could");
            println!("    not pay one. build: the part with no repair running because a level was building\n");
            println!("{table}");
        }
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
        for (at, share) in args.meters.iter().enumerate() {
            println!(
                "a daily allowance of {:.0}% of core: bonus is what the points counted, total is core + bonus\n",
                100.0 * share
            );
            println!("{}", meter_table(&players, at));
        }
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
