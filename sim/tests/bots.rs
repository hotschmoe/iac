// Built-in bots through the same seam a host uses: a headless world seats
// them, they see `GameState` and send orders, the engine does the rest.

use iac_shared::pace::Pace;
use iac_shared::protocol::{is_bot_name, Command, ServerMessage};
use iac_sim::bot::{Attention, Bot, Personality, Roster, Style, DAY};
use iac_sim::engine::AuthError;
use iac_sim::headless::{Headless, PlayerSummary};
use iac_sim::script::IdlePlayer;
use iac_sim::{views, GameEngine, Snapshot};

const TICKS: u64 = 3_000;

/// Every personality flown hands-on, the explorer and the turtle also as
/// hybrid and check-in players, with look-ins every 40 minutes so several
/// fall inside a run.
fn styles() -> Vec<Style> {
    let quick = |personality, attention| Style { sessions_per_day: 24, ..Style::new(personality, attention) };
    let mut styles: Vec<Style> = Personality::ALL.into_iter().map(|p| quick(p, Attention::HandsOn)).collect();
    for attention in [Attention::Hybrid, Attention::CheckIn] {
        styles.extend([quick(Personality::Explorer, attention), quick(Personality::Turtle, attention)]);
    }
    styles
}

/// A blitz world with those bots and one player who never acts.
fn world(seed: u64) -> Headless {
    let mut world = Headless::new(seed, Pace::parse("blitz").unwrap());
    for style in styles() {
        let name = style.label().replace('/', "-");
        world.add_player(Box::new(Bot::new(name, style)), style.label()).expect("seat bot");
    }
    world.add_player(Box::new(IdlePlayer { label: "idle".to_string() }), "idle").expect("seat idle");
    world
}

/// The same bots as accounts of the world, played by a roster as a server
/// plays them, beside one ordinary account.
fn hosted(seed: u64) -> (GameEngine, Roster, u64) {
    let mut engine = GameEngine::new(seed, Pace::parse("blitz").unwrap());
    let human = engine.authenticate("Human", None, false, || "h".repeat(64)).expect("register").player_id;
    let mut roster = Roster::default();
    let (added, refusal) = roster.enlist(&mut engine, &styles(), 0, || "b".repeat(64));
    assert_eq!((added, refusal), (styles().len(), None));
    (engine, roster, human)
}

fn host_tick(engine: &mut GameEngine, roster: &mut Roster) {
    roster.take_turns(engine);
    engine.tick().unwrap();
    roster.observe(engine);
    engine.drain_events();
    engine.persist_dirty_state();
}

fn hash(world: &Headless) -> u64 {
    world.engine.snapshot().hash()
}

fn player<'a>(players: &'a [PlayerSummary], style: &str) -> &'a PlayerSummary {
    players.iter().find(|p| p.style == style).unwrap_or_else(|| panic!("no player of style {style}"))
}

#[test]
fn a_world_of_bots_replays_from_its_seed() {
    let (mut a, mut b) = (world(7), world(7));
    for _ in 0..3 {
        a.run(TICKS / 3).unwrap();
        b.run(TICKS / 3).unwrap();
        assert_eq!(hash(&a), hash(&b), "bot worlds diverged by tick {}", a.engine.current_tick());
    }
    let turns = |w: &Headless| w.summaries().iter().map(|p| (p.turns, p.orders_accepted)).collect::<Vec<_>>();
    assert_eq!(turns(&a), turns(&b));

    let mut other = world(8);
    other.run(TICKS / 3).unwrap();
    let mut again = world(7);
    again.run(TICKS / 3).unwrap();
    assert_ne!(hash(&other), hash(&again), "a different seed must give a different world");
}

#[test]
fn a_check_in_miner_brings_ore_home_under_standing_orders_within_its_fuel_budget() {
    let mut world = Headless::new(7, Pace::parse("season").unwrap());
    let style = Style::new(Personality::Miner, Attention::CheckIn);
    world.add_player(Box::new(Bot::new("miner-check-in", style)), style.label()).expect("seat bot");
    world.run(DAY).unwrap();

    let miner = &world.summaries()[0];
    assert!(miner.turns < DAY / 20, "it looked in a few times: {} turns", miner.turns);
    assert!(miner.field.ore > 1.0, "a day of mine_and_return brought home {:.2} points of ore", miner.field.ore);
    let fuel = iac_shared::scaling::resource_weight(&iac_shared::constants::Resources {
        deuterium: miner.field.refuel_deuterium,
        ..Default::default()
    });
    assert!(fuel < miner.field.ore, "{fuel:.2} points of deuterium burned for {:.2} of ore", miner.field.ore);
}

