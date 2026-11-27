"""Adds the docked "Joints" panel to a manifest: its runtime.ui, from plugin/panel.json. The
component's `describe` returns the same file, so the two cannot differ. The panel's actions run
the plugin's entries: Update joints, Edit a joint (a double click on a row) and Refresh.

    python3 tools/panel_manifest.py <plugin.json in> <panel.json> <plugin.json out>
"""
import json
import sys

src, panel, out = sys.argv[1:4]
m = json.load(open(src, encoding="utf-8"))
m["runtime"]["ui"] = json.load(open(panel, encoding="utf-8"))
json.dump(m, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
