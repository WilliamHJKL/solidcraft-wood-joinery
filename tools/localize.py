#!/usr/bin/env python3
"""Writes a translated copy of the plugin manifest.

Usage: tools/localize.py plugin/plugin.json plugin/locales/fr.json out/plugin.json

SolidCraft shows a plugin's texts as its manifest gives them, in one language. A locale file
maps each English text to its translation. Every text a user sees must be in it: a missing one
is an error, so a translation cannot fall behind the manifest silently.

The values of a choice are texts too. A translated value ends with the script value in brackets,
as in "classique (classic)": the plugin reads the brackets, so it needs no list of languages.
"""
import json
import sys


def texts(manifest):
    """Every (holder, key) whose value a user sees."""
    yield manifest, "name"
    yield manifest, "description"
    for entry in manifest["runtime"]["entries"]:
        for slot in entry.get("slots", []):
            yield slot, "name"
        for prop in entry["input_schema"]["properties"].values():
            for key in ("title", "description"):
                if key in prop:
                    yield prop, key
            for option in prop.get("oneOf", []):
                for key in ("title", "description"):
                    if key in option:
                        yield option, key


def translate_condition(cond, props, words):
    """Turns the choice values a condition names into their translations, in place."""
    if not isinstance(cond, dict):
        return
    for key in ("all", "any"):
        for inner in cond.get(key, []):
            translate_condition(inner, props, words)
    translate_condition(cond.get("not"), props, words)
    if "enum" not in props.get(cond.get("field"), {}):
        return
    if "eq" in cond:
        cond["eq"] = words[cond["eq"]]
    if "in" in cond:
        cond["in"] = [words[v] for v in cond["in"]]


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    source, locale, out = sys.argv[1:]
    with open(source, encoding="utf-8") as f:
        manifest = json.load(f)
    with open(locale, encoding="utf-8") as f:
        words = json.load(f)
    missing, used = [], set()
    for holder, key in texts(manifest):
        english = holder[key]
        if english in words:
            holder[key] = words[english]
            used.add(english)
        else:
            missing.append(english)
    for entry in manifest["runtime"]["entries"]:
        for prop in entry["input_schema"]["properties"].values():
            for i, value in enumerate(prop.get("enum", [])):
                if value not in words:
                    missing.append(value)
                    continue
                used.add(value)
                if words[value] != value and not words[value].endswith(f"({value})"):
                    raise SystemExit(f"{locale}: the translation of {value!r} must end with ({value})")
                if prop.get("default") == value:
                    prop["default"] = words[value]
                prop["enum"][i] = words[value]
    # A field shown only for some choices names the choice's value, which is translated above.
    for entry in manifest["runtime"]["entries"]:
        props = entry["input_schema"]["properties"]
        for prop in props.values():
            translate_condition(prop.get("x-solidcraft-visible-when"), props, words)
    unused = sorted(set(words) - used)
    if missing or unused:
        for text in missing:
            print(f"{locale}: no translation for {text!r}", file=sys.stderr)
        for text in unused:
            print(f"{locale}: {text!r} is not in the manifest any more", file=sys.stderr)
        raise SystemExit(1)
    if len(manifest["name"]) > 80 or len(manifest["description"]) > 240:
        raise SystemExit(f"{locale}: name over 80 or description over 240 characters")
    # Compact: SolidCraft refuses a manifest over 16 KiB, and nobody reads the packed copy.
    data = json.dumps(manifest, ensure_ascii=False, separators=(",", ":")) + "\n"
    if len(data.encode()) > 16 * 1024:
        raise SystemExit(f"{locale}: the manifest is over SolidCraft's 16 KiB limit")
    with open(out, "w", encoding="utf-8") as f:
        f.write(data)


if __name__ == "__main__":
    main()
