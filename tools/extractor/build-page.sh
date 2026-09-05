#!/usr/bin/env bash
# Assembles the GitHub Pages site into site/.
#
# The page is a consumer of the same verify.mjs and extract.mjs the web builder
# uses -- they are copied in, not reimplemented -- so publishing the page from
# the same CI run that builds the module keeps the two in step by construction.
set -euo pipefail
cd "$(dirname "$0")"

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
# The hashes of a verified reference run, if one has been recorded. Not part of
# the manifest -- a converter's output depends on what the user supplied, so no
# manifest can state it -- but this project can say what one known-good run
# produced, and the page uses it to tell a user whether their extraction matches.
if [[ -f reference.json ]]; then
  cp reference.json site/
else
  echo "note: no reference.json; results will carry no verified-run verdict"
fi

# Where the page reads its manifest from. This is the dist manifest published
# by scripts/make_manifest.py and mirrored into this same site by
# build_dist.py -- the identical file a third-party installer fetches, not a
# second copy in a shape only this page understands. The module, and every
# other file the manifest names, resolves beside it.
#
# Release assets cannot be used directly: they are not CORS-fetchable, which is
# the reason the mirror exists (the spec's spec/01-distribution.md).
MANIFEST_URL="${MANIFEST_URL:-manifest.json}"
printf '{\n  "manifestUrl": "%s"\n}\n' "$MANIFEST_URL" > site/config.json
echo "page reads its manifest from: $MANIFEST_URL"

# Nothing here is Jekyll, and Jekyll would swallow files it does not recognise.
touch site/.nojekyll

echo "site/ ready ($(du -sh site | cut -f1))"
