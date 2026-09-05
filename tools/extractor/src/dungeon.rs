//! The dungeon half of the pipeline: assets 3-10, 11-45, 46-55, 58-59, 97-98.
//!
//! This is a port of both halves of the Python for those keys — the reading
//! side in `extract_resources.py` (`print_room` x320, `get_entrance_info`,
//! `print_default_rooms`, `print_overlay_rooms`, `get_chest_info`,
//! `pits_hurt_player`) and the writing side in `compile_resources.py`
//! (`print_dungeon_rooms`, `print_dungeon_sprites`, `print_dungeon_secrets`,
//! `print_dungeon_map`). The YAML that sits between them in the Python is not
//! reproduced; the structures below are what the files would have carried.
//!
//! # Names are not carried
//!
//! The Python round-trips almost every enumerated field through a *name*:
//! `extract` writes `tables.kType0Names[index]`, `compile` reads
//! `tables.kType0Names_rev[name]`. Whether that composes to the identity
//! depends on the tables having no duplicate names, which is not obvious and
//! is not documented anywhere. It was checked exhaustively against
//! `tables.py`, and the result is:
//!
//! | table | round trip |
//! |---|---|
//! | `kType0Names` (248), `kType1Names` (128), `kType2Names` (65) | identity, and no name is shared *between* the three, so `print_layer`'s `if/elif/elif` probe always lands on the originating type |
//! | `kSpriteNames` (284) incl. the `".%d"` subtype infix | identity for every `(type, subtype)` pair whose base name contains `-`; the 28 names without one (`'02'`, `'10'`, …) would raise in `extract` for a non-zero subtype and so cannot occur |
//! | `kSecretNames`, `kMusicNames` (dicts) | identity |
//! | `kPalaceNames`, `kBg2`, `kCollisionNames`, `kEffectNames` | identity |
//! | `kTagNames` (64) | identity **except** index 31, which shares the name `"Crash"` with index 30 and therefore encodes back as 30 |
//!
//! So the port carries indices, not names, and reproduces the one lossy case
//! explicitly ([`encode_tag`]). Every place the Python would have raised —
//! an index outside a name table, a `KeyError` on a dict — is reproduced as an
//! `Err` rather than being papered over, so a table that stops being an
//! identity becomes a loud failure instead of a silent byte change.
//!
//! # The lossy conversions that *do* change bytes
//!
//! Several fields survive the trip only partially, and the .dat contains the
//! narrowed value rather than the ROM's. These are reproduced deliberately:
//!
//! - room header `p8`: only `& 3` survives (`stair3_dest[1]`).
//! - door bytes: bits 2-3 of the position byte are dropped.
//! - `kEntranceData_quadrant1`: the source byte is masked to `& 0x22`.
//! - `kEntranceData_doorSettings`: masked to `& 0xbffe`.
//! - `kEntranceData_palace`: `v -> ((v + 2) >> 1)`, then `-1` or `(i-1)*2`;
//!   an odd positive `v` does not survive.
//! - a "drop key" sprite's x byte is forced to 0, and a sprite carrying *two*
//!   drop markers emits **none** (`compile_resources.py:499` tests
//!   `len(s) == 5`, and two markers make the list 6 long).
//! - a high sprite (`kSpriteNames[type + 0x100]`) loses bits 5-6 of its y byte.
//!
//! # `Layer3.doors or []`
//!
//! `extract_resources.py:472,476,480` writes a `Layer*.doors` key only when
//! the door list is non-empty, so on the compile side a `None` covers both
//! "no door section in the stream" and "an empty door section".
//! `print_dungeon_rooms` then treats layers 1 and 2 with `y.get(...)` — `None`
//! writes no `0xf0 0xff` marker — but layer 3 with `y.get(...) or []`, which
//! turns `None` into an empty list and *does* write the marker and set the
//! door offset. That asymmetry is the reason `kDungeonRoomDoorOffs` is always
//! defined. It is reproduced in [`add_rooms`] exactly, via
//! [`Room::doors_for_layer`].

use crate::pack::Assets;
use crate::rom::Rom;

type Result<T> = core::result::Result<T, String>;

// ---------------------------------------------------------------------------
// Table sizes and the few validity sets that are not contiguous ranges.
// ---------------------------------------------------------------------------

/// `len(tables.kType0Names)`.
const N_TYPE0: u32 = 248;
/// `len(tables.kType1Names)`.
const N_TYPE1: u32 = 128;
/// `len(tables.kType2Names)`.
const N_TYPE2: u32 = 65;
/// `len(tables.kSpriteNames)` — indices `0..0x100` are the ordinary sprites and
/// `0x100..0x11c` the ones reached through the `x >= 0xe0` encoding.
const N_SPRITE: u32 = 284;
/// `len(tables.kBg2)`.
const N_BG2: u32 = 9;
/// `len(tables.kCollisionNames)` — note the field is 3 bits wide, so a room
/// with collision 5, 6 or 7 would raise in the Python.
const N_COLLISION: u32 = 5;
/// `len(tables.kEffectNames)`.
const N_EFFECT: u32 = 8;
/// `len(tables.kTagNames)`.
const N_TAG: u32 = 64;

/// `tables.kMusicNames.keys()` — a dict, and deliberately not a contiguous
/// range: a music byte outside this set is a `KeyError` in the Python.
const MUSIC_KEYS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 240, 241, 242, 243, 255,
];

/// `tables.kSecretNames.keys()` — likewise a dict with gaps.
const SECRET_KEYS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 128, 130,
    132, 134, 136,
];

/// `tables.kPalaceNames` has 15 entries; the index is `(int8 + 2) >> 1`.
const N_PALACE: i32 = 15;

/// The one place a name table is *not* an identity. `kTagNames[30]` and
/// `kTagNames[31]` are both `"Crash"`, so `kTagNames_rev` — built by
/// `invert_list`, last write wins — maps that name to 30, and a room tagged 31
/// would come back as 30. No room in the US ROM is, but reproducing it costs
/// one comparison and keeps the port honest about the round trip.
fn encode_tag(tag: u8) -> u8 {
    if tag == 31 {
        30
    } else {
        tag
    }
}

// ---------------------------------------------------------------------------
// The decoded structures — what the YAML would have carried.
// ---------------------------------------------------------------------------

/// One entry of `Layer1` / `Layer2` / `Layer3`.
///
/// `kind` is which of the three name tables the object came from, which on the
/// compile side decides the encoding. The Python rediscovers it by probing the
/// three `_rev` dicts in order; because no name is shared between the tables
/// that probe always returns the originating type, so carrying it is exact.
#[derive(Clone, Copy, Debug)]
pub struct Obj {
    pub x: u8,
    pub y: u8,
    pub kind: u8,
    /// The index within that table. For `kind == 1` this is the *combined*
    /// `index2 = (index & 7) << 4 | H << 2 | W`, which is what the name is
    /// looked up by and therefore what survives.
    pub index: u8,
    /// Only meaningful for `kind == 0`; the `'s'` field, `"W*H"`.
    pub w: u8,
    pub h: u8,
}

/// One entry of `Layer*.doors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Door {
    pub ty: u8,
    pub pos: u8,
    pub dir: u8,
}

