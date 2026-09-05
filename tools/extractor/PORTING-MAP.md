# Porting map: `tables/` Python -> Rust/wasm

Contract document for the Rust port of the Zelda 3 asset pipeline. Everything
below was read out of the code and, where stated, measured by running it.

## 0. The oracle, reproduced

```
cp tables/ + other/ to a scratch dir, drop the US ROM in as tables/zelda3.sfc
python3 restool.py --extract-from-rom -r zelda3.sfc
-> zelda3_assets.dat   683,888 bytes
   sha256 0fe2e4bd75d70f06fb9a74cd3a9cb336c838149b831b56e8792114a89292c793
```

Reproduced during the writing of this document, so the numbers here are
measured, not inferred. The ROM used is
`Legend of Zelda, The - A Link to the Past (U) [!].smc`
(sha1 `6D4F10A8B10E10DBE624CB23CF03B88BB8252973`, `util.py:15`).

The build contains **165 assets**. Every size and offset quoted below comes
from that run.

Wall-clock for the whole Python pipeline is about **4 seconds** (the two slow
steps are `print_all_dungeon_rooms` at 1.1 s and `print_dungeon_rooms` at
1.9 s, both dominated by YAML). The port will be far quicker than that. Stages
are still worth doing — the ABI requires them — but do not design around an
assumption that this is a minute-long job.

---

## 1. The output format

Produced by `compile_resources.write_assets_to_file` (`compile_resources.py:771-812`).
Everything is little-endian.

### Construction

```python
# compile_resources.py:786-793
for i, (k, (tp, data)) in enumerate(assets.items()):
  key_sig += k.encode('utf8') + b'\0'      # NUL-terminated, NUL-joined, trailing NUL
  all_data.append(data)

assets_sig = b'Zelda3_v0     \n\0' + hashlib.sha256(key_sig).digest()   # :795
hdr = assets_sig + b'\x00' * 32 + struct.pack('II', len(all_data), len(key_sig))  # :800
encoded_sizes = array.array('I', [len(i) for i in all_data])            # :802
file_data = hdr + encoded_sizes + key_sig                               # :804
for v in all_data:                                                      # :806-809
  while len(file_data) & 3: file_data += b'\0'
  file_data += v
```

Note `assets.items()` — a plain `dict`, so **insertion order is the file
order**. That order is section 2, and it is load-bearing twice: it fixes the
size array and it fixes `key_sig`, which is hashed into the magic.

### Layout

```
offset  size    contents
------  ------  --------------------------------------------------------------
0       16      magic: "Zelda3_v0     \n\0"
                  ('Z','e','l','d','a','3','_','v','0', 5 x 0x20, 0x0A, 0x00)
16      32      SHA-256 of key_sig (the NUL-terminated key-name blob, below)
48      32      32 zero bytes  (reserved)
80      4       u32 asset count            = 165
84      4       u32 key_sig length         = 3252
88      4*N     u32 size[i] for each asset (N = 165 -> 660 bytes)
748     3252    key_sig: each key name UTF-8 + b'\0', in asset order
4000    ...     asset payloads, each preceded by 0-3 NUL bytes so that the
                payload starts on a 4-byte boundary
```

For the US build: `key_sig` is 3252 bytes and its SHA-256 is
`1baee92d4aaefc32311b99c51b2bd8c58465ada9246c0f9bb0a93983ae6533cf`.
The key blob ends at 4000, which is already aligned, so asset 0 begins at
offset 4000. Total 683,888.

Two details that are easy to get wrong:

- The padding rule is *before* each payload, driven by the length of the file
  so far — not by the payload's own size, and there is **no trailing padding**
  after the last asset. Header + sizes + key blob are not separately padded;
  the first padding decision is made after the key blob.
- `array.array('I')` is the platform's `unsigned int`. On every platform this
  ships to it is 4 bytes little-endian. Emit u32 LE.

### Element types

`assets[name] = (type_tag, bytes)` (`compile_resources.py:22-40`). The tag
(`uint8`/`int8`/`uint16`/`int16`/`packed`) never reaches the .dat — it only
drives the generated C header (`--print-assets-header`). The bytes are
`array.array('B'|'b'|'H'|'h')` of the value list, i.e. native LE, truncating
to the width. `add_asset_int8` on a value outside -128..127 would raise in
Python; the Rust equivalent should assert the same rather than wrapping
silently.

### `pack_arrays` (`compile_resources.py:89-99`)

The `packed` encoding, used for variable-length arrays-of-arrays:

```
if total_payload < 65536 and count <= 8192:
    u16 off[0..count-2]      # cumulative offsets, NOT including entry 0 (=0)
    payload bytes (all entries concatenated)
    u16 (count - 1)          # trailer
else:
    u32 off[0..count-2]
    payload bytes
    u16 (8192 + count - 1)   # trailer, high bit set to signal 32-bit offsets
```

`len(arr) == 0` returns `b''`. The `offs` used for the size test is the sum of
all entries *except the last*, not the total — reproduce it literally.
`pack_arrays` nests: `kDialogue` is `pack_arrays` of per-language
`pack_arrays([dict_packed, dialogue_packed])`.

---

## 2. The ordered asset list

`restool.py:44-46` -> `compile_resources.main(args)` (`:814`) ->
`print_all(args)` (`:756-769`), which calls, in this order:

```
print_sound_banks, print_dungeon_rooms, print_enemy_damage_data,
print_link_graphics, print_dungeon_sprites, print_map32_to_map16,
print_images, print_misc, print_dialogue, print_dungeon_map,
print_tilemaps, print_overworld, print_overworld_tables
```

Inside `print_overworld_tables`, assets 107-162 are emitted by
`OutArrays.write()` (`compile_resources.py:216-229`) in the order the arrays
were **registered** with `A.add(...)`, not the order they were filled — the
registration calls are scattered through `:233-425` and interleaved with the
loops that populate earlier arrays. Follow the `A.add` call order exactly.

`elems` is the element count for typed arrays; `bytes` is the payload length in
the .dat.

