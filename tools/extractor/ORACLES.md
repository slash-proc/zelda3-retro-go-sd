# Byte-exact oracles for the Zelda 3 asset pipeline

Measured on 2026-09-03, Python 3.13 (Pillow 11.3.0, PyYAML), Linux x86_64.
All runs were performed in a scratch copy of `tables/` + `other/` (the Python writes
intermediates into its own working directory). Commands are run **from the `tables/`
directory**; `zelda3_assets.dat` is written to the current working directory.

ROMs used:

| Tag | Path | Identified by tool as |
|---|---|---|
| US | `.../roms/Legend of Zelda, The - A Link to the Past (U) [!].smc` | (default) |
| FR | `.../zelda3 snes fra/Legend of Zelda, The - A Link to the Past (France).sfc` | `fr - "Legend of Zelda, The - A Link to the Past (France)"` |
| DE | `.../zelda3 snes deu/Legend of Zelda, The - A Link to the Past (Germany).sfc` | `de - "Legend of Zelda, The - A Link to the Past (Germany)"` |

## 1. Command sequences

Extraction from the US ROM must happen once; it produces the intermediates (including
`dialogue.txt`) that every compile consumes. Per `restool.py:23-28`, `--extract-dialogue`
short-circuits and exits after writing that language's dialogue + font, so it must be a
separate invocation with that language's ROM. `compile_resources.py:130-131` then requires
`dialogue_<lang>.txt` to already exist on disk when `--languages` is given.

```sh
# once, US ROM — produces all intermediates + dialogue.txt
python3 restool.py --extract-from-rom -r "$US"

# per extra language, that language's ROM — writes dialogue_XX.txt + font_XX.png, then exits
python3 restool.py --extract-dialogue -r "$FR"
python3 restool.py --extract-dialogue -r "$DE"

# build (compile only, no re-extract), against the US ROM
python3 restool.py -r "$US" --languages fr
python3 restool.py -r "$US" --languages de
python3 restool.py -r "$US" --languages fr,de
```

Notes:
- `--extract-dialogue` does **not** accept/need `--languages`; the language is auto-detected
  from the ROM header (`util.load_rom(rom, True)`).
- `--extract-from-rom` may be combined with `--languages` in one call; the compile phase is
  identical either way.

## 2. `zelda3_assets.dat` oracles

| Build | Size (bytes) | sha256 |
|---|---|---|
| a. US only (baseline) | 683,888 | `0fe2e4bd75d70f06fb9a74cd3a9cb336c838149b831b56e8792114a89292c793` |
| b. US + French (`--languages fr`) | 723,536 | `6c09b76eff3528b3b89c6abe0a032763f7a58df7556761e9f647a9f0d685a2ba` |
| c. US + German (`--languages de`) | 725,600 | `415bc15135259835f6b80bd36613a8896dbc8125aa65d4c85360e88b31849ed8` |
| d. US + FR + DE (`--languages fr,de`) | 765,256 | `2cb153aa5d21afc6114e6d2c2de2c1ee40bbce974c64b4ab8f589207a98bd38b` |

The published US baseline was **reproduced exactly** (size and hash both match).

**Language order is significant.** `--languages de,fr` yields a *different* file than
`--languages fr,de`:

| Build | Size | sha256 |
|---|---|---|
| US + DE + FR (`--languages de,fr`) | 765,256 | `b4ccc5de91b3310beaea5f6e5a67697f448a28f8f0e084cce3179ed0d692a90d` |

Same size, different bytes — the languages are packed in the order given on the command line,
and each gets an index `i` recorded in `kDialogueMap`. The port must preserve caller order.

## 3. Intermediates produced by the `--extract-dialogue` step

| File | Bytes | sha256 |
|---|---|---|
| `dialogue_fr.txt` | 71,470 | `4c4be0f7a21556078dc73a3673fc16160571b681ab9512afe9a12f9f0ce10264` |
| `font_fr.png` | 3,154 | `bdadc98181cb61799ca8592e6f8474f81e8f8828309ed2e5b24585e694a1391f` |
| `dialogue_de.txt` | 72,687 | `dc2cd9049bf19fb4420b55aa7199ed9e3b59323c0aeee4774611ed786126ac65` |
| `font_de.png` | 3,180 | `a59f8f41b269162f8cd8874a1a4a392c61b825a6a8647eaf2e344950f485c46b` |

Each `--extract-dialogue` run writes exactly these two files and nothing else (verified by a
before/after `find` diff of the whole tree). Both are byte-identical on re-runs.

For reference, the US equivalent comes from `--extract-from-rom`:

| File | Bytes | sha256 |
|---|---|---|
| `dialogue.txt` | 67,730 | `864de3794df372f8dcb6f7303cfd10fef62b2552b0cc068c6f49f245e591f978` |

All dialogue files have 397 non-empty lines (one entry per message), so the Rust port can hold
these as in-memory string tables keyed by language; the `.txt`/`.png` files are purely a
transport between the two Python invocations and need not exist on disk.