/// One entry of `Sprites`.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub x: u8,
    pub y: u8,
    /// 0 = `"upper"`, 1 = `"lower"`.
    pub floor: u8,
    /// Index into `kSpriteNames`; `>= 0x100` means the `x >= 0xe0` encoding.
    pub idx: u16,
    /// The subtype infixed into the name as `".%d"`. Always 0 for `idx >= 0x100`.
    pub subtype: u8,
    /// The trailing `"drop_key"` / `"drop_big_key"` markers appended to this
    /// sprite's list, as their y bytes (`0xfe` / `0xfd`). Held as a list rather
    /// than an `Option` so the two-marker case reproduces the Python's
    /// `len(s) == 5` test, which emits nothing at all.
    pub drops: Vec<u8>,
}

/// One entry of `Secrets`.
#[derive(Clone, Copy, Debug)]
pub struct Secret {
    pub x: u8,
    pub y: u8,
    /// Key into `kSecretNames`.
    pub kind: u8,
}

/// One entry of `Chests`. The Python list is heterogeneous — a plain `int` for
/// an ordinary chest, a `str` ending in `!` for a big one — which is an enum
/// here rather than two parallel lists, so the ordering within a room (which
/// reaches `kDungeonRoomChests`) cannot drift.
#[derive(Clone, Copy, Debug)]
pub struct Chest {
    pub data: u8,
    pub big: bool,
}

/// `print_room`'s `header` dict, decomposed the way the YAML holds it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Header {
    pub floor1: u8,
    pub floor2: u8,
    pub layout: u8,
    pub start_quadrant: u8,
    pub bg2: u8,
    pub collision: u8,
    pub lights_out: u8,
    pub palette: u8,
    pub blockset: u8,
    pub enemyblk: u8,
    pub effect: u8,
    pub tag0: u8,
    pub tag1: u8,
    /// `hole0_dest`, `stair0_dest` .. `stair3_dest`: the destination byte and
    /// the two-bit selector packed into `p7` / `p8`.
    pub dest: [(u8, u8); 5],
    pub tele_msg: u16,
    pub sort_sprites: u8,
    pub pits_hurt_player: bool,
}

impl Header {
    /// `get_room_header` (`compile_resources.py:562-577`) — the 14-byte record
    /// that `append_scan_bytes` deduplicates.
    fn record(&self) -> [u8; 14] {
        let p7 = self.dest[0].1 | self.dest[1].1 << 2 | self.dest[2].1 << 4 | self.dest[3].1 << 6;
        let p8 = self.dest[4].1;
        [
            self.bg2 << 5 | self.collision << 2 | self.lights_out,
            self.palette,
            self.blockset,
            self.enemyblk,
            self.effect,
            encode_tag(self.tag0),
            encode_tag(self.tag1),
            p7,
            p8,
            self.dest[0].0,
            self.dest[1].0,
            self.dest[2].0,
            self.dest[3].0,
            self.dest[4].0,
        ]
    }
}

/// One `dungeon/dungeon-<i>.yaml`.
#[derive(Clone, Debug, Default)]
pub struct Room {
    pub header: Header,
    pub sprites: Vec<Sprite>,
    pub secrets: Vec<Secret>,
    pub chests: Vec<Chest>,
    pub layers: [Vec<Obj>; 3],
    /// `None` where the YAML would have had no `Layer<n>.doors` key at all,
    /// which is both "the stream had no door section" and "it had an empty
    /// one" — `extract_resources.py:472` writes the key only `if doors:`.
    pub doors: [Option<Vec<Door>>; 3],
}

impl Room {
    /// `y.get('Layer<n>.doors')` for layers 1 and 2, and
    /// `y.get('Layer3.doors') or []` for layer 3 — see the module docs.
    fn doors_for_layer(&self, layer: usize) -> Option<&[Door]> {
        match self.doors[layer].as_deref() {
            Some(d) => Some(d),
            None if layer == 2 => Some(&[]),
            None => None,
        }
    }
}

/// One entry of `Entrances` or `StartingPoints` — `_get_entrance_info_one`.
///
/// Fields are stored post-transform exactly as the YAML holds them, because
/// several of the compile-side reversals are not the identity and the
/// intermediate form is what the reversal is defined against.
#[derive(Clone, Debug, Default)]
pub struct Entrance {
    /// `e['room'] = i`, assigned by `print_dungeon_rooms` from the room the
    /// entrance was filed under. Since `get_entrance_info` groups by exactly
    /// that room word the two always agree; the port asserts it rather than
    /// relying on the aliasing the Python relies on.
    pub room: u16,
    pub scroll_xy: [i32; 2],
    pub player_xy: [i32; 2],
    pub camera_xy: [i32; 2],
    pub blockset: u8,
    pub music: u8,
    /// Index into `kPalaceNames`, i.e. `(int8 + 2) >> 1`.
    pub palace: i32,
    pub doorway_orientation: i32,
    pub plane: u8,
    pub ladder_level: u8,
    /// `quadrants[0..1]`: whether the x/y quadrant is doubled.
    pub double_x: bool,
    pub double_y: bool,
    /// `quadrants[2]`, one of 0, 2, 16, 18 (`kQuadrantNames`).
    pub quadrant2: u8,
    pub floor: i32,
    /// `repair_scroll_bounds`, absent when all eight are zero.
    pub repair_scroll_bounds: Option<[i32; 8]>,
    /// `house_exit_door`, already reduced to the word the compile side
    /// reassembles; see [`Entrance::door_settings`].
    pub house_exit_door: ExitDoor,
    /// `associated_entrance_index`, starting points only.
    pub associated_entrance_index: u16,
}

/// `get_exit_door` (`extract_resources.py:296-302`) — a three-way tag, because
/// `0` and `0xffff` are distinguished from a real door and encode back
/// literally while a real door is masked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExitDoor {
    #[default]
    None,
    None0xffff,
    Door {
        bombable: bool,
        a: u16,
        b: u16,
    },
}

impl Entrance {
    /// `get_exit_door` on the compile side (`compile_resources.py:632-635`).
    /// Note this is *not* the identity on the source word: bit 0 and bit 14 are
    /// dropped, so the result is `x & 0xbffe`.
    fn door_settings(&self) -> u16 {
        match self.house_exit_door {
            ExitDoor::None => 0,
            ExitDoor::None0xffff => 0xffff,
            ExitDoor::Door { bombable, a, b } => {
                (if bombable { 1u16 } else { 0 }) << 15 | a << 1 | b << 7
            }
        }
    }

    /// `get_quadrant1`.
    fn quadrant1(&self) -> u8 {
        (self.double_x as u8) * 0x20 + (self.double_y as u8) * 0x2
    }

    /// `get_palace` — `-1` for `kPalaceNames[0]`, else `(i - 1) * 2`.
    fn palace_value(&self) -> i32 {
        if self.palace == 0 {
            -1
        } else {
            (self.palace - 1) * 2
        }
    }

    fn base_x(&self) -> i32 {
        ((self.room as i32) & 0x00f) << 9
    }

    fn base_y(&self) -> i32 {
        ((self.room as i32) & 0x1f0) << 5
    }

