//! Graphics and direct-table assets: PORTING-MAP.md section 2 entries
//! **56, 57, 64-65, 66-93 and 99-104**.
//!
//! These are the parts of the pipeline that never touch a YAML or text
//! intermediate. Everything here is a function of the US ROM alone, so the
//! extract half and the compile half collapse into a single pass and the
//! Python's on-disk `linksprite.png` disappears entirely (see PORTING-MAP.md
//! 3.3 and 3.5).
//!
//! Python sources, all in `tables/`:
//!
//! | assets | Python |
//! |--------|--------|
//! | 56     | `compile_resources.print_enemy_damage_data` (`:714`) |
//! | 57     | `compile_resources.print_link_graphics` (`:732`) + `sprite_sheets.decode_4bit_tileset_link` (`:74`) |
//! | 64-65  | `compile_resources.print_images` (`:101`) |
//! | 66-93  | `compile_resources.print_misc` (`:148`) |
//! | 99-104 | `compile_resources.print_tilemaps` (`:718`) |
//!
//! The three ordering gaps are other slices' work: 58-63 sit between 57 and 64
//! (dungeon sprites and the map32 tables) and 94-98 between 93 and 99
//! (dialogue and the dungeon map). [`add_all`] emits only its own keys, in the
//! section-2 order, so a caller that interleaves the slices in asset order gets
//! the right file.

use crate::codec::{compressed_bytes, OffsetOrder};
use crate::pack::Assets;
use crate::rom::Rom;

pub type Result<T> = core::result::Result<T, String>;

/// `tables.kCompSpritePtrs` (`tables.py:884`). 108 entries; the first twelve
/// are stored raw and the rest compressed — see [`add_packed_graphics`].
pub const COMP_SPRITE_PTRS: [u32; 108] = [
    0x10f000, 0x10f600, 0x10fc00, 0x118200, 0x118800, 0x118e00, 0x119400, 0x119a00, 0x11a000,
    0x11a600, 0x11ac00, 0x11b200, 0x14fffc, 0x1585d4, 0x158ab6, 0x158fbe, 0x1593f8, 0x1599a6,
    0x159f32, 0x15a3d7, 0x15a8f1, 0x15aec6, 0x15b418, 0x15b947, 0x15bed0, 0x15c449, 0x15c975,
    0x15ce7c, 0x15d394, 0x15d8ac, 0x15ddc0, 0x15e34c, 0x15e8e8, 0x15ee31, 0x15f3a6, 0x15f92d,
    0x15feba, 0x1682ff, 0x1688e0, 0x168e41, 0x1692df, 0x169883, 0x169cd0, 0x16a26e, 0x16a275,
    0x16a787, 0x16aa06, 0x16ae9d, 0x16b3ff, 0x16b87e, 0x16be6b, 0x16c13d, 0x16c619, 0x16cbbb,
    0x16d0f1, 0x16d641, 0x16d95a, 0x16dd99, 0x16e278, 0x16e760, 0x16ed25, 0x16f20f, 0x16f6b7,
    0x16fa5f, 0x16fd29, 0x1781cd, 0x17868d, 0x178b62, 0x178fd5, 0x179527, 0x17994b, 0x179ea7,
    0x17a30e, 0x17a805, 0x17acf8, 0x17b2a2, 0x17b7f9, 0x17bc93, 0x17c237, 0x17c78e, 0x17cd55,
    0x17d2bc, 0x17d82f, 0x17dcec, 0x17e1cc, 0x17e36b, 0x17e842, 0x17eb38, 0x17ed58, 0x17f06c,
    0x17f4fd, 0x17fa39, 0x17ff86, 0x18845c, 0x1889a1, 0x188d64, 0x18919d, 0x189610, 0x189857,
    0x189b24, 0x189dd2, 0x18a03f, 0x18a4ed, 0x18a7ba, 0x18aedf, 0x18af0d, 0x18b520, 0x18b953,
];

