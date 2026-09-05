# Changelog

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