    /// `get_rc` (`compile_resources.py:614-623`). The eight bytes are rebuilt
    /// by adding back exactly what `get_se` subtracted, so the result is the
    /// ROM's own bytes — including in the all-zero case, where the absent
    /// `repair_scroll_bounds` stands in for a bounds block that equalled the
    /// computed base.
    fn relative_coords(&self) -> [i32; 8] {
        let room = self.room as i32;
        let base_x = (room & 0xf) * 2;
        let base_y = (room >> 4) * 2;
        let ym = (self.player_xy[1] & 0x100) >> 8;
        let xm = (self.player_xy[0] & 0x100) >> 8;
        let qqq = if room >= 242 && !self.double_x { xm } else { 0 };
        let l = [
            base_y + ym,
            base_y,
            base_y + ym,
            base_y + 1,
            base_x + xm,
            base_x + qqq,
            base_x + xm,
            base_x + qqq + 1,
        ];
        let rep = self.repair_scroll_bounds.unwrap_or([0; 8]);
        let mut out = [0i32; 8];
        for i in 0..8 {
            out[i] = l[i] + rep[i];
        }
        out
    }
}

/// Everything the dungeon stages read out of the ROM, in one pass.
pub struct Dungeon {
    /// The 320 rooms, in index order.
    pub rooms: Vec<Room>,
    /// `get_entrance_info(0)`, flattened back into entrance-index order.
    pub entrances: Vec<Entrance>,
    /// `get_entrance_info(1)`, in starting-point-index order.
    pub starting_points: Vec<Entrance>,
    /// `print_default_rooms` — 8 object lists.
    pub default_rooms: Vec<Vec<Obj>>,
    /// `print_overlay_rooms` — 19 object lists.
    pub overlay_rooms: Vec<Vec<Obj>>,
}

// ---------------------------------------------------------------------------
// The reading side.
// ---------------------------------------------------------------------------

/// `decode_room_objects` (`extract_resources.py:245-283`).
///
/// Returns the address just past the record, the objects, and the doors —
/// `None` when the stream ended with `0xffff` before any door section, which is
/// what makes the `Layer*.doors` key absent.
fn decode_room_objects(rom: &Rom, mut p: u32) -> Result<(u32, Vec<Obj>, Option<Vec<Door>>)> {
    let mut objs = Vec::new();
    loop {
        let (p0, p1, p2) = (rom.get_byte(p)?, rom.get_byte(p + 1)?, rom.get_byte(p + 2)?);
        let a = p0 as u32 | (p1 as u32) << 8;
        if a == 0xffff {
            return Ok((p + 2, objs, None));
        }
        if a == 0xfff0 {
            p += 2;
            break;
        }
        // `A & 0xfc` only ever inspects the low byte.
        if (a & 0xfc) != 0xfc {
            let index = p2 as u32;
            let dst = ((p1 as u32) >> 2) << 7 | ((p0 as u32) & 0xfc) >> 1;
            let x = ((dst >> 1) & 0x3f) as u8;
            let y = ((dst >> 7) & 0x3f) as u8;
            let w = p0 & 3;
            let h = p1 & 3;
            if index < 0xf8 {
                check_index("kType0Names", index, N_TYPE0)?;
                objs.push(Obj { x, y, kind: 0, index: index as u8, w, h });
            } else {
                let index2 = (index & 7) << 4 | (h as u32) << 2 | w as u32;
                check_index("kType1Names", index2, N_TYPE1)?;
                objs.push(Obj { x, y, kind: 1, index: index2 as u8, w: 0, h: 0 });
            }
        } else {
            // subtype 2: 111111xx xxxxyyyy yyiiiiii
            let x = (((p0 as u32) << 4 | (p1 as u32) >> 4) & 0x3f) as u8;
            let y = (((p1 as u32) << 2 | (p2 as u32) >> 6) & 0x3f) as u8;
            let index = (p2 & 0x3f) as u32;
            check_index("kType2Names", index, N_TYPE2)?;
            objs.push(Obj { x, y, kind: 2, index: index as u8, w: 0, h: 0 });
        }
        p += 3;
    }

    let mut doors = Vec::new();
    loop {
        let b0 = rom.get_byte(p)?;
        let b1 = rom.get_byte(p + 1)?;
        let a = b0 as u32 | (b1 as u32) << 8;
        if a == 0xffff {
            return Ok((p + 2, objs, Some(doors)));
        }
        doors.push(Door { ty: b1, pos: b0 >> 4, dir: (a & 3) as u8 });
        p += 2;
    }
}

fn check_index(table: &str, index: u32, len: u32) -> Result<()> {
    if index >= len {
        return Err(format!("{table}[{index}] is out of range ({len} entries)"));
    }
    Ok(())
}

/// `get_chest_info` (`extract_resources.py:285-292`), returned as a per-room
/// list in ROM order so a room's chests keep the order they are stored in.
fn read_chests(rom: &Rom) -> Result<Vec<Vec<Chest>>> {
    let ea = 0x81e96e;
    let mut all: Vec<Vec<Chest>> = vec![Vec::new(); 320];
    for i in 0..(504 / 3) {
        let room = rom.get_word(ea + i * 3)?;
        let data = rom.get_byte(ea + i * 3 + 2)?;
        let key = (room & 0x7fff) as usize;
        if key >= 320 {
            return Err(format!("chest {i} names room {key}, which is outside 0..320"));
        }
        all[key].push(Chest { data, big: (room & 0x8000) != 0 });
    }
    Ok(all)
}

/// `pits_hurt_player` (`extract_resources.py:369-371`) — 57 room indices.
fn read_pits_hurt_player(rom: &Rom) -> Result<Vec<u16>> {
    let mut v = Vec::with_capacity(57);
    for i in 0..57u32 {
        v.push(rom.get_word(0x80990C + i * 2)? as u16);
    }
    Ok(v)
}

