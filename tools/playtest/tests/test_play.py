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


def round_(tick, mine=True, owner=None, dmg=5.0, sector=(1, 1)):
    return {"tick": tick, "kind": "CombatRound", "sector": {"q": sector[0], "r": sector[1]}, "target_owner": owner,
            "hull_damage": dmg, "mine": mine}


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
        msgs = [tick_msg(*[round_(t, owner="x") for t in range(100, 106)]), tick_msg(round_(106, mine=False, sector=(5, 5)))]
        lines = play.collapse_events(msgs)
        self.assertEqual(len(lines), 2)
        self.assertIn("ticks 100-105: 6 shots, you took 30 hull", lines[0])
        self.assertIn("witnessed", lines[1])

    def test_mine_filter_drops_witnessed_fights(self):
        lines = play.collapse_events([tick_msg(round_(1, mine=False))], mine_only=True)
        self.assertEqual(lines, [])

    def test_identical_lines_get_a_count_and_ships_sum(self):
        hold = {"tick": 1, "kind": "PolicyAction", "fleet_id": 2, "preset": "prospect", "action": "hold", "reason": "nothing left"}
        built = {"tick": 2, "kind": "ShipBuilt", "ship_class": "Scout", "count": 1}
        lines = play.collapse_events([tick_msg(hold, built, dict(hold, tick=3), built)])
        self.assertTrue(any("hold" in l and "x2" in l for l in lines))
        self.assertEqual(len([l for l in lines if "hold" in l]), 1)
        self.assertIn("  built 2x Scout", lines)

    def test_errors_can_be_hidden_and_completions_are_sentences(self):
        msgs = [{"type": "error", "code": "QueueFull", "message": "full"},
                tick_msg({"tick": 5, "kind": "BuildingCompleted", "building_type": "MetalMine", "new_level": 4})]
        self.assertIn("  ERROR QueueFull full", play.collapse_events(msgs))
        quiet = play.collapse_events(msgs, errors=False)
        self.assertEqual(quiet, ["  t5 built MetalMine L4"])

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

    def test_errors_go_to_the_command_they_name(self):
        cmds = [{"action": "build", "building_type": "MetalMine"}, {"action": "research", "tech": "Navigation"},
                {"action": "harvest", "fleet_id": 99, "resource": "Auto"}]
        errs = [{"code": "FleetNotFound", "message": "fleet 99 not found"},
                {"code": "NoResearchLab", "message": "Navigation needs Research Lab level 2"}]
        got, left = play.match_errors(cmds, errs)
        self.assertIsNone(got[0])
        self.assertEqual(got[1]["code"], "NoResearchLab")
        self.assertEqual(got[2]["code"], "FleetNotFound")
        self.assertEqual(left, [])

    def test_queue_full_goes_to_the_last_matching_order(self):
        cmds = [{"action": "build", "building_type": "A"}, {"action": "build", "building_type": "B"}, {"action": "research", "tech": "T"}]
        got, _ = play.match_errors(cmds, [{"code": "QueueFull", "message": "the building queue is full (3 items)"}])
        self.assertIsNone(got[0])
        self.assertIsNotNone(got[1])
        self.assertIsNone(got[2])

    def test_unmatchable_error_is_reported_not_lost(self):
        got, left = play.match_errors([{"action": "scan", "fleet_id": 2}], [{"code": "X", "message": "a"}, {"code": "Y", "message": "b"}])
        self.assertEqual(len(left) + sum(g is not None for g in got), 2)


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
        board = {"tick": fs["tick"], "entries": [{"rank": 1, "name": "Other", "agent": True, "score": 90, "core": 80, "combat": 10, "explore": 0},
                                                 {"rank": 2, "name": fs["player"]["name"], "agent": True, "score": 40.04, "core": 30, "combat": 20, "explore": 0}]}
        s = play.score_of(fs, board)
        self.assertEqual((s["rank"], s["of"], s["score"]), (2, 2, 40.0))
        self.assertTrue(s["bonus_capped"])
        doc = play.build_state(fs, board, 5)
        head = play.headline("x", doc)
        self.assertIn("SCORE 40.0 rank 2/2", head)
        self.assertIn("CAPPED", head)
        self.assertIn("IDLE QUEUES", head)
        self.assertIn("Unspent stock scores 0", head)

    def test_server_warnings_pass_through_when_present(self):
        fs = fresh()
        fs["warnings"] = ["fuel is short"]
        self.assertIn("fuel is short", play.build_state(fs, None, None)["alerts"])

    def test_status_prints_and_brief_is_shorter(self):
        fs = fresh()
        doc = play.build_state(fs, None, 30)
        full, brief = io.StringIO(), io.StringIO()
        with redirect_stdout(full):
            play.summarize(fs, doc)
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
        rows = [("ships", "Frigate", None, None, None, [{"name": "Shipyard", "need": 4, "have": 2, "met": False}]),
                ("ships", "Cruiser", None, None, None, [{"name": "Shipyard", "need": 4, "have": 2, "met": False},
                                                         {"name": "Cruiser Tech", "need": 1, "have": 0, "met": False}])]
        text = play.unlock_text(rows, "Shipyard", 4)[0]
        self.assertIn("Frigate", text)
        self.assertIn("toward Cruiser", text)


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
        built = {"kind": "ShipBuilt", "ship_class": "Scout", "count": 1, "tick": 1}
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
        auto = copy.deepcopy(fs)
        auto["fleets"][0]["policy"] = "prospect"
        auto_arrived = copy.deepcopy(arrived)
        auto_arrived["fleets"][0]["policy"] = "prospect"
        self.assertEqual(play.wait_reasons(self.snap(auto), self.snap(auto_arrived), [], "any"), [])

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