#[test]
fn a_check_in_builder_orders_only_the_ships_and_turrets_its_stockpile_covers() {
    let mut world = Headless::new(7, Pace::parse("season").unwrap());
    let style = Style::new(Personality::Turtle, Attention::CheckIn);
    world.add_player(Box::new(Bot::new("turtle-check-in", style)), style.label()).expect("seat bot");
    world.run(2 * DAY).unwrap();

    let turtle = &world.summaries()[0];
    assert!(turtle.turns < 2 * DAY / 20, "it looked in a few times: {} turns", turtle.turns);
    assert_eq!(turtle.orders_refused, 0, "a batch is paid when it is ordered, so it sends none it cannot pay for");
    assert!(turtle.score.defence > 0.0 && turtle.ships > 2, "it did order turrets and hulls: {:?}, {} ships", turtle.score, turtle.ships);
    assert!(turtle.buildings["Metal Mine"] >= 6, "{:?}", turtle.buildings);
}

#[test]
fn a_pilot_pays_for_no_batch_whose_tech_or_building_still_waits_for_resources() {
    let mut world = Headless::new(1, Pace::parse("season").unwrap());
    let style = Style::new(Personality::Raider, Attention::HandsOn);
    let id = world.add_player(Box::new(Bot::new("raider-hands-on", style)), style.label()).expect("seat bot");
    let mut yard_busy = 0;
    for _ in 0..DAY / 2 {
        world.step().unwrap();
        let player = &world.engine.players[&id];
        yard_busy += u64::from(player.ship_queue.is_some());
        if player.ship_queue.is_none() && !player.ship_pending.is_empty() {
            assert!(
                player.research_queue.is_some() || !player.building_queue.is_empty(),
                "tick {}: {:?} is paid for and waits on something that has not started",
                world.engine.current_tick(),
                player.ship_pending[0].item,
            );
        }
    }
    assert!(yard_busy > 0, "the raider did order ships and turrets");
}

#[test]
fn bots_build_fly_and_delegate_with_orders_the_engine_accepts() {
    let mut world = world(7);
    world.run_sampled(TICKS, TICKS / 3).unwrap();
    let players = world.summaries();
    let idle = player(&players, "idle");
    assert_eq!((idle.turns, idle.orders_accepted), (0, 0));

    for p in players.iter().filter(|p| p.style != "idle") {
        assert!(p.score.econ > 20.0 * idle.score.econ, "{} built nothing: econ {}", p.name, p.score.econ);
        assert!(
            p.orders_refused * 50 <= p.orders_accepted,
            "{} sends orders the engine refuses: {} of {}",
            p.name,
            p.orders_refused,
            p.orders_accepted + p.orders_refused,
        );
        assert_eq!(p.samples.len(), 3, "{} has a sample per interval", p.name);
    }

    let pilot = player(&players, "explorer/hands-on");
    assert!(pilot.field.sectors_charted > 20, "the pilot delivered {} charts", pilot.field.sectors_charted);
    assert!(pilot.score.explore > 0.0 && pilot.score.extra > 0.0);

    let turtle = player(&players, "turtle/hands-on");
    assert_eq!((turtle.field.sectors_charted, turtle.field.kills), (0, 0), "a turtle stays home");
    assert!(turtle.score.defence > 0.0, "a turtle builds defences");

    let absentee = player(&players, "explorer/check-in");
    assert!(
        absentee.turns > 0 && absentee.turns < TICKS / 2 && absentee.turns < pilot.turns,
        "a check-in player is away most of the time: {} turns against the pilot's {}",
        absentee.turns,
        pilot.turns,
    );
    assert!(
        absentee.field.sectors_charted > 0,
        "standing orders chart for a check-in explorer while it is away",
    );
    assert!(
        world.engine.fleets.values().filter(|f| f.owner_id == pilot.player_id).all(|f| !world.engine.policies.contains_key(&f.id)),
        "a hands-on pilot uses no standing orders",
    );
}

