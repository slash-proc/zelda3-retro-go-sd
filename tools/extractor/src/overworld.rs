//! The overworld half of the pipeline: assets 60-63, 105-106 and 107-164 of
//! PORTING-MAP.md section 2.
//!
//! Three Python functions are ported here, plus the extract-side reads they
//! consume:
//!
//! | stage | Python | assets |
//! |---|---|---|
//! | [`add_map32_to_map16`] | `extract_resources.print_map32_to_map16` + `compile_resources.print_map32_to_map16` | 60-63 |
//! | [`add_overworld`] | `compile_resources.print_overworld` | 105-106 |
//! | [`add_overworld_tables`] | `extract_resources.print_overworld_area` + `compile_resources.print_overworld_tables` | 107-164 |
//!
//! The Python round-trips the middle stage through 160 `overworld-<i>.yaml`
//! files and the first through `map32_to_map16.txt`. PORTING-MAP.md 3.1 and 3.7
//! record both as lossless, so the structures below are carried in memory and
//! no serialisation is reproduced. What *is* reproduced is everything the
//! round trip normalises:
//!
//! - the name tables. `extract` writes music, ambient, secret and sprite
//!   *names*; `compile` looks them back up in the `…Rev` inversions. All four
//!   dicts were checked injective over their live domains, so the round trip is
//!   the identity — but the domain checks are kept as real errors here, because
//!   a value outside the table is a `KeyError` in Python, not a silent zero.
//! - `Holes` is absent from the yaml when empty (`compile_resources.py:325`
//!   tests `'Holes' not in y`) — modelled as an empty list, which behaves
//!   identically because the loop body never runs.
//! - list order. `Exits`, `Entrances`, `Items`, `Travel`, `Holes` and the
//!   sprite lists are consumed positionally or appended in order, so every
//!   collection is a `Vec` in ROM order.
//!
//! # Hazards honoured here
//!
//! - **`(a - b) & 0x3f` with a negative `a - b`** — `extract_resources.py:55`
//!   and `:116` (`load_xy`) and `compile_resources.py:330` (`(y - 8) & 0x3f`).
//!   `-` binds tighter than `&` in Python, so both are `(a - b) & 0x3f`, and
//!   the subtraction really does go negative. All of this arithmetic is `i32`;
//!   `>>` on a negative is an arithmetic shift in both languages.
//! - **Do not pre-zero.** `OutArrays.write` asserts every element is an `int`
//!   (`compile_resources.py:218`), i.e. that arrays created with
//!   `initializer = None` were fully covered by the data. They are
//!   `Vec<Option<i32>>` here and [`OutArray::finish`] is that assert. Arrays
//!   the Python creates with `initializer = 0` are pre-filled with `Some(0)`.
//! - **Registration order, not fill order.** Assets 107-162 are emitted by
//!   `OutArrays.write` in the order the arrays were `A.add`-ed, which is
//!   interleaved with the loops that fill earlier ones. The emission block at
//!   the end of [`add_overworld_tables`] follows the `A.add` order literally.
//! - **`sorted(holes)`** is a lexicographic sort of `(entrance_id, pos, area)`
//!   tuples (`compile_resources.py:334`); the tuple order is kept.
//! - No map type appears anywhere iteration order can reach the output: the
//!   per-area buckets are `Vec`s indexed by area.

use crate::codec::{compressed_bytes, OffsetOrder};
use crate::pack::Assets;
use crate::rom::Rom;

pub type Result<T> = core::result::Result<T, String>;

// ---------------------------------------------------------------------------
// Assets 60-63 — map32 -> map16
// ---------------------------------------------------------------------------

/// `extract_resources.print_map32_to_map16` (`:12-27`), as a `Vec<[u16; 4]>`
/// of 8872 rows instead of 8872 lines of `'%5d: %4d, %4d, %4d, %4d'`.
///
/// Row `i * 4 + j` holds `[t0[j], t1[j], t2[j], t3[j]]`, i.e. it is indexed by
/// the *quadrant* and each column is one of the four ROM tables. The compile
/// side then reads it back the other way round, column by column.
pub fn read_map32_to_map16(rom: &Rom) -> Result<Vec<[u16; 4]>> {
    // `getit`: six ROM bytes hold four 12-bit map16 indices, the high nibbles
    // packed into the last two bytes.
    fn getit(rom: &Rom, ea: u32) -> Result<[u16; 4]> {
        let mut ov = [0u16; 6];
        for (j, o) in ov.iter_mut().enumerate() {
            *o = rom.get_byte(ea + j as u32)? as u16;
        }
        Ok([
            ov[0] | (ov[4] >> 4) << 8,
            ov[1] | (ov[4] & 0xf) << 8,
            ov[2] | (ov[5] >> 4) << 8,
            ov[3] | (ov[5] & 0xf) << 8,
        ])
    }

    let mut tab = vec![[0u16; 4]; 2218 * 4];
    for i in 0..2218u32 {
        let t0 = getit(rom, 0x838000 + i * 6)?;
        let t1 = getit(rom, 0x83b400 + i * 6)?;
        let t2 = getit(rom, 0x848000 + i * 6)?;
        let t3 = getit(rom, 0x84b400 + i * 6)?;
        for j in 0..4 {
            tab[(i * 4) as usize + j] = [t0[j], t1[j], t2[j], t3[j]];
        }
    }
    Ok(tab)
}

/// Assets **60-63**: `kMap32ToMap16_0` .. `kMap32ToMap16_3`.
///
/// `compile_resources.print_map32_to_map16` (`:42-67`). Column `c` of the table
/// is re-packed in groups of four consecutive rows back into the ROM's six-byte
/// form. 2218 groups x 6 bytes = 13308 bytes per asset.
pub fn add_map32_to_map16(rom: &Rom, a: &mut Assets) -> Result<()> {
    add_map32_to_map16_from(&read_map32_to_map16(rom)?, a)
}