| # | key | type | elems | bytes | added by (compile_resources.py) | source |
|---|-----|------|-------|-------|--------------------------------|--------|
| 0 | `kSoundBank_intro` | uint8 | 50066 | 50066 | `print_sound_banks` (:751) | compile_music.print_song("intro") — sound_intro.txt + sfx.txt + music_info.yaml + sound/*.brr |
| 1 | `kSoundBank_indoor` | uint8 | 12756 | 12756 | `print_sound_banks` (:751) | compile_music.print_song("indoor") — sound_indoor.txt |
| 2 | `kSoundBank_ending` | uint8 | 8354 | 8354 | `print_sound_banks` (:751) | compile_music.print_song("ending") — sound_ending.txt |
| 3 | `kDungeonRoom` | uint8 | 50381 | 50381 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 4 | `kDungeonRoomOffs` | uint16 | 320 | 640 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 5 | `kDungeonRoomDoorOffs` | uint16 | 320 | 640 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 6 | `kDungeonRoomHeaders` | uint8 | 3104 | 3104 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 7 | `kDungeonRoomHeadersOffs` | uint16 | 320 | 640 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 8 | `kDungeonRoomChests` | uint8 | 504 | 504 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 9 | `kDungeonRoomTeleMsg` | uint16 | 320 | 640 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 10 | `kDungeonPitsHurtPlayer` | uint16 | 57 | 114 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 11 | `kEntranceData_rooms` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 12 | `kEntranceData_relativeCoords` | uint8 | 1064 | 1064 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 13 | `kEntranceData_scrollX` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 14 | `kEntranceData_scrollY` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 15 | `kEntranceData_playerX` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 16 | `kEntranceData_playerY` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 17 | `kEntranceData_cameraX` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 18 | `kEntranceData_cameraY` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 19 | `kEntranceData_blockset` | uint8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 20 | `kEntranceData_floor` | int8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 21 | `kEntranceData_palace` | int8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 22 | `kEntranceData_doorwayOrientation` | uint8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 23 | `kEntranceData_startingBg` | uint8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 24 | `kEntranceData_quadrant1` | uint8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 25 | `kEntranceData_quadrant2` | uint8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 26 | `kEntranceData_doorSettings` | uint16 | 133 | 266 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 27 | `kEntranceData_musicTrack` | uint8 | 133 | 133 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 28 | `kStartingPoint_rooms` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 29 | `kStartingPoint_relativeCoords` | uint8 | 56 | 56 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 30 | `kStartingPoint_scrollX` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 31 | `kStartingPoint_scrollY` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 32 | `kStartingPoint_playerX` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 33 | `kStartingPoint_playerY` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 34 | `kStartingPoint_cameraX` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 35 | `kStartingPoint_cameraY` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 36 | `kStartingPoint_blockset` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 37 | `kStartingPoint_floor` | int8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 38 | `kStartingPoint_palace` | int8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 39 | `kStartingPoint_doorwayOrientation` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 40 | `kStartingPoint_startingBg` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 41 | `kStartingPoint_quadrant1` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 42 | `kStartingPoint_quadrant2` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 43 | `kStartingPoint_doorSettings` | uint16 | 7 | 14 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 44 | `kStartingPoint_entrance` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 45 | `kStartingPoint_musicTrack` | uint8 | 7 | 7 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 46 | `kDungeonRoomDefault` | uint8 | 646 | 646 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 47 | `kDungeonRoomDefaultOffs` | uint16 | 8 | 16 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 48 | `kDungeonRoomOverlay` | uint8 | 566 | 566 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 49 | `kDungeonRoomOverlayOffs` | uint16 | 19 | 38 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml (+default_rooms/overlay_rooms.yaml) |
| 50 | `kDungeonSecrets` | uint8 | 2887 | 2887 | `print_dungeon_rooms` (:525) | dungeon/dungeon-*.yaml Secrets |
| 51 | `kDungAttrsForTile_Offs` | uint16 | 21 | 42 | `print_dungeon_rooms` (:525) | ROM 0x8e9000, 21 words |
| 52 | `kDungAttrsForTile` | uint8 | 1024 | 1024 | `print_dungeon_rooms` (:525) | ROM 0x8e902a, 1024 |
| 53 | `kMovableBlockDataInit` | uint16 | 198 | 396 | `print_dungeon_rooms` (:525) | ROM 0x84f1de, 198 words |
| 54 | `kTorchDataInit` | uint16 | 144 | 288 | `print_dungeon_rooms` (:525) | ROM 0x84F36A, 144 words |
| 55 | `kTorchDataJunk` | uint16 | 48 | 96 | `print_dungeon_rooms` (:525) | ROM 0x84F48a, 48 words |
| 56 | `kEnemyDamageData` | uint8 | 1728 | 1728 | `print_enemy_damage_data` (:714) | util.decomp(0x83e800, be=True) decompressed |
| 57 | `kLinkGraphics` | uint8 | 28672 | 28672 | `print_link_graphics` (:732) | linksprite.png -> 4bpp; == ROM 0x108000, 0x7000 |
| 58 | `kDungeonSprites` | uint8 | 4965 | 4965 | `print_dungeon_sprites` (:460) | dungeon/dungeon-*.yaml Sprites |
| 59 | `kDungeonSpriteOffs` | uint16 | 320 | 640 | `print_dungeon_sprites` (:460) | dungeon/dungeon-*.yaml Sprites |
| 60 | `kMap32ToMap16_0` | uint8 | 13308 | 13308 | `print_map32_to_map16` (:42) | map32_to_map16.txt (ROM 0x838000/0x83b400/0x848000/0x84b400) |
| 61 | `kMap32ToMap16_1` | uint8 | 13308 | 13308 | `print_map32_to_map16` (:42) | map32_to_map16.txt (ROM 0x838000/0x83b400/0x848000/0x84b400) |
| 62 | `kMap32ToMap16_2` | uint8 | 13308 | 13308 | `print_map32_to_map16` (:42) | map32_to_map16.txt (ROM 0x838000/0x83b400/0x848000/0x84b400) |
| 63 | `kMap32ToMap16_3` | uint8 | 13308 | 13308 | `print_map32_to_map16` (:42) | map32_to_map16.txt (ROM 0x838000/0x83b400/0x848000/0x84b400) |
| 64 | `kSprGfx` | packed | — | 133479 | `print_images` (:101) | ROM kCompSpritePtrs[0..107]; i<12 raw 0x600 else decomp to find comp_len, store COMPRESSED bytes |
| 65 | `kBgGfx` | packed | — | 119899 | `print_images` (:101) | ROM kCompBgPtrs[0..114] (115 entries), decomp for comp_len, store COMPRESSED bytes |
| 66 | `kOverworldMapGfx` | uint8 | 16384 | 16384 | `print_misc` (:148) | ROM 0x18c000, 0x4000 |
| 67 | `kLightOverworldTilemap` | uint8 | 4096 | 4096 | `print_misc` (:148) | ROM 0xac727, 4096 |
| 68 | `kDarkOverworldTilemap` | uint8 | 1024 | 1024 | `print_misc` (:148) | ROM 0xaD727, 1024 |
| 69 | `kPredefinedTileData` | uint16 | 6438 | 12876 | `print_misc` (:148) | ROM 0x9B52, 6438 words |
| 70 | `kMap16ToMap8` | uint16 | 15008 | 30016 | `print_misc` (:148) | ROM 0x8f8000, 3752*4 words |
| 71 | `kGeneratedWishPondItem` | uint8 | 256 | 256 | `print_misc` (:148) | ROM 0x888450, 256 |
| 72 | `kGeneratedBombosArr` | uint8 | 256 | 256 | `print_misc` (:148) | ROM 0x8890FC, 256 |
| 73 | `kGeneratedEndSequence15` | uint8 | 256 | 256 | `print_misc` (:148) | ROM 0x8ead25, 256 |
| 74 | `kEnding_Credits_Text` | uint8 | 1989 | 1989 | `print_misc` (:148) | ROM 0x8EB178, 1989 |
| 75 | `kEnding_Credits_Offs` | uint16 | 394 | 788 | `print_misc` (:148) | ROM 0x8EB93d, 394 words |
| 76 | `kEnding_MapData` | uint16 | 160 | 320 | `print_misc` (:148) | ROM 0x8EB038, 160 words |
| 77 | `kEnding0_Offs` | uint16 | 17 | 34 | `print_misc` (:148) | ROM 0x8EC2E1, 17 words |
| 78 | `kEnding0_Data` | uint8 | 917 | 917 | `print_misc` (:148) | ROM 0x8EBF4C, 917 |
| 79 | `kPalette_DungBgMain` | uint16 | 1800 | 3600 | `print_misc` (:148) | ROM 0x9BD734, 1800 words |
| 80 | `kPalette_MainSpr` | uint16 | 120 | 240 | `print_misc` (:148) | ROM 0x9BD218, 120 words |
| 81 | `kPalette_ArmorAndGloves` | uint16 | 75 | 150 | `print_misc` (:148) | ROM 0x9BD308, 75 words (override_armor_palette is None) |
| 82 | `kPalette_Sword` | uint16 | 12 | 24 | `print_misc` (:148) | ROM 0x9BD630, 12 words |
| 83 | `kPalette_Shield` | uint16 | 12 | 24 | `print_misc` (:148) | ROM 0x9BD648, 12 words |
| 84 | `kPalette_SpriteAux3` | uint16 | 84 | 168 | `print_misc` (:148) | ROM 0x9BD39E, 84 words |
| 85 | `kPalette_MiscSprite_Indoors` | uint16 | 77 | 154 | `print_misc` (:148) | ROM 0x9BD446, 77 words |
| 86 | `kPalette_SpriteAux1` | uint16 | 168 | 336 | `print_misc` (:148) | ROM 0x9BD4E0, 168 words |
| 87 | `kPalette_OverworldBgMain` | uint16 | 210 | 420 | `print_misc` (:148) | ROM 0x9BE6C8, 210 words |
| 88 | `kPalette_OverworldBgAux12` | uint16 | 420 | 840 | `print_misc` (:148) | ROM 0x9BE86C, 420 words |
| 89 | `kPalette_OverworldBgAux3` | uint16 | 98 | 196 | `print_misc` (:148) | ROM 0x9BE604, 98 words |
| 90 | `kPalette_PalaceMapBg` | uint16 | 96 | 192 | `print_misc` (:148) | ROM 0x9BE544, 96 words |
| 91 | `kPalette_PalaceMapSpr` | uint16 | 21 | 42 | `print_misc` (:148) | ROM 0x9BD70A, 21 words |
| 92 | `kHudPalData` | uint16 | 64 | 128 | `print_misc` (:148) | ROM 0x9BD660, 64 words |
| 93 | `kOverworldMapPaletteData` | uint16 | 256 | 512 | `print_misc` (:148) | ROM 0x8ADB27, 256 words |
| 94 | `kDialogue` | packed | — | 37233 | `print_dialogue` (:121) | pack(pack(dict), pack(compressed dialogue.txt)) |
| 95 | `kDialogueFont` | packed | — | 4201 | `print_dialogue` (:121) | pack(font.png->4096 bytes, widths) == ROM 0x8e8000+4096 / 0x8ECADF+99 |
| 96 | `kDialogueMap` | packed | — | 11 | `print_dialogue` (:121) | pack("us", [0,0,flags]) |
| 97 | `kDungMap_FloorLayout` | packed | — | 1503 | `print_dungeon_map` (:440) | ROM 0xa0000+word(0x8AF605+i*2), 14 fixed sizes |
| 98 | `kDungMap_Tiles` | packed | — | 214 | `print_dungeon_map` (:440) | ROM 0xa0000+word(0x8AFBE4+i*2), len = size - count(0x0f) |
| 99 | `kBgTilemap_0` | uint8 | 1115 | 1115 | `print_tilemaps` (:718) | ROM 0xcdd6d, length from decode_one() scan |
| 100 | `kBgTilemap_1` | uint8 | 1467 | 1467 | `print_tilemaps` (:718) | ROM 0xce7bf, length from decode_one() scan |
| 101 | `kBgTilemap_2` | uint8 | 177 | 177 | `print_tilemaps` (:718) | ROM 0xce2a8, length from decode_one() scan |
| 102 | `kBgTilemap_3` | uint8 | 81 | 81 | `print_tilemaps` (:718) | ROM 0xce63c, length from decode_one() scan |
| 103 | `kBgTilemap_4` | uint8 | 233 | 233 | `print_tilemaps` (:718) | ROM 0xce456, length from decode_one() scan |
| 104 | `kBgTilemap_5` | uint8 | 661 | 661 | `print_tilemaps` (:718) | ROM 0xeda9c, length from decode_one() scan |
| 105 | `kOverworld_Hibytes_Comp` | packed | — | 25696 | `print_overworld` (:191) | ROM ptr table 0x82F94D, 160 entries, compressed bytes |
| 106 | `kOverworld_Lobytes_Comp` | packed | — | 35667 | `print_overworld` (:191) | ROM ptr table 0x82FB2D, 160 entries, compressed bytes |
| 107 | `kOverworldMapIsSmall` | uint8 | 192 | 192 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 108 | `kOverworldAuxTileThemeIndexes` | uint8 | 128 | 128 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 109 | `kOverworldBgPalettes` | uint8 | 136 | 136 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 110 | `kOverworld_SignText` | uint16 | 128 | 256 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 111 | `kOwMusicSets` | uint8 | 256 | 256 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 112 | `kOwMusicSets2` | uint8 | 96 | 96 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 113 | `kBirdTravel_ScreenIndex` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 114 | `kBirdTravel_Map16LoadSrcOff` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 115 | `kBirdTravel_ScrollX` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 116 | `kBirdTravel_ScrollY` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 117 | `kBirdTravel_LinkXCoord` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 118 | `kBirdTravel_LinkYCoord` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 119 | `kBirdTravel_CameraXScroll` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 120 | `kBirdTravel_CameraYScroll` | uint16 | 17 | 34 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 121 | `kBirdTravel_Unk1` | int8 | 17 | 17 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 122 | `kBirdTravel_Unk3` | int8 | 17 | 17 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 123 | `kWhirlpoolAreas` | uint16 | 8 | 16 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 124 | `kOverworld_Entrance_Area` | uint16 | 129 | 258 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 125 | `kOverworld_Entrance_Pos` | uint16 | 129 | 258 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 126 | `kOverworld_Entrance_Id` | uint8 | 129 | 129 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 127 | `kFallHole_Area` | uint16 | 19 | 38 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 128 | `kFallHole_Pos` | uint16 | 19 | 38 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 129 | `kFallHole_Entrances` | uint8 | 19 | 19 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 130 | `kExitData_ScreenIndex` | uint8 | 79 | 79 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 131 | `kExitDataRooms` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 132 | `kExitData_Map16LoadSrcOff` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 133 | `kExitData_ScrollX` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 134 | `kExitData_ScrollY` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 135 | `kExitData_XCoord` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 136 | `kExitData_YCoord` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 137 | `kExitData_CameraXScroll` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 138 | `kExitData_CameraYScroll` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 139 | `kExitData_NormalDoor` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 140 | `kExitData_FancyDoor` | uint16 | 79 | 158 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 141 | `kExitData_Unk1` | int8 | 79 | 79 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 142 | `kExitData_Unk3` | int8 | 79 | 79 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 143 | `kSpExit_Top` | uint16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 144 | `kSpExit_Bottom` | uint16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 145 | `kSpExit_Left` | uint16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 146 | `kSpExit_Right` | uint16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 147 | `kSpExit_Tab4` | int16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 148 | `kSpExit_Tab5` | int16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 149 | `kSpExit_Tab6` | int16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 150 | `kSpExit_Tab7` | int16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 151 | `kSpExit_LeftEdgeOfMap` | uint16 | 16 | 32 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 152 | `kSpExit_Dir` | uint8 | 16 | 16 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 153 | `kSpExit_SprGfx` | uint8 | 16 | 16 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 154 | `kSpExit_AuxGfx` | uint8 | 16 | 16 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 155 | `kSpExit_PalBg` | uint8 | 16 | 16 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 156 | `kSpExit_PalSpr` | uint8 | 16 | 16 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 157 | `kOverworldSecrets_Offs` | uint16 | 128 | 256 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 158 | `kOverworldSecrets` | uint8 | 1187 | 1187 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 159 | `kOverworldSpriteOffs` | uint16 | 432 | 864 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 160 | `kOverworldSprites` | uint8 | 2797 | 2797 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 161 | `kOverworldSpriteGfx` | uint8 | 256 | 256 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 162 | `kOverworldSpritePalettes` | uint8 | 256 | 256 | `print_overworld_tables` (:231) | overworld/overworld-*.yaml |
| 163 | `kMap8DataToTileAttr` | uint8 | 512 | 512 | `print_overworld_tables` (:231) | ROM 0x8E9459, 512 |
| 164 | `kSomeTileAttr` | uint8 | 3824 | 3824 | `print_overworld_tables` (:231) | ROM 0x9bf110, 3824 |

### Notes on individual entries

- 0-2: `print_song` also runs `compare_with_orig` (`compile_music.py:386`)
  which diffs the serialised 64 KB SPC image against `sound/<song>.spc` and
  raises on mismatch. It is a self-check, not an input to the output.
- 57 `kLinkGraphics`: verified equal to `ROM.get_bytes(0x108000, 0x7000)`.
  The PNG round-trip is an identity here; see 3.3.
- 64/65: note these store the **compressed** ROM bytes. `util.decomp` is run
  only to discover the compressed length (`return_length=True`).
  `kSprGfx` has 108 entries, `kBgGfx` has `len(kCompBgPtrs)` = 115.
- 81 `kPalette_ArmorAndGloves`: `sprite_sheets.override_armor_palette` is
  `None` (`sprite_sheets.py:9`), so this is the plain ROM read. The commented-
  out override at `:10-14` is not a code path.
- 94-96: with no `--languages`, `languages == ['us']` — one entry each.
  `kDialogueMap` is `pack_arrays([pack_arrays([b'us', bytes([0,0,flags])])])`,
  `flags = uses_new_format('us') = False = 0`.
- 99-104 `kBgTilemap_*`: the length comes from `decode_one` (`:720-728`)
  walking the tilemap opcode stream until a byte with bit 7 set, then `+1`.

---

## 3. The extract -> compile data flow

`restool.py:34-36` runs `extract_resources.main()` (`:529-536`), which writes
files; `compile_resources` then reads them. In one process none of this needs
to exist. Below, for each file: what writes it, what reads it, the in-memory
structure to pass instead, and whether the serialisation loses or normalises
anything.

### 3.1 `overworld/overworld-<i>.yaml` — 160 files, one per area head

- Written `extract_resources.print_overworld_area` (`:145-233`),
  `yaml.dump(..., default_flow_style=None, sort_keys=False)` at `:232`.
- Read `compile_resources.load_overworld_yaml` (`:187-189`), consumed by
  `print_overworld_tables` (`:231-437`).
- **Pass instead:** the `y` dict as a struct:
  `Header{name, size:"small"|"big", gfx:i32, palette:i32, sign_text:i32,
  music:{tag->name}, ambient:{tag->name}}`, `Travel[]`, `Entrances[]`,
  `Holes[]` (key absent when empty — `compile_resources.py:325` tests
  `'Holes' not in y`), `Exits[]`, `Items[]`, and either
  `Sprites.Beginning`/`Sprites.FirstPart`/`Sprites.SecondPart` (area < 64) or
  `Sprites` (64..143). Areas >= 144 have no sprite key at all and
  `print_overworld_tables` never asks for one (`do_sprite_range` ranges stop at
  144).
- **Lossless.** Only ints, short ASCII strings and lists of those. Insertion
  order is preserved by `sort_keys=False`, and the compile side addresses
  everything by key, so map ordering does not matter. **List order does
  matter** — `Exits`, `Entrances`, `Items`, `Travel`, `Holes`, `sprites` are
  all consumed positionally or appended in order.
- The `name` fields (`kAreaNames`) are written and never read back. The port
  does not need `tables.kAreaNames` at all.
- Which areas exist is decided twice, identically: `extract` at `:239`
  (`area_heads[i&63] == (i&63)` or `i>=128`), `compile` at
  `is_area_head` (`:206-207`). Keep one predicate.

### 3.2 `dungeon/dungeon-<i>.yaml` (320), `dungeon/default_rooms.yaml`, `dungeon/overlay_rooms.yaml`

- Written `extract_resources.print_room`/`print_all_dungeon_rooms` (`:377-487`),
  `print_default_rooms` (`:489-500`), `print_overlay_rooms` (`:502-513`).
- Read `compile_resources.load_dungeon_yaml` (`:456-458`) by
  `print_dungeon_sprites` (`:460`), `print_dungeon_secrets` (`:496`),
  `print_dungeon_rooms` (`:525`); the default/overlay files at `:697,706`.
- **Pass instead:** `Room{Header{...}, Sprites:[[x,y,"upper"|"lower",name,
  (optional "drop_key"|"drop_big_key")]], Secrets:[[x,y,name]],
  Chests:[int | "N!"], Entrances:[..], StartingPoints:[..] (key optional),
  Layer1/2/3:[{x,y,s?,n}], Layer1.doors/Layer2.doors/Layer3.doors (optional)}`.
- **Lossless**, with three traps:
  - `Chests` is a *heterogeneous* list: `int` for a normal chest, `str`
    ending in `!` for a big chest (`extract:452-459`, `compile:597-602`). In
    Rust use an enum, and do not let YAML's "1!" quoting fool you.
  - `Layer*.doors` keys are **absent** when there are no doors on that layer
    (`extract:472,476,480`); `print_dungeon_rooms` distinguishes
    `y.get('Layer1.doors')` returning `None` (write no `0xf0 0xff` marker,
    `door_offset` stays `None`) from `[]` — and for layer 3 it forces
    `y.get('Layer3.doors') or []`, i.e. an empty list, which *does* emit the
    marker (`compile_resources.py:657`). That asymmetry is deliberate; keep it.
  - Object `s` is the string `"W*H"` (`extract:265`), re-parsed with
    `int(o['s'][0])`/`int(o['s'][2])` (`compile:530-531`). Only type-0 objects
    carry it. Store `(w,h)` and skip the string.
- Sprite names carry an embedded subtype: extract inserts `".%d" % subtype`
  before the `-` (`extract:434-436`), compile parses it back out
  (`compile:475-480`). Keep the name-as-key design or replace it with
  `(index, subtype)` — but if you replace it, mind that `kSpriteNames` has
  entries with no `-` (e.g. `'02'`, `'10'`) and the parse is guarded by
  `len(name) > 2 and name[2] == '.'`.
- `default_rooms.yaml` / `overlay_rooms.yaml` are keyed `Default0..7` /
  `Overlay0..18` and read back by index (`compile:698,707`) — a `Vec` is fine.

### 3.3 `linksprite.png`

- Written `sprite_sheets.decode_link_sprites` (`:109-111`): mode-`P` PNG,
  128x448, 16-entry palette.
- Read `compile_resources.print_link_graphics` (`:732`), `Image.open(...).tobytes()`
  -> one byte of palette index per pixel, then re-packed to 4bpp.
- **Pass instead:** the `bytearray` from `decode_4bit_tileset_link()`
  (`sprite_sheets.py:74-87`), 128*448 palette indices.
- **Lossless, and in fact an identity**: measured, `kLinkGraphics` ==
  `ROM.get_bytes(0x108000, 0x7000)` byte for byte. The decode-to-indices,
  PNG round trip, and re-encode cancel out exactly. The port may either copy
  the ROM bytes directly or reproduce the transform; **prefer reproducing the
  transform** so the equality stays a checkable property rather than a silent
  assumption, but there is no PNG codec needed either way.

### 3.4 `font.png` (and `font_<lang>.png`)

- Written `sprite_sheets.decode_font` (`:162-199`), mode-`P`, 143x136.
- Read `sprite_sheets.encode_font_from_png` (`:201-226`), called from
  `compile_resources.print_dialogue` (`:135`).
- **Pass instead:** `(data, W)` where `data = ROM.get_bytes(0x8e8000, 256*16)`
  and `W = ROM.get_bytes(0x8ECADF, 99)` (`kFontTypes['us']`,
  `sprite_sheets.py:149`).
- **Lossless, and asserted so by the code itself**: `decode_font:199` runs
  `assert (data, W) == encode_font_from_png(lang)` for every language except
  `pt`. That assertion executed in the reference run. So for `us` the whole
  PNG round trip is provably the identity and can be replaced by the two ROM
  reads. The `pt` path is the exception (it applies `get_pt_remapper`,
  `sprite_sheets.py:139-146`, and reads widths from a third byte) and is
  excluded from the assert — do not assume identity there.
- The width byte is encoded *pictorially* — `decode_font:192` writes pixel
  value 255 at `base_offs + W[j] - 1`, and `get_width` (`:213-217`) scans for
  it. If you keep the PNG path for other languages, note `get_width` returns
  `i + 1` where `i` is left over from the loop, so a row with no 255 pixel
  yields 8, not an error.

### 3.5 `hud_icons.png`, `sprites/*.png`, `sprites/all_sheets*.png`

- Written `sprite_sheets.decode_hud_icons` (`:121`),
  `decode_sprite_sheets` (`:544`), called from `extract_resources.main:534-535`.
- **Read by nothing on the default path.** `sprites/*.png` is read only by
  `load_sprite_sheets` (`:557`), reached only under `--sprites-from-png`
  (`compile_resources.py:102`). `hud_icons.png` is read by nothing at all.
- **Drop both from the port**, along with `sprite_sheet_info.py` (1,547 lines),
  `palette_usage.bin`, `other/3x5_font.png`, `MasterTilesheets`, and the
  palette-preview machinery at `sprite_sheets.py:228-542`. Confirmed by grep:
  the only importers are the two dead entry points. This also removes the last
  reason to have a PNG *encoder* in the module, and removes the need for
  `other/3x5_font.png` that the handover expected to embed.
- If `--sprites-from-png` is ever wanted it is a large, separate job: it
  decodes 24-bit PNGs, finds framing tags by pixel pattern, and validates a
  checksum (`sprite_sheets.py:557-633`). Ship without it; say so in PROJECT.md.

### 3.6 `dialogue.txt`

- Written `extract_resources.print_dialogue` (`:242-243`) ->
  `text_compression.print_strings` (`:460-467`), one line per string as
  `"%d: %s" % (i+1, text)`.
- Read `compile_resources.compress_dialogue` (`:69-74`), which splits on the
  **first** `': '` and re-compresses (`text_compression.compress_strings`).
- **Pass instead:** `Vec<String>` — the decoded strings, in order.
- **Lossy in principle, identity in practice for `us`.** Measured: for all 397
  US strings the greedy re-compression equals the original ROM byte stream
  minus its trailing `0x7f` terminator. So `kDialogue` *could* be sliced
  straight out of the ROM — **do not**. The compressor must exist anyway for
  translated ROMs, the equality is a property of this ROM's own encoder, and
  the decode/encode pair is the thing the tests should assert.
- Real hazards in the text round trip:
  - `print_strings:462-464` inserts a synthetic 397th string when the decode
    yields 396 (PAL layout). US yields 397 and skips it.
  - Non-ASCII: the file is opened `encoding='utf8'` on both sides
    (`extract:243`, `compile:70`). Strings are decoded from a per-language
    alphabet of Python `str`, several entries of which are multi-codepoint
    (`"[1HeartL]"`) or non-ASCII (`"ö"`, `"…"`). Index by *character*, not by
    byte: `compress_strings` slices `s[i:]` and advances by `len(k)` in
    characters. In Rust this must be a `Vec<char>` or careful char-boundary
    work — a `&[u8]` slice will silently break on any non-ASCII alphabet.
  - `decode_strings_generic` mutates a module-level `dict_expansion` list
    (`text_compression.py:412,449`). Dead statistics; drop it.

### 3.7 `map32_to_map16.txt`

- Written `extract_resources.print_map32_to_map16` (`:12-27`) as
  `'%5d: %4d, %4d, %4d, %4d'`, 8872 lines.
- Read `compile_resources.print_map32_to_map16` (`:42-67`) — `int()` on each
  field, into `tab[i] = [4 ints]`.
- **Pass instead:** `Vec<[u16; 4]>` of length 8872 (or the four
  `Vec<u16>` columns).
- **Lossless.** Fixed-width decimal padding, values 0..4095.

### 3.8 The sound path

`extract_music.extract_sound_data` (`:451-459`) writes, per song
(`intro`, `indoor`, `ending`):

| file | writer | reader | notes |
|------|--------|--------|-------|
| `sound/<song>.spc` | `extract_music.py:454` | `compile_music.compare_with_orig` (`:389`) | 64 KB image, `None` -> 0. Self-check only. |
| `sound_<song>.txt` | `extract_music.print_song` (`:270-290`) | `compile_music.process_file` (`:290`) | the assembly-like listing |
| `sfx.txt` | `extract_music.print_all_sfx` (`:395-449`), intro only | `compile_music.print_song:435`, intro only | |
| `music_info.yaml` | `extract_music.dump_music_info` (`:303-353`), intro only | `compile_music.serialize_song:336` | ints only |
| `sound/sound<N>.pcm.brr` | `dump_brr_audio:300` | `compile_music.serialize_song:340` | raw BRR bytes, 25 samples |
| `sound/sound<N>.pcm` | `dump_brr_audio:301` | nothing | decoded PCM, dead output |

- **Pass instead:** the object graph directly. `extract_music` builds
  `types_for_ea: {ea -> Song|SongList|Phrase|Pattern}` and `compile_music`
  rebuilds `types_for_name: {name -> ...}` from the printed text. Skipping the
  text means skipping `note_to_str`/`kKeysDict` entirely, plus all the `%2d` /
  `%2x` formatting. `music_info` is a plain struct. The BRR samples are
  `Vec<Vec<u8>>` indexed by the sample filename, which is only a dedup key
  (`kDupSamples = {10:9, 20:19}`, `extract_music.py:305`) — model it as a
  sample index, not a path string.
- **Lossy / normalising — the traps:**
  - **Object identity via names.** `extract_music` names things
    `Pattern_0x2b00`; `compile_music.get_type_for_name` (`:43-62`) re-derives
    `ea` by parsing the hex after `_0x` (`:60-61`). Two different objects at
    the same address cannot exist, and the *type* is re-derived from the name
    prefix (`Song_`/`Phrase_`/`Pattern_`/`SongList_`/`Sfx_`/`SfxPort`,
    `compile_music.py:292-307`). If you pass objects directly you must keep the
    `ea` and the type tag on each object.
  - **Emission order.** `extract_music.print_song:288` emits in
    `sorted(types_for_ea.items())` — ascending address, *skipping imported
    objects* (`is_imported`, i.e. objects whose address is outside the loaded
    bank, `:130-134`). `compile_music.print_song:437` then re-sorts:
    `sorted(sorted_ents, key=lambda x: x.ea)`. For `intro`, `sorted_ents` is
    `sound_intro.txt`'s entries followed by `sfx.txt`'s (`:433-435`), and
    `sorted` is **stable**, so any two entities with equal `ea` keep
    music-before-sfx order. Reproduce with a stable sort on `ea` over that
    concatenation. Do not use an unstable sort.
  - `Serializer.write_obj` (`:251-278`) asserts the running write cursor
    matches each object's recorded `ea`, except at `kGapStartAddrs =
    (0x2b00, 0x2880, 0xd000)` where it hard-seeks (`:185,256`). So layout is
    *checked* against the extracted addresses, not recomputed — the addresses
    are real inputs.
  - `Serializer.write` asserts each target byte is still `None` (`:195`) — no
    overwrites. `write_at`/`write_word`/`memory[...] = ...` bypass that check
    and are used for the sample/instrument tables. Model memory as
    `[Option<u8>; 65536]`; the `None`-ness is semantic, since
    `produce_loadable_seq` (`:406-425`) emits only the defined runs, as
    `(len_lo, len_hi, addr_lo, addr_hi, bytes...)` records terminated by
    `0x0000`.
  - `indoor` special-cases `Song_0x2880` (`compile_music.py:374-377`): marked
    defined with `write_addr = 0x2880` after serialisation, so it resolves as
    a reloc target without being written.
  - `extract_music.note_to_str:156` computes `octave = note / 12` — **a
    float** — then formats `'%d' % (octave + 1)`, which truncates toward zero.
    For the input range this equals `note / 12 + 1` in integers. If you keep
    the text path, use integer division; if you pass objects, the function
    disappears.
  - `decode_pattern` needs `next_ea` — the address of the *next* item on the
    priority queue (`extract_music.py:287`) — to decide whether a pattern
    falls through. The queue is a `heapq` of `(ea, obj)` tuples. Python's
    heap compares the second element when addresses tie; addresses are unique
    keys in `types_for_ea` so ties cannot occur, but a Rust `BinaryHeap` of
    `(ea, obj)` would need `Ord` on the payload. Use a min-heap keyed on `ea`
    alone, or a sorted worklist.
  - `decode_sfx` (`:355-393`) and `write_sfx_pattern` (`:129-161`) round-trip
    through a positional text format with `'%3d'`/`'---'` sentinels and
    `re.split(r' +', line)`. Passing tuples directly removes all of it.
- `brr_tools`: `tables/decode_music.py:1` imports it and there is no such
  module in the repo. **`decode_music.py` is imported by nothing** — not
  `restool.py`, not any file under `tables/`. It is dead. Likewise
  `util.encode_brr_generic` (`:281-332`) and `compile_resources.compress_store`
  (`:76-87`): no callers. The port needs **no BRR encoder** — only
  `util.decode_brr` (`:230-266`), and that only feeds `sound/sound<N>.pcm`,
  which nothing reads, so it can be dropped as well. The BRR data that reaches
  the .dat is copied verbatim from the ROM. The handover's open question 1 is
  answered: the music path needs no external tool.

### 3.9 Checked-in inputs that survive

After 3.5, `palette_usage.bin` and `other/3x5_font.png` are both unreachable.
The module needs **no embedded data files at all** — only the constant tables
compiled from `tables/tables.py` (~1,100 lines of arrays and name lists) and
`text_compression.py`'s alphabets and dictionaries.

---

## 4. Proposed `PHASES`

Dependency order. Names as they appear to a user in the progress bar.
"Assets" refers to the numbering in section 2.

| # | stage name | what it does | assets |
|---|-----------|--------------|--------|
| 1 | Reading the ROM | identify by SHA-1, strip a 0x200 SMC header, set language | — |
| 2 | Reading dungeon rooms | `print_room` x320: headers, objects, doors, sprites, secrets, chests | — |
| 3 | Reading room entrances | `get_entrance_info(0)`, `get_entrance_info(1)` | — |
| 4 | Reading template rooms | default and overlay room object lists | — |
| 5 | Reading the overworld | `print_overworld_area` for 160 area heads | — |
| 6 | Reading overworld links | exits, travel points, entrances, holes | — |
| 7 | Reading tile mappings | map32 -> map16 tables | — |
| 8 | Reading dialogue | decode all 397 strings from the ROM | — |
| 9 | Reading music banks | load the three sound banks into SPC memory | — |
| 10 | Decoding music | walk songs, phrases, patterns; decode sfx | — |
| 11 | Reading instruments | sample table, instruments, sfx instruments, BRR samples | — |
| 12 | Reading Link's sprites | 4bpp tileset at 0x108000 | — |
| 13 | Reading the font | font tiles and character widths | — |
| 14 | Building sound banks | serialise the three banks, relocate, verify against SPC | 0-2 |
| 15 | Building dungeon rooms | object/door streams, headers, offsets | 3-10 |
| 16 | Building entrances | entrance and starting-point tables | 11-45 |
| 17 | Building room templates | defaults, overlays, secrets, tile attributes | 46-55 |
| 18 | Building enemy data | decompress the damage table | 56 |
| 19 | Packing Link's sprites | re-encode to 4bpp | 57 |
| 20 | Building dungeon sprites | sprite streams and offsets | 58-59 |
| 21 | Building tile mappings | four map32 columns | 60-63 |
| 22 | Packing graphics | measure and pack sprite and background tilesets | 64-65 |
| 23 | Copying palettes and tables | the direct ROM extracts | 66-93 |
| 24 | Compressing dialogue | dictionary, greedy re-encode, font, language map | 94-96 |
| 25 | Building dungeon maps | floor layouts and tiles | 97-98 |
| 26 | Building tilemaps | the six background tilemaps | 99-104 |
| 27 | Packing overworld maps | hi/lo byte streams | 105-106 |
| 28 | Building overworld tables | music sets, travel, exits, secrets, sprites | 107-164 |
| 29 | Writing the asset file | key blob, hash, size array, alignment | — |

29 stages. Stages 1-13 are the extract half, 14-28 the compile half, and the
split matches the function boundaries so each stage is one or two Python
functions. If a finer bar is wanted, stage 2 and stage 15 are the two that
dominate and can each be split by room range.

---

## 5. Hazards

### Python semantics the output depends on

- **`dict` insertion order** decides the whole file (section 1). Use
  `IndexMap` or a `Vec<(name, bytes)>` plus a duplicate check — never
  `HashMap`. `add_asset_*` asserts `name not in assets`; keep that assert.
- **`text_compression.encode_greedy_from_dict` is first-match, not
  longest-match.** `rev` is `{first_char: {phrase: index}}` built by iterating
  the dictionary in order (`:495-496`), then `for k, v in r.items(): if
  a.startswith(k)` (`:475-478`). The winner is the *earliest dictionary entry*
  that is a prefix — which happens to be the longest only because the tables
  are written longest-first. A `HashMap` or `BTreeMap` here changes the output.
  Preserve the declaration order of every `dictionary` list.
- **Stable `sorted`** in `compile_music.print_song:437` (equal `ea` across the
  music/sfx concatenation) and in `compile_resources.py:334`
  (`sorted(holes)` — tuple ordering on `(entrance_id, pos, area)`; entrance ids
  are unique so it is only tuple-lexicographic, but keep the tuple order).
  `extract_music.print_song:288` sorts `types_for_ea.items()` — tuples of
  `(int, object)`; ties are impossible, but a Rust sort over `(ea, obj)` must
  not require `Ord` on the object.
- **Negative operands to `&` and `>>`.** `extract_resources.py:55` and `:116`
  compute `(load_offs >> 7) - (scroll_y >> 4) & 0x3f` — `-` binds tighter than
  `&`, and the subtraction can go negative. Python masks the two's-complement
  representation; do the arithmetic in `i32` and mask, not in `u32` with a
  wrapping sub, and never in `usize`. Same shape at
  `compile_resources.py:330` (`(y - 8) & 0x3f`). Arithmetic right shift on
  negatives appears in `util.decode_brr` (`:249-253`) — `i32 >>` in Rust
  matches, but only for signed types.
- **Arbitrary-precision ints.** Nothing in the reachable path needs more than
  64 bits. `sprite_sheets.py:350-351` and the tag decoder at `:576-588` build
  ~64-bit values but live in the dead sprite-sheet path.
- **Floats.** Exactly one on a live path, and it does not reach the output:
  `extract_music.note_to_str:156` (`note / 12`, then `%d`). `util.decode_brr`
  is integer throughout. There is no float in the .dat.
- **`assets.items()` vs the C header.** `--print-assets-header` regenerates
  `assets.h`; the port does not need to emit it, but if it ever does, the
  `#define` index must match the same order.
- **`@cache` / `lru_cache`** on `util.get_bytes`, `util.get_words`,
  `load_overworld_yaml`, `load_dungeon_yaml`, `get_exit_datas`,
  `get_entrance_info`, etc. is pure memoisation of pure functions. It affects
  speed only — but note `load_*_yaml` returning the *same mutable dict* to
  several callers matters: `print_dungeon_rooms:594-596` **mutates** the
  entrance dicts (`e['room'] = i`) that `print_entrance_info` later reads.
  Model that dependency explicitly rather than relying on aliasing.

### The compression routines

- `util.decomp` (`:176-227`) is the only decompressor needed. Two variants via
  `offset_is_be`: overworld and enemy-damage use big-endian copy offsets, the
  graphics tilesets use little-endian (`compile_resources.py:107,116,195`).
  The `copy` command reads from the *output* buffer being built and may read
  bytes it is currently writing (`result[offs]` inside the loop) — a
  byte-at-a-time copy, not a `copy_from_slice`. Reproduce literally.
- `Reader.next` (`util.py:169-174`) advances the SNES address and skips the
  unmapped half-bank when `(ea & 0xffff) == 0`. `LoadedRom.get_bytes` uses a
  *different* rule (`(addr & 0x8000) == 0`, `util.py:105`), and
  `decomp`'s returned length is `(reader.ea - ea) & 0x7fff`. These three
  address arithmetics are not interchangeable; give each its own function.
- `LoadedRom.get_byte` asserts `ea & 0x8000` — LoROM addressing. Keep the
  assert; it catches address-table typos immediately.
- There is **no compressor** to port (see 3.8).

### Text compression

- Per-language `alphabet` and `dictionary` tables, plus two encoders
  (`org_encoder:146`, `new_encoder:180`) selected by `Lang*.encoder`. `us`
  uses `org`. The command syntax inside strings is `[Name]` or `[Name NN]`
  with a two-digit decimal parameter (`decode_strings_generic:436`), parsed
  back at `encode_greedy_from_dict:479-485` — note it first tries the *whole*
  bracketed token as an alphabet entry (`a2i.get(a[:cmdlen+2])`, so
  `"[1HeartL]"` is a character, not a command) before treating it as a command.
- `new_encoder` can legitimately return `()` — zero bytes — for
  `Window 0`, `Sound 64` and `ScrollSpd 0` (`text_compression.py:174-176,192`).
  A Rust signature returning a single byte will be wrong.
- Alphabet lookup is by `str`, so `a2i[a[0]]` indexes by **character**. See
  3.6.
- `LangPT.__init__` asserts `len(self.alphabet) == 121` (`:363-364`) — only
  `LangPT` and `LangUS` variants are instantiated at `:391-403`, and
  `kLanguages` builds *all eleven* at import time. `'us'` and `'redux'` are
  two separate `LangUS()` instances.

### Other

- `append_scan_bytes` (`compile_resources.py:518-523`) deduplicates room
  headers by finding the longest suffix of the accumulated buffer that is a
  prefix of the new record, then appending only the remainder. It scans `n`
  from `len(little)` down to 0 and returns the *first* match — with
  `n == len(little)` meaning the record already ends the buffer. Off-by-one
  here silently changes `kDungeonRoomHeaders` and every offset in
  `kDungeonRoomHeadersOffs`.
- `OutArrays.write` asserts every element is an `int` (`:218`) — i.e. that
  every slot got filled. Several arrays are created with `initializer=None`
  and rely on the data to cover them. Keep the assert as a real check; in Rust
  that means `Vec<Option<i32>>` during the fill, unwrapped at write time, not
  `Vec<i32>` pre-zeroed — pre-zeroing would turn a missing entry into a
  silently wrong zero.
- `kMusicNamesRev`/`kAmbientSoundNameRev`/`kSecretNamesRev`/`kSpriteNamesRev`
  are `{value: key}` inversions of dicts (`tables.py:523,537,864,882`). If any
  value repeated, the last one would win. None do, but build them as explicit
  static maps rather than inverting at runtime, so a typo is a compile error.
- `restool.py:30` always loads the ROM before dispatch, and
  `LoadedRom.__init__:86-87` **rejects any non-US ROM** unless
  `support_multilanguage`. The default (and the only path that builds a .dat)
  is US-only. `--extract-dialogue` is the multi-language entry
  (`restool.py:23-28`) and writes `dialogue_<lang>.txt` for a *later* build
  with `--languages`; the two are separate ROMs and separate invocations. The
  handover's open question 2: a language build needs a second ROM's
  `dialogue_*.txt` on disk (`compile_resources.py:130-133` raises if absent),
  so as an ABI it is two inputs, not one. Ship the US path first.
- `--sprites-from-png` (handover question 3): the default is `False`, the
  reference output uses the ROM path, and nothing else reads
  `sprites/*.png`. The port only needs the default.
- `requirements.txt` lists numpy; nothing under `tables/` imports it.
  Confirmed by grep. Ignore it.
