# Zelda 3 asset conversion as a verifiable wasm module

A rewrite of the `tables/*.py` resource tool as a single wasm module that takes
a Zelda 3 ROM (plus, optionally, translated ROMs) and returns
`zelda3_assets.dat`. Output is meant to be byte-for-byte identical to the
Python.

> **Status: complete.** All 165 assets are ported. The module's output is
> byte-identical to the Python reference for the US build and for every
> language build checked (`fr`, `de`, `de,fr`); `./check.sh` proves it against
> a live Python run. See [`PROJECT.md`](PROJECT.md).

    ROM (in linear memory)  ->  [ wasm module ]  ->  zelda3_assets.dat

The point is not "Python in a browser". The point is that a stranger's web tool
can fetch this module and run it on a user's proprietary ROM *without trusting
this repository*, because the module's inability to do anything except
transform bytes is checkable from the binary. It **imports nothing at all** —
no filesystem, no network, no clock, no randomness, no JS bridge.

This repo is the reference implementation of a pattern other game ports are
meant to copy, so the machinery here is deliberately project-independent.

## Where things are

| | |
|---|---|
| [`PROJECT.md`](PROJECT.md) | Zelda 3 specifics: accepted ROMs, output, flags, coverage |
| [`docs/spec/abi.md`](../../docs/spec/abi.md) | the module contract: exports, stages, versioning |
| [`docs/spec/verification.md`](../../docs/spec/verification.md) | what `verify.mjs` checks, and why each check |
| [`docs/spec/security-model.md`](../../docs/spec/security-model.md) | what this protects against, and what it does not |
| [`docs/spec/distribution.md`](../../docs/spec/distribution.md) | how a web tool finds a module — and why not from releases |
| [`docs/host-integration.md`](../../docs/host-integration.md) | requirements for a tool that runs modules |
| [`docs/porting.md`](../../docs/porting.md) | porting another project to this pattern |

## Files

| | |
|---|---|
| `src/extract.rs` | input roles, the named stages, and serialisation |
| `src/hash.rs` | SHA-1 and SHA-256, hand-rolled to keep the crate dependency-free |
| `src/lib.rs` | the wasm ABI |
| `verify.mjs` | conformance verifier — dependency-free, browser or node |
| `extract.mjs` | host runner (verify + instantiate + drive) |
| `manifest.mjs` | release manifest generator |
| `record-reference.mjs` | records verified output hashes, run only by `check.sh` |
| `test.mjs` | verifier tests: non-conformant modules that must be rejected |
| `test-abi.mjs` | ABI behaviour: errors, flags, cancellation, stepped/one-shot parity |
| `test-page.mjs` | drives the published page in a real browser |
| `page/` | the manual conversion site ([`page/README.md`](page/README.md)) |
| `build-page.sh` | assembles `site/` from the page and the built module |

## Build and test

```console
$ rustup target add wasm32-unknown-unknown
$ ./check.sh                      # build, verifier tests, conformance
$ ./check.sh /path/to/zelda3.sfc     # also parity against the Python, and ABI tests
```

`check.sh` uses whichever `cargo` is first on `PATH`; with both a distro rustc
and rustup installed, make sure `~/.cargo/bin` comes first or the wasm target
will appear to be missing.

The page:

```console
$ ./build-page.sh && (cd site && python3 -m http.server 8731)
$ node test-page.mjs                 # load-only checks, no ROM needed
$ node test-page.mjs /path/to/zelda3.sfc # full run, needs playwright
```

There is also a native binary (`cargo build --release`, then
`target/release/zelda3-restool-cli <rom> <out>`) running the identical code path
without a wasm runtime. It exists so the port can be diffed against the Python
directly; it is not a release artifact.

No timings are quoted yet: the scaffold does no work, so any number would be
meaningless. The size to compare against is what a browser must fetch, which
today is under 30 KB of module with no network access of its own, versus about
10 MB of Python runtime plus PyPI installs.
