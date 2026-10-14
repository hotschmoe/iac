"""Tests for the play CLI's parsers and formatters. Run: python3 -m unittest discover -s tools/playtest/tests"""
import copy, importlib.util, io, json, pathlib, unittest
from contextlib import redirect_stdout

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("play", HERE.parent / "play.py")
play = importlib.util.module_from_spec(spec)
spec.loader.exec_module(play)

FS = json.loads((HERE / "fixtures" / "full_state.json").read_text())


def fresh():
    return copy.deepcopy(FS)


def scan(tick, fleet=2, sector=(0, 0), revealed=7, hostiles=1, signals=(), threats=()):
    return {"tick": tick, "kind": "ScanCompleted", "fleet_id": fleet, "sector": {"q": sector[0], "r": sector[1]},
            "sectors_revealed": revealed, "hostiles_detected": hostiles,
            "signals": [{"sector": {"q": q, "r": r}, "signal": k, "threat_band": b} for q, r, k, b in signals],
            "threats": [{"sector": {"q": 0, "r": 0}, "threat": {"rating": t, "est_power": 10, "basis": "estimate"}} for t in threats]}


def round_(tick, mine=True, took=5, dealt=7, sector=(1, 1)):
    side = lambda hull, destroyed: {"groups": [], "volleys": [], "shots": 3, "power": 30, "absorbed": 1,
                                    "hull_damage": hull, "destroyed": destroyed}
    return {"tick": tick, "kind": "BattleRound", "sector": {"q": sector[0], "r": sector[1]}, "round": 1,
            "fleets": side(dealt, 1), "hostiles": side(took, 0), "mine": mine}


def tick_msg(*events):
    return {"type": "tick_update", "tick": 1, "events": list(events)}


