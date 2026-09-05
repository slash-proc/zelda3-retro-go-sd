# Changelog

## [v0.2.0]

### Added

- A version picker. The page reads this site's own `dist/versions.json`,
  offers every version the mirror holds and defaults to the newest, so a
  release reaches users without redeploying the page. Switching version
  reloads the module, its inputs and its accepted hashes, and discards any
  file already chosen: a file one version accepts another may refuse, and
  carrying it over would start a run on input this version never approved.
  Pre-releases are hidden until asked for, and each version shows the
  firmware ABI it needs, which is the only place a user learns it.
- A single download containing the whole install: `Zelda 3.bin`,
  `zelda3.ro` and the `zelda3_assets.dat` the run just produced, as
  `zelda3-<tag>-gwrg.zip`. The converted file on its own was only half of
  what somebody needs, and the manifest already named and hashed the rest.
  Published files are fetched, checked against the size and hash the
  manifest declares, and a mismatch refuses the zip rather than shipping a
  broken install.
- `zip.mjs`, a dependency-free zip writer. Entries are deflated where that
  helps and stored where it does not; a browser without `CompressionStream`
  stores everything and still produces a valid archive. Names keep their
  spaces, which this project needs.
- `test-i18n.mjs`, which demands every string the page asks for from every
  locale it offers. A renamed key used to be invisible: the page still
  loads and the control is simply blank for that language.

### Changed

- The page is now byte-identical to the one smw ships, and carries no fact
  about this particular game. The list of accepted translations comes from
  the variants' own labels in the manifest, rather than being inferred from
  their ids with `Intl.DisplayNames` and a carve-out for the one id that was
  not a language code.
- `outputs[].maxBytes` is enforced. The manifest states a ceiling per output
  as well as one for the whole run, and only the second was being applied.
- `config.json` points at the version index rather than one manifest.
  `MANIFEST_URL` still pins a build to a single manifest, which is what an
  offline bundle needs.

## [v0.1.0]
Published under the GWRG distribution model, with the asset extractor.

### Added

- The Zelda 3 asset extractor, vendored into `tools/extractor/`: a
  zero-import WASM module that turns your own ROM into `zelda3_assets.dat`,
  with optional translated ROMs adding languages to the same file.
- Publishing under the [GWRG distribution
  spec](https://github.com/slash-proc/gwrg-dist-spec): a `manifest.json`
  declaring both device files and the extractor, an offline bundle, and a
  GitHub Pages mirror of `dist/` a web installer can read.
- A conversion page at the site root, so a ROM can be converted in the
  browser without installing anything. It reads the published manifest
  rather than a copy of its own, which makes it a test of the distribution
  model and not merely a tool beside it.

### Changed

- Whether an unrecognised ROM may be used is now decided before the run,
  from the manifest's `inputs[].strict`. Both of this project's inputs are
  strict: everything is read out of the US cartridge by address, and a
  translation that matches no known hash cannot be identified at all.
- The extractor no longer refuses a ROM it does not recognise. It says so
  through `warnings` and carries on, leaving admission to the host, which
  is the party holding the file, the hashes and the user.

### Fixed

- Nothing

### Install

Use the conversion page, or unzip the release archive onto the SD card root
and supply `zelda3_assets.dat` yourself. The archive contains:

- `/homebrews/Zelda 3.bin` — GWHB homebrew
- `/homebrews/zelda3.ro` — bulk `.rodata` sidecar (required)

Optional coverflow override: `/covers/homebrew/Zelda 3.img` (JPEG ≤186×100,
≤10 KiB). Firmware ABI must match `SDK_VERSION` in this repository.

## [v0.0.1]
Initial version

### Added

- Nothing

### Changed

- Nothing

### Fixed

- Nothing

### Install

Unzip the release archive onto the SD card root. It already contains:

- `/homebrews/Zelda 3.bin` — GWHB homebrew
- `/homebrews/zelda3.ro` — bulk `.rodata` sidecar (required)

You still need the game assets (not shipped in the zip — extract from your own
ROM):

```bash
# Place a Zelda: A Link to the Past ROM as external/zelda3/zelda3.sfc
# (supported hashes: see external/zelda3/README.md)
make -C external/zelda3 tables/zelda3_assets.dat
cp external/zelda3/tables/zelda3_assets.dat /path/to/sd/homebrews/zelda3_assets.dat
```

Optional coverflow override: `/covers/homebrew/Zelda 3.img` (JPEG ≤186×100,
≤10 KiB). Firmware ABI must match `SDK_VERSION` in this repository.
