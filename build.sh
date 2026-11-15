#!/bin/sh
# Builds the plugin and packs it, once per language:
#   dist/wood-joinery/ and dist/wood-joinery.zip          English
#   dist/<lang>/wood-joinery/ and dist/wood-joinery-<lang>.zip   one per file in plugin/locales/
# A .zip is what Preferences > Plugins > Install plugin... takes. Install one language only:
# every package has the same plugin id.
set -eu
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown -p wood-joinery
rm -rf dist
mkdir -p dist/wood-joinery
cargo run --release -q -p packager -- target/wasm32-unknown-unknown/release/wood_joinery.wasm dist/wood-joinery/plugin.wasm
cp plugin/plugin.json dist/wood-joinery/plugin.json
pack() { # pack <folder> <zip>
    python3 -c "
import sys, zipfile
with zipfile.ZipFile(sys.argv[2], 'w', zipfile.ZIP_DEFLATED) as z:
    for name in ('plugin.json', 'plugin.wasm'):
        z.write(sys.argv[1] + '/' + name, 'wood-joinery/' + name)
" "$1" "$2"
}
pack dist/wood-joinery dist/wood-joinery.zip
for locale in plugin/locales/*.json; do
    lang=$(basename "$locale" .json)
    mkdir -p "dist/$lang/wood-joinery"
    python3 tools/localize.py plugin/plugin.json "$locale" "dist/$lang/wood-joinery/plugin.json"
    cp dist/wood-joinery/plugin.wasm "dist/$lang/wood-joinery/plugin.wasm"
    pack "dist/$lang/wood-joinery" "dist/wood-joinery-$lang.zip"
done
ls -l dist/*.zip
