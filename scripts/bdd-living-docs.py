#!/usr/bin/env python3
"""Generate searchable HTML living documentation from BDD feature files.

Scans tests/features/ for .feature files, parses Gherkin scenarios,
and produces a single-page HTML report at docs/living-docs.html.

REQ-TEST-042: BDD scenarios rendered as searchable HTML report from CI.
"""

import os
import re
import sys
from pathlib import Path
from html import escape

FEATURES_DIR = Path("tests/features")
OUTPUT_PATH = Path("docs/living-docs.html")


def parse_feature_file(path: Path) -> dict:
    """Parse a .feature file into a structured dict."""
    content = path.read_text(encoding="utf-8")
    feature_name = ""
    scenarios = []
    current_scenario = None

    for line in content.splitlines():
        stripped = line.strip()
        if stripped.startswith("Feature:"):
            feature_name = stripped[len("Feature:"):].strip()
        elif stripped.startswith("Scenario:") or stripped.startswith("Scenario Outline:"):
            if current_scenario:
                scenarios.append(current_scenario)
            tag = "Scenario Outline:" if "Outline" in stripped else "Scenario:"
            current_scenario = {
                "name": stripped[len(tag):].strip(),
                "steps": [],
                "tags": [],
            }
        elif stripped.startswith("@") and current_scenario is None:
            pass  # Feature-level tags
        elif stripped.startswith("@") and current_scenario:
            current_scenario["tags"].append(stripped)
        elif any(stripped.startswith(kw) for kw in ("Given", "When", "Then", "And", "But")):
            if current_scenario:
                current_scenario["steps"].append(stripped)

    if current_scenario:
        scenarios.append(current_scenario)

    return {
        "name": feature_name,
        "path": str(path),
        "category": path.parent.name,
        "scenarios": scenarios,
    }


def render_html(features: list) -> str:
    """Render features into a searchable HTML page."""
    total_scenarios = sum(len(f["scenarios"]) for f in features)
    categories = sorted(set(f["category"] for f in features))

    rows = []
    for feat in sorted(features, key=lambda f: (f["category"], f["name"])):
        for sc in feat["scenarios"]:
            steps_html = "<br>".join(escape(s) for s in sc["steps"])
            tags_html = " ".join(escape(t) for t in sc["tags"])
            rows.append(f"""
            <tr class="scenario-row" data-category="{escape(feat['category'])}">
                <td>{escape(feat['category'])}</td>
                <td>{escape(feat['name'])}</td>
                <td>{escape(sc['name'])}</td>
                <td class="steps">{steps_html}</td>
                <td class="tags">{tags_html}</td>
            </tr>""")

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>aegis-cli Living Documentation</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; background: #1e1e2e; color: #cdd6f4; }}
h1 {{ color: #89b4fa; }}
.stats {{ color: #a6adc8; margin-bottom: 1rem; }}
input {{ padding: 0.5rem; width: 400px; border: 1px solid #585b70; border-radius: 4px;
         background: #313244; color: #cdd6f4; margin-bottom: 1rem; }}
table {{ border-collapse: collapse; width: 100%; }}
th {{ background: #313244; color: #89b4fa; padding: 0.5rem; text-align: left;
      border-bottom: 2px solid #585b70; position: sticky; top: 0; }}
td {{ padding: 0.5rem; border-bottom: 1px solid #313244; vertical-align: top; }}
tr:hover {{ background: #313244; }}
.steps {{ font-size: 0.85em; color: #a6adc8; }}
.tags {{ font-size: 0.8em; color: #f9e2af; }}
.hidden {{ display: none; }}
select {{ padding: 0.5rem; border: 1px solid #585b70; border-radius: 4px;
          background: #313244; color: #cdd6f4; margin-left: 1rem; }}
</style>
</head>
<body>
<h1>aegis-cli Living Documentation</h1>
<p class="stats">{len(features)} features, {total_scenarios} scenarios across {len(categories)} categories</p>
<div>
<input type="text" id="search" placeholder="Search scenarios..." oninput="filterRows()">
<select id="category-filter" onchange="filterRows()">
<option value="">All categories</option>
{"".join(f'<option value="{c}">{c}</option>' for c in categories)}
</select>
</div>
<table>
<thead><tr><th>Category</th><th>Feature</th><th>Scenario</th><th>Steps</th><th>Tags</th></tr></thead>
<tbody id="scenarios">
{"".join(rows)}
</tbody>
</table>
<script>
function filterRows() {{
  const q = document.getElementById('search').value.toLowerCase();
  const cat = document.getElementById('category-filter').value;
  document.querySelectorAll('.scenario-row').forEach(row => {{
    const text = row.textContent.toLowerCase();
    const matchQ = !q || text.includes(q);
    const matchCat = !cat || row.dataset.category === cat;
    row.classList.toggle('hidden', !(matchQ && matchCat));
  }});
}}
</script>
</body>
</html>"""


def main():
    if not FEATURES_DIR.exists():
        print(f"ERROR: {FEATURES_DIR} not found", file=sys.stderr)
        sys.exit(1)

    features = []
    for feature_file in sorted(FEATURES_DIR.rglob("*.feature")):
        feat = parse_feature_file(feature_file)
        if feat["scenarios"]:
            features.append(feat)

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(render_html(features), encoding="utf-8")
    print(f"Generated {OUTPUT_PATH}: {len(features)} features, "
          f"{sum(len(f['scenarios']) for f in features)} scenarios")


if __name__ == "__main__":
    main()
