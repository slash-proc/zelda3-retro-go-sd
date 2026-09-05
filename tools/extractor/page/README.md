# The manual conversion page

A static page that converts a ROM in the browser, published to GitHub Pages by
CI. It serves two purposes:

1. **For users** — a way to produce `zelda3_assets.dat` without a terminal.
2. **For the project** — the reference consumer of the extractor spec. It uses
   the same `verify.mjs` and `extract.mjs` a web builder does, so if the ABI or
   the manifest format drifts, this page breaks in CI first.

It is also the **distribution endpoint**: GitHub release assets are not
CORS-fetchable, so a consuming web tool reads `manifest.json` and the module
from this Pages site. See [`docs/spec/distribution.md`](../../../docs/spec/distribution.md).

## Files

| | |
|---|---|
| `index.html` | markup |
| `style.css` | styles; light and dark |
| `app.js` | fetches and verifies the module, drives the flow |
| `worker.js` | runs the extraction off the main thread |

`build-page.sh` assembles these with `verify.mjs`, `extract.mjs`, the built
module and a generated `manifest.json` into `site/`.

## Design notes

**Verification is silent.** It has no section of its own. Users cannot act on
the details and saying "verified!" is reassurance, not information. Instead the
info box — which answers *what goes in and what comes out* — renders only once
the module has been hash-matched and verified, so its presence is the result of
the check while its content is something a user wants. On failure the page says
it cannot run, and why.

**No expert controls.** An unrecognised base ROM is almost always a modified
copy, so the page sets `noHashCheck` itself and says what it assumed, rather
than exposing a checkbox nobody can evaluate. Language is the one genuine
choice a user can make, and it is made by which ROMs they add, beside the file
picker.

**Verdicts, not hashes.** The output shows `Hash matches ✓`; the 64 characters
are one click away for anyone who wants to compare them.

## Two failure modes to know about

`verify.mjs` and `extract.mjs` are loaded directly by the browser as well as
run under node. Node-only constructs in them throw at import time and take the
page down **silently** — no error anywhere, just a page where nothing happens.
Both have already bitten this page: a `#!/usr/bin/env node` shebang, and a bare
`process.argv` in a CLI block. `build-page.sh` now fails the build on either,
and `test-page.mjs` loads the page in a real browser and fails on any console
error, which is the check that would have caught them.