/// `tables.kCompBgPtrs` (`tables.py:901`). 115 entries, all compressed.
pub const COMP_BG_PTRS: [u32; 115] = [
    0x11b800, 0x11bce2, 0x11c15f, 0x11c675, 0x11cb84, 0x11cf4c, 0x11d2ce, 0x11d726, 0x11d9cf,
    0x11dec4, 0x11e393, 0x11e893, 0x11ed7d, 0x11f283, 0x11f746, 0x11fc21, 0x11fff2, 0x128498,
    0x128a0e, 0x128f30, 0x129326, 0x129804, 0x129d5b, 0x12a272, 0x12a6fe, 0x12aa77, 0x12ad83,
    0x12b167, 0x12b51d, 0x12b840, 0x12bd54, 0x12c1c9, 0x12c73d, 0x12cc86, 0x12d198, 0x12d6b1,
    0x12db6a, 0x12e0ea, 0x12e6bd, 0x12eb51, 0x12f135, 0x12f6c5, 0x12fc71, 0x138129, 0x138693,
    0x138bad, 0x139117, 0x139609, 0x139b21, 0x13a074, 0x13a619, 0x13ab2b, 0x13b00c, 0x13b4f5,
    0x13b9eb, 0x13bebf, 0x13c3ce, 0x13c817, 0x13cb68, 0x13cfb5, 0x13d460, 0x13d8c2, 0x13dd7a,
    0x13e266, 0x13e7af, 0x13ece5, 0x13f245, 0x13f6f0, 0x13fc30, 0x1480e9, 0x14863b, 0x148a7c,
    0x148f2a, 0x149346, 0x1497ed, 0x149cc2, 0x14a173, 0x14a61d, 0x14ab5d, 0x14b083, 0x14b4bd,
    0x14b94e, 0x14be0e, 0x14c291, 0x14c7ba, 0x14cce4, 0x14d1db, 0x14d6bd, 0x14db77, 0x14ded1,
    0x14e2ac, 0x14e754, 0x14ebae, 0x14ef4e, 0x14f309, 0x14f6f4, 0x14fa55, 0x14ff8c, 0x14ff93,
    0x14ff9a, 0x14ffa1, 0x14ffa8, 0x14ffaf, 0x14ffb6, 0x14ffbd, 0x14ffc4, 0x14ffcb, 0x14ffd2,
    0x14ffd9, 0x14ffe0, 0x14ffe7, 0x14ffee, 0x14fff5, 0x18b520, 0x18b953,
];

/// `print_tilemaps.kSrcs` (`compile_resources.py:719`).
pub const BG_TILEMAP_SRCS: [u32; 6] = [0xcdd6d, 0xce7bf, 0xce2a8, 0xce63c, 0xce456, 0xeda9c];

/// Where Link's 4bpp tileset lives, and how much of it the port reads:
/// `sprite_sheets.decode_4bit_tileset_link` (`:76`) takes
/// `0x800 * height / 32` bytes with `height = 448`, i.e. `0x7000`.
pub const LINK_GFX_ADDR: u32 = 0x108000;
pub const LINK_GFX_LEN: usize = 0x7000;
const LINK_GFX_HEIGHT: usize = 448;
const LINK_GFX_WIDTH: usize = 128;

/// Every asset this module owns, in PORTING-MAP.md section 2 order:
/// 56, 57, 64, 65, 66-93, 99-104.
///
/// The caller is responsible for the assets in between; nothing here reads or
/// depends on them.
pub fn add_all(rom: &Rom, a: &mut Assets) -> Result<()> {
    add_enemy_damage_data(rom, a)?; // 56
    add_link_graphics(rom, a)?; // 57
    add_packed_graphics(rom, a)?; // 64-65
    add_palettes_and_tables(rom, a)?; // 66-93
    add_bg_tilemaps(rom, a)?; // 99-104
    Ok(())
}

// ---------------------------------------------------------------------------
// 56 — kEnemyDamageData
// ---------------------------------------------------------------------------

