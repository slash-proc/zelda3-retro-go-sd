# Zelda 3 for Game & Watch Retro-Go SD

Standalone **GWHB homebrew** port of [zelda3](https://github.com/sylverb/zelda3)
(`sd` branch), packaged with this repo's Retro-Go SD core SDK.

## Build

```bash
git submodule update --init --recursive
make                 # → Zelda 3.bin + zelda3.ro
make docker          # same, inside sylverb/retro-go-sd-builder
make host            # → zelda3_host (SDL2 desktop preview)
```

Requires `arm-none-eabi-gcc` (hard-float `fpv5-d16`) and
`pip install -r requirements.txt` (Pillow for the cover). Host also needs
SDL2 (`brew install sdl2` / `libsdl2-dev`).

## Host preview

```bash
mkdir -p homebrews
cp /path/to/zelda3_assets.dat homebrews/
# or: HOST_SD=/path/to/sdcard  (expects $HOST_SD/homebrews/zelda3_assets.dat)
./zelda3_host
```

`HOST_OFW_MARIO=1` selects the Mario OFW face-button layout (default = Zelda).
The host build links `.rodata` normally (no `zelda3.ro` patch).

## Install on SD

1. Copy `Zelda 3.bin` to `/homebrews/Zelda 3.bin`.
2. Copy `zelda3.ro` to `/homebrews/zelda3.ro` (bulk `.rodata` sidecar).
3. Build and copy the asset sidecar:

```bash
# Place a Zelda: A Link to the Past ROM as external/zelda3/zelda3.sfc
# (see external/zelda3/README.md for supported hashes)
make -C external/zelda3 tables/zelda3_assets.dat
cp external/zelda3/tables/zelda3_assets.dat /path/to/sd/homebrews/zelda3_assets.dat
```

The homebrew loads `/homebrews/zelda3.ro` and `/homebrews/zelda3_assets.dat`
via `odroid_overlay_cache_file_in_flash`. An optional cover override can live
at `/covers/homebrew/Zelda 3.img`.

## Layout

| Path | Role |
|------|------|
| `external/zelda3/` | Engine submodule (HEADLESS build) |
| `src/main_zelda3.c` | G&W platform (LCD, audio, input, save/SRAM, rodata patch) |
| `src/zelda3_core.ld` | Custom link: game `.rodata` → sidecar VMA |
| `src/zelda3_borders.h` | Side border 1bpp art |
| `sdk/` | Vendored ABI bridge, headers, packer |

## Notes

- Default build is **30 FPS** (`LIMIT_30FPS=1`) with audio rendered for two
  frames per display frame (same as the in-firmware Zelda3 homebrew).
- `FEATURES=128` enables skip-intro-on-keypress.
- Battery HUD uses `odroid_input_read_battery` (ABI) when
  `BATTERY_INDICATOR=1`.
- Button bindings follow the Mario vs Zelda OFW face layout
  (`get_ofw_is_mario`).