/// [`add_map32_to_map16`] over an already-read table, so the reading stage and
/// the building stage can be separate phases without reading the ROM twice.
pub fn add_map32_to_map16_from(tab: &[[u16; 4]], a: &mut Assets) -> Result<()> {

    fn pack(v: [u16; 4]) -> [u8; 6] {
        [
            (v[0] & 0xff) as u8,
            (v[1] & 0xff) as u8,
            (v[2] & 0xff) as u8,
            (v[3] & 0xff) as u8,
            (((v[0] >> 8) << 4) | (v[1] >> 8)) as u8,
            (((v[2] >> 8) << 4) | (v[3] >> 8)) as u8,
        ]
    }

    let mut res: [Vec<u8>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for block in (0..tab.len()).step_by(4) {
        for (c, out) in res.iter_mut().enumerate() {
            out.extend_from_slice(&pack([
                tab[block][c],
                tab[block + 1][c],
                tab[block + 2][c],
                tab[block + 3][c],
            ]));
        }
    }

    a.add_uint8("kMap32ToMap16_0", &res[0])?;
    a.add_uint8("kMap32ToMap16_1", &res[1])?;
    a.add_uint8("kMap32ToMap16_2", &res[2])?;
    a.add_uint8("kMap32ToMap16_3", &res[3])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Assets 105-106 — the overworld hi/lo byte streams
// ---------------------------------------------------------------------------

/// Assets **105-106**: `kOverworld_Hibytes_Comp`, `kOverworld_Lobytes_Comp`.
///
/// `compile_resources.print_overworld` (`:191-203`). Two 160-entry pointer
/// tables of 24-bit addresses; each stream is decompressed only to measure its
/// compressed length, and the **compressed** ROM bytes are what gets stored.
/// The overworld codec uses big-endian copy offsets.
pub fn add_overworld(rom: &Rom, a: &mut Assets) -> Result<()> {
    for (name, table) in [
        ("kOverworld_Hibytes_Comp", 0x82F94Du32),
        ("kOverworld_Lobytes_Comp", 0x82FB2D),
    ] {
        let mut r: Vec<Vec<u8>> = Vec::with_capacity(160);
        for i in 0..160u32 {
            let addr = rom.get_24(table + i * 3)?;
            r.push(compressed_bytes(rom, addr, OffsetOrder::Big)?);
        }
        a.add_packed(name, &r)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The extract side: `print_overworld_area` for the 160 area heads
// ---------------------------------------------------------------------------

/// `is_area_head` (`compile_resources.py:206-207`), which is the same predicate
/// as `extract_resources.print_all_overworld_areas:239`. One predicate, used by
/// both halves, as PORTING-MAP.md 3.1 asks.
pub fn is_area_head(rom: &Rom, i: u32) -> Result<bool> {
    Ok(i >= 128 || rom.get_byte(0x82A5EC + (i & 63))? == (i & 63) as u8)
}

/// One music slot: the two nibbles `get_music_byte` recombines.
///
/// `extract` splits the ROM byte into `kMusicNames[x & 0xf]` and
/// `kAmbientSoundName[x >> 4]`; `compile` rebuilds `musicRev | ambientRev << 4`.
/// Both dicts are injective, so this is the identity — the split is kept so the
/// domain checks stay real.
#[derive(Clone, Copy, Debug)]
pub struct MusicSlot {
    pub music: u8,
    pub ambient: u8,
}

impl MusicSlot {
    /// Splits a ROM byte, validating both halves against the name tables the
    /// way a Python `dict` lookup would.
    fn from_byte(x: u8) -> Result<MusicSlot> {
        let music = x & 0xf;
        let ambient = x >> 4;
        // `kMusicNames` (tables.py:480) covers 0..=34 plus 240..=243 and 255;
        // a low nibble is always inside 0..=15, so always present.
        // `kAmbientSoundName` (tables.py:525) has only the nine odd-ish keys.
        if !matches!(ambient, 0 | 1 | 3 | 5 | 7 | 9 | 11 | 13 | 15) {
            return Err(format!("no kAmbientSoundName entry for {ambient}"));
        }
        Ok(MusicSlot { music, ambient })
    }

    /// `get_music_byte` (`compile_resources.py:238-239`).
    fn to_byte(self) -> i32 {
        self.music as i32 | (self.ambient as i32) << 4
    }
}

/// The `Header` map of `overworld-<i>.yaml`.
///
/// `name` (`kAreaNames`) is written and never read back, so it is not carried
/// (PORTING-MAP.md 3.1). `gfx`, `palette` and `sign_text` are `-1` outside the
/// ranges the ROM tables cover, exactly as the Python writes them.
#[derive(Clone, Debug)]
pub struct Header {
    pub is_small: bool,
    pub gfx: i32,
    pub palette: i32,
    pub sign_text: i32,
    /// `beginning`, `zelda`, `sword`, `agahnim` for areas < 64; a single
    /// `agahnim` slot for 64..160. Indices into [`Header::music`] are the tag
    /// order, so `music[3]` is always `agahnim`.
    pub music: Vec<MusicSlot>,
}

impl Header {
    /// The `agahnim` slot: index 3 for a four-slot header, index 0 otherwise.
    fn agahnim(&self) -> MusicSlot {
        self.music[self.music.len() - 1]
    }
}

/// One entry of `Travel` — a bird statue or a whirlpool.
#[derive(Clone, Debug)]
pub struct Travel {
    /// `bird_travel_id` for `i < 9`; `None` means the entry carries
    /// `whirlpool_src_area` instead. The two keys are mutually exclusive.
    pub bird_travel_id: Option<usize>,
    pub whirlpool_src_area: i32,
    pub xy: [i32; 2],
    pub scroll_xy: [i32; 2],
    pub camera_xy: [i32; 2],
    pub load_xy: [i32; 2],
    pub unk: [i32; 2],
}

/// One entry of `Entrances`.
#[derive(Clone, Debug)]
pub struct OwEntrance {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub entrance_id: i32,
}

/// One entry of `Holes`.
#[derive(Clone, Debug)]
pub struct Hole {
    pub x: i32,
    pub y: i32,
    pub entrance_id: i32,
}

/// The four `door` flavours (`extract_resources.py:78-83`). The name decides
/// which of the two door tables the entry lands in and whether bit 15 is set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorKind {
    Bombable,
    Wooden,
    Palace,
    Sanctuary,
}

/// The `special_exit` map, present when `0x180 <= room < 0x190`.
#[derive(Clone, Copy, Debug)]
pub struct SpecialExit {
    pub dir: i32,
    pub spr_gfx: i32,
    pub aux_gfx: i32,
    pub pal_bg: i32,
    pub pal_spr: i32,
    pub top: i32,
    pub bottom: i32,
    pub left: i32,
    pub right: i32,
    pub left_edge_of_map: i32,
    pub unk4: i32,
    pub unk5: i32,
    pub unk6: i32,
    pub unk7: i32,
}

/// One entry of `Exits`.
#[derive(Clone, Debug)]
pub struct Exit {
    pub index: usize,
    pub room: i32,
    pub xy: [i32; 2],
    pub scroll_xy: [i32; 2],
    pub camera_xy: [i32; 2],
    pub load_xy: [i32; 2],
    pub unk: [i32; 2],
    pub special_exit: Option<SpecialExit>,
    /// `(kind, a, b)` — `door[0]`, `door[1]`, `door[2]` in the yaml.
    pub door: Option<(DoorKind, i32, i32)>,
}

/// One `Items` entry: `[x, y, kSecretNames[b]]`, with the name replaced by the
/// index it inverts back to.
#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub x: i32,
    pub y: i32,
    pub secret: u8,
}

/// One of the `Sprites*` maps: the `info` sub-map plus the sprite list.
#[derive(Clone, Debug, Default)]
pub struct SpriteStage {
    /// `None` for areas >= 128, where `get_info` returns an empty map and
    /// `print_overworld_tables` never indexes it.
    pub info: Option<(i32, i32)>,
    /// `[x, y, sprite index]`, in ROM order.
    pub sprites: Vec<[i32; 3]>,
}

/// Everything one `overworld-<i>.yaml` holds, for one area head.
#[derive(Clone, Debug)]
pub struct Area {
    pub index: u32,
    pub header: Header,
    pub travel: Vec<Travel>,
    pub entrances: Vec<OwEntrance>,
    /// Empty stands in for the key being absent.
    pub holes: Vec<Hole>,
    pub exits: Vec<Exit>,
    pub items: Vec<Item>,
    /// `Sprites.Beginning` / `.FirstPart` / `.SecondPart` for areas < 64.
    pub sprites_beginning: Option<SpriteStage>,
    pub sprites_first_part: Option<SpriteStage>,
    pub sprites_second_part: Option<SpriteStage>,
    /// `Sprites` for areas 64..144. Areas >= 144 have no sprite key at all.
    pub sprites: Option<SpriteStage>,
}

/// `get_exit_datas` (`extract_resources.py:28-84`), bucketed by `screen_index`.
///
/// The return is indexed by screen index; `r[i]` is the Python's
/// `r.get(i, [])`, in append order.
fn get_exit_datas(rom: &Rom) -> Result<Vec<Vec<Exit>>> {
    let mut r: Vec<Vec<Exit>> = (0..256).map(|_| Vec::new()).collect();
    for i in 0..79u32 {
        let room = rom.get_word(0x82dd8a + i * 2)? as i32;
        let screen_index = rom.get_byte(0x82DE28 + i)? as i32;
        let load_offs = rom.get_word(0x82DE77 + i * 2)? as i32;
        let scroll_y = rom.get_word(0x82DF15 + i * 2)? as i32;
        let scroll_x = rom.get_word(0x82DFB3 + i * 2)? as i32;
        let pos_y = rom.get_word(0x82E051 + i * 2)? as i32;
        let pos_x = rom.get_word(0x82E0EF + i * 2)? as i32;
        let camera_y = rom.get_word(0x82E18D + i * 2)? as i32;
        let camera_x = rom.get_word(0x82E22B + i * 2)? as i32;
        let unk1 = rom.get_int8(0x82E2C9 + i)?;
        let unk3 = rom.get_int8(0x82E318 + i)?;
        let ndoor = rom.get_word(0x82E367 + i * 2)? as i32;
        let fdoor = rom.get_word(0x82E405 + i * 2)? as i32;
        let base_x = (screen_index & 7) << 9;
        let base_y = (screen_index & 56) << 6;

        let scroll_xy = [scroll_x - base_x, scroll_y - base_y];
        // `(a - b) & 0x3f` with a possibly negative `a - b`; see the module
        // docs. `>>` on a negative is arithmetic in both languages.
        let load_xy = [
            ((load_offs >> 1) - (scroll_xy[0] >> 4)) & 0x3f,
            ((load_offs >> 7) - (scroll_xy[1] >> 4)) & 0x3f,
        ];

        let special_exit = if (0x180..0x190).contains(&room) {
            let k = (room - 0x180) as u32;
            Some(SpecialExit {
                dir: rom.get_byte(0x82E801 + k)? as i32 >> 1,
                spr_gfx: rom.get_byte(0x82E811 + k)? as i32,
                aux_gfx: rom.get_byte(0x82E821 + k)? as i32,
                pal_bg: rom.get_byte(0x82E831 + k)? as i32,
                pal_spr: rom.get_byte(0x82E841 + k)? as i32,
                top: rom.get_word(0x82e6e1 + k * 2)? as i32,
                bottom: rom.get_word(0x82e701 + k * 2)? as i32,
                left: rom.get_word(0x82e721 + k * 2)? as i32,
                right: rom.get_word(0x82e741 + k * 2)? as i32,
                left_edge_of_map: rom.get_word(0x82E7E1 + k * 2)? as i32,
                unk4: rom.get_int16(0x82e761 + k * 2)?,
                unk6: rom.get_int16(0x82e781 + k * 2)?,
                unk5: rom.get_int16(0x82e7a1 + k * 2)?,
                unk7: rom.get_int16(0x82e7c1 + k * 2)?,
            })
        } else {
            None
        };

        // Both tables are consulted; the Python asserts they are not both set,
        // and the second assignment would win if they were.
        let mut door = None;
        if ndoor != 0 {
            if fdoor != 0 {
                return Err(format!("exit {i}: both door tables are non-zero"));
            }
            let kind = if ndoor & 0x8000 != 0 { DoorKind::Bombable } else { DoorKind::Wooden };
            door = Some((kind, (ndoor & 0x7e) >> 1, (ndoor & 0x3f80) >> 7));
        }
        if fdoor != 0 {
            let kind = if fdoor & 0x8000 != 0 { DoorKind::Palace } else { DoorKind::Sanctuary };
            door = Some((kind, (fdoor & 0x7e) >> 1, (fdoor & 0x3f80) >> 7));
        }

        r[screen_index as usize].push(Exit {
            index: i as usize,
            room,
            xy: [pos_x - base_x, pos_y - base_y],
            scroll_xy,
            camera_xy: [camera_x - base_x, camera_y - base_y],
            load_xy,
            unk: [unk1, unk3],
            special_exit,
            door,
        });
    }
    Ok(r)
}

/// `get_loadoffs` (`extract_resources.py:86-91`, repeated verbatim at
/// `compile_resources.py:241-246`). `c` is a scroll pair, `d` a load pair.
fn get_loadoffs(c: [i32; 2], d: [i32; 2]) -> i32 {
    let x = (c[0] >> 4) + d[0];
    let y = (c[1] >> 4) + d[1];
    ((y & 0x3f) << 7) | ((x & 0x3f) << 1)
}

/// `get_ow_travel_infos` (`extract_resources.py:93-124`), bucketed by
/// `screen_index`.
fn get_ow_travel_infos(rom: &Rom) -> Result<Vec<Vec<Travel>>> {
    let mut r: Vec<Vec<Travel>> = (0..256).map(|_| Vec::new()).collect();
    for i in 0..17u32 {
        let screen_index = rom.get_word(0x82EAE5 + i * 2)? as i32;
        let load_offs = rom.get_word(0x82EB07 + i * 2)? as i32;
        let scroll_y = rom.get_word(0x82EB29 + i * 2)? as i32;
        let scroll_x = rom.get_word(0x82EB4B + i * 2)? as i32;
        let pos_y = rom.get_word(0x82EB6D + i * 2)? as i32;
        let pos_x = rom.get_word(0x82EB8F + i * 2)? as i32;
        let camera_y = rom.get_word(0x82EBB1 + i * 2)? as i32;
        let camera_x = rom.get_word(0x82EBD3 + i * 2)? as i32;
        let unk1 = rom.get_int8(0x82EBF5 + i * 2)?;
        let unk3 = rom.get_int8(0x82EC17 + i * 2)?;
        let base_x = (screen_index & 7) << 9;
        let base_y = (screen_index & 56) << 6;

        let (bird_travel_id, whirlpool_src_area) = if i < 9 {
            (Some(i as usize), 0)
        } else {
            (None, rom.get_word(0x82ECF8 + (i - 9) * 2)? as i32)
        };

        let scroll_xy = [scroll_x - base_x, scroll_y - base_y];
        let load_xy = [
            ((load_offs >> 1) - (scroll_xy[0] >> 4)) & 0x3f,
            ((load_offs >> 7) - (scroll_xy[1] >> 4)) & 0x3f,
        ];

        // `extract_resources.py:119` — the round trip is checked on the way
        // out, so a wrong shift shows up here rather than in the .dat.
        let t0 = get_loadoffs(scroll_xy, load_xy);
        if t0 != load_offs {
            return Err(format!(
                "travel {i}: get_loadoffs gave {t0:#x}, ROM says {load_offs:#x}"
            ));
        }

        r[screen_index as usize].push(Travel {
            bird_travel_id,
            whirlpool_src_area,
            xy: [pos_x - base_x, pos_y - base_y],
            scroll_xy,
            camera_xy: [camera_x - base_x, camera_y - base_y],
            load_xy,
            unk: [unk1, unk3],
        });
    }
    Ok(r)
}

/// `get_ow_entrance_info` (`extract_resources.py:126-133`), bucketed by area.
fn get_ow_entrance_info(rom: &Rom) -> Result<Vec<Vec<OwEntrance>>> {
    let mut r: Vec<Vec<OwEntrance>> = (0..256).map(|_| Vec::new()).collect();
    for i in 0..129u32 {
        let area = rom.get_word(0x9BB96F + i * 2)? as usize;
        let pos = rom.get_word(0x9BBA71 + i * 2)? as i32;
        let entrance_id = rom.get_byte(0x9BBB73 + i)? as i32;
        if area >= r.len() {
            return Err(format!("entrance {i}: area {area} out of range"));
        }
        r[area].push(OwEntrance {
            index: i as usize,
            x: (pos >> 1) & 0x3f,
            y: (pos >> 7) & 0x3f,
            entrance_id,
        });
    }
    Ok(r)
}

/// `get_hole_infos` (`extract_resources.py:135-143`), bucketed by area.
fn get_hole_infos(rom: &Rom) -> Result<Vec<Vec<Hole>>> {
    let mut r: Vec<Vec<Hole>> = (0..256).map(|_| Vec::new()).collect();
    for i in 0..19u32 {
        let pos = rom.get_word(0x9BB800 + i * 2)? as i32 + 0x400;
        let area = rom.get_word(0x9BB826 + i * 2)? as usize;
        let entrance_id = rom.get_byte(0x9BB84C + i)? as i32;
        if area >= r.len() {
            return Err(format!("hole {i}: area {area} out of range"));
        }
        r[area].push(Hole { x: (pos >> 1) & 0x3f, y: (pos >> 7) & 0x3f, entrance_id });
    }
    Ok(r)
}

/// `kSecretNames` (`tables.py:866-881`) — the key set, so that a byte outside
/// it is the error a Python `KeyError` would be rather than a silent zero.
const SECRET_KEYS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 128, 130,
    132, 134, 136,
];

/// `kSpriteNames` has 284 entries (`tables.py:860`); overworld sprite bytes
/// stop at `0xff`, so every value read is a valid index.
const SPRITE_NAME_COUNT: usize = 284;

/// `print_overworld_area`'s `get_items` (`extract_resources.py:167-178`).
fn get_items(rom: &Rom, area: u32) -> Result<Vec<Item>> {
    if area >= 128 {
        return Ok(Vec::new());
    }
    let mut ea = 0x9b0000 | rom.get_word(0x9BC2F9 + area * 2)?;
    let mut xs = Vec::new();
    while rom.get_word(ea)? != 0xffff {
        let pos = rom.get_word(ea)? as i32;
        if pos % 2 != 0 {
            return Err(format!("area {area}: item position {pos} is odd"));
        }
        let secret = rom.get_byte(ea + 2)?;
        if !SECRET_KEYS.contains(&secret) {
            return Err(format!("no kSecretNames entry for {secret:#x}"));
        }
        xs.push(Item { x: pos / 2 % 64, y: pos / 2 / 64, secret });
        ea += 3;
    }
    Ok(xs)
}

/// `decode_sprites` (`extract_resources.py:204-211`). Note the ROM order is
/// `y, x, w` and the list stores `[x, y, name]`.
fn decode_sprites(rom: &Rom, base_addr: u32, area: u32) -> Result<Vec<[i32; 3]>> {
    let mut ea = 0x890000 + rom.get_word(base_addr + area * 2)?;
    let mut r = Vec::new();
    while rom.get_byte(ea)? != 0xff {
        let y = rom.get_byte(ea)? as i32;
        let x = rom.get_byte(ea + 1)? as i32;
        let w = rom.get_byte(ea + 2)? as usize;
        if w >= SPRITE_NAME_COUNT {
            return Err(format!("no kSpriteNames entry for {w}"));
        }
        r.push([x, y, w as i32]);
        ea += 3;
    }
    Ok(r)
}

/// `print_overworld_area` (`extract_resources.py:145-233`) for one area, minus
/// the `yaml.dump`.
pub fn read_overworld_area(
    rom: &Rom,
    area: u32,
    is_small: &[u8],
    travel: &[Vec<Travel>],
    entrances: &[Vec<OwEntrance>],
    holes: &[Vec<Hole>],
    exits: &[Vec<Exit>],
) -> Result<Area> {
    // `get_music`: four slots below 64, one above.
    let music_slots = if area < 64 {
        let mut v = Vec::with_capacity(4);
        for k in [0u32, 64, 128, 192] {
            v.push(MusicSlot::from_byte(rom.get_byte(0x82C303 + area + k)?)?);
        }
        v
    } else {
        if area >= 64 + 96 {
            return Err(format!("area {area} is past the music tables"));
        }
        vec![MusicSlot::from_byte(rom.get_byte(0x82C403 + area - 64)?)?]
    };

    let header = Header {
        is_small: is_small[area as usize] != 0,
        gfx: if area < 128 { rom.get_byte(0x80FC9C + area)? as i32 } else { -1 },
        palette: if area < 136 { rom.get_byte(0x80FD1C + area)? as i32 } else { -1 },
        sign_text: if area < 128 { rom.get_word(0x87F51D + area * 2)? as i32 } else { -1 },
        music: music_slots,
    };

    // `get_info(stage)` — an empty map for areas >= 128, and the stage is
    // forced to 3 for areas >= 64.
    let get_info = |stage: u32| -> Result<Option<(i32, i32)>> {
        if area >= 128 {
            return Ok(None);
        }
        let stage = if area >= 64 { 3 } else { stage };
        let k = (area & 63) + stage * 64;
        Ok(Some((
            rom.get_byte(0x80FA41 + k)? as i32,
            rom.get_byte(0x80FB41 + k)? as i32,
        )))
    };

    let (beginning, first, second, plain) = if area < 64 {
        (
            Some(SpriteStage { info: get_info(0)?, sprites: decode_sprites(rom, 0x89C881, area)? }),
            Some(SpriteStage { info: get_info(1)?, sprites: decode_sprites(rom, 0x89C901, area)? }),
            Some(SpriteStage { info: get_info(2)?, sprites: decode_sprites(rom, 0x89CA21, area)? }),
            None,
        )
    } else if area < 144 {
        (
            None,
            None,
            None,
            Some(SpriteStage { info: get_info(2)?, sprites: decode_sprites(rom, 0x89CA21, area)? }),
        )
    } else {
        (None, None, None, None)
    };

    Ok(Area {
        index: area,
        header,
        travel: travel[area as usize].clone(),
        entrances: entrances[area as usize].clone(),
        holes: holes[area as usize].clone(),
        exits: exits[area as usize].clone(),
        items: get_items(rom, area)?,
        sprites_beginning: beginning,
        sprites_first_part: first,
        sprites_second_part: second,
        sprites: plain,
    })
}

/// `print_all_overworld_areas` (`:235-240`) plus `load_overworld_yaml` on the
/// compile side, collapsed: the areas that exist, in ascending order. This is
/// `loaded_areas` (`compile_resources.py:250`).
pub fn read_all_overworld_areas(rom: &Rom) -> Result<Vec<Area>> {
    read_areas(rom, &read_links(rom)?)
}

/// The four cross-area tables every overworld area head is decorated with,
/// plus the small/large flags: `get_ow_travel_infos`, `get_ow_entrance_info`,
/// `get_hole_infos` and `get_exit_datas`, each bucketed by screen index.
///
/// Read once and shared by all 160 areas, so it is its own stage.
pub struct Links {
    pub is_small: Vec<u8>,
    pub travel: Vec<Vec<Travel>>,
    pub entrances: Vec<Vec<OwEntrance>>,
    pub holes: Vec<Vec<Hole>>,
    pub exits: Vec<Vec<Exit>>,
}

/// Stage "Reading overworld links". Must run before [`read_areas`].
pub fn read_links(rom: &Rom) -> Result<Links> {
    Ok(Links {
        is_small: rom.get_bytes(0x82F88D, 192)?,
        travel: get_ow_travel_infos(rom)?,
        entrances: get_ow_entrance_info(rom)?,
        holes: get_hole_infos(rom)?,
        exits: get_exit_datas(rom)?,
    })
}

/// Stage "Reading the overworld": `print_overworld_area` for the area heads.
pub fn read_areas(rom: &Rom, l: &Links) -> Result<Vec<Area>> {
    let mut out = Vec::new();
    for i in 0..160u32 {
        if is_area_head(rom, i)? {
            out.push(read_overworld_area(
                rom, i, &l.is_small, &l.travel, &l.entrances, &l.holes, &l.exits,
            )?);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Assets 107-164 — the overworld tables
// ---------------------------------------------------------------------------

/// One `OutArrays` slot: a fixed-size array of "not yet written" slots.
///
/// `initializer = None` is `None`, and [`OutArray::finish`] is
/// `assert isinstance(j, int)` (`compile_resources.py:218`). Pre-zeroing would
/// turn a slot the data failed to cover into a silently wrong zero, so the
/// `Option` is load-bearing rather than decorative.
struct OutArray {
    name: &'static str,
    slots: Vec<Option<i32>>,
}

impl OutArray {
    fn new(name: &'static str, size: usize) -> OutArray {
        OutArray { name, slots: vec![None; size] }
    }

    /// `initializer = 0`.
    fn zeroed(name: &'static str, size: usize) -> OutArray {
        OutArray { name, slots: vec![Some(0); size] }
    }

    fn len(&self) -> usize {
        self.slots.len()
    }

    fn get(&self, i: usize) -> Option<i32> {
        self.slots[i]
    }

    /// A plain `arr[i] = v`, with Python's out-of-range `IndexError`.
    fn set(&mut self, i: usize, v: i32) -> Result<()> {
        let name = self.name;
        let n = self.slots.len();
        *self
            .slots
            .get_mut(i)
            .ok_or_else(|| format!("{name}[{i}] is out of range (len {n})"))? = Some(v);
        Ok(())
    }

    /// The `assert isinstance(j, int)` pass.
    fn finish(&self) -> Result<Vec<i32>> {
        let mut out = Vec::with_capacity(self.slots.len());
        for (i, s) in self.slots.iter().enumerate() {
            out.push(s.ok_or_else(|| format!("{}[{i}] was never written", self.name))?);
        }
        Ok(out)
    }
}

/// `awrite` (`compile_resources.py:248-252`). A big area occupies a 2x2 block
/// of screens, so the value is mirrored into `key + 1`, `key + 8` and
/// `key + 9`. The smallness test is on the *area*, the write on the *key*, and
/// the two are not always the same number (`kOverworldSpriteGfx` indexes by
/// `(i & 63) + stage * 64`).
fn awrite(arr: &mut OutArray, is_small: &OutArray, area: u32, key: usize, value: i32) -> Result<()> {
    arr.set(key, value)?;
    if area < 128 && is_small.get(area as usize) == Some(0) {
        arr.set(key + 1, value)?;
        arr.set(key + 8, value)?;
        arr.set(key + 9, value)?;
    }
    Ok(())
}

/// Assets **107-164**: everything `print_overworld_tables`
/// (`compile_resources.py:231-437`) emits, in `A.add` registration order,
/// followed by the two direct ROM extracts.
pub fn add_overworld_tables(rom: &Rom, a: &mut Assets) -> Result<()> {
    add_overworld_tables_from(rom, &read_all_overworld_areas(rom)?, a)
}

/// [`add_overworld_tables`] over already-read areas, so the reading and
/// building halves can be separate phases without reading the ROM twice.
pub fn add_overworld_tables_from(rom: &Rom, loaded_areas: &[Area], a: &mut Assets) -> Result<()> {

    // --- the header-driven arrays -----------------------------------------
    let mut is_small = OutArray::zeroed("kOverworldMapIsSmall", 192);
    let mut aux_tile_theme = OutArray::new("kOverworldAuxTileThemeIndexes", 128);
    let mut bg_palettes = OutArray::new("kOverworldBgPalettes", 136);
    let mut sign_text = OutArray::new("kOverworld_SignText", 128);
    let mut music_sets = OutArray::new("kOwMusicSets", 256);
    let mut music_sets2 = OutArray::new("kOwMusicSets2", 96);

    // Pass one fills `kOverworldMapIsSmall` as it goes, and `awrite` reads it
    // back within the same loop -- an area's own entry is always set before its
    // first `awrite`, so the ordering is safe and is kept literal.
    for area in loaded_areas {
        let i = area.index;
        let h = &area.header;
        is_small.set(i as usize, if h.is_small { 1 } else { 0 })?;
        if (i as usize) < aux_tile_theme.len() {
            let v = h.gfx;
            awrite(&mut aux_tile_theme, &is_small, i, i as usize, v)?;
        }
        if (i as usize) < bg_palettes.len() {
            let v = h.palette;
            awrite(&mut bg_palettes, &is_small, i, i as usize, v)?;
        }
        if (i as usize) < sign_text.len() {
            let v = h.sign_text;
            awrite(&mut sign_text, &is_small, i, i as usize, v)?;
        }
        if i < 64 {
            for (k, slot) in h.music.iter().enumerate() {
                let key = i as usize + k * 64;
                let v = slot.to_byte();
                awrite(&mut music_sets, &is_small, i, key, v)?;
            }
        } else if i < 64 + 96 {
            let v = h.agahnim().to_byte();
            awrite(&mut music_sets2, &is_small, i, i as usize - 64, v)?;
        }
    }

    // --- bird travel and whirlpools ---------------------------------------
    let mut bt_screen = OutArray::new("kBirdTravel_ScreenIndex", 17);
    let mut bt_loadsrc = OutArray::new("kBirdTravel_Map16LoadSrcOff", 17);
    let mut bt_scroll_x = OutArray::new("kBirdTravel_ScrollX", 17);
    let mut bt_scroll_y = OutArray::new("kBirdTravel_ScrollY", 17);
    let mut bt_link_x = OutArray::new("kBirdTravel_LinkXCoord", 17);
    let mut bt_link_y = OutArray::new("kBirdTravel_LinkYCoord", 17);
    let mut bt_cam_x = OutArray::new("kBirdTravel_CameraXScroll", 17);
    let mut bt_cam_y = OutArray::new("kBirdTravel_CameraYScroll", 17);
    let mut bt_unk1 = OutArray::new("kBirdTravel_Unk1", 17);
    let mut bt_unk3 = OutArray::new("kBirdTravel_Unk3", 17);
    let mut whirlpool_areas = OutArray::new("kWhirlpoolAreas", 8);

    let mut next_whirlpool_id = 0usize;
    for area in loaded_areas {
        let i = area.index;
        for t in &area.travel {
            let j = match t.bird_travel_id {
                Some(id) => id,
                None => {
                    whirlpool_areas.set(next_whirlpool_id, t.whirlpool_src_area)?;
                    let j = next_whirlpool_id + 9;
                    next_whirlpool_id += 1;
                    j
                }
            };
            let base_x = ((i & 7) << 9) as i32;
            let base_y = ((i & 56) << 6) as i32;
            bt_screen.set(j, i as i32)?;
            bt_loadsrc.set(j, get_loadoffs(t.scroll_xy, t.load_xy))?;
            bt_scroll_x.set(j, t.scroll_xy[0] + base_x)?;
            bt_scroll_y.set(j, t.scroll_xy[1] + base_y)?;
            bt_link_x.set(j, t.xy[0] + base_x)?;
            bt_link_y.set(j, t.xy[1] + base_y)?;
            bt_cam_x.set(j, t.camera_xy[0] + base_x)?;
            bt_cam_y.set(j, t.camera_xy[1] + base_y)?;
            bt_unk1.set(j, t.unk[0])?;
            bt_unk3.set(j, t.unk[1])?;
        }
    }

    // --- overworld entrances ----------------------------------------------
    let mut ent_area = OutArray::new("kOverworld_Entrance_Area", 129);
    let mut ent_pos = OutArray::new("kOverworld_Entrance_Pos", 129);
    let mut ent_id = OutArray::new("kOverworld_Entrance_Id", 129);

    for area in loaded_areas {
        for e in &area.entrances {
            let j = e.index;
            if ent_id.get(j).is_some() {
                return Err(format!("overworld entrance {j} was claimed twice"));
            }
            ent_area.set(j, area.index as i32)?;
            ent_id.set(j, e.entrance_id)?;
            ent_pos.set(j, (e.x << 1) | (e.y << 7))?;
        }
    }

    // --- holes -------------------------------------------------------------
    let mut hole_area = OutArray::new("kFallHole_Area", 19);
    let mut hole_pos = OutArray::new("kFallHole_Pos", 19);
    let mut hole_entrances = OutArray::new("kFallHole_Entrances", 19);

    // `(entrance_id, pos, area)` tuples, then `sorted(holes)`: a plain
    // lexicographic tuple sort. `(y - 8) & 0x3f` goes negative for y < 8.
    let mut holes: Vec<(i32, i32, i32)> = Vec::new();
    for area in loaded_areas {
        for e in &area.holes {
            holes.push((
                e.entrance_id,
                (e.x << 1) | (((e.y - 8) & 0x3f) << 7),
                area.index as i32,
            ));
        }
    }
    holes.sort();
    for (i, (entrance, pos, area)) in holes.iter().enumerate() {
        hole_area.set(i, *area)?;
        hole_pos.set(i, *pos)?;
        hole_entrances.set(i, *entrance)?;
    }

    // --- exits and special exits ------------------------------------------
    let mut ex_screen = OutArray::new("kExitData_ScreenIndex", 79);
    let mut ex_rooms = OutArray::new("kExitDataRooms", 79);
    let mut ex_loadsrc = OutArray::new("kExitData_Map16LoadSrcOff", 79);
    let mut ex_scroll_x = OutArray::new("kExitData_ScrollX", 79);
    let mut ex_scroll_y = OutArray::new("kExitData_ScrollY", 79);
    let mut ex_x = OutArray::new("kExitData_XCoord", 79);
    let mut ex_y = OutArray::new("kExitData_YCoord", 79);
    let mut ex_cam_x = OutArray::new("kExitData_CameraXScroll", 79);
    let mut ex_cam_y = OutArray::new("kExitData_CameraYScroll", 79);
    let mut ex_ndoor = OutArray::zeroed("kExitData_NormalDoor", 79);
    let mut ex_fdoor = OutArray::zeroed("kExitData_FancyDoor", 79);
    let mut ex_unk1 = OutArray::new("kExitData_Unk1", 79);
    let mut ex_unk3 = OutArray::new("kExitData_Unk3", 79);

    let mut sp_top = OutArray::zeroed("kSpExit_Top", 16);
    let mut sp_bottom = OutArray::zeroed("kSpExit_Bottom", 16);
    let mut sp_left = OutArray::zeroed("kSpExit_Left", 16);
    let mut sp_right = OutArray::zeroed("kSpExit_Right", 16);
    let mut sp_tab4 = OutArray::zeroed("kSpExit_Tab4", 16);
    let mut sp_tab5 = OutArray::zeroed("kSpExit_Tab5", 16);
    let mut sp_tab6 = OutArray::zeroed("kSpExit_Tab6", 16);
    let mut sp_tab7 = OutArray::zeroed("kSpExit_Tab7", 16);
    let mut sp_left_edge = OutArray::zeroed("kSpExit_LeftEdgeOfMap", 16);
    let mut sp_dir = OutArray::zeroed("kSpExit_Dir", 16);
    let mut sp_spr_gfx = OutArray::zeroed("kSpExit_SprGfx", 16);
    let mut sp_aux_gfx = OutArray::zeroed("kSpExit_AuxGfx", 16);
    let mut sp_pal_bg = OutArray::zeroed("kSpExit_PalBg", 16);
    let mut sp_pal_spr = OutArray::zeroed("kSpExit_PalSpr", 16);

    for area in loaded_areas {
        let i = area.index;
        for e in &area.exits {
            let j = e.index;
            let base_x = ((i & 7) << 9) as i32;
            let base_y = ((i & 56) << 6) as i32;
            if ex_screen.get(j).is_some() {
                return Err(format!("exit {j} was claimed twice"));
            }
            ex_screen.set(j, i as i32)?;
            ex_rooms.set(j, e.room)?;
            ex_loadsrc.set(j, get_loadoffs(e.scroll_xy, e.load_xy))?;
            ex_scroll_x.set(j, e.scroll_xy[0] + base_x)?;
            ex_scroll_y.set(j, e.scroll_xy[1] + base_y)?;
            ex_x.set(j, e.xy[0] + base_x)?;
            ex_y.set(j, e.xy[1] + base_y)?;
            ex_cam_x.set(j, e.camera_xy[0] + base_x)?;
            ex_cam_y.set(j, e.camera_xy[1] + base_y)?;
            ex_unk1.set(j, e.unk[0])?;
            ex_unk3.set(j, e.unk[1])?;
            if let Some((kind, d1, d2)) = e.door {
                let bits = (d1 << 1) | (d2 << 7);
                match kind {
                    DoorKind::Bombable => ex_ndoor.set(j, bits | 0x8000)?,
                    DoorKind::Wooden => ex_ndoor.set(j, bits)?,
                    DoorKind::Palace => ex_fdoor.set(j, bits | 0x8000)?,
                    DoorKind::Sanctuary => ex_fdoor.set(j, bits)?,
                }
            }
            if let Some(se) = e.special_exit {
                // The Python rebinds `j` here; the special-exit slot is indexed
                // by the room, not by the exit.
                let j = (e.room - 0x180) as usize;
                sp_dir.set(j, se.dir * 2)?;
                sp_spr_gfx.set(j, se.spr_gfx)?;
                sp_aux_gfx.set(j, se.aux_gfx)?;
                sp_pal_bg.set(j, se.pal_bg)?;
                sp_pal_spr.set(j, se.pal_spr)?;
                sp_top.set(j, se.top)?;
                sp_bottom.set(j, se.bottom)?;
                sp_left.set(j, se.left)?;
                sp_right.set(j, se.right)?;
                sp_left_edge.set(j, se.left_edge_of_map)?;
                sp_tab4.set(j, se.unk4)?;
                sp_tab5.set(j, se.unk5)?;
                sp_tab6.set(j, se.unk6)?;
                sp_tab7.set(j, se.unk7)?;
            }
        }
    }

    // --- overworld secrets -------------------------------------------------
    let mut secrets_offs = OutArray::new("kOverworldSecrets_Offs", 128);
    let mut secrets: Vec<u8> = Vec::new();
    for area in loaded_areas {
        let i = area.index;
        if area.items.is_empty() {
            continue;
        }
        if i >= 128 {
            return Err(format!("area {i} has Items but is not a light/dark screen"));
        }
        let j = secrets.len() as i32;
        awrite(&mut secrets_offs, &is_small, i, i as usize, j)?;
        for e in &area.items {
            let pos = (e.x << 1) | (e.y << 7);
            secrets.push((pos & 0xff) as u8);
            secrets.push((pos >> 8) as u8);
            secrets.push(e.secret);
        }
        secrets.push(0xff);
        secrets.push(0xff);
    }
    // Areas with no secrets point at the terminator of the whole blob.
    let tail = secrets.len() as i32 - 2;
    for i in 0..128 {
        if secrets_offs.get(i).is_none() {
            secrets_offs.set(i, tail)?;
        }
    }

    // --- overworld sprites -------------------------------------------------
    let mut sprite_offs = OutArray::zeroed("kOverworldSpriteOffs", 144 * 3);
    let mut sprites: Vec<u8> = vec![0xff];
    let mut sprite_gfx = OutArray::new("kOverworldSpriteGfx", 256);
    let mut sprite_palettes = OutArray::new("kOverworldSpritePalettes", 256);

    // `do_sprite_range` (`compile_resources.py:406-418`). `stagename` is a
    // selector over the four sprite maps; `sprite_stage_idxs` are the
    // `kOverworldSpriteOffs` planes the list is registered into.
    #[derive(Clone, Copy)]
    enum StageName {
        Beginning,
        FirstPart,
        SecondPart,
        Plain,
    }
    fn pick(area: &Area, s: StageName) -> Result<&SpriteStage> {
        let v = match s {
            StageName::Beginning => &area.sprites_beginning,
            StageName::FirstPart => &area.sprites_first_part,
            StageName::SecondPart => &area.sprites_second_part,
            StageName::Plain => &area.sprites,
        };
        v.as_ref().ok_or_else(|| format!("area {} has no sprite stage", area.index))
    }

    for (start, end, stagename, stage_idxs, infostage) in [
        (0u32, 64u32, StageName::Beginning, &[0usize][..], 0usize),
        (0, 64, StageName::FirstPart, &[1][..], 1),
        (0, 64, StageName::SecondPart, &[2][..], 2),
        (64, 144, StageName::Plain, &[1, 2][..], 3),
    ] {
        for area in loaded_areas {
            let i = area.index;
            if i < start || i >= end {
                continue;
            }
            let st = pick(area, stagename)?;
            if i < 128 {
                let (gfx, palette) = st
                    .info
                    .ok_or_else(|| format!("area {i} has no sprite info"))?;
                let key = ((i & 63) as usize) + infostage * 64;
                awrite(&mut sprite_gfx, &is_small, i, key, gfx)?;
                awrite(&mut sprite_palettes, &is_small, i, key, palette)?;
            }
            if !st.sprites.is_empty() {
                for stage in stage_idxs {
                    sprite_offs.set(stage * 144 + i as usize, sprites.len() as i32)?;
                }
                for e in &st.sprites {
                    sprites.push(e[1] as u8);
                    sprites.push(e[0] as u8);
                    sprites.push(e[2] as u8);
                }
                sprites.push(0xff);
            }
        }
    }

    // --- A.write(), in registration order ---------------------------------
    fn u8s(a: &OutArray) -> Result<Vec<u8>> {
        a.finish()?
            .into_iter()
            .map(|v| {
                u8::try_from(v).map_err(|_| format!("{}: {v} does not fit in a uint8", a.name))
            })
            .collect()
    }
    fn u16s(a: &OutArray) -> Result<Vec<u16>> {
        a.finish()?
            .into_iter()
            .map(|v| {
                u16::try_from(v).map_err(|_| format!("{}: {v} does not fit in a uint16", a.name))
            })
            .collect()
    }

    macro_rules! out_u8 {
        ($arr:expr) => {{
            let v = u8s(&$arr)?;
            a.add_uint8($arr.name, &v)?;
        }};
    }
    macro_rules! out_u16 {
        ($arr:expr) => {{
            let v = u16s(&$arr)?;
            a.add_uint16($arr.name, &v)?;
        }};
    }
    macro_rules! out_i8 {
        ($arr:expr) => {{
            let v = $arr.finish()?;
            a.add_int8($arr.name, &v)?;
        }};
    }
    macro_rules! out_i16 {
        ($arr:expr) => {{
            let v = $arr.finish()?;
            a.add_int16($arr.name, &v)?;
        }};
    }

    out_u8!(is_small);
    out_u8!(aux_tile_theme);
    out_u8!(bg_palettes);
    out_u16!(sign_text);
    out_u8!(music_sets);
    out_u8!(music_sets2);

    out_u16!(bt_screen);
    out_u16!(bt_loadsrc);
    out_u16!(bt_scroll_x);
    out_u16!(bt_scroll_y);
    out_u16!(bt_link_x);
    out_u16!(bt_link_y);
    out_u16!(bt_cam_x);
    out_u16!(bt_cam_y);
    out_i8!(bt_unk1);
    out_i8!(bt_unk3);
    out_u16!(whirlpool_areas);

    out_u16!(ent_area);
    out_u16!(ent_pos);
    out_u8!(ent_id);

    out_u16!(hole_area);
    out_u16!(hole_pos);
    out_u8!(hole_entrances);

    out_u8!(ex_screen);
    out_u16!(ex_rooms);
    out_u16!(ex_loadsrc);
    out_u16!(ex_scroll_x);
    out_u16!(ex_scroll_y);
    out_u16!(ex_x);
    out_u16!(ex_y);
    out_u16!(ex_cam_x);
    out_u16!(ex_cam_y);
    out_u16!(ex_ndoor);
    out_u16!(ex_fdoor);
    out_i8!(ex_unk1);
    out_i8!(ex_unk3);

    out_u16!(sp_top);
    out_u16!(sp_bottom);
    out_u16!(sp_left);
    out_u16!(sp_right);
    out_i16!(sp_tab4);
    out_i16!(sp_tab5);
    out_i16!(sp_tab6);
    out_i16!(sp_tab7);
    out_u16!(sp_left_edge);
    out_u8!(sp_dir);
    out_u8!(sp_spr_gfx);
    out_u8!(sp_aux_gfx);
    out_u8!(sp_pal_bg);
    out_u8!(sp_pal_spr);

    out_u16!(secrets_offs);
    a.add_uint8("kOverworldSecrets", &secrets)?;

    out_u16!(sprite_offs);
    a.add_uint8("kOverworldSprites", &sprites)?;
    out_u8!(sprite_gfx);
    out_u8!(sprite_palettes);

    // The two tail extracts, outside `OutArrays` (`:436-437`).
    a.add_uint8("kMap8DataToTileAttr", &rom.get_bytes(0x8E9459, 512)?)?;
    a.add_uint8("kSomeTileAttr", &rom.get_bytes(0x9bf110, 3824)?)?;
    Ok(())
}

/// Every asset this module owns, in the order PORTING-MAP.md section 2 gives:
/// 60-63, then 105-106, then 107-164.
///
/// The three stages are also public individually so the phase table can drive
/// them separately: [`add_map32_to_map16`] (60-63), [`add_overworld`]
/// (105-106), [`add_overworld_tables`] (107-164).
pub fn add_all(rom: &Rom, a: &mut Assets) -> Result<()> {
    add_map32_to_map16(rom, a)?;
    add_overworld(rom, a)?;
    add_overworld_tables(rom, a)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loadoffs_masks_after_a_negative_subtraction() {
        // scroll_xy[1] negative: `-16 >> 4` is -1, and the pair must still
        // round-trip through the 6-bit mask.
        assert_eq!(get_loadoffs([-16, -16], [1, 1]), 0);
        assert_eq!(get_loadoffs([0, 0], [0x3f, 0x3f]), (0x3f << 7) | (0x3f << 1));
    }

    #[test]
    fn holes_sort_lexicographically_on_the_tuple() {
        let mut v = vec![(3, 100, 5), (1, 999, 0), (1, 2, 7)];
        v.sort();
        assert_eq!(v, vec![(1, 2, 7), (1, 999, 0), (3, 100, 5)]);
    }

    #[test]
    fn an_unwritten_slot_is_an_error_not_a_zero() {
        let a = OutArray::new("kThing", 3);
        assert!(a.finish().is_err());
    }
}

/// The byte-exactness test. Builds a .dat holding only the overworld assets and
/// diffs it against the Python's `zelda3_assets.dat` with `compare.mjs`.
///
/// ```sh
/// ZELDA3_ROM="/path/to/zelda3.smc" ZELDA3_ORACLE=/path/to/zelda3_assets.dat \
///   cargo test -- --ignored overworld
/// ```
#[cfg(test)]
mod oracle_tests {
    use super::*;
    use crate::pack::Assets;

    #[test]
    #[ignore = "needs ZELDA3_ROM and ZELDA3_ORACLE"]
    fn overworld_assets_match_the_python() {
        let Ok(rom_path) = std::env::var("ZELDA3_ROM") else { return };
        let Ok(oracle) = std::env::var("ZELDA3_ORACLE") else { return };
        let rom = Rom::new(std::fs::read(rom_path).unwrap());
        assert_eq!(rom.language, Some("us"));

        let mut a = Assets::new();
        add_all(&rom, &mut a).unwrap();

        let dir = std::env::temp_dir().join("zelda3-overworld-test");
        std::fs::create_dir_all(&dir).unwrap();
        let ours = dir.join("overworld.dat");
        std::fs::write(&ours, a.serialise()).unwrap();

        let out = std::process::Command::new("node")
            .arg("compare.mjs")
            .arg(&oracle)
            .arg(&ours)
            .arg("--all")
            .output()
            .expect("node compare.mjs");
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        println!("{text}");

        // Rows read `name  <oracle>  <ours>  status`; the status is last.
        let ok = |name: &str| {
            text.lines().any(|l| {
                let t = l.trim_end();
                t.starts_with(name)
                    && t[name.len()..].starts_with(' ')
                    && t.ends_with(" ok")
            })
        };
        let bad: Vec<String> = a
            .iter()
            .map(|i| i.name.clone())
            .filter(|n| !ok(n))
            .collect();
        assert!(bad.is_empty(), "not ok: {bad:?}");
    }
}