The PNG is not incidental: `sprite_sheets.decode_font()` writes `font_XX.png` and then asserts
`(data, W) == encode_font_from_png(lang)` (round-trip check), and the *compile* step reads the
font back with `Image.open(kFontTypes[lang][2]).tobytes()`. In memory the equivalent is the
raw 8bpp indexed bitmap of size `(128+15) x (17 * ft[1]/32)` plus the per-glyph width table
`W`, sourced from ROM addresses in `sprite_sheets.py:150-159` (`fr`: gfx `0xCC6E8`, 256 glyphs,
widths at `0x8CDEAF` x112; `de`: gfx `0xCC6E8`, 256 glyphs, widths at `0x8CDECF` x112).

## 4. What `--languages fr` actually changes in the output

`--print-assets-header` output is **byte-identical** between the US-only build and every
language build (verified by `diff` for `fr` and `fr,de`). No asset keys are added, removed, or
reordered; `kNumberOfAssets` stays **165**, and `kAssets_Sig` is unchanged (it is a sha256 over
the concatenated key names only — see `compile_resources.py:795`).

Only three asset *payloads* change, all produced by `print_dialogue()`:

| Asset key | US | +fr | +de | +fr,de |
|---|---|---|---|---|
| `kDialogue` | 37,233 | 72,658 | 74,722 | 110,151 |
| `kDialogueFont` | 4,201 | 8,415 | 8,415 | 12,629 |
| `kDialogueMap` | 11 | 22 | 22 | 33 |

Every other one of the 165 assets is byte-identical across all four builds (verified by parsing
the `.dat` directory and hashing each entry). Structure per `compile_resources.py:111-135`:

- `kDialogue` — packed array, one entry per language: `pack_arrays([dict_packed, dialogue_packed])`
  where the dictionary comes from `text_compression.encode_dictionary(lang)` and the body from
  `compress_dialogue(dialogue_filename(lang), lang)`.
- `kDialogueFont` — packed array, one entry per language: `pack_arrays([font_data, font_width])`
  from `sprite_sheets.encode_font_from_png(lang)`. Exactly 4,214 bytes per added language.
- `kDialogueMap` — packed array, one entry per language: `pack_arrays([lang_utf8_name, bytes([i, i, flags])])`,
  where `flags = uses_new_format(lang) | (2 if i != 0 else 0)`. Exactly 11 bytes per language
  for the two-letter codes `us`/`fr`/`de`.

So for the port: the multi-language path affects **only** the dialogue text encoder, the
dialogue dictionary encoder, and the font encoder. Everything else is US-ROM-derived and
language-invariant.

## 5. Timings (wall clock, warm page cache)

| Phase | Command | Time |
|---|---|---|
| Extract only | `--extract-from-rom --no-build -r $US` | 1.62 s |
| Compile only (US) | `-r $US` | 2.33 s |
| Extract + compile (US) | `--extract-from-rom -r $US` | 3.84 s / 3.90 s (two runs) |
| Dialogue extract (FR) | `--extract-dialogue -r $FR` | 0.08 s |
| Dialogue extract (DE) | `--extract-dialogue -r $DE` | 0.06 s |
| Compile `--languages fr` | | 2.34 s |
| Compile `--languages de` | | 2.33 s |
| Compile `--languages fr,de` | | 2.45 s |

Suggested `typicalRuntimeMs` for a full US build: **~3,900 ms** native Python. Split for a
progress bar: extract ≈ 42% of the work, compile ≈ 58%. Per-language dialogue extraction is
negligible (<100 ms each); adding two languages costs only ~120 ms of extra compile time.
A WASM build should be expected to be slower than these native numbers.

## 6. Determinism and diagnostics

**The Python output is fully deterministic.** The US baseline was built twice from scratch and
produced the identical sha256 `0fe2e4...c793` both times. The `--languages fr` build was also
run twice with an identical hash (`6c09b7...a2ba`), and both dialogue/font intermediates
re-hashed identically on a second `--extract-dialogue`. No timestamps, no dict-ordering
instability, and no PRNG were observed in the output path.

Warnings/errors observed:

- `tables.py:14: SyntaxWarning: invalid escape sequence '\ '` — emitted on stderr on every
  invocation, from a docstring/comment in `tables.py`. Cosmetic; does not affect output.
- No other warnings, no exceptions, and empty stderr (apart from the above) for all
  language builds.

Operational gotchas relevant to the port:

- Running `--languages fr` without having run `--extract-dialogue` on the FR ROM raises
  `dialogue_fr.txt not found. You need to extract it with --extract-dialogue using the ROM of
  that language.` (`compile_resources.py:130-131`).
- A compile requires the intermediates from a prior `--extract-from-rom`; a fresh checkout
  cannot compile directly.
- `--print-assets-header` writes the C header to **stdout** and still writes
  `zelda3_assets.dat` as a side effect — it is not a dry-run.
- Valid language codes (`text_compression.py:391-403`): `us, de, fr, fr-c, en, es, pl, pt,
  redux, nl, sv`. `us` is always index 0 and may not be passed in `--languages`.
- `dialogue_filename()` maps `fr-c` to `dialogue_fr_c.txt` (hyphen becomes underscore).
