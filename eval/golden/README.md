# eval/golden — frozen golden set

Frozen requirement fixtures with known-correct verify outcomes. Every CI run
replays these through the loop (in a network-captured sandbox) and computes the
§5 dashboard metrics over them; `scripts/ci-metrics.py` reads this directory.

Each fixture is a `*.json` file describing one requirement attempt and its
expected, deterministic outcome. Fields mirror `internal/metrics.Attempt` so the
Python metric computation matches the Go collector exactly:

| field             | meaning                                            |
|-------------------|----------------------------------------------------|
| `requirement_id`  | the golden requirement attempted                   |
| `closed`          | verify passed and the requirement closed           |
| `escalated`       | handed to a human / parked                         |
| `first_pass`      | closed without a retry                             |
| `turns`           | agent round-trips                                  |
| `tool_calls`      | total tool calls emitted                           |
| `valid_tool_calls`| well-formed tool calls                             |
| `tokens`          | total tokens (incl. reasoning)                     |
| `wall_clock_ms`   | end-to-end attempt latency (ms)                    |
| `stages_ms`       | per-stage timing: prefill/decode/verify/harness    |

The set is intentionally small and frozen so metrics are deterministic and a
real regression (model/harness/prompt) shows up as an ACR drop, not noise.