class Events(unittest.TestCase):
    def test_scans_collapse_to_one_line_per_fleet(self):
        msgs = [tick_msg(scan(10, signals=[(3, 4, "HostileMass", 2)], threats=[3, 5]), scan(11, revealed=5, hostiles=0,
                                                                                         signals=[(3, 4, "HostileMass", 3)]))]
        lines = play.collapse_events(msgs)
        self.assertEqual(len(lines), 1)
        self.assertIn("scan x2", lines[0])
        self.assertIn("12 sectors revealed, 1 hostile", lines[0])
        self.assertIn("1 HostileMass @3,4 (band 3)", lines[0])
        self.assertIn("worst T5", lines[0])
        self.assertLess(len(lines[0]), 220)

    def test_scans_of_two_fleets_stay_separate(self):
        lines = play.collapse_events([tick_msg(scan(10, fleet=2), scan(10, fleet=3))])
        self.assertEqual(len(lines), 2)

    def test_combat_rounds_collapse_per_sector(self):
        msgs = [tick_msg(*[round_(t) for t in range(100, 106)]), tick_msg(round_(106, mine=False, sector=(5, 5)))]
        lines = play.collapse_events(msgs)
        self.assertEqual(len(lines), 2)
        self.assertIn("ticks 100-105: 6 round(s), 36 shots, your side took 30 hull and lost 0 ship(s), dealt 42 and destroyed 6",
                      lines[0])
        self.assertIn("witnessed", lines[1])

    def test_mine_filter_drops_witnessed_fights(self):
        lines = play.collapse_events([tick_msg(round_(1, mine=False))], mine_only=True)
        self.assertEqual(lines, [])

    def test_identical_lines_get_a_count_and_ships_sum(self):
        hold = {"tick": 1, "kind": "PolicyAction", "fleet_id": 2, "preset": "prospect", "action": "hold", "reason": "nothing left"}
        built = {"tick": 2, "kind": "ShipBuilt", "ship_class": "LightFighter", "count": 1}
        lines = play.collapse_events([tick_msg(hold, built, dict(hold, tick=3), built)])
        self.assertTrue(any("hold" in l and "x2" in l for l in lines))
        self.assertEqual(len([l for l in lines if "hold" in l]), 1)
        self.assertIn("  built 2x LightFighter", lines)

    def test_errors_can_be_hidden_and_completions_are_sentences(self):
        msgs = [{"type": "error", "code": "QueueFull", "message": "full"},
                tick_msg({"tick": 5, "kind": "BuildingCompleted", "building_type": "MetalMine", "new_level": 4})]
        self.assertIn("  ERROR QueueFull full", play.collapse_events(msgs))
        quiet = play.collapse_events(msgs, errors=False)
        self.assertEqual(quiet, ["  t5 built MetalMine L4"])

    def test_one_pile_is_named_one_way_in_every_line(self):
        destroyed = {"tick": 3246, "kind": "FleetDestroyed", "fleet_id": 9, "is_npc": True, "sector": {"q": 2, "r": -1},
                     "salvage": {"metal": 89.3, "crystal": 48.7, "deuterium": 24.4}, "label": "light fighter convoy"}
        ended = {"tick": 3246, "kind": "CombatEnded", "sector": {"q": 2, "r": -1}, "outcome": "hostiles_destroyed", "rounds": 2,
                 "losses": [], "salvage": {"metal": 89.3, "crystal": 48.7, "deuterium": 24.4},
                 "salvage_pile": {"metal": 279.0, "crystal": 152.2, "deuterium": 76.1}}
        self.assertIn("its wreckage 89/49/24 added to the pile there", play.event_text(destroyed))
        self.assertIn("wreckage 89/49/24 (on wreckage from before: 279/152/76 lie there now)", play.event_text(ended))
        alone = dict(ended)
        del alone["salvage_pile"]
        self.assertTrue(play.event_text(alone).endswith("; wreckage 89/49/24"))
        scoop = {"tick": 3248, "kind": "SalvageCollected", "fleet_id": 78, "sector": {"q": 2, "r": -1},
                 "resources": {"metal": 279.0, "crystal": 152.2, "deuterium": 45.3}, "remaining": {"metal": 0, "crystal": 0, "deuterium": 30.8}}
        self.assertEqual(play.event_text(scoop), "fleet 78 scooped 279/152/45 of wreckage @2,-1 into its hold, 0/0/31 left on the ground")

    def test_what_enters_or_leaves_the_stockpile_is_said(self):
        e = {"tick": 1769, "kind": "Stockpile", "player_id": 83, "fleet_id": 84, "cause": "unloaded",
             "amount": {"metal": 4312.0, "crystal": 0.0, "deuterium": 0.0}}
        self.assertEqual(play.event_text(e), "STOCKPILE: fleet 84 unloaded into the stockpile: 4312/0/0")
        self.assertIn("scooped wreckage at home into the stockpile: 8100/2700/0",
                      play.event_text(dict(e, cause="wreckage", amount={"metal": 8100.0, "crystal": 2700.0, "deuterium": 0.0})))

    def test_a_raid_repelled_with_losses_says_what_was_lost(self):
        odds = {"label": "repelled", "destroyed": 1.0, "outlasted": 0.0, "lost": 0.0, "kept": 0.82, "raiders_kept": 0.0}
        self.assertTrue(play.odds_words(odds).startswith("REPELLED and you LOSE about 18% of what stands there"))
        self.assertTrue(play.odds_words(dict(odds, kept=1.0)).startswith("REPELLED and you lose nothing that stands there"))
        resolved = {"tick": 2720, "kind": "RaidResolved", "defended": True, "outcome": "hostiles_destroyed", "rounds": 2,
                    "lost": [{"unit": "LightFighter", "count": 1}], "raiders": [], "raiders_lost": [],
                    "resources_lost": {"metal": 0, "crystal": 0, "deuterium": 0}}
        text = play.event_text(resolved)
        self.assertIn("REPELLED, the raiders destroyed, and you LOST some of what stood there after 2 round(s): you lost 1x LightFighter", text)
        self.assertIn("and you lost nothing", play.event_text(dict(resolved, lost=[])))

    def test_alert_levels(self):
        self.assertTrue(play.is_alert({"kind": "Alert", "level": "Warning"}))
        self.assertFalse(play.is_alert({"kind": "Alert", "level": "Info"}))
        self.assertTrue(play.is_alert({"kind": "RaidIncoming"}))
        self.assertFalse(play.is_alert({"kind": "FleetDestroyed", "mine": False, "is_npc": True}))