/// `_get_entrance_info_one` (`extract_resources.py:294-357`).
///
/// `set` selects between the 133 entrances and the 7 starting points; the two
/// share every field but read them from different tables.
fn read_entrance(rom: &Rom, i: u32, set: usize) -> Result<Entrance> {
    let pick = |a: u32, b: u32| if set == 0 { a } else { b };

    let room = rom.get_word(pick(0x82C813, 0x82DB6E) + i * 2)? as i32;

    let player_x = rom.get_word(pick(0x82D063, 0x82DBDE) + i * 2)? as i32 - ((room & 0x00f) << 9);
    let player_y = rom.get_word(pick(0x82CF59, 0x82DBD0) + i * 2)? as i32 - ((room & 0x1f0) << 5);

    let scroll_x = rom.get_word(pick(0x82CD45, 0x82DBB4) + i * 2)? as i32 - ((room & 0x00f) << 9);
    let scroll_y = rom.get_word(pick(0x82CE4F, 0x82DBC2) + i * 2)? as i32 - ((room & 0x1f0) << 5);

    let camera_x = rom.get_word(pick(0x82D277, 0x82DBFA) + i * 2)? as i32;
    let camera_y = rom.get_word(pick(0x82D16D, 0x82DBEC) + i * 2)? as i32;

    let music = rom.get_byte(pick(0x82D82E, 0x82DC4E) + i)?;
    if !MUSIC_KEYS.contains(&music) {
        return Err(format!("entrance {i} names music {music}, absent from kMusicNames"));
    }

    let palace_raw = rom.get_int8(pick(0x82D48B, 0x82DC16) + i)?;
    let palace = (palace_raw + 2) >> 1;
    if !(0..N_PALACE).contains(&palace) {
        return Err(format!("entrance {i} palace index {palace} is out of range"));
    }

    let doorway_orientation = if set == 0 { rom.get_int8(0x82D510 + i)? } else { 0 };

    let plane_byte = rom.get_byte(pick(0x82D595, 0x82DC1D) + i)?;

    let quad_byte = rom.get_byte(pick(0x82D61a, 0x82DC24) + i)?;
    let double_x = (quad_byte & 0x20) != 0;
    let double_y = (quad_byte & 0x2) != 0;

    let quadrant2 = rom.get_byte(pick(0x82D69F, 0x82DC2B) + i)?;
    // kQuadrantNames is a four-entry dict; anything else is a KeyError.
    if !matches!(quadrant2, 0 | 2 | 16 | 18) {
        return Err(format!("entrance {i} quadrant byte {quadrant2} is not in kQuadrantNames"));
    }

    let floor = rom.get_int8(pick(0x82D406, 0x82DC0F) + i)?;

    // get_se: the eight scroll-repair bytes minus the base the compile side
    // adds back. `xm`/`ym`/`qqq` are read off player_xy, which may be negative;
    // Python's `&` on a negative int masks its two's complement, so the
    // arithmetic is done in i32 throughout and never in an unsigned type.
    let se_base = pick(0x82C91D, 0x82DB7C);
    let base_x = (room & 0xf) * 2;
    let base_y = (room >> 4) * 2;
    let ym = (player_y & 0x100) >> 8;
    let xm = (player_x & 0x100) >> 8;
    let qqq = if room >= 242 && !double_x { xm } else { 0 };
    let b = |k: u32| -> Result<i32> { Ok(rom.get_byte(se_base + i * 8 + k)? as i32) };
    let se = [
        b(0)? - base_y - ym,
        b(1)? - base_y,
        b(2)? - base_y - ym,
        b(3)? - base_y - 1,
        b(4)? - base_x - xm,
        b(5)? - base_x - qqq,
        b(6)? - base_x - xm,
        b(7)? - base_x - 1 - qqq,
    ];
    let repair_scroll_bounds = if se == [0; 8] { None } else { Some(se) };

    // get_exit_door.
    let x = rom.get_word(pick(0x82D724, 0x82DC32) + i * 2)? as u16;
    let house_exit_door = if x == 0 {
        ExitDoor::None
    } else if x == 0xffff {
        ExitDoor::None0xffff
    } else {
        ExitDoor::Door {
            bombable: (x & 0x8000) != 0,
            a: (x & 0x7e) >> 1,
            b: (x & 0x3f80) >> 7,
        }
    };

    let associated_entrance_index =
        if set == 1 { rom.get_word(0x82DC40 + i * 2)? as u16 } else { 0 };

    if room < 0 || room >= 320 {
        // `print_dungeon_rooms` only ever assigns `e['room']` for rooms it
        // walks (0..320); an entrance outside that would leave the slot None
        // and raise "Entrance %d not defined".
        return Err(format!("entrance {i} is in room {room}, outside 0..320"));
    }

    Ok(Entrance {
        room: room as u16,
        scroll_xy: [scroll_x, scroll_y],
        player_xy: [player_x, player_y],
        camera_xy: [camera_x, camera_y],
        blockset: rom.get_byte(pick(0x82D381, 0x82DC08) + i)?,
        music,
        palace,
        doorway_orientation,
        plane: plane_byte & 0xf,
        ladder_level: plane_byte >> 4,
        double_x,
        double_y,
        quadrant2,
        floor,
        repair_scroll_bounds,
        house_exit_door,
        associated_entrance_index,
    })
}

/// `get_sprites` inside `print_room` (`extract_resources.py:416-440`).
fn read_sprites(rom: &Rom, room_index: u32) -> Result<(u8, Vec<Sprite>)> {
    let base = 0x890000 + rom.get_word(0x89D62E + room_index * 2)?;
    let sort_sprites = rom.get_byte(base)?;
    let mut ea = base + 1;
    let mut r: Vec<Sprite> = Vec::new();
    while rom.get_byte(ea)? != 0xff {
        let y = rom.get_byte(ea)?;
        let x = rom.get_byte(ea + 1)?;
        let ty = rom.get_byte(ea + 2)?;
        // The Python's `if type == 0xe4: ... elif x >= 0xe0:` — a 0xe4 sprite
        // whose y is neither 0xfe nor 0xfd falls through to the *generic* path,
        // never to the high-sprite one.
        if ty == 0xe4 && (y == 0xfe || y == 0xfd) {
            let last = r
                .last_mut()
                .ok_or_else(|| format!("room {room_index} begins with a drop marker"))?;
            last.drops.push(y);
            ea += 3;
            continue;
        }
        if ty != 0xe4 && x >= 0xe0 {
            let idx = ty as u32 + 0x100;
            check_index("kSpriteNames", idx, N_SPRITE)?;
            r.push(Sprite {
                x: x & 0x1f,
                y: y & 0x1f,
                floor: y >> 7,
                idx: idx as u16,
                subtype: 0,
                drops: Vec::new(),
            });
            ea += 3;
            continue;
        }
        let subtype = (x >> 5) | ((y >> 5) & 3) << 3;
        check_index("kSpriteNames", ty as u32, N_SPRITE)?;
        if subtype != 0 && !sprite_name_has_dash(ty) {
            // `name.index('-')` raises for these; see the module docs.
            return Err(format!(
                "room {room_index}: sprite {ty:#04x} has subtype {subtype} but no '-' in its name"
            ));
        }
        r.push(Sprite {
            x: x & 0x1f,
            y: y & 0x1f,
            floor: y >> 7,
            idx: ty as u16,
            subtype,
            drops: Vec::new(),
        });
        ea += 3;
    }
    Ok((sort_sprites, r))
}

/// The 28 entries of `kSpriteNames[0..0x100]` that are bare hex names with no
/// `-`, and so cannot carry a subtype infix.
fn sprite_name_has_dash(ty: u8) -> bool {
    const NO_DASH: &[u8] = &[
        0x02, 0x10, 0x2D, 0x70, 0x89, 0x94, 0x9C, 0xA3, 0xA4, 0xAB, 0xB8, 0xD7, 0xE6, 0xED, 0xEF,
        0xF0, 0xF1, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF,
    ];
    !NO_DASH.contains(&ty)
}

/// `get_secrets` inside `print_room` (`extract_resources.py:442-450`).
fn read_secrets(rom: &Rom, room_index: u32) -> Result<Vec<Secret>> {
    let mut ea = 0x810000 | rom.get_word(0x81db69 + room_index * 2)?;
    let mut xs = Vec::new();
    while rom.get_word(ea)? != 0xffff {
        let pos = rom.get_word(ea)?;
        if pos % 2 != 0 {
            return Err(format!("room {room_index}: secret position {pos} is odd"));
        }
        let kind = rom.get_byte(ea + 2)?;
        if !SECRET_KEYS.contains(&kind) {
            return Err(format!("room {room_index}: secret {kind} is absent from kSecretNames"));
        }
        xs.push(Secret { x: (pos / 2 % 64) as u8, y: (pos / 2 / 64) as u8, kind });
        ea += 3;
    }
    Ok(xs)
}

