#!/usr/bin/env bash
# Assembles the GitHub Pages site into site/.
#
# The page is a consumer of the same verify.mjs and extract.mjs the web builder
# uses -- they are copied in, not reimplemented -- so publishing the page from
# the same CI run that builds the module keeps the two in step by construction.
set -euo pipefail
cd "$(dirname "$0")"

WASM="target/wasm32-unknown-unknown/release/zelda3_restool.wasm"
[[ -f "$WASM" ]] || { echo "build the module first: cargo build --release --target wasm32-unknown-unknown --lib" >&2; exit 1; }

rm -rf site
mkdir -p site
cp page/index.html page/style.css page/app.js page/worker.js page/i18n.js site/
cp verify.mjs extract.mjs site/

# These files are loaded directly by the browser. Node-only constructs in them
# fail at import time and take the whole page down silently, which is a much
# worse failure than a build error -- so make it a build error. Both of these
# have bitten this page already.
for f in site/verify.mjs site/extract.mjs site/app.js site/worker.js site/i18n.js; do
  if head -c 2 "$f" | grep -q '#!'; then
    echo "$f starts with a shebang; browsers cannot parse it" >&2
    exit 1
  fi
  if grep -n 'process\.' "$f" | grep -qv 'typeof process'; then
    if ! grep -q 'typeof process !== "undefined"' "$f"; then
      echo "$f uses process.* without a typeof guard; it will throw in a browser" >&2
      exit 1
    fi
  fi
done
cp "$WASM" site/
node manifest.mjs site/zelda3_restool.wasm site/manifest.json

# Where the page reads the manifest from. Locally that is the copy beside it;
# in CI it is the published, tag-pinned copy on the dist branch, so the page
# exercises the same fetch a third-party consumer makes rather than a
# same-origin shortcut. Release assets cannot be used: they are not
# CORS-fetchable (docs/spec/distribution.md).
printf '{\n  "manifestUrl": "%s"\n}\n' "${MANIFEST_URL:-manifest.json}" > site/config.json
echo "page reads its manifest from: ${MANIFEST_URL:-manifest.json}"

# Nothing here is Jekyll, and Jekyll would swallow files it does not recognise.
touch site/.nojekyll

echo "site/ ready ($(du -sh site | cut -f1))"