class Commands(unittest.TestCase):
    def test_parse_objects_arrays_and_lines(self):
        a = '{"action":"scan","fleet_id":2}'
        self.assertEqual(play.parse_commands([a, a])[0], [json.loads(a)] * 2)
        self.assertEqual(play.parse_commands(["[" + a + "," + a + "]"])[0], [json.loads(a)] * 2)
        self.assertEqual(play.parse_commands([a + "\n" + a])[0], [json.loads(a)] * 2)

    def test_parse_rejoins_arguments_split_by_the_shell(self):
        out, err = play.parse_commands(['{"action":', '"scan",', '"fleet_id":2}'])
        self.assertIsNone(err)
        self.assertEqual(out, [{"action": "scan", "fleet_id": 2}])

    def test_parse_errors(self):
        self.assertIsNotNone(play.parse_commands(["{nope"])[1])
        self.assertIsNotNone(play.parse_commands(["[1,2]"])[1])

    def test_standing_orders_are_taken_as_an_order_and_sent_as_their_own_message(self):
        sent = play.normalize({"action": "policy_update", "fleet_id": 2, "preset": "prospect"})
        self.assertEqual(sent, {"type": "policy_update", "fleet_id": 2, "preset": "prospect"})
        self.assertEqual(play.normalize({"type": "policy_update", "fleet_id": 2, "preset": "prospect"})["type"], "policy_update")
        self.assertEqual(play.normalize({"action": "scan", "fleet_id": 2}), {"action": "scan", "fleet_id": 2})

    def test_each_answer_belongs_to_the_order_before_its_fence(self):
        fs = fresh()
        load = {"action": "load", "fleet_id": fs["fleets"][0]["id"], "resource": "metal", "amount": 3000}
        fid, fence = play.fence_for(fs, load)
        self.assertEqual(fence, {"action": "odds", "fleet_id": fid, "sectors": []})
        lf ={"type": "error", "code": "InsufficientResources", "message": "Light Fighter x1 costs 3000 metal"}
        short = {"type": "error", "code": "NoResources", "message": "the stockpile holds 1982.3 metal; fleet 84 cannot load 3000.0"}
        answer = {"type": "odds", "fleet_id": fid, "sectors": []}
        board = {"type": "leaderboard", "entries": []}
        log = [lf, answer, board, short, answer, {"type": "tick_update", "tick": 9, "events": []}]
        windows, seen, after = play.split_windows(log, [fid, fid])
        self.assertEqual(seen, 2)
        self.assertEqual(play.refusal_of(windows[0]), lf, "the Light Fighter's refusal is the batch's first order's")
        self.assertEqual(play.refusal_of(windows[1]), short, "and the load's is the load's, whatever its words name")
        self.assertEqual(after[0]["type"], "tick_update")
        self.assertIsNone(play.reply_line(load, board), "a leaderboard the daemon asked for is no order's answer")
        self.assertIsNone(play.refusal_of([board]))

    def test_a_fence_with_no_fleet_is_answered_by_the_refusal_for_fleet_0(self):
        fs = fresh()
        fs["fleets"] = []
        fid, _ = play.fence_for(fs, {"action": "build", "building_type": "MetalMine"})
        self.assertEqual(fid, 0)
        refusal = {"type": "error", "code": "FleetNotFound", "message": "fleet 0 not found (or it has no ships left)"}
        windows, seen, _ = play.split_windows([refusal], [0])
        self.assertEqual((seen, windows), (1, [[]]))

    def test_cancels_report_their_order_id(self):
        cmds = [{"action": "cancel_queued", "queue_type": "Building", "id": 6},
                {"action": "cancel_queued", "queue_type": "Research", "id": 7}]
        qev = [{"action": "Cancelled", "item": "Crystal Mine Lv.2", "id": 6}]
        self.assertIn("#6", play.queue_outcome(cmds[0], qev, set()))
        self.assertIsNone(play.queue_outcome(cmds[1], qev, set()))