/// `print_room` (`extract_resources.py:373-487`) for one room.
fn read_room(
    rom: &Rom,
    room_index: u32,
    chests: Vec<Chest>,
    pits_hurt: bool,
) -> Result<Room> {
    let p = 0x1f8000 + room_index * 3;
    let room_addr = rom.get_24(p)?;

    let mut hp = 0x40000 | rom.get_word(0x4f502 + room_index * 2)?;
    if hp == 0x4FFEF {
        hp = 0x82EDC5; // "just some place with zeros"
    }

    let floor = rom.get_byte(room_addr)?;
    let layout = rom.get_byte(room_addr + 1)?;
    let flags = rom.get_byte(hp)?;
    let p7 = rom.get_byte(hp + 7)?;
    let p8 = rom.get_byte(hp + 8)?;

    let (sort_sprites, sprites) = read_sprites(rom, room_index)?;

    let bg2 = (flags >> 5) as u32;
    check_index("kBg2", bg2, N_BG2)?;
    let collision = (flags >> 2 & 7) as u32;
    check_index("kCollisionNames", collision, N_COLLISION)?;
    let effect = rom.get_byte(hp + 4)?;
    check_index("kEffectNames", effect as u32, N_EFFECT)?;
    let tag0 = rom.get_byte(hp + 5)?;
    check_index("kTagNames", tag0 as u32, N_TAG)?;
    let tag1 = rom.get_byte(hp + 6)?;
    check_index("kTagNames", tag1 as u32, N_TAG)?;

    let header = Header {
        floor1: floor & 0xf,
        floor2: floor >> 4,
        layout: layout >> 2,
        start_quadrant: layout & 3,
        bg2: bg2 as u8,
        collision: collision as u8,
        lights_out: flags & 1,
        palette: rom.get_byte(hp + 1)?,
        blockset: rom.get_byte(hp + 2)?,
        enemyblk: rom.get_byte(hp + 3)?,
        effect,
        tag0,
        tag1,
        dest: [
            (rom.get_byte(hp + 9)?, p7 & 3),
            (rom.get_byte(hp + 10)?, p7 >> 2 & 3),
            (rom.get_byte(hp + 11)?, p7 >> 4 & 3),
            (rom.get_byte(hp + 12)?, p7 >> 6 & 3),
            (rom.get_byte(hp + 13)?, p8 & 3),
        ],
        tele_msg: rom.get_word(0x87F61D + room_index * 2)? as u16,
        sort_sprites,
        pits_hurt_player: pits_hurt,
    };

    let secrets = read_secrets(rom, room_index)?;

    let mut p = room_addr + 2;
    let mut layers: [Vec<Obj>; 3] = Default::default();
    let mut doors: [Option<Vec<Door>>; 3] = Default::default();
    for layer in 0..3 {
        let (np, objs, d) = decode_room_objects(rom, p)?;
        p = np;
        layers[layer] = objs;
        // `if doors:` — the key is written only for a *non-empty* list, so an
        // empty door section is indistinguishable from none at all.
        doors[layer] = match d {
            Some(d) if !d.is_empty() => Some(d),
            _ => None,
        };
    }

    Ok(Room { header, sprites, secrets, chests, layers, doors })
}

/// A template room list: `print_default_room` / `print_overlay_room`. Both
/// assert the stream carries no door section.
fn read_template_room(rom: &Rom, p: u32, what: &str, idx: u32) -> Result<Vec<Obj>> {
    let room_addr = rom.get_24(p)?;
    let (_, objs, doors) = decode_room_objects(rom, room_addr)?;
    if doors.is_some() {
        return Err(format!("{what}{idx} has a door section, which the Python asserts against"));
    }
    Ok(objs)
}

/// Stage "Reading dungeon rooms": `print_room` x320, with the chest and
/// pit tables it needs folded in.
pub fn read_rooms(rom: &Rom) -> Result<Vec<Room>> {
    let mut chests = read_chests(rom)?;
    let pits = read_pits_hurt_player(rom)?;

    let mut rooms = Vec::with_capacity(320);
    for i in 0..320u32 {
        let c = core::mem::take(&mut chests[i as usize]);
        rooms.push(read_room(rom, i, c, pits.contains(&(i as u16)))?);
    }
    Ok(rooms)
}

/// Stage "Reading room entrances": `get_entrance_info(0)` and `(1)`, returned
/// as (entrances, starting points).
pub fn read_entrances(rom: &Rom) -> Result<(Vec<Entrance>, Vec<Entrance>)> {
    let mut entrances = Vec::with_capacity(133);
    for i in 0..133u32 {
        entrances.push(read_entrance(rom, i, 0)?);
    }
    let mut starting_points = Vec::with_capacity(7);
    for i in 0..7u32 {
        starting_points.push(read_entrance(rom, i, 1)?);
    }
    Ok((entrances, starting_points))
}

/// Stage "Reading template rooms": the default and overlay object lists,
/// returned as (defaults, overlays).
pub fn read_templates(rom: &Rom) -> Result<(Vec<Vec<Obj>>, Vec<Vec<Obj>>)> {
    let mut default_rooms = Vec::with_capacity(8);
    for i in 0..8u32 {
        default_rooms.push(read_template_room(rom, 0x84EF2F + i * 3, "Default", i)?);
    }
    let mut overlay_rooms = Vec::with_capacity(19);
    for i in 0..19u32 {
        overlay_rooms.push(read_template_room(rom, 0x84ECC0 + i * 3, "Overlay", i)?);
    }
    Ok((default_rooms, overlay_rooms))
}

/// Stage: the whole reading half for the dungeon. Covers `print_room` x320,
/// `get_entrance_info(0)` and `(1)`, and the default/overlay template rooms.
///
/// The three parts are also public individually ([`read_rooms`],
/// [`read_entrances`], [`read_templates`]) so the phase table can report them
/// as separate stages without doing the work twice.
pub fn read(rom: &Rom) -> Result<Dungeon> {
    let rooms = read_rooms(rom)?;
    let (entrances, starting_points) = read_entrances(rom)?;
    let (default_rooms, overlay_rooms) = read_templates(rom)?;
    Ok(Dungeon { rooms, entrances, starting_points, default_rooms, overlay_rooms })
}

// ---------------------------------------------------------------------------
// The writing side.
// ---------------------------------------------------------------------------

/// `print_layer` (`compile_resources.py:527-556`).
///
/// Appends the object stream, an optional door section, and the `0xff 0xff`
/// terminator. Returns the door offset — the position just past the
/// `0xf0 0xff` marker — which is `None` exactly when `doors` is `None`.
fn print_layer(data: &mut Vec<u8>, objs: &[Obj], doors: Option<&[Door]>) -> Result<Option<usize>> {
    for o in objs {
        let (p0, p1, p2) = match o.kind {
            0 => {
                let (w, h) = (o.w as u32, o.h as u32);
                if w > 3 || h > 3 {
                    return Err(format!("object size {w}*{h} is out of range"));
                }
                (o.x as u32 * 4 + w, o.y as u32 * 4 + h, o.index as u32)
            }
            1 => {
                let index = o.index as u32;
                (
                    o.x as u32 * 4 + (index & 3),
                    o.y as u32 * 4 + (index >> 2 & 3),
                    (index >> 4) + 0xf8,
                )
            }
            _ => {
                let (x, y) = (o.x as u32, o.y as u32);
                (
                    0xfc + (x >> 4 & 3),
                    (x << 4 & 0xf0) | (y >> 2 & 0x0f),
                    o.index as u32 | (y << 6 & 0xc0),
                )
            }
        };
        data.push(p0 as u8);
        data.push(p1 as u8);
        data.push(p2 as u8);
    }
    let mut door_offset = None;
    if let Some(doors) = doors {
        data.extend_from_slice(&[0xf0, 0xff]);
        door_offset = Some(data.len());
        for d in doors {
            data.push(d.dir | d.pos << 4);
            data.push(d.ty);
        }
    }
    data.extend_from_slice(&[0xff, 0xff]);
    Ok(door_offset)
}

