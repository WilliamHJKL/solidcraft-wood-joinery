#!/bin/sh
# Builds the plugin and packs it: dist/wood-joinery/ (the folder SolidCraft reads) and
# dist/wood-joinery.zip (the file Preferences > Plugins > Install plugin... takes).
set -eu
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown -p wood-joinery
rm -rf dist
mkdir -p dist/wood-joinery
cargo run --release -q -p packager -- target/wasm32-unknown-unknown/release/wood_joinery.wasm dist/wood-joinery/plugin.wasm
cp plugin/plugin.json dist/wood-joinery/plugin.json
python3 -c "
import zipfile
with zipfile.ZipFile('dist/wood-joinery.zip', 'w', zipfile.ZIP_DEFLATED) as z:
    for name in ('plugin.json', 'plugin.wasm'):
        z.write('dist/wood-joinery/' + name, 'wood-joinery/' + name)
"
ls -l dist/wood-joinery dist/wood-joinery.zip