class State(unittest.TestCase):
    def test_state_document_has_the_agent_fields(self):
        doc = play.build_state(fresh(), None, 12.3, ["t1 ALERT[Warning] x"])
        for key in ("stock", "cap", "per_s", "levels", "queues", "idle", "next", "fleets", "score_rule", "minutes_left", "alerts", "tick"):
            self.assertIn(key, doc)
        self.assertEqual(set(doc["queues"]), {"build", "research", "ship"})
        self.assertEqual(doc["minutes_left"], 12.3)
        self.assertIn("t1 ALERT[Warning] x", doc["alerts"])
        self.assertIn("Unspent stock scores 0", doc["score_rule"])
        text = json.dumps(doc)
        self.assertNotIn("\n", text)
        self.assertLess(len(text), 12000)
        for f in doc["fleets"]:
            for key in ("id", "at", "state", "fuel", "range_hops", "power", "cargo"):
                self.assertIn(key, f)

    def test_next_costs_and_locks(self):
        doc = play.build_state(fresh(), None, None)
        mine = doc["next"]["buildings"]["MetalMine"]
        self.assertEqual(len(mine["cost"]), 3)
        self.assertIn("to", mine)
        self.assertTrue(any("locked" in v for v in doc["next"]["buildings"].values()))

    def test_waiting_items_show_inside_their_queue_with_shortfall(self):
        fs = fresh()
        hw = fs["homeworld"]
        hw["build_queue"] = [{"building_type": "MetalMine", "target_level": 3, "start_tick": fs["tick"], "end_tick": fs["tick"] + 5}]
        hw["build_pending"] = [{"building_type": "CrystalMine", "target_level": 3, "ticks": 4, "waiting_for": {"metal": 35.4, "crystal": 0, "deuterium": 0}}]
        hw["production"]["metal"] = 5.0
        q = play.queues_of(fs)["build"]
        self.assertEqual(len(q["running"]), 1)
        self.assertEqual(q["waiting"][0]["short"], {"metal": 35})
        self.assertEqual(q["waiting"][0]["covered_in_s"], 7)
        self.assertEqual(q["room"], hw.get("build_slots", 1) + hw.get("queue_waiting_max", 3) - 2)
        self.assertEqual(q["open"], 0)
        line = play.queue_line("build", q)
        self.assertIn("waiting[0]", line)
        self.assertIn("needs 35 metal", line)

    def test_idle_queue_has_an_open_slot(self):
        fs = fresh()
        fs["homeworld"]["build_queue"] = []
        fs["homeworld"]["build_pending"] = []
        self.assertEqual(play.queues_of(fs)["build"]["open"], fs["homeworld"].get("build_slots", 1))

    def test_score_row_and_headline(self):
        fs = fresh()
        row = lambda rank, name, score, b, r, f: {"rank": rank, "name": name, "agent": True, "score": score, "buildings": b,
                                                  "research": r, "fleet": f, "outpost": 0, "defence": 0}
        board = {"tick": fs["tick"], "entries": [row(1, "Other", 90, 60, 20, 10), row(2, fs["player"]["name"], 40.04, 25.04, 10, 5)]}
        s = play.score_of(fs, board)
        self.assertEqual((s["rank"], s["of"], s["score"], s["fleet"]), (2, 2, 40.0, 5))
        self.assertEqual(s["leader"], {"name": "Other", "score": 90, "gap": 50.0,
                                       "gap_by_part": {"buildings": 35.0, "research": 10.0, "fleet": 5.0}})
        self.assertNotIn("bonus", s)
        doc = play.build_state(fs, board, 5)
        head = play.headline("x", doc)
        self.assertIn("SCORE 40.0 rank 2/2 (buildings 25.0, research 10.0, fleet 5.0)", head)
        self.assertIn("Standing: leader Other 90.0, 50.0 ahead of you (buildings +35.0, research +10.0, fleet +5.0)", head)
        self.assertIn("IDLE QUEUES", head)
        self.assertIn("Unspent stock scores 0", head)
        self.assertIn("Defences score 0", head)

    def test_next_items_say_what_is_short(self):
        fs = fresh()
        fs["player"]["resources"] = {"metal": 0.0, "crystal": 0.0, "deuterium": 0.0}
        fs["homeworld"]["production"] = {"metal": 2.0, "crystal": 0.0, "deuterium": 0.0}
        doc = play.build_state(fs, None, None)
        mine = doc["next"]["buildings"]["MetalMine"]
        self.assertFalse(mine["ok"])
        self.assertEqual(mine["short"]["metal"], mine["cost"][0])
        rich = fresh()
        rich["player"]["resources"] = {"metal": 1e9, "crystal": 1e9, "deuterium": 1e9}
        payable = play.build_state(rich, None, None)["next"]["buildings"]["MetalMine"]
        self.assertTrue(payable["ok"])
        self.assertNotIn("short", payable)

    def test_a_reserve_holding_the_slot_and_a_queue_all_short_say_so(self):
        fs = fresh()
        hw = fs["homeworld"]
        hw["build_queue"] = []
        hw["production"]["metal"] = 5.0
        hw["build_pending"] = [
            {"id": 11, "building_type": "SolarPlant", "target_level": 9, "ticks": 4, "reserve": True, "waiting_on": "Resources",
             "waiting_for": {"metal": 500.0, "crystal": 0, "deuterium": 0}},
            {"id": 12, "building_type": "MetalStorage", "target_level": 1, "ticks": 4, "waiting_on": "Order"}]
        q = play.queues_of(fs)["build"]
        self.assertIn("the reserve on SolarPlant->L9 #11 holds the free slot while it is short (500 metal, covered in ~100s)", q["note"])
        self.assertIn("1 order(s) behind it that could start now wait for it: MetalStorage->L1 #12", q["note"])
        self.assertIn("NOTE: the reserve", play.queue_line("build", q))
        hw["build_pending"] = [
            {"id": 11, "building_type": "SolarPlant", "target_level": 9, "ticks": 4, "waiting_on": "Resources",
             "waiting_for": {"metal": 500.0, "crystal": 0, "deuterium": 0}},
            {"id": 13, "building_type": "CrystalMine", "target_level": 9, "ticks": 4, "waiting_on": "Resources",
             "waiting_for": {"metal": 50.0, "crystal": 0, "deuterium": 0}}]
        q = play.queues_of(fs)["build"]
        self.assertIn("every waiting order is short, so the free slot stays idle; the first to be covered is CrystalMine->L9 #13 (50 metal) in ~10s",
                      q["note"])
        hw["build_queue"] = [{"building_type": "MetalMine", "target_level": 3, "start_tick": 0, "end_tick": 99}] * hw.get("build_slots", 1)
        self.assertNotIn("note", play.queues_of(fs)["build"], "with no slot free the queue only waits for one")

    def test_state_keys(self):
        doc = play.build_state(fresh(), None, 5)
        picked, err = play.pick_keys(doc, ["stock", "queues"])
        self.assertIsNone(err)
        self.assertEqual(list(picked), ["stock", "queues"])
        _, err = play.pick_keys(doc, ["stock", "nope"])
        self.assertIn("no such key in state: nope", err)
        self.assertIn("stock", err)

    def test_server_warnings_pass_through_when_present(self):
        fs = fresh()
        fs["warnings"] = ["fuel is short"]
        self.assertIn("fuel is short", play.build_state(fs, None, None)["alerts"])

    def test_status_prints_and_brief_is_shorter(self):
        fs = fresh()
        doc = play.build_state(fs, None, 30)
        full, brief = io.StringIO(), io.StringIO()
        with redirect_stdout(full):
            play.summarize(fs, doc, {})
        with redirect_stdout(brief):
            for n, q in doc["queues"].items():
                print(play.queue_line(n, q))
        self.assertGreater(len(full.getvalue()), len(brief.getvalue()))
        self.assertIn("Build queue", full.getvalue())