/// `append_scan_bytes` (`compile_resources.py:518-523`).
///
/// Finds the longest suffix of `big` that is a prefix of `little`, appends only
/// the remainder, and returns where the record starts. `n` counts *down* from
/// `len(little)`, so the first match wins and `n == len(little)` means the
/// record already ends the buffer. Python's `big[-n:]` silently yields the
/// whole buffer when `n > len(big)`, which then cannot equal an `n`-long slice;
/// the explicit length guard here is what reproduces that.
fn append_scan_bytes(big: &mut Vec<u8>, little: &[u8]) -> usize {
    for n in (0..=little.len()).rev() {
        if n == 0 || (big.len() >= n && big[big.len() - n..] == little[..n]) {
            let offset = big.len() - n;
            big.extend_from_slice(&little[n..]);
            return offset;
        }
    }
    unreachable!("the n == 0 arm always matches")
}

fn as_u8(name: &str, i: usize, v: i32) -> Result<u8> {
    u8::try_from(v).map_err(|_| format!("{name}[{i}] = {v} does not fit in a uint8"))
}

fn as_u16(name: &str, i: usize, v: i32) -> Result<u16> {
    u16::try_from(v).map_err(|_| format!("{name}[{i}] = {v} does not fit in a uint16"))
}

/// Assets **3-10**: `kDungeonRoom`, `kDungeonRoomOffs`,
/// `kDungeonRoomDoorOffs`, `kDungeonRoomHeaders`, `kDungeonRoomHeadersOffs`,
/// `kDungeonRoomChests`, `kDungeonRoomTeleMsg`, `kDungeonPitsHurtPlayer`.
///
/// The first half of `print_dungeon_rooms` (`compile_resources.py:579-664`).
pub fn add_rooms(d: &Dungeon, a: &mut Assets) -> Result<()> {
    let mut data: Vec<u8> = Vec::new();
    let mut offsets = vec![0u16; 320];
    let mut door_offsets = vec![0u16; 320];
    let mut room_headers: Vec<u8> = Vec::new();
    let mut header_offsets = vec![0u16; 320];
    let mut chests: Vec<u8> = Vec::new();
    let mut sign_texts = vec![0u16; 320];
    let mut pits_hurt_player: Vec<u16> = Vec::new();

    for (i, room) in d.rooms.iter().enumerate() {
        let h = &room.header;
        if h.pits_hurt_player {
            pits_hurt_player.push(i as u16);
        }
        offsets[i] = as_u16("kDungeonRoomOffs", i, data.len() as i32)?;
        data.push(h.floor1 + h.floor2 * 16);
        data.push(h.layout * 4 + h.start_quadrant);
        print_layer(&mut data, &room.layers[0], room.doors_for_layer(0))?;
        print_layer(&mut data, &room.layers[1], room.doors_for_layer(1))?;
        // Layer 3 always gets `or []`, so the offset is always defined.
        let off = print_layer(&mut data, &room.layers[2], room.doors_for_layer(2))?
            .expect("layer 3 always writes a door section");
        door_offsets[i] = as_u16("kDungeonRoomDoorOffs", i, off as i32)?;
        header_offsets[i] = as_u16(
            "kDungeonRoomHeadersOffs",
            i,
            append_scan_bytes(&mut room_headers, &h.record()) as i32,
        )?;
        sign_texts[i] = h.tele_msg;
        for c in &room.chests {
            chests.push((i & 0xff) as u8);
            chests.push(if c.big { ((i >> 8) | 0x80) as u8 } else { (i >> 8) as u8 });
            chests.push(c.data);
        }
    }

    a.add_uint8("kDungeonRoom", &data)?;
    a.add_uint16("kDungeonRoomOffs", &offsets)?;
    a.add_uint16("kDungeonRoomDoorOffs", &door_offsets)?;
    a.add_uint8("kDungeonRoomHeaders", &room_headers)?;
    a.add_uint16("kDungeonRoomHeadersOffs", &header_offsets)?;
    a.add_uint8("kDungeonRoomChests", &chests)?;
    a.add_uint16("kDungeonRoomTeleMsg", &sign_texts)?;
    a.add_uint16("kDungeonPitsHurtPlayer", &pits_hurt_player)?;
    Ok(())
}

/// `print_entrance_info` (`compile_resources.py:612-651`) for one table.
fn add_entrance_info(entrances: &[Entrance], prefix: &str, a: &mut Assets) -> Result<()> {
    let n = |suffix: &str| format!("{prefix}{suffix}");

    a.add_uint16(&n("rooms"), &entrances.iter().map(|e| e.room).collect::<Vec<_>>())?;

    let mut rc = Vec::with_capacity(entrances.len() * 8);
    for (i, e) in entrances.iter().enumerate() {
        for v in e.relative_coords() {
            rc.push(as_u8(&n("relativeCoords"), i, v)?);
        }
    }
    a.add_uint8(&n("relativeCoords"), &rc)?;

    // Each of these adds back the base the extract side subtracted, so the
    // value written is the ROM's original word.
    let mut f = |suffix: &str, get: &dyn Fn(&Entrance) -> i32| -> Result<()> {
        let name = n(suffix);
        let mut v = Vec::with_capacity(entrances.len());
        for (i, e) in entrances.iter().enumerate() {
            v.push(as_u16(&name, i, get(e))?);
        }
        a.add_uint16(&name, &v)
    };
    f("scrollX", &|e| e.scroll_xy[0] + e.base_x())?;
    f("scrollY", &|e| e.scroll_xy[1] + e.base_y())?;
    f("playerX", &|e| e.player_xy[0] + e.base_x())?;
    f("playerY", &|e| e.player_xy[1] + e.base_y())?;
    f("cameraX", &|e| e.camera_xy[0])?;
    f("cameraY", &|e| e.camera_xy[1])?;

    a.add_uint8(&n("blockset"), &entrances.iter().map(|e| e.blockset).collect::<Vec<_>>())?;
    a.add_int8(&n("floor"), &entrances.iter().map(|e| e.floor).collect::<Vec<_>>())?;
    a.add_int8(&n("palace"), &entrances.iter().map(|e| e.palace_value()).collect::<Vec<_>>())?;

    // `add_asset_uint8` on a negative int8 raises OverflowError in Python.
    let mut dw = Vec::with_capacity(entrances.len());
    for (i, e) in entrances.iter().enumerate() {
        dw.push(as_u8(&n("doorwayOrientation"), i, e.doorway_orientation)?);
    }
    a.add_uint8(&n("doorwayOrientation"), &dw)?;

    a.add_uint8(
        &n("startingBg"),
        &entrances.iter().map(|e| e.plane + e.ladder_level * 16).collect::<Vec<_>>(),
    )?;
    a.add_uint8(&n("quadrant1"), &entrances.iter().map(|e| e.quadrant1()).collect::<Vec<_>>())?;
    a.add_uint8(&n("quadrant2"), &entrances.iter().map(|e| e.quadrant2).collect::<Vec<_>>())?;
    a.add_uint16(
        &n("doorSettings"),
        &entrances.iter().map(|e| e.door_settings()).collect::<Vec<_>>(),
    )?;
    if prefix == "kStartingPoint_" {
        let mut v = Vec::with_capacity(entrances.len());
        for (i, e) in entrances.iter().enumerate() {
            v.push(as_u8(&n("entrance"), i, e.associated_entrance_index as i32)?);
        }
        a.add_uint8(&n("entrance"), &v)?;
    }
    a.add_uint8(&n("musicTrack"), &entrances.iter().map(|e| e.music).collect::<Vec<_>>())?;
    Ok(())
}