/// Asset 56. `print_enemy_damage_data` (`compile_resources.py:714`):
/// `util.decomp(0x83e800, ..., offset_is_be=True)`, and unlike 64/65 it is the
/// *decompressed* output that is stored. 1728 bytes.
pub fn add_enemy_damage_data(rom: &Rom, a: &mut Assets) -> Result<()> {
    let d = crate::codec::decomp(rom, 0x83e800, OffsetOrder::Big)?;
    a.add_uint8("kEnemyDamageData", &d.data)
}

// ---------------------------------------------------------------------------
// 57 — kLinkGraphics
// ---------------------------------------------------------------------------

/// `sprite_sheets.decode_4bit_tileset_link` (`:74-87`). 4bpp SNES tiles in,
/// one palette index per pixel out, 128 x 448.
///
/// This is the half the Python writes to `linksprite.png`; no PNG codec is
/// needed because the compile side immediately reads the same indices back
/// (PORTING-MAP.md 3.3).
pub fn decode_4bit_tileset_link(rom: &Rom) -> Result<Vec<u8>> {
    let data = rom.get_bytes(LINK_GFX_ADDR, LINK_GFX_LEN)?;
    // Not pre-zeroed as a shortcut: every pixel of the 128x448 field is
    // assigned exactly once below, and the vector is sized so the indexing
    // matches the Python's `bytearray(128*height)`.
    let mut dst = vec![0u8; LINK_GFX_WIDTH * LINK_GFX_HEIGHT];
    for i in 0..(16 * LINK_GFX_HEIGHT / 8) {
        let (tx, ty) = (i % 16, i / 16);
        let offs = i * 32;
        let toffs = tx * 8 + ty * 8 * LINK_GFX_WIDTH;
        for y in 0..8 {
            let d0 = data[offs + y * 2];
            let d1 = data[offs + y * 2 + 1];
            let d2 = data[offs + y * 2 + 16];
            let d3 = data[offs + y * 2 + 17];
            for x in 0..8 {
                let t = ((d0 >> x) & 1) + ((d1 >> x) & 1) * 2 + ((d2 >> x) & 1) * 4
                    + ((d3 >> x) & 1) * 8;
                dst[toffs + y * LINK_GFX_WIDTH + (7 - x)] = t;
            }
        }
    }
    Ok(dst)
}

/// `encode_4bit_sprite` nested in `print_link_graphics`
/// (`compile_resources.py:735-744`). The inverse of the decode above.
fn encode_4bit_sprite(data: &[u8], offset: usize, pitch: usize, out: &mut Vec<u8>) {
    let mut b = [0u8; 32];
    for y in 0..8 {
        for x in 0..8 {
            let v = data[offset + y * pitch + x];
            b[y * 2] |= (v & 1) << (7 - x);
            b[y * 2 + 1] |= ((v >> 1) & 1) << (7 - x);
            b[y * 2 + 16] |= ((v >> 2) & 1) << (7 - x);
            b[y * 2 + 17] |= ((v >> 3) & 1) << (7 - x);
        }
    }
    out.extend_from_slice(&b);
}

/// Asset 57. `print_link_graphics` (`compile_resources.py:732`), reading the
/// indices straight from [`decode_4bit_tileset_link`] instead of from
/// `linksprite.png`. 28672 bytes.
///
/// The round trip is the identity — measured, `kLinkGraphics ==
/// ROM[0x108000..+0x7000]` — but PORTING-MAP.md 3.3 asks for the transform to
/// be reproduced so that stays a checkable property rather than an assumption,
/// and the test module below checks it.
pub fn add_link_graphics(rom: &Rom, a: &mut Assets) -> Result<()> {
    a.add_uint8("kLinkGraphics", &link_graphics(rom)?)
}

/// The `kLinkGraphics` payload on its own, so the identity can be tested.
pub fn link_graphics(rom: &Rom) -> Result<Vec<u8>> {
    Ok(encode_link_graphics(&decode_4bit_tileset_link(rom)?))
}

