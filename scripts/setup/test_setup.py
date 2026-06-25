"""Unit tests for the setup orchestrator (run via `python3 -m unittest discover
-s scripts/setup -t .` from the repo root; bridged into rtmx verify by a Go test).
"""
import io
import unittest

from scripts.setup import catalog, profile, steps, ui
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


if __name__ == "__main__":
    unittest.main()