/// Assets **11-45**: the `kEntranceData_*` and `kStartingPoint_*` tables.
pub fn add_entrances(d: &Dungeon, a: &mut Assets) -> Result<()> {
    add_entrance_info(&d.entrances, "kEntranceData_", a)?;
    add_entrance_info(&d.starting_points, "kStartingPoint_", a)?;
    Ok(())
}

/// `print_dungeon_secrets` (`compile_resources.py:496-516`).
///
/// The first 640 bytes are a per-room `u16` pointer table that is written into
/// the same buffer it points into, so the pointers are the buffer length *at
/// the moment the room's block starts*. Rooms with no secrets are patched
/// afterwards to point at the final `0xff 0xff`, at `len - 2`.
fn dungeon_secrets(d: &Dungeon) -> Vec<u8> {
    let mut result: Vec<Option<u8>> = vec![None; 640];
    for (i, room) in d.rooms.iter().enumerate() {
        if room.secrets.is_empty() {
            continue;
        }
        let l = result.len();
        result[i * 2] = Some((l & 0xff) as u8);
        result[i * 2 + 1] = Some((l >> 8) as u8);
        for s in &room.secrets {
            let pos = (s.x as usize + s.y as usize * 64) * 2;
            result.push(Some((pos & 0xff) as u8));
            result.push(Some((pos >> 8) as u8));
            result.push(Some(s.kind));
        }
        result.push(Some(0xff));
        result.push(Some(0xff));
    }
    for i in 0..320 {
        if result[i * 2].is_none() {
            let l = result.len() - 2;
            result[i * 2] = Some((l & 0xff) as u8);
            result[i * 2 + 1] = Some((l >> 8) as u8);
        }
    }
    // Every slot is filled by construction; the `Option` is here so a missed
    // one is a panic rather than a silent zero, matching `OutArrays.write`'s
    // "every element is an int" assert.
    result.into_iter().map(|b| b.expect("kDungeonSecrets slot left unset")).collect()
}

/// Assets **46-55**: `kDungeonRoomDefault(Offs)`, `kDungeonRoomOverlay(Offs)`,
/// `kDungeonSecrets`, and the five direct ROM tables that close
/// `print_dungeon_rooms`.
pub fn add_templates(rom: &Rom, d: &Dungeon, a: &mut Assets) -> Result<()> {
    let mut data: Vec<u8> = Vec::new();
    let mut offsets = vec![0u16; 8];
    for i in 0..8 {
        offsets[i] = as_u16("kDungeonRoomDefaultOffs", i, data.len() as i32)?;
        print_layer(&mut data, &d.default_rooms[i], None)?;
    }
    a.add_uint8("kDungeonRoomDefault", &data)?;
    a.add_uint16("kDungeonRoomDefaultOffs", &offsets)?;

    let mut data: Vec<u8> = Vec::new();
    let mut offsets = vec![0u16; 19];
    for i in 0..19 {
        offsets[i] = as_u16("kDungeonRoomOverlayOffs", i, data.len() as i32)?;
        print_layer(&mut data, &d.overlay_rooms[i], None)?;
    }
    a.add_uint8("kDungeonRoomOverlay", &data)?;
    a.add_uint16("kDungeonRoomOverlayOffs", &offsets)?;

    a.add_uint8("kDungeonSecrets", &dungeon_secrets(d))?;

    a.add_uint16("kDungAttrsForTile_Offs", &rom.get_words(0x8e9000, 21)?)?;
    a.add_uint8("kDungAttrsForTile", &rom.get_bytes(0x8e902a, 1024)?)?;
    a.add_uint16("kMovableBlockDataInit", &rom.get_words(0x84f1de, 198)?)?;
    a.add_uint16("kTorchDataInit", &rom.get_words(0x84F36A, 144)?)?;
    a.add_uint16("kTorchDataJunk", &rom.get_words(0x84F48a, 48)?)?;
    Ok(())
}

/// Assets **58-59**: `kDungeonSprites`, `kDungeonSpriteOffs`.
///
/// `print_dungeon_sprites` (`compile_resources.py:460-494`). The stream starts
/// with a two-byte `[0, 0xff]` preamble that offset 0 — the "no sprites"
/// sentinel — points at, so a room is skipped only when it has no sprites
/// *and* `sort_sprites == 0`.
pub fn add_sprites(d: &Dungeon, a: &mut Assets) -> Result<()> {
    let mut offsets = vec![0u16; 320];
    let mut data: Vec<u8> = vec![0, 0xff];
    for (i, room) in d.rooms.iter().enumerate() {
        let sortmode = room.header.sort_sprites;
        if room.sprites.is_empty() && sortmode == 0 {
            continue;
        }
        offsets[i] = as_u16("kDungeonSpriteOffs", i, data.len() as i32)?;
        data.push(sortmode);
        for s in &room.sprites {
            if s.x > 0x1f || s.y > 0x1f {
                return Err(format!("room {i}: sprite at ({}, {}) is out of range", s.x, s.y));
            }
            let f = s.floor as u32;
            if s.idx >= 0x100 {
                data.push((f << 7 | s.y as u32) as u8);
                data.push(s.x | 7 << 5);
                data.push((s.idx & 0xff) as u8);
            } else {
                let ss = s.subtype as u32;
                data.push((f << 7 | (ss >> 3) << 5 | s.y as u32) as u8);
                data.push(s.x | ((ss as u8 & 7) << 5));
                data.push(s.idx as u8);
            }
            // `if len(s) == 5` — exactly one drop marker. Two markers make the
            // Python list six long and emit nothing.
            if s.drops.len() == 1 {
                data.extend_from_slice(&[s.drops[0], 0, 0xe4]);
            }
        }
        data.push(0xff);
    }
    a.add_uint8("kDungeonSprites", &data)?;
    a.add_uint16("kDungeonSpriteOffs", &offsets)?;
    Ok(())
}