#[test]
fn the_view_a_bot_folds_from_tick_updates_is_the_full_state() {
    let (mut engine, mut roster, _human) = hosted(7);
    let (mut compared, mut marked) = (0, 0);
    for _ in 0..1_500 {
        host_tick(&mut engine, &mut roster);
        for seat in roster.seats() {
            let Some(view) = seat.view() else { continue };
            let full = views::full_state(&engine, seat.player_id()).expect("the bot's account");
            let (folded, full) = (serde_json::to_value(view).unwrap(), serde_json::to_value(&full).unwrap());
            if folded != full {
                let differs = |field: &str| folded[field] != full[field];
                let sector = folded["known_sectors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .zip(full["known_sectors"].as_array().into_iter().flatten())
                    .find(|(a, b)| a != b);
                panic!(
                    "tick {}: {} sees a different world (player {}, fleets {}, homeworld {}, sectors {}): {sector:?}",
                    engine.current_tick(),
                    seat.player().style().label(),
                    differs("player"),
                    differs("fleets"),
                    differs("homeworld"),
                    differs("known_sectors"),
                );
            }
            compared += 1;
            marked = marked.max(view.known_sectors.iter().filter(|s| s.chart.is_some()).count());
        }
    }
    assert!(compared > 2_500, "bots acting tick after tick were followed: {compared} views compared");
    assert!(marked > 50, "charts were made and marked: {marked} in the fullest view compared");
}

#[test]
fn a_world_of_bots_resumed_from_a_snapshot_plays_on_as_if_never_stopped() {
    let (mut straight, mut roster, _) = hosted(7);
    let (mut paused, mut paused_roster, _) = hosted(7);
    for _ in 0..TICKS / 2 {
        host_tick(&mut straight, &mut roster);
        host_tick(&mut paused, &mut paused_roster);
    }
    let json = serde_json::to_string(&paused.snapshot()).unwrap();
    let snapshot: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot.bots.len(), styles().len(), "bot accounts and what they remember are part of the world");
    let mut resumed = GameEngine::restore(snapshot);
    let mut resumed_roster = Roster::resume(&resumed);
    assert_eq!(resumed_roster.seats().len(), styles().len());
    let (added, refusal) = resumed_roster.enlist(&mut resumed, &styles(), 0, || "b".repeat(64));
    assert_eq!((added, refusal), (0, None), "the bots the world already has are the ones asked for");

    for leg in 0..3 {
        for _ in 0..TICKS / 6 {
            host_tick(&mut straight, &mut roster);
            host_tick(&mut resumed, &mut resumed_roster);
        }
        assert_eq!(
            straight.snapshot().hash(),
            resumed.snapshot().hash(),
            "the resumed world diverged by tick {} (leg {leg})",
            straight.current_tick(),
        );
    }
    let turns = |roster: &Roster| roster.seats().iter().map(|s| s.turns()).sum::<u64>();
    assert!(turns(&resumed_roster) > 1_000, "the resumed bots kept playing");
}

#[test]
fn bots_carry_their_tag_in_their_name_and_stand_apart_on_the_board() {
    let (mut engine, mut roster, human) = hosted(7);
    for _ in 0..600 {
        host_tick(&mut engine, &mut roster);
    }
    let bot_names: Vec<String> =
        roster.seats().iter().map(|seat| engine.players[&seat.player_id()].name.clone()).collect();
    assert!(bot_names.iter().all(|name| is_bot_name(name)), "{bot_names:?}");

    for taken in ["BOT-Wannabe", "bot-wannabe", bot_names[0].as_str()] {
        let refusal = engine.authenticate(taken, None, false, || "x".repeat(64)).err();
        assert!(
            matches!(refusal, Some(AuthError::InvalidName(_) | AuthError::TokenRequired)),
            "{taken} must not be open to a client: {refusal:?}",
        );
    }

    let board = |engine: &mut GameEngine, viewer: u64, limit: u16| match engine.execute(viewer, Command::Leaderboard { limit }).reply {
        Some(ServerMessage::Leaderboard(board)) => board,
        other => panic!("no leaderboard: {other:?}"),
    };
    let seen = board(&mut engine, human, 50);
    assert_eq!(seen.entries.len(), 1, "only the human is ranked: {:?}", seen.entries);
    assert_eq!((seen.entries[0].name.as_str(), seen.entries[0].rank), ("Human", 1));
    assert_eq!(seen.bots.len(), bot_names.len());
    assert!(seen.bots.iter().all(|row| bot_names.contains(&row.name) && !row.agent));
    assert!(seen.bots.windows(2).all(|pair| pair[0].score >= pair[1].score && pair[0].rank + 1 == pair[1].rank));
    assert!(seen.bots[0].score > seen.entries[0].score, "a bot that plays outscores an idle human and still takes no rank");

    let last_name = seen.bots.last().expect("bots on the board").name.clone();
    let last = roster.seats().iter().map(|seat| seat.player_id()).find(|id| engine.players[id].name == last_name);
    let short = board(&mut engine, last.expect("the last bot's account"), 1);
    assert_eq!(short.bots.len(), 1);
    assert_eq!(short.you.map(|row| row.rank), Some(bot_names.len() as u32), "a bot finds its own row");
}