class Costs(unittest.TestCase):
    def test_costs_list_three_levels_with_unlocks_and_payback(self):
        text = "\n".join(play.costs_lines(fresh(), "buildings"))
        self.assertIn("payback", text)
        self.assertIn("unlocks", text)
        mine = text.split("MetalMine")[1].split("CrystalMine")[0]
        self.assertEqual(mine.count("-> L"), 3)
        self.assertIn("~", mine)

    def test_extrapolation_follows_growth(self):
        steps = play.future_steps("MetalMine", 1, 20, {"level": 2, "cost": {"metal": 90, "crystal": 22, "deuterium": 0}, "ticks": 2}, "buildings")
        self.assertEqual([s["level"] for s in steps], [2, 3, 4])
        self.assertEqual(steps[1]["cost"]["metal"], 135)
        self.assertTrue(steps[0]["exact"] and not steps[1]["exact"])

    def test_maxed_has_no_steps(self):
        self.assertEqual(play.future_steps("MetalMine", 20, 20, None, "buildings"), [])

    def test_unlock_text(self):
        rows = [("ships", "SmallCargo", None, None, None, [{"name": "Shipyard", "need": 2, "have": 1, "met": False}]),
                ("ships", "Recycler", None, None, None, [{"name": "Shipyard", "need": 2, "have": 1, "met": False},
                                                          {"name": "Combustion Drive", "need": 6, "have": 2, "met": False}])]
        text = play.unlock_text(rows, "Shipyard", 2)[0]
        self.assertIn("SmallCargo", text)
        self.assertIn("toward Recycler", text)