/// Assets **97-98**: `kDungMap_FloorLayout`, `kDungMap_Tiles`.
///
/// `print_dungeon_map` (`compile_resources.py:440-453`). The tile run's length
/// is derived from the floor layout: it is the layout size minus the number of
/// `0x0f` bytes in it, so the two reads are coupled and cannot be split.
pub fn add_maps(rom: &Rom, a: &mut Assets) -> Result<()> {
    const SIZES: [usize; 14] = [75, 125, 50, 75, 175, 75, 50, 75, 50, 200, 150, 75, 100, 200];
    let mut layouts = Vec::with_capacity(14);
    let mut tiles = Vec::with_capacity(14);
    for i in 0..14u32 {
        let addr = 0xa0000 + rom.get_word(0x8AF605 + i * 2)?;
        let b = rom.get_bytes(addr, SIZES[i as usize])?;
        let nonzero = b.len() - b.iter().filter(|&&x| x == 0xf).count();
        layouts.push(b);
        let addr = 0xa0000 + rom.get_word(0x8AFBE4 + i * 2)?;
        tiles.push(rom.get_bytes(addr, nonzero)?);
    }
    a.add_packed("kDungMap_FloorLayout", &layouts)?;
    a.add_packed("kDungMap_Tiles", &tiles)?;
    Ok(())
}

/// Every dungeon asset, in the order `compile_resources.print_all` emits them:
/// 3-10, 11-45, 46-55, then 58-59 and 97-98.
///
/// Note 56-57 and 60-96 belong to other stages, so a caller that wants the
/// full 165-key order must interleave; the stage functions above are exposed
/// for exactly that.
pub fn add_all(rom: &Rom, a: &mut Assets) -> Result<()> {
    let d = read(rom)?;
    add_rooms(&d, a)?;
    add_entrances(&d, a)?;
    add_templates(rom, &d, a)?;
    add_sprites(&d, a)?;
    add_maps(rom, a)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_scan_bytes_reuses_the_longest_suffix() {
        let mut big = b"abcde".to_vec();
        // "cde" already ends the buffer, so only "f" is appended.
        assert_eq!(append_scan_bytes(&mut big, b"cdef"), 2);
        assert_eq!(big, b"abcdef");
        // A record that already ends the buffer appends nothing.
        assert_eq!(append_scan_bytes(&mut big, b"def"), 3);
        assert_eq!(big, b"abcdef");
        // No overlap at all: n falls all the way to 0.
        assert_eq!(append_scan_bytes(&mut big, b"xy"), 6);
        assert_eq!(big, b"abcdefxy");
    }

    #[test]
    fn append_scan_bytes_never_matches_more_than_the_buffer_holds() {
        // Python's big[-n:] yields the whole (shorter) buffer, which cannot
        // equal an n-long prefix, so a 2-byte buffer can never claim a 3-byte
        // overlap even when it is a prefix of the record.
        let mut big = b"ab".to_vec();
        assert_eq!(append_scan_bytes(&mut big, b"abc"), 0);
        assert_eq!(big, b"abc");
    }

    #[test]
    fn layer_3_writes_a_door_marker_even_with_no_doors() {
        let room = Room::default();
        assert!(room.doors_for_layer(0).is_none());
        assert!(room.doors_for_layer(1).is_none());
        assert_eq!(room.doors_for_layer(2), Some(&[] as &[Door]));

        let mut data = Vec::new();
        assert_eq!(print_layer(&mut data, &[], None).unwrap(), None);
        assert_eq!(data, vec![0xff, 0xff]);

        let mut data = Vec::new();
        assert_eq!(print_layer(&mut data, &[], Some(&[])).unwrap(), Some(2));
        assert_eq!(data, vec![0xf0, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn tag_31_aliases_to_30() {
        assert_eq!(encode_tag(30), 30);
        assert_eq!(encode_tag(31), 30);
        assert_eq!(encode_tag(29), 29);
    }

    #[test]
    fn exit_door_drops_bits_0_and_14() {
        for x in [0x1234u16, 0xbfff, 0x4001, 0x8ffe] {
            let e = Entrance {
                house_exit_door: ExitDoor::Door {
                    bombable: (x & 0x8000) != 0,
                    a: (x & 0x7e) >> 1,
                    b: (x & 0x3f80) >> 7,
                },
                ..Default::default()
            };
            assert_eq!(e.door_settings(), x & 0xbffe);
        }
        assert_eq!(Entrance { house_exit_door: ExitDoor::None, ..Default::default() }
            .door_settings(), 0);
        assert_eq!(Entrance { house_exit_door: ExitDoor::None0xffff, ..Default::default() }
            .door_settings(), 0xffff);
    }

    #[test]
    fn palace_survives_only_for_minus_one_and_even_values() {
        for v in -1..28i32 {
            let idx = (v + 2) >> 1;
            if !(0..N_PALACE).contains(&idx) {
                continue;
            }
            let e = Entrance { palace: idx, ..Default::default() };
            let out = e.palace_value();
            if v == -1 || (v >= 0 && v % 2 == 0) {
                assert_eq!(out, v, "palace {v} should survive");
            }
        }
    }
}

/// Byte-exactness against the Python oracle. Needs the US cartridge and the
/// reference `.dat`, so it is `#[ignore]`d like the other ROM tests:
///
/// ```sh
/// ZELDA3_ROM=... ZELDA3_ORACLE_DAT=... cargo test -- --ignored dungeon
/// ```
#[cfg(test)]
mod oracle_tests {
    use super::*;

    fn rom() -> Option<Rom> {
        Some(Rom::new(std::fs::read(std::env::var("ZELDA3_ROM").ok()?).ok()?))
    }

    /// Parses the reference `.dat` into (name, payload) pairs.
    fn oracle() -> Option<Vec<(String, Vec<u8>)>> {
        let buf = std::fs::read(std::env::var("ZELDA3_ORACLE_DAT").ok()?).ok()?;
        let count = u32::from_le_bytes(buf[80..84].try_into().unwrap()) as usize;
        let key_len = u32::from_le_bytes(buf[84..88].try_into().unwrap()) as usize;
        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            let o = 88 + i * 4;
            sizes.push(u32::from_le_bytes(buf[o..o + 4].try_into().unwrap()) as usize);
        }
        let keys_at = 88 + 4 * count;
        let names: Vec<String> = buf[keys_at..keys_at + key_len]
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        let mut p = keys_at + key_len;
        let mut out = Vec::with_capacity(count);
        for (name, size) in names.into_iter().zip(sizes) {
            while p & 3 != 0 {
                p += 1;
            }
            out.push((name, buf[p..p + size].to_vec()));
            p += size;
        }
        Some(out)
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM and ZELDA3_ORACLE_DAT"]
    fn every_dungeon_asset_is_byte_exact() {
        let (Some(rom), Some(oracle)) = (rom(), oracle()) else { return };
        let mut a = Assets::new();
        add_all(&rom, &mut a).unwrap();

        let mut checked = 0;
        for asset in a.iter() {
            let want = oracle
                .iter()
                .find(|(n, _)| *n == asset.name)
                .unwrap_or_else(|| panic!("{} is not in the oracle", asset.name));
            assert_eq!(
                asset.data.len(),
                want.1.len(),
                "{}: {} bytes, oracle has {}",
                asset.name,
                asset.data.len(),
                want.1.len()
            );
            assert!(asset.data == want.1, "{} differs from the oracle", asset.name);
            checked += 1;
        }
        assert_eq!(checked, 57, "expected 57 dungeon assets");
    }

    /// Writes a `.dat` holding only the dungeon assets, for `compare.mjs`.
    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn write_partial_dat() {
        let Some(rom) = rom() else { return };
        let Ok(out) = std::env::var("ZELDA3_PARTIAL_DAT") else { return };
        let mut a = Assets::new();
        add_all(&rom, &mut a).unwrap();
        std::fs::write(out, a.serialise()).unwrap();
    }
}
