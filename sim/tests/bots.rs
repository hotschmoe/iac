// Built-in bots through the same seam a host uses: a headless world seats
// them, they see `GameState` and send orders, the engine does the rest.

use iac_shared::pace::Pace;
use iac_sim::bot::{Attention, Bot, Personality};
use iac_sim::headless::{Headless, PlayerSummary};
use iac_sim::script::IdlePlayer;

const TICKS: u64 = 3_000;

/// A blitz world with every personality flown hands-on, the explorer and the
/// turtle also as hybrid and check-in players (look-ins every 40 minutes so
/// several fall inside the run), and one player who never acts.
fn world(seed: u64) -> Headless {
    let mut world = Headless::new(seed, Pace::parse("blitz").unwrap());
    let mut seat = |personality: Personality, attention: Attention| {
        let style = format!("{}/{}", personality.label(), attention.label());
        let name = style.replace('/', "-");
        let bot = Bot::new(name, personality, attention).sessions_per_day(24);
        world.add_player(Box::new(bot), style).expect("seat bot");
    };
    for personality in Personality::ALL {
        seat(personality, Attention::HandsOn);
    }
    for attention in [Attention::Hybrid, Attention::CheckIn] {
        seat(Personality::Explorer, attention);
        seat(Personality::Turtle, attention);
    }
    world.add_player(Box::new(IdlePlayer { label: "idle".to_string() }), "idle").expect("seat idle");
    world
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