class Wait(unittest.TestCase):
    def snap(self, fs=None):
        return play.snapshot(fs or fresh())

    def test_nothing_changed_keeps_waiting(self):
        base = self.snap()
        self.assertEqual(play.wait_reasons(base, self.snap(), []), [])

    def test_finished_item_frees_room(self):
        fs = fresh()
        fs["homeworld"]["build_queue"] = [{"building_type": "MetalMine", "target_level": 3, "start_tick": 0, "end_tick": 99}]
        base = self.snap(fs)
        done = fresh()
        done["homeworld"]["build_queue"] = []
        why = play.wait_reasons(base, self.snap(done), [], "idle")
        self.assertTrue(any("build queue" in w for w in why))

    def test_idle_queue_becoming_payable_triggers(self):
        poor = fresh()
        poor["player"]["resources"] = {"metal": 0.0, "crystal": 0.0, "deuterium": 0.0}
        rich = fresh()
        rich["player"]["resources"] = {"metal": 1e6, "crystal": 1e6, "deuterium": 1e6}
        why = play.wait_reasons(self.snap(poor), self.snap(rich), [], "idle")
        self.assertTrue(any("now payable" in w for w in why))

    def test_busy_queue_does_not_trigger_on_money(self):
        fs = fresh()
        fs["player"]["resources"] = {"metal": 0.0, "crystal": 0.0, "deuterium": 0.0}
        fs["homeworld"]["build_queue"] = [{"building_type": "MetalMine", "target_level": 3, "start_tick": 0, "end_tick": 99}]
        rich = copy.deepcopy(fs)
        rich["player"]["resources"] = {"metal": 1e6, "crystal": 1e6, "deuterium": 1e6}
        why = play.wait_reasons(self.snap(fs), self.snap(rich), [], "idle")
        self.assertFalse([w for w in why if w.startswith("build")])

    def test_alert_always_returns_and_ordinary_events_only_for_event_mode(self):
        base = self.snap()
        alert = {"kind": "Alert", "level": "Critical", "message": "fleet lost", "tick": 1}
        built = {"kind": "ShipBuilt", "ship_class": "LightFighter", "count": 1, "tick": 1}
        for mode in ("any", "idle", "event", "done"):
            self.assertTrue(play.wait_reasons(base, base, [alert], mode), mode)
        self.assertEqual(play.wait_reasons(base, base, [built], "any"), [])
        self.assertTrue(play.wait_reasons(base, base, [built], "event"))

    def test_manual_fleet_arriving_returns_but_autopilot_does_not(self):
        fs = fresh()
        fs["fleets"][0]["state"] = "Moving"
        fs["fleets"][0]["policy"] = None
        base = self.snap(fs)
        arrived = copy.deepcopy(fs)
        arrived["fleets"][0]["state"] = "Idle"
        self.assertTrue(any("fleet" in w for w in play.wait_reasons(base, self.snap(arrived), [], "any")))
        self.assertEqual(play.wait_reasons(base, self.snap(arrived), [], "idle"), [])
        orders = {"preset": "prospect",
                  "params": {"min_fuel_pct": 10, "cargo_return_pct": 85, "max_range": 6, "engage_ratio_x10": 20,
                             "disengage_ratio_x10": 9, "fuel_budget_pct": 35}}
        auto = copy.deepcopy(fs)
        auto["fleets"][0]["policy"] = orders
        auto_arrived = copy.deepcopy(arrived)
        auto_arrived["fleets"][0]["policy"] = orders
        self.assertEqual(play.wait_reasons(self.snap(auto), self.snap(auto_arrived), [], "any"), [])
        self.assertEqual(play.fleet_rows(fs)[0]["policy"], "manual")
        self.assertIn(" prospect (range 6, engage 2.0x, disengage 0.9x, fuel 35%, home at cargo 85%, reserve 10%) ",
                      play.fleet_line(play.fleet_rows(auto)[0]))

    def test_lost_fleet_returns(self):
        base = self.snap()
        gone = fresh()
        gone["fleets"] = []
        self.assertTrue(any("gone" in w for w in play.wait_reasons(base, self.snap(gone), [], "any")))

    def test_done_mode_waits_for_everything(self):
        busy = fresh()
        busy["homeworld"]["build_queue"] = [{"building_type": "MetalMine", "target_level": 3, "start_tick": 0, "end_tick": 99}]
        self.assertEqual(play.wait_reasons(self.snap(busy), self.snap(busy), [], "done"), [])
        idle = fresh()
        idle["homeworld"]["build_queue"] = []
        idle["fleets"][0]["state"] = "Idle"
        self.assertTrue(any("finished" in w for w in play.wait_reasons(self.snap(busy), self.snap(idle), [], "done")))


class View(unittest.TestCase):
    def test_tick_updates_fold_into_the_view(self):
        fs = fresh()
        upd = {"type": "tick_update", "tick": fs["tick"] + 5, "player": dict(fs["player"], resources={"metal": 1.0, "crystal": 2.0, "deuterium": 3.0}),
               "fleets": [], "homeworld_update": fs["homeworld"], "sector_updates": [fs["known_sectors"][0]]}
        out = play.merge_view(fs, upd)
        self.assertEqual(out["tick"], FS["tick"] + 5)
        self.assertEqual(out["player"]["resources"]["metal"], 1.0)
        self.assertEqual(out["fleets"], [])
        self.assertEqual(len(out["known_sectors"]), len(FS["known_sectors"]))

    def test_updates_before_any_full_state_are_ignored(self):
        self.assertIsNone(play.merge_view(None, {"type": "tick_update", "tick": 1, "fleets": []}))


if __name__ == "__main__":
    unittest.main()