/// The re-encoding half on its own, over the pixel indices
/// [`decode_4bit_tileset_link`] produced, so the reading stage and the packing
/// stage can be separate phases without decoding twice.
pub fn encode_link_graphics(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(56 * 16 * 32);
    for y in 0..56 {
        for x in 0..16 {
            encode_4bit_sprite(data, y * LINK_GFX_WIDTH * 8 + x * 8, LINK_GFX_WIDTH, &mut out);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 64-65 — kSprGfx, kBgGfx
// ---------------------------------------------------------------------------

/// Assets 64 and 65. `print_images` (`compile_resources.py:101`) on the default
/// path (`--sprites-from-png` is off, PORTING-MAP.md 3.5).
///
/// Both store the **compressed** ROM bytes; `util.decomp` runs only to discover
/// how long each stream is (`return_length=True`). Copy offsets are
/// little-endian for these two tables, unlike the overworld and enemy-damage
/// streams. The first twelve sprite entries are uncompressed and take a fixed
/// `0x600` bytes each.
pub fn add_packed_graphics(rom: &Rom, a: &mut Assets) -> Result<()> {
    let mut all: Vec<Vec<u8>> = Vec::with_capacity(COMP_SPRITE_PTRS.len());
    for (i, &ea) in COMP_SPRITE_PTRS.iter().enumerate() {
        if i < 12 {
            all.push(rom.get_bytes(ea, 0x600)?);
        } else {
            all.push(compressed_bytes(rom, ea, OffsetOrder::Little)?);
        }
    }
    a.add_packed("kSprGfx", &all)?;

    let mut all: Vec<Vec<u8>> = Vec::with_capacity(COMP_BG_PTRS.len());
    for &ea in COMP_BG_PTRS.iter() {
        all.push(compressed_bytes(rom, ea, OffsetOrder::Little)?);
    }
    a.add_packed("kBgGfx", &all)
}

// ---------------------------------------------------------------------------
// 66-93 — the direct ROM extracts
// ---------------------------------------------------------------------------

/// Assets 66-93. `print_misc` (`compile_resources.py:148`), a flat list of ROM
/// reads in declaration order.
///
/// `kPalette_ArmorAndGloves` is the plain ROM read:
/// `sprite_sheets.override_armor_palette` is `None` (`sprite_sheets.py:9`) and
/// the commented-out override below it is not a code path.
pub fn add_palettes_and_tables(rom: &Rom, a: &mut Assets) -> Result<()> {
    a.add_uint8("kOverworldMapGfx", &rom.get_bytes(0x18c000, 0x4000)?)?;
    a.add_uint8("kLightOverworldTilemap", &rom.get_bytes(0xac727, 4096)?)?;
    a.add_uint8("kDarkOverworldTilemap", &rom.get_bytes(0xad727, 1024)?)?;

    a.add_uint16("kPredefinedTileData", &rom.get_words(0x9b52, 6438)?)?;
    a.add_uint16("kMap16ToMap8", &rom.get_words(0x8f8000, 3752 * 4)?)?;

    a.add_uint8("kGeneratedWishPondItem", &rom.get_bytes(0x888450, 256)?)?;
    a.add_uint8("kGeneratedBombosArr", &rom.get_bytes(0x8890fc, 256)?)?;

    a.add_uint8("kGeneratedEndSequence15", &rom.get_bytes(0x8ead25, 256)?)?;
    a.add_uint8("kEnding_Credits_Text", &rom.get_bytes(0x8eb178, 1989)?)?;
    a.add_uint16("kEnding_Credits_Offs", &rom.get_words(0x8eb93d, 394)?)?;
    a.add_uint16("kEnding_MapData", &rom.get_words(0x8eb038, 160)?)?;
    a.add_uint16("kEnding0_Offs", &rom.get_words(0x8ec2e1, 17)?)?;
    a.add_uint8("kEnding0_Data", &rom.get_bytes(0x8ebf4c, 917)?)?;

    a.add_uint16("kPalette_DungBgMain", &rom.get_words(0x9bd734, 1800)?)?;
    a.add_uint16("kPalette_MainSpr", &rom.get_words(0x9bd218, 120)?)?;

    a.add_uint16("kPalette_ArmorAndGloves", &rom.get_words(0x9bd308, 75)?)?;
    a.add_uint16("kPalette_Sword", &rom.get_words(0x9bd630, 12)?)?;
    a.add_uint16("kPalette_Shield", &rom.get_words(0x9bd648, 12)?)?;

    a.add_uint16("kPalette_SpriteAux3", &rom.get_words(0x9bd39e, 84)?)?;
    a.add_uint16("kPalette_MiscSprite_Indoors", &rom.get_words(0x9bd446, 77)?)?;
    a.add_uint16("kPalette_SpriteAux1", &rom.get_words(0x9bd4e0, 168)?)?;

    a.add_uint16("kPalette_OverworldBgMain", &rom.get_words(0x9be6c8, 210)?)?;
    a.add_uint16("kPalette_OverworldBgAux12", &rom.get_words(0x9be86c, 420)?)?;
    a.add_uint16("kPalette_OverworldBgAux3", &rom.get_words(0x9be604, 98)?)?;
    a.add_uint16("kPalette_PalaceMapBg", &rom.get_words(0x9be544, 96)?)?;
    a.add_uint16("kPalette_PalaceMapSpr", &rom.get_words(0x9bd70a, 21)?)?;
    a.add_uint16("kHudPalData", &rom.get_words(0x9bd660, 64)?)?;

    a.add_uint16("kOverworldMapPaletteData", &rom.get_words(0x8adb27, 256)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 99-104 — kBgTilemap_0..5
// ---------------------------------------------------------------------------

/// `decode_one` (`compile_resources.py:720-728`). Walks the tilemap opcode
/// stream until it meets a byte with bit 7 set, and returns the number of bytes
/// consumed **plus one** — that terminator byte is part of the stored asset.
///
/// The scan uses plain `p + 1` address arithmetic and `ROM.get_byte`, which is
/// [`Rom::get_byte`]'s LoROM mapping, *not* the `get_bytes` wrap rule and not
/// `Reader::next`; all six streams stay inside one bank so the distinction
/// never bites, but open-coding a different rule here would be a silent bug.
pub fn tilemap_length(rom: &Rom, start: u32) -> Result<usize> {
    let mut p = start;
    while rom.get_byte(p)? & 0x80 == 0 {
        let is_memset = rom.get_byte(p + 2)? & 0x40 != 0;
        let len = ((rom.get_byte(p + 2)? as u32 * 256 + rom.get_byte(p + 3)? as u32) & 0x3fff) + 1;
        p += 4;
        p += if is_memset { 2 } else { len };
    }
    Ok((p - start) as usize + 1)
}

/// Assets 99-104. `print_tilemaps` (`compile_resources.py:718`): six streams
/// whose lengths are not tabulated anywhere and have to be measured by
/// [`tilemap_length`].
pub fn add_bg_tilemaps(rom: &Rom, a: &mut Assets) -> Result<()> {
    for (i, &s) in BG_TILEMAP_SRCS.iter().enumerate() {
        let l = tilemap_length(rom, s)?;
        a.add_uint8(&format!("kBgTilemap_{i}"), &rom.get_bytes(s, l)?)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------

/// Tests that need the real cartridge, skipped unless `ZELDA3_ROM` points at a
/// US ROM:
///
/// ```sh
/// ZELDA3_ROM="/path/to/zelda3.sfc" cargo test -- --ignored
/// ```
///
/// `compare_against_oracle` additionally needs `ZELDA3_ORACLE_DAT` pointing at
/// the Python's `zelda3_assets.dat` and `node` on `PATH`. It builds a .dat
/// holding only this module's assets — every other key is present but empty, so
/// `compare.mjs` reports them as MISSING, which is expected — and diffs it.
#[cfg(test)]
mod rom_tests {
    use super::*;
    use crate::assets::ASSET_TABLE;

    /// The keys this module owns, for the test's own bookkeeping.
    const OWNED: &[&str] = &[
        "kEnemyDamageData",
        "kLinkGraphics",
        "kSprGfx",
        "kBgGfx",
        "kOverworldMapGfx",
        "kLightOverworldTilemap",
        "kDarkOverworldTilemap",
        "kPredefinedTileData",
        "kMap16ToMap8",
        "kGeneratedWishPondItem",
        "kGeneratedBombosArr",
        "kGeneratedEndSequence15",
        "kEnding_Credits_Text",
        "kEnding_Credits_Offs",
        "kEnding_MapData",
        "kEnding0_Offs",
        "kEnding0_Data",
        "kPalette_DungBgMain",
        "kPalette_MainSpr",
        "kPalette_ArmorAndGloves",
        "kPalette_Sword",
        "kPalette_Shield",
        "kPalette_SpriteAux3",
        "kPalette_MiscSprite_Indoors",
        "kPalette_SpriteAux1",
        "kPalette_OverworldBgMain",
        "kPalette_OverworldBgAux12",
        "kPalette_OverworldBgAux3",
        "kPalette_PalaceMapBg",
        "kPalette_PalaceMapSpr",
        "kHudPalData",
        "kOverworldMapPaletteData",
        "kBgTilemap_0",
        "kBgTilemap_1",
        "kBgTilemap_2",
        "kBgTilemap_3",
        "kBgTilemap_4",
        "kBgTilemap_5",
    ];

    fn load() -> Option<Rom> {
        let path = std::env::var("ZELDA3_ROM").ok()?;
        Some(Rom::new(std::fs::read(path).ok()?))
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn link_graphics_round_trip_is_the_identity() {
        let Some(rom) = load() else { return };
        assert_eq!(
            link_graphics(&rom).unwrap(),
            rom.get_bytes(LINK_GFX_ADDR, LINK_GFX_LEN).unwrap(),
            "the decode/encode pair must cancel exactly (PORTING-MAP.md 3.3)"
        );
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn payload_sizes_match_the_reference_build() {
        let Some(rom) = load() else { return };
        let mut a = Assets::new();
        add_all(&rom, &mut a).unwrap();
        for name in OWNED {
            let want = ASSET_TABLE
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, _, b)| *b)
                .expect("owned key is in the asset table");
            let got = a.get(name).expect("add_all added it").data.len();
            assert_eq!(got, want, "{name}");
        }
    }

    /// Order within the slice must be the section-2 order, because the file
    /// order is the contract.
    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn keys_are_added_in_section_2_order() {
        let Some(rom) = load() else { return };
        let mut a = Assets::new();
        add_all(&rom, &mut a).unwrap();
        let got: Vec<&str> = a.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(got, OWNED);
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM and ZELDA3_ORACLE_DAT"]
    fn compare_against_oracle() {
        let Some(rom) = load() else { return };
        let Ok(oracle) = std::env::var("ZELDA3_ORACLE_DAT") else { return };

        let mut mine = Assets::new();
        add_all(&rom, &mut mine).unwrap();

        // A full 165-key file so the header matches: every key registered in
        // the canonical order, ours filled in, the rest left empty.
        let mut full = Assets::new();
        for (name, kind, _) in ASSET_TABLE {
            full.add_placeholder(name, *kind).unwrap();
        }
        for a in mine.iter() {
            full.fill(&a.name, a.kind, a.data.clone()).unwrap();
        }

        let dir = std::env::temp_dir().join("zelda3-graphics-slice");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("partial.dat");
        std::fs::write(&out, full.serialise()).unwrap();

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let res = std::process::Command::new("node")
            .arg(root.join("compare.mjs"))
            .arg(&oracle)
            .arg(&out)
            .arg("--all")
            .output()
            .expect("node compare.mjs");
        let text = String::from_utf8_lossy(&res.stdout).into_owned()
            + &String::from_utf8_lossy(&res.stderr);
        println!("{text}");

        let mut bad = Vec::new();
        for name in OWNED {
            let ok = text
                .lines()
                .any(|l| l.split_whitespace().any(|w| w == *name) && l.contains("ok"));
            if !ok {
                bad.push(*name);
            }
        }
        assert!(bad.is_empty(), "not reported ok by compare.mjs: {bad:?}");
    }
}
