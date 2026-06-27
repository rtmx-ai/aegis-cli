"""Unit tests for the setup orchestrator (run via `python3 -m unittest discover
-s scripts/setup -t .` from the repo root; bridged into rtmx verify by a Go test).
"""
import io
import json
import pathlib
import tempfile
import unittest

from scripts.setup import catalog, origin, profile, steps, ui
from scripts.setup.orchestrator import Orchestrator


class TestUI(unittest.TestCase):
    def test_bar(self):
        self.assertIn("  0%", ui.bar(0))
        self.assertIn("100%", ui.bar(100))
        self.assertIn(" 50%", ui.bar(50))
        self.assertIn("100%", ui.bar(150))  # clamped

    def test_bounce_moves(self):
        self.assertNotEqual(ui.bounce(0), ui.bounce(3))
        self.assertIn("█", ui.bounce(2))

    def test_fmt_dur(self):
        self.assertEqual(ui.fmt_dur(75), "1m15s")

    def test_truncate(self):
        self.assertEqual(ui.truncate("abcdef", 4), "abc…")
        self.assertEqual(ui.truncate("ab", 4), "ab")

    def test_strike_plain_when_not_tty(self):
        self.assertEqual(ui.UI(stream=io.StringIO()).strike("x"), "x")


class TestProfile(unittest.TestCase):
    def test_pct_from_time(self):
        self.assertIsNone(profile.pct_from_time(5, None))
        self.assertAlmostEqual(profile.pct_from_time(5, 10), 50.0)
        self.assertEqual(profile.pct_from_time(100, 10), 99.0)  # capped


class TestCatalog(unittest.TestCase):
    def test_load_and_recommended(self):
        models = catalog.load()
        self.assertGreaterEqual(len(models), 1)
        self.assertTrue(catalog.recommended_id(models))

    def test_gb(self):
        self.assertEqual(catalog.gb(14249045120), 13)

    def test_required_ram(self):
        self.assertEqual(catalog.required_ram(10 * 10**9), int(10 * 10**9 * 1.1) + 2 * 1073741824)

    def test_fits(self):
        self.assertTrue(catalog.fits(10**9, None))            # unknown RAM -> allow
        self.assertTrue(catalog.fits(10**9, 16 * 1024**3))
        self.assertFalse(catalog.fits(50 * 10**9, 16 * 1024**3))

    def test_default_choice_is_fit_aware(self):
        models = [{"id": "big", "size": 50 * 10**9, "recommended": True},
                  {"id": "small", "size": 10**9}]
        self.assertEqual(catalog.default_choice(models, 200 * 1024**3), "big")    # all fit -> recommended
        self.assertEqual(catalog.default_choice(models, 16 * 1024**3), "small")   # big won't fit -> largest fitting
        self.assertIsNone(catalog.default_choice(models, 1 * 1024**3))            # nothing fits


class TestSteps(unittest.TestCase):
    def test_build_steps_order(self):
        ids = [s.id for s in steps.build_steps(("skip", ""))]
        self.assertEqual(ids[:5], ["toolchain", "aegis", "opencode", "llama", "model"])

    def test_model_choice_cmd_uses_fetch(self):
        self.assertIn("fetch-model.sh", " ".join(steps.ModelStep("choice", "phi-4-mini").cmd()))

    def test_model_skip_has_no_cmd(self):
        self.assertIsNone(steps.ModelStep("skip", "").cmd())

    def test_scriptstep_is_done(self):
        self.assertTrue(steps.ScriptStep("x", "X", ["true"], "setup.sh").is_done())
        self.assertFalse(steps.ScriptStep("y", "Y", ["true"], "/no/such/file").is_done())

    def test_llama_parses_cmake_pct(self):
        s = steps.ScriptStep("llama", "L", ["true"], "x", progress_kind="cmake")
        self.assertEqual(s.progress("[ 71%] Building CXX object", 0), 71.0)

    def test_install_step_only_with_flag(self):
        self.assertEqual(steps.build_steps(("skip", ""), install=True)[-1].id, "install")
        self.assertNotIn("install", [s.id for s in steps.build_steps(("skip", ""))])

    def test_scriptstep_done_summary(self):
        self.assertIn("setup.sh", steps.ScriptStep("x", "X", ["true"], "setup.sh").done_summary())


class TestOrchestrator(unittest.TestCase):
    def test_idempotent_skip(self):
        class Done(steps.Step):
            id, title = "done", "Done"

            def is_done(self):
                return True

        rc = Orchestrator(ui.UI(stream=io.StringIO())).run([Done()])
        self.assertEqual(rc, 0)

    def test_rugged_step_exception_isolated(self):
        class Boom(steps.Step):
            id, title = "boom", "Boom"

            def is_done(self):
                raise RuntimeError("kaboom")

        out = io.StringIO()
        rc = Orchestrator(ui.UI(stream=out)).run([Boom()])
        self.assertEqual(rc, 1)            # failure surfaced
        self.assertIn("setup.log", out.getvalue())  # not a crash

    def test_already_done_reports_summary_not_title(self):
        class Built(steps.Step):
            id, title = "built", "Building X"

            def is_done(self):
                return True

            def done_summary(self):
                return "bin/x (6M)"

        out = io.StringIO()
        Orchestrator(ui.UI(stream=out)).run([Built()])
        v = out.getvalue()
        self.assertIn("Already done. bin/x (6M)", v)
        self.assertNotIn("Building X (already done)", v)  # no title echo


class OriginPolicyTest(unittest.TestCase):
    def test_origin_policy_prompt(self):
        # MODEL-008: the init prompt writes a per-country policy from the catalog's origins.
        with tempfile.TemporaryDirectory() as d:
            cat = pathlib.Path(d) / "catalog.json"
            cat.write_text(json.dumps({"models": [
                {"id": "g", "origin": "US"}, {"id": "q", "origin": "CN"}]}))
            pol = pathlib.Path(d) / "origin-policy.json"
            # allow US, deny CN — keyed off the country in the prompt (order-independent).
            ask = lambda prompt: "y" if "US" in prompt else "n"
            written = origin.configure(
                ask=ask, interactive=True, catalog_path=cat, policy_path=pol)
            self.assertEqual(written["countries"], {"US": "allow", "CN": "deny"})
            self.assertEqual(written["default"], "deny")
            on_disk = json.loads(pol.read_text())
            self.assertEqual(on_disk["countries"]["CN"], "deny")

    def test_non_interactive_leaves_default(self):
        # Non-interactive runs do not touch the shipped policy.
        self.assertIsNone(origin.configure(interactive=False))


if __name__ == "__main__":
    unittest.main()
