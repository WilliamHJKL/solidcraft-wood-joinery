#!/bin/sh
# Builds the plugin and packs it, once per language:
#   dist/wood-joinery/ and dist/wood-joinery-en.zip       English
#   dist/<lang>/wood-joinery/ and dist/wood-joinery-<lang>.zip   one per file in plugin/locales/
# A .zip is what Preferences > Plugins > Install plugin... takes. Install one language only:
# every package has the same plugin id.
set -eu
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown -p wood-joinery
rm -rf dist
mkdir -p dist/wood-joinery
cargo run --release -q -p packager -- target/wasm32-unknown-unknown/release/wood_joinery.wasm dist/wood-joinery/plugin.wasm
# The packed manifest is compact: SolidCraft refuses one over 64 KiB (16 KiB before 0.45).
python3 -c "
import json, sys
data = json.dumps(json.load(open(sys.argv[1], encoding='utf-8')), ensure_ascii=False, separators=(',', ':')) + '\n'
if len(data.encode()) > 64 * 1024:
    raise SystemExit('the manifest is over the 64 KiB limit of SolidCraft')
open(sys.argv[2], 'w', encoding='utf-8').write(data)
" plugin/plugin.json dist/wood-joinery/plugin.json
# The command icons (SolidCraft 0.44 and later draw them; older versions ignore them).
cp -r plugin/icons dist/wood-joinery/icons
pack() { # pack <folder> <zip>
    python3 -c "
import os, sys, zipfile
with zipfile.ZipFile(sys.argv[2], 'w', zipfile.ZIP_DEFLATED) as z:
    for name in ('plugin.json', 'plugin.wasm'):
        z.write(sys.argv[1] + '/' + name, 'wood-joinery/' + name)
    for name in sorted(os.listdir(sys.argv[1] + '/icons')):
        z.write(sys.argv[1] + '/icons/' + name, 'wood-joinery/icons/' + name)
" "$1" "$2"
}
pack dist/wood-joinery dist/wood-joinery-en.zip
for locale in plugin/locales/*.json; do
    lang=$(basename "$locale" .json)
    mkdir -p "dist/$lang/wood-joinery"
    python3 tools/localize.py plugin/plugin.json "$locale" "dist/$lang/wood-joinery/plugin.json"
    cp dist/wood-joinery/plugin.wasm "dist/$lang/wood-joinery/plugin.wasm"
    cp -r plugin/icons "dist/$lang/wood-joinery/icons"
    pack "dist/$lang/wood-joinery" "dist/wood-joinery-$lang.zip"
done
# SolidCraft refuses a manifest over 64 KiB (16 KiB before 0.45; this plugin needs 0.46). Keep an
# eighth of it free for the next joints: a field's rule goes in its title, and the README holds the
# longer explanations.
python3 -c "
import glob, os, sys
worst = max(os.path.getsize(f) for f in glob.glob('dist/**/plugin.json', recursive=True))
print(f'largest manifest: {worst} of 65536 bytes ({100 * worst // 65536} %)')
sys.exit(f'a manifest is over 56 KiB: shorten titles, or move a description to the README' if worst > 56 * 1024 else 0)
"
ls -l dist/*.zip
