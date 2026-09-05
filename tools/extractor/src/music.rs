//! The music and sound path: assets 0-2, `kSoundBank_intro`,
//! `kSoundBank_indoor` and `kSoundBank_ending`.
//!
//! This is a port of `tables/extract_music.py` and `tables/compile_music.py`
//! taken together. The Python splits the job across a filesystem: the extractor
//! writes `sound_<song>.txt`, `sfx.txt`, `music_info.yaml`, `sound/*.brr` and a
//! 64 KB `sound/<song>.spc`, and the compiler parses those back. Nothing about
//! that round trip is load-bearing except where it *normalises*, so the object
//! graph is passed in memory here and only the normalisations are reproduced:
//!
//! * A `Call` command (`0xef`) carries `note_length`/`volstuff` out of
//!   `decode_pattern` but `Pattern.__str__` prints only 4-tuples' first two
//!   fields, so both are silently dropped before `compile_music` sees them.
//!   [`PLine::Call`] therefore has no length/volume fields at all.
//! * `note_to_str` maps a note byte through `kKeys` and back through
//!   `kKeysDict`; the composition is the identity on `0..=73`, so notes are
//!   carried as the raw 7-bit value and the string table disappears. The one
//!   float on the whole live path (`octave = note / 12`, `extract_music.py:156`)
//!   goes with it.
//! * The BRR samples reach the .dat verbatim from SPC memory. `util.decode_brr`
//!   only ever fed `sound/sound<N>.pcm`, which nothing reads; the *only* thing
//!   the decoder was used for on the live path is `len(r) // 16 * 9`, the
//!   number of BRR bytes, which is `9 * blocks` and is obtained here by walking
//!   the end-of-block flag. No BRR codec is needed in either direction.
//! * Object identity in the Python is the `Foo_0x1234` *name*: `compile_music`
//!   re-derives the address by parsing the hex and re-derives the type from the
//!   prefix. Here every entity keeps its `ea` and its [`Kind`] directly, which
//!   is the same relation without the parsing.
//!
//! # Stages
//!
//! Four public entry points, matching PORTING-MAP.md section 4:
//!
//! | fn | stage | assets |
//! |----|-------|--------|
//! | [`read_music_banks`] | 9, "Reading music banks" | — |
//! | [`decode_music`] | 10, "Decoding music" | — |
//! | [`read_instruments`] | 11, "Reading instruments" | — |
//! | [`build_sound_banks`] | 14, "Building sound banks" | 0-2 |
//!
//! [`add_all`] runs all four and registers assets 0, 1 and 2 in that order.

use crate::pack::Assets;
use crate::rom::Rom;

pub type Result<T> = core::result::Result<T, String>;

// ---------------------------------------------------------------------------
// tables
// ---------------------------------------------------------------------------

/// `extract_music.kEffectByteLength` / `compile_music.kEffectByteLength`.
const EFFECT_BYTE_LENGTH: [usize; 27] = [
    1, 1, 2, 3, 0, 1, 2, 1, 2, 1, 1, 3, 0, 1, 2, 3, 1, 3, 3, 0, 1, 3, 0, 3, 3, 3, 1,
];

/// Index of `'Call'` in `kEffectNames`, i.e. command byte `0xef`.
const EFFECT_CALL: u8 = 15;

/// `compile_music.kGapStartAddrs` — the three addresses at which `write_obj` is
/// allowed to hard-seek instead of asserting the cursor already matches.
const GAP_START_ADDRS: [u32; 3] = [0x2b00, 0x2880, 0xd000];

/// `extract_music.dump_music_info.kDupSamples`. Sample 10 shares sample 9's BRR
/// and sample 20 shares 19's; in the Python this is a filename collision in
/// `sample_to_addr`, here it is an index.
fn dup_sample(i: usize) -> usize {
    match i {
        10 => 9,
        20 => 19,
        _ => i,
    }
}

/// The three banks, in the order `print_sound_banks` emits them, with the ROM
/// address of each bank's upload script.
const SONGS: [(&str, u32); 3] =
    [("intro", 0x998000), ("indoor", 0x9b8000), ("ending", 0x9ad380)];

// ---------------------------------------------------------------------------
// SPC memory
// ---------------------------------------------------------------------------

/// The APU's 64 KB address space as the loader leaves it. `None` means "no
/// upload record covered this byte", and that is semantic, not a detail:
/// `produce_loadable_seq` emits only the defined runs, and `get_type_for_ea`
/// uses undefined-ness to decide an object is *imported* — living outside this
/// bank — and must not be written out.
pub struct SpcMemory {
    pub bytes: Vec<Option<u8>>,
    pub entry_point: u32,
}

impl SpcMemory {
    fn byte(&self, ea: u32) -> Result<u8> {
        match self.bytes.get(ea as usize).copied().flatten() {
            Some(b) => Ok(b),
            None => Err(format!("SPC read of undefined byte at {ea:#x}")),
        }
    }

    fn word(&self, ea: u32) -> Result<u32> {
        Ok(self.byte(ea)? as u32 | (self.byte(ea + 1)? as u32) * 256)
    }

    fn defined(&self, ea: u32) -> bool {
        matches!(self.bytes.get(ea as usize), Some(Some(_)))
    }
}

/// `extract_music.load_sound_bank` (`:9-27`). Walks a list of
/// `(u16 length, u16 target)` records in the ROM, copying each run into SPC
/// memory, until a zero length gives the entry point.
///
/// The ROM cursor advances with the `get_bytes` rule — step one, and if bit 15
/// of the low word is clear add `0x8000` to skip the unmapped half-bank — but
/// note the `ea += 4` that steps over a record header does *not* apply it. That
/// asymmetry is in the Python and is reproduced.
fn load_sound_bank(rom: &Rom, mut ea: u32) -> Result<SpcMemory> {
    let mut bytes = vec![None; 65536];
    for _ in 0..=256 {
        let numbytes = rom.get_word(ea)?;
        let target = rom.get_word(ea + 2)?;
        if numbytes == 0 {
            return Ok(SpcMemory { bytes, entry_point: target });
        }
        ea += 4;
        for i in 0..numbytes {
            let dst = (target + i) as usize;
            if dst >= 65536 {
                return Err(format!("sound bank upload runs past 0xffff at {dst:#x}"));
            }
            bytes[dst] = Some(rom.get_byte(ea)?);
            ea = crate::rom::advance_mapped(ea, 1);
        }
    }
    Err("sound bank has more than 256 upload records".into())
}

// ---------------------------------------------------------------------------
// the object graph
// ---------------------------------------------------------------------------

/// The entity types. In the Python this is a Python class, recovered on the
/// compile side by matching the `Song_` / `Phrase_` / `Pattern_` / `SongList_`
/// / `Sfx_` / `SfxPort` prefix of the printed name
/// (`compile_music.py:292-307`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Song,
    SongList,
    Phrase,
    Pattern,
    SfxPattern,
    SfxList,
}

/// One entry of a `Song`'s phrase list. `PhraseLoop` is a `(count, target)`
/// pair encoded inline rather than a pointer.
#[derive(Clone, Copy)]
pub enum PhraseItem {
    Phrase(usize),
    Loop { loops: u32, jmp: i32 },
}

/// One line of a music `Pattern`.
#[derive(Clone)]
pub enum PLine {
    /// `0x80 | note`, optionally preceded by a length byte and a length+volume
    /// byte pair. `note` is the raw 7-bit value, `0..=73`.
    Note { note: u8, note_length: Option<u8>, volstuff: Option<u8> },
    /// `0xef`: a two-byte pointer plus a loop count. The length/volume bytes
    /// that may precede it in the ROM are dropped by the text round trip.
    Call { target: usize, loops: u8 },
    /// `0xe0 + idx` and `EFFECT_BYTE_LENGTH[idx]` argument bytes.
    Effect { idx: u8, args: Vec<u8> },
}

/// One line of an sfx pattern.
#[derive(Clone)]
pub enum SLine {
    /// `0xe0` + instrument number.
    SetInstrument(u8),
    /// `0xff`. Always last.
    Restart,
    /// A note, with optional length and left/right volume prefix bytes, and
    /// optionally a pitch slide (`0xf9` with a note, `0xf1` without).
    Note {
        note: Option<u8>,
        note_length: Option<u8>,
        volume_left: Option<u8>,
        volume_right: Option<u8>,
        pitch_slide: Option<[u8; 3]>,
    },
}

/// The per-entity payload. Reference fields hold arena indices, which is what
/// the Python's name lookup resolves to.
#[derive(Clone)]
pub enum Body {
    Song(Vec<PhraseItem>),
    SongList(Vec<Option<usize>>),
    Phrase(Vec<Option<usize>>),
    Pattern { lines: Vec<PLine>, fallthrough: bool },
    SfxPattern { lines: Vec<SLine>, fallthrough: bool },
    SfxList { patterns: Vec<Option<usize>>, next: Vec<u8>, echo: Vec<u8> },
}

/// One named object. `ea` is the address it was found at and, on the compile
/// side, the address `write_obj` asserts the write cursor has reached.
#[derive(Clone)]
pub struct Ent {
    pub kind: Kind,
    pub ea: u32,
    /// `extract_music.get_type_for_ea:130-134`: an object whose first byte was
    /// never uploaded into this bank. It is referenced but not emitted.
    pub imported: bool,
    /// `Song.index`, printed as a comment and never read back.
    pub index: usize,
    pub body: Body,
}

/// Everything one bank contributes. `emitted` is the entity list in the order
/// the Python's text files would carry it: ascending `ea` for the music file,
/// then the three sfx port lists and the sfx patterns for `sfx.txt`.
pub struct Bank {
    pub name: &'static str,
    pub memory: SpcMemory,
    pub arena: Vec<Ent>,
    pub emitted: Vec<usize>,
    songs_in_bank: u32,
}

// ---------------------------------------------------------------------------
// stage 9 — reading the banks
// ---------------------------------------------------------------------------

/// Stage 9, "Reading music banks". Uploads each of the three banks into its own
/// 64 KB SPC image and works out how many songs the song list at `0xd000`
/// holds. Contributes no assets on its own.
pub fn read_music_banks(rom: &Rom) -> Result<Vec<Bank>> {
    let mut banks = Vec::with_capacity(3);
    for (name, ea) in SONGS {
        let memory = load_sound_bank(rom, ea)?;
        // `extract_music.load_song:256-267`. `intro` derives the count from the
        // first pointer; `indoor` and `ending` hardcode 0xd046.
        let songs_in_bank = if name == "intro" {
            (memory.word(0xd000)? - 0xd000) / 2
        } else {
            (0xd046 - 0xd000) / 2
        };
        banks.push(Bank { name, memory, arena: Vec::new(), emitted: Vec::new(), songs_in_bank });
    }
    Ok(banks)
}

// ---------------------------------------------------------------------------
// stage 10 — decoding
// ---------------------------------------------------------------------------

/// The walker's state. `by_ea` replaces `types_for_ea`; indexing it by address
/// gives both the O(1) lookup and the ascending-address iteration that
/// `sorted(types_for_ea.items())` performs, with no map in sight.
struct Walker<'a> {
    mem: &'a SpcMemory,
    arena: Vec<Ent>,
    by_ea: Vec<Option<usize>>,
    /// `pqueue_by_ea`, kept sorted ascending. Python's `heapq` holds
    /// `(ea, obj)` tuples but addresses are unique keys in `types_for_ea`, so
    /// ordering never falls through to the object.
    queue: Vec<u32>,
}

impl<'a> Walker<'a> {
    fn new(mem: &'a SpcMemory) -> Walker<'a> {
        Walker { mem, arena: Vec::new(), by_ea: vec![None; 65536], queue: Vec::new() }
    }

    /// `get_type_for_ea` (`:118-135`). Address 0 is a null reference; anything
    /// below 256 is rejected the way the Python's assert does.
    fn get(&mut self, ea: u32, kind: Kind) -> Result<Option<usize>> {
        if ea == 0 {
            return Ok(None);
        }
        if ea < 256 {
            return Err(format!("music object address {ea:#x} is below 0x100"));
        }
        if let Some(i) = self.by_ea[ea as usize] {
            if self.arena[i].kind != kind {
                return Err(format!(
                    "{ea:#x} is already a {:?}, cannot also be a {kind:?}",
                    self.arena[i].kind
                ));
            }
            return Ok(Some(i));
        }
        let imported = !self.mem.defined(ea);
        let body = match kind {
            Kind::Song => Body::Song(Vec::new()),
            Kind::SongList => Body::SongList(Vec::new()),
            Kind::Phrase => Body::Phrase(Vec::new()),
            Kind::Pattern => Body::Pattern { lines: Vec::new(), fallthrough: false },
            Kind::SfxPattern => Body::SfxPattern { lines: Vec::new(), fallthrough: false },
            Kind::SfxList => {
                Body::SfxList { patterns: Vec::new(), next: Vec::new(), echo: Vec::new() }
            }
        };
        let i = self.arena.len();
        self.arena.push(Ent { kind, ea, imported, index: 0, body });
        self.by_ea[ea as usize] = Some(i);
        if !imported {
            let at = self.queue.partition_point(|&q| q < ea);
            self.queue.insert(at, ea);
        }
        Ok(Some(i))
    }

    fn pop(&mut self) -> Option<u32> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    fn peek(&self) -> Option<u32> {
        self.queue.first().copied()
    }
}

/// `note_to_str` composed with `kKeysDict`: the identity on `0..=73`. Values
/// above 73 hit `assert 0` in the Python.
fn check_note(note: u8) -> Result<u8> {
    if note > 73 {
        return Err(format!("note value {note} has no key name"));
    }
    Ok(note)
}

/// `decode_pattern` (`:176-211`). `next_ea` is the head of the work queue at
/// the moment this pattern was popped; reaching it means the pattern runs into
/// the next object rather than terminating, which the Python records as a
/// `Fallthrough` line and `write_pattern` turns into "do not emit the
/// terminating zero".
fn decode_pattern(w: &mut Walker, idx: usize, next_ea: Option<u32>) -> Result<()> {
    let start_ea = w.arena[idx].ea;
    let mut ea = start_ea;
    let mut lines = Vec::new();
    let mut fallthrough = false;
    loop {
        if ea != start_ea && Some(ea) == next_ea {
            fallthrough = true;
            break;
        }
        let (mut note_length, mut volstuff) = (None, None);
        let mut cmd = w.mem.byte(ea)?;
        ea += 1;
        if cmd == 0 {
            break;
        }
        if cmd & 0x80 == 0 {
            note_length = Some(cmd);
            cmd = w.mem.byte(ea)?;
            ea += 1;
            if cmd & 0x80 == 0 {
                volstuff = Some(cmd);
                cmd = w.mem.byte(ea)?;
                ea += 1;
            }
        }
        if cmd == 0xef {
            let addr = w.mem.word(ea)?;
            let loops = w.mem.byte(ea + 2)?;
            ea += 3;
            // `to_str` would raise on a null target, so a null Call cannot
            // survive the Python's own round trip either.
            let target = w.get(addr, Kind::Pattern)?.ok_or_else(|| {
                format!("Call at {:#x} targets address 0", ea - 3)
            })?;
            lines.push(PLine::Call { target, loops });
        } else if cmd >= 0xe0 {
            if note_length.is_some() || volstuff.is_some() {
                return Err(format!(
                    "effect {cmd:#x} at {:#x} carries a note length or volume",
                    ea - 1
                ));
            }
            let idx = cmd - 0xe0;
            let n = EFFECT_BYTE_LENGTH[idx as usize];
            let mut args = Vec::with_capacity(n);
            for i in 0..n {
                args.push(w.mem.byte(ea + i as u32)?);
            }
            ea += n as u32;
            lines.push(PLine::Effect { idx, args });
        } else {
            // `assert(cmd & 0x80)`: a byte below 0x80 here would have been
            // consumed as a length or volume above.
            if cmd & 0x80 == 0 {
                return Err(format!("pattern byte {cmd:#x} at {:#x} is not a note", ea - 1));
            }
            lines.push(PLine::Note {
                note: check_note(cmd & 0x7f)?,
                note_length,
                volstuff,
            });
        }
    }
    w.arena[idx].body = Body::Pattern { lines, fallthrough };
    Ok(())
}

/// `decode_phrase` (`:213-214`): eight pattern pointers.
fn decode_phrase(w: &mut Walker, idx: usize) -> Result<()> {
    let ea = w.arena[idx].ea;
    let mut patterns = Vec::with_capacity(8);
    for i in 0..8 {
        let p = w.mem.word(ea + i * 2)?;
        patterns.push(w.get(p, Kind::Pattern)?);
    }
    w.arena[idx].body = Body::Phrase(patterns);
    Ok(())
}

/// `decode_song` (`:216-235`). A word below `0x100` is a loop count followed by
/// a backwards target, which must be one of the addresses already visited.
fn decode_song(w: &mut Walker, idx: usize) -> Result<()> {
    let mut ea = w.arena[idx].ea;
    let mut phrases = Vec::new();
    let mut eas_in_phrase = Vec::new();
    loop {
        eas_in_phrase.push(ea);
        let phrase = w.mem.word(ea)?;
        if phrase == 0 {
            break;
        }
        if phrase < 0x100 {
            if phrase == 0x80 || phrase == 0x81 {
                return Err(format!("song loop count {phrase:#x} at {ea:#x} is reserved"));
            }
            let tgt = w.mem.word(ea + 2)?;
            if !eas_in_phrase.contains(&tgt) {
                return Err(format!("song loop at {ea:#x} jumps to {tgt:#x}, not a visited address"));
            }
            // Negative and always even, so truncating and flooring agree.
            phrases.push(PhraseItem::Loop { loops: phrase, jmp: (tgt as i32 - ea as i32) / 2 });
            ea += 4;
        } else {
            let p = w
                .get(phrase, Kind::Phrase)?
                .ok_or_else(|| format!("song phrase pointer at {ea:#x} is null"))?;
            phrases.push(PhraseItem::Phrase(p));
            ea += 2;
        }
    }
    w.arena[idx].body = Body::Song(phrases);
    Ok(())
}

/// `decode_sfx` (`:355-393`). Same length/volume prefix idea as a music
/// pattern, but with a separate left and right volume byte and two pitch-slide
/// encodings.
fn decode_sfx(mem: &SpcMemory, mut ea: u32, next_addr: u32) -> Result<(Vec<SLine>, bool)> {
    let mut r = Vec::new();
    loop {
        if ea == next_addr {
            return Ok((r, true));
        }
        let mut b = mem.byte(ea)?;
        ea += 1;
        if b == 0 {
            return Ok((r, false));
        }
        let mut note_length = None;
        let (mut volume_left, mut volume_right) = (None, None);
        if b & 0x80 == 0 {
            note_length = Some(b);
            b = mem.byte(ea)?;
            ea += 1;
            if b & 0x80 == 0 {
                volume_left = Some(b);
                b = mem.byte(ea)?;
                ea += 1;
                if b & 0x80 == 0 {
                    volume_right = Some(b);
                    b = mem.byte(ea)?;
                    ea += 1;
                }
            }
        }
        if b == 0xe0 {
            if note_length.is_some() || volume_left.is_some() || volume_right.is_some() {
                return Err(format!("SetInstrument at {ea:#x} carries a length or volume"));
            }
            let n = mem.byte(ea)?;
            ea += 1;
            r.push(SLine::SetInstrument(n));
        } else if b == 0xf9 || b == 0xf1 {
            let note = if b == 0xf9 {
                let n = mem.byte(ea)?;
                ea += 1;
                Some(check_note(n & 0x7f)?)
            } else {
                None
            };
            let slide = [mem.byte(ea)?, mem.byte(ea + 1)?, mem.byte(ea + 2)?];
            ea += 3;
            r.push(SLine::Note {
                note,
                note_length,
                volume_left,
                volume_right,
                pitch_slide: Some(slide),
            });
        } else if b == 0xff {
            if note_length.is_some() || volume_left.is_some() || volume_right.is_some() {
                return Err(format!("Restart at {ea:#x} carries a length or volume"));
            }
            r.push(SLine::Restart);
            return Ok((r, false));
        } else {
            r.push(SLine::Note {
                note: Some(check_note(b & 0x7f)?),
                note_length,
                volume_left,
                volume_right,
                pitch_slide: None,
            });
        }
    }
}

/// The sfx patterns `print_all_sfx` names explicitly after walking the three
/// port tables (`extract_music.py:417-433`) — entry points that nothing in the
/// tables points at.
const EXTRA_SFX: [u32; 17] = [
    0x1a5b, 0x1d1c, 0x1ee2, 0x1f13, 0x1f1c, 0x252d, 0x2533, 0x26a2, 0x277e, 0x279d, 0x27c9,
    0x27f6, 0x2807, 0x2818, 0x2829, 0x2831, 0x284a,
];

/// The three sfx port tables: `(base, entries, has_echo_column)`.
const SFX_PORTS: [(u32, u32, bool); 3] =
    [(0x17c0, 32, false), (0x1820, 63, true), (0x191c, 63, true)];

/// Stage 10, "Decoding music". Walks each bank's song list, phrases and
/// patterns, and — for `intro` only — the three sfx port tables and every sfx
/// pattern they reach. Contributes no assets on its own.
pub fn decode_music(banks: &mut [Bank]) -> Result<()> {
    for bank in banks.iter_mut() {
        let mut w = Walker::new(&bank.memory);

        // `get_song_list(0xd000, SONGS_IN_BANK)` (`print_song:271`).
        let list = w.get(0xd000, Kind::SongList)?.expect("0xd000 is not null");
        let mut songs = Vec::with_capacity(bank.songs_in_bank as usize);
        for i in 0..bank.songs_in_bank {
            let ea = w.memory_word(0xd000 + i * 2)?;
            let s = w.get(ea, Kind::Song)?;
            if let Some(s) = s {
                w.arena[s].index = i as usize;
            }
            songs.push(s);
        }
        w.arena[list].body = Body::SongList(songs);

        // The hand-listed extra entry points, per bank (`print_song:272-284`).
        match bank.name {
            "intro" => {
                for ea in [0xD878u32, 0xD8A8, 0xD8B8, 0xDf11, 0xe37c] {
                    w.get(ea, Kind::Phrase)?;
                }
            }
            "indoor" => {
                w.get(0xDc5e, Kind::Phrase)?;
                w.get(0xDc6e, Kind::Phrase)?;
                w.get(0xe905, Kind::Pattern)?;
                w.get(0xe94a, Kind::Phrase)?;
            }
            "ending" => {
                w.get(0x2a10, Kind::Phrase)?;
            }
            _ => unreachable!(),
        }

        // The work loop. `next_ea` is read *after* the pop and *before* the
        // decode, so objects discovered while decoding this one do not affect
        // its own fallthrough decision. Python evaluates the argument
        // expression first for exactly the same reason.
        while let Some(ea) = w.pop() {
            let next_ea = w.peek();
            let idx = w.by_ea[ea as usize].expect("queued address has an entity");
            match w.arena[idx].kind {
                Kind::Song => decode_song(&mut w, idx)?,
                Kind::SongList => {} // already filled; `decode_any` does nothing
                Kind::Phrase => decode_phrase(&mut w, idx)?,
                Kind::Pattern => decode_pattern(&mut w, idx, next_ea)?,
                k => return Err(format!("{k:?} should not be on the music work queue")),
            }
        }

        // `print_song:288-290`: ascending address, imported objects skipped.
        let mut emitted: Vec<usize> = Vec::new();
        for ea in 0..65536usize {
            if let Some(i) = w.by_ea[ea] {
                if !w.arena[i].imported {
                    emitted.push(i);
                }
            }
        }

        // `print_all_sfx`, intro only. The port tables come first in the file,
        // then the patterns in ascending address order.
        if bank.name == "intro" {
            let mut items: Vec<u32> = Vec::new();
            let mut ports: Vec<usize> = Vec::new();
            for (base, num, has_echo) in SFX_PORTS {
                let li = w.get(base, Kind::SfxList)?.expect("port base is not null");
                let next_ea = base + num * 2;
                let echo_ea = next_ea + num;
                let mut patterns = Vec::with_capacity(num as usize);
                let mut next = Vec::with_capacity(num as usize);
                let mut echo = Vec::new();
                for i in 0..num {
                    let ea = w.memory_word(base + i * 2)?;
                    if ea != 0 && !items.contains(&ea) {
                        items.push(ea);
                    }
                    patterns.push(if ea == 0 { None } else { Some(ea as usize) });
                    next.push(w.mem.byte(next_ea + i)?);
                    if has_echo {
                        echo.push(w.mem.byte(echo_ea + i)?);
                    }
                }
                // Resolved to arena indices below, once every pattern exists.
                w.arena[li].body = Body::SfxList { patterns, next, echo };
                ports.push(li);
            }
            for ea in EXTRA_SFX {
                if !items.contains(&ea) {
                    items.push(ea);
                }
            }
            items.sort_unstable();

            let mut pats = Vec::with_capacity(items.len());
            for (i, &ea) in items.iter().enumerate() {
                let next_addr = if i + 1 < items.len() { items[i + 1] } else { 0 };
                let (lines, fallthrough) = decode_sfx(w.mem, ea, next_addr)?;
                let idx = w.get(ea, Kind::SfxPattern)?.expect("sfx address is not null");
                w.arena[idx].body = Body::SfxPattern { lines, fallthrough };
                pats.push(idx);
            }

            // Second pass over the port tables: turn the addresses stashed in
            // `patterns` into arena indices.
            for &li in &ports {
                let resolved: Vec<Option<usize>> = match &w.arena[li].body {
                    Body::SfxList { patterns, .. } => patterns
                        .iter()
                        .map(|p| p.map(|ea| w.by_ea[ea].expect("sfx target was decoded")))
                        .collect(),
                    _ => unreachable!(),
                };
                if let Body::SfxList { patterns, .. } = &mut w.arena[li].body {
                    *patterns = resolved;
                }
            }

            emitted.extend(ports);
            emitted.extend(pats);
        }

        bank.arena = w.arena;
        bank.emitted = emitted;
    }
    Ok(())
}

impl Walker<'_> {
    fn memory_word(&self, ea: u32) -> Result<u32> {
        self.mem.word(ea)
    }
}

// ---------------------------------------------------------------------------
// stage 11 — instruments and samples
// ---------------------------------------------------------------------------

/// One entry of the ADSR triple, shared by instruments and sfx instruments
/// (`add_sustain_decay_etc`, `extract_music.py:316-322`). Split into fields on
/// extract and recombined on compile, which forces bit 7 of `adsr1` to 1.
#[derive(Clone, Copy, Default)]
pub struct Adsr {
    pub decay: u8,
    pub attack: u8,
    pub sustain_level: u8,
    pub sustain_rate: u8,
    pub vxgain: u8,
}

impl Adsr {
    fn read(mem: &SpcMemory, ea: u32) -> Result<Adsr> {
        let (adsr1, adsr2, gain) = (mem.byte(ea)?, mem.byte(ea + 1)?, mem.byte(ea + 2)?);
        Ok(Adsr {
            decay: (adsr1 >> 4) & 7,
            attack: adsr1 & 0xf,
            sustain_level: adsr2 >> 5,
            sustain_rate: adsr2 & 0x1f,
            vxgain: gain,
        })
    }

    fn adsr1(self) -> u8 {
        0x80 | self.decay << 4 | self.attack
    }

    fn adsr2(self) -> u8 {
        self.sustain_level << 5 | self.sustain_rate
    }
}

#[derive(Clone)]
pub struct Sample {
    /// `kDupSamples`-folded index; the Python's `sample_to_addr` key.
    pub file: usize,
    /// Present only when bit 1 of the first BRR header byte is set.
    pub repeat: Option<u32>,
    /// The BRR bytes of *this* sample's `file`, i.e. already deduplicated.
    pub brr: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct Instrument {
    pub sample: u8,
    pub adsr: Adsr,
    pub pitch_base: u16,
}

#[derive(Clone, Copy)]
pub struct SfxInstrument {
    pub voll: u8,
    pub volr: u8,
    pub pitch: u16,
    pub sample: u8,
    pub adsr: Adsr,
    pub pitch_base: u8,
}

/// `music_info.yaml` as a struct. Ints only, so the YAML round trip is exact
/// and nothing here needs a normalisation.
pub struct MusicInfo {
    pub samples: Vec<Sample>,
    pub instruments: Vec<Instrument>,
    pub note_gate_off: Vec<u8>,
    pub note_volume: Vec<u8>,
    pub sfx_instruments: Vec<SfxInstrument>,
}

/// Stage 11, "Reading instruments". Reads the 25-entry sample directory, the
/// instrument and sfx-instrument tables and the note gate/volume tables out of
/// the intro bank's SPC image, and copies each sample's BRR bytes.
///
/// The BRR length is the only thing `util.decode_brr` was ever needed for on
/// the live path: `len(r) // 16 * 9` is nine bytes per block, and the block
/// count is found by walking the end flag in bit 0 of each block header. No
/// sample decoding happens, and none is needed — the bytes reach the .dat
/// verbatim.
pub fn read_instruments(mem: &SpcMemory) -> Result<MusicInfo> {
    // How many 9-byte BRR blocks a sample has.
    let brr_blocks = |start: u32| -> Result<u32> {
        let mut n = 0;
        loop {
            let cmd = mem.byte(start + n * 9)?;
            n += 1;
            if cmd & 1 != 0 {
                return Ok(n);
            }
            if n > 8192 {
                return Err(format!("BRR sample at {start:#x} has no end block"));
            }
        }
    };

    let mut samples = Vec::with_capacity(25);
    for i in 0..25u32 {
        let start = mem.word(0x3c00 + i * 4)?;
        let rep = mem.word(0x3c00 + i * 4 + 2)?;
        let file = dup_sample(i as usize);
        let repeat = if mem.byte(start)? & 2 != 0 {
            Some((rep - start) / 9 * 16)
        } else {
            None
        };
        // The BRR that gets written is the *deduplicated* file's, so it comes
        // from `sound<file>.pcm.brr`, not from this entry's own start address.
        let file_start = mem.word(0x3c00 + file as u32 * 4)?;
        let n = brr_blocks(file_start)? * 9;
        let mut brr = Vec::with_capacity(n as usize);
        for x in 0..n {
            brr.push(mem.byte(file_start + x)?);
        }
        samples.push(Sample { file, repeat, brr });
    }

    let mut instruments = Vec::with_capacity(25);
    for i in 0..25u32 {
        let ea = 0x3d00 + i * 6;
        instruments.push(Instrument {
            sample: mem.byte(ea)?,
            adsr: Adsr::read(mem, ea + 1)?,
            pitch_base: ((mem.byte(ea + 4)? as u16) << 8) | mem.byte(ea + 5)? as u16,
        });
    }

    let mut note_gate_off = Vec::with_capacity(8);
    for i in 0..8u32 {
        note_gate_off.push(mem.byte(0x3D96 + i)?);
    }
    let mut note_volume = Vec::with_capacity(16);
    for i in 0..16u32 {
        note_volume.push(mem.byte(0x3D9E + i)?);
    }

    let mut sfx_instruments = Vec::with_capacity(25);
    for i in 0..25u32 {
        let ea = 0x3e00 + i * 9;
        sfx_instruments.push(SfxInstrument {
            voll: mem.byte(ea)?,
            volr: mem.byte(ea + 1)?,
            pitch: mem.word(ea + 2)? as u16,
            sample: mem.byte(ea + 4)?,
            adsr: Adsr::read(mem, ea + 5)?,
            pitch_base: mem.byte(ea + 8)?,
        });
    }

    Ok(MusicInfo { samples, instruments, note_gate_off, note_volume, sfx_instruments })
}

// ---------------------------------------------------------------------------
// stage 14 — serialising
// ---------------------------------------------------------------------------

/// `compile_music.Serializer` (`:187-288`).
///
/// `memory` is `Option<u8>` for the same reason the extract side's is: what is
/// left undefined never reaches the output. [`Serializer::write`] refuses to
/// overwrite a defined byte, exactly as the Python's `assert`; `write_at`,
/// `write_word` and the direct `memory[...]` stores used by the instrument
/// tables deliberately do not.
struct Serializer {
    memory: Vec<Option<u8>>,
    /// `(patch address, arena index of the target)`.
    relocs: Vec<(u32, usize)>,
    addr: Option<u32>,
}

impl Serializer {
    fn new() -> Serializer {
        Serializer { memory: vec![None; 65536], relocs: Vec::new(), addr: None }
    }

    fn cur(&self) -> Result<u32> {
        self.addr.ok_or_else(|| "write with no current address".to_string())
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let mut a = self.cur()?;
        for &d in data {
            if a as usize >= 65536 {
                return Err("write past 0xffff".into());
            }
            if self.memory[a as usize].is_some() {
                return Err(format!("byte {a:#x} written twice"));
            }
            self.memory[a as usize] = Some(d);
            a += 1;
        }
        self.addr = Some(a);
        Ok(())
    }

    fn write_at(&mut self, mut a: u32, data: &[u8]) {
        for &d in data {
            self.memory[a as usize] = Some(d);
            a += 1;
        }
    }

    fn write_word(&mut self, a: u32, v: u32) {
        self.memory[a as usize] = Some((v & 0xff) as u8);
        self.memory[a as usize + 1] = Some((v >> 8 & 0xff) as u8);
    }

    /// `write_reloc_entry`: a two-byte hole, recorded for patching unless the
    /// reference is null.
    fn write_reloc_entry(&mut self, r: Option<usize>) -> Result<()> {
        self.write(&[0, 0])?;
        if let Some(r) = r {
            self.relocs.push((self.cur()? - 2, r));
        }
        Ok(())
    }
}

/// `produce_loadable_seq` (`:406-425`). Emits each maximal run of defined bytes
/// as `(len_lo, len_hi, addr_lo, addr_hi, bytes...)`, terminated by a zero
/// length. Note the loop shape: a run is only emitted when `j != start`, which
/// is what skips the leading undefined region without emitting an empty record.
fn produce_loadable_seq(s: &Serializer) -> Vec<u8> {
    let mut r = Vec::new();
    let (mut start, mut i) = (0usize, 0usize);
    while start < 0x10000 {
        while i < 0x10000 && s.memory[i].is_some() {
            i += 1;
        }
        let j = i;
        while i < 0x10000 && s.memory[i].is_none() {
            i += 1;
        }
        if j == start {
            start = i;
            continue;
        }
        let n = j - start;
        r.push((n & 0xff) as u8);
        r.push((n >> 8) as u8);
        r.push((start & 0xff) as u8);
        r.push((start >> 8) as u8);
        for k in start..j {
            r.push(s.memory[k].expect("run is defined"));
        }
        start = i;
    }
    r.push(0);
    r.push(0);
    r
}

/// Per-entity compile state: `defined` and `write_addr` in the Python's object.
#[derive(Clone, Copy, Default)]
struct EntState {
    defined: bool,
    seen: bool,
    write_addr: Option<u32>,
}

fn write_pattern(s: &mut Serializer, lines: &[PLine], fallthrough: bool) -> Result<()> {
    for line in lines {
        match line {
            PLine::Note { note, note_length, volstuff } => {
                if let Some(n) = note_length {
                    s.write(&[*n])?;
                }
                if let Some(v) = volstuff {
                    s.write(&[*v])?;
                }
                s.write(&[0x80 | note])?;
            }
            PLine::Call { target, loops } => {
                s.write(&[0xe0 + EFFECT_CALL, 0, 0, *loops])?;
                let at = s.cur()? - 3;
                s.relocs.push((at, *target));
            }
            PLine::Effect { idx, args } => {
                if args.len() != EFFECT_BYTE_LENGTH[*idx as usize] {
                    return Err(format!("effect {idx} has {} args", args.len()));
                }
                s.write(&[0xe0 + idx])?;
                s.write(args)?;
            }
        }
    }
    if !fallthrough {
        s.write(&[0])?;
    }
    Ok(())
}

/// `write_sfx_pattern` (`:129-161`). The nested length/volume writes mirror the
/// nested reads in `decode_sfx`: a volume byte can only appear after a length
/// byte, and the right volume only after the left.
fn write_sfx_pattern(s: &mut Serializer, lines: &[SLine], fallthrough: bool) -> Result<()> {
    for (i, line) in lines.iter().enumerate() {
        match line {
            SLine::SetInstrument(n) => s.write(&[0xe0, *n])?,
            SLine::Restart => {
                s.write(&[0xff])?;
                if i != lines.len() - 1 {
                    return Err("Restart is not the last sfx line".into());
                }
                return Ok(());
            }
            SLine::Note { note, note_length, volume_left, volume_right, pitch_slide } => {
                if let Some(n) = note_length {
                    s.write(&[*n])?;
                    if let Some(l) = volume_left {
                        s.write(&[*l])?;
                        if let Some(r) = volume_right {
                            s.write(&[*r])?;
                        }
                    }
                }
                match (pitch_slide, note) {
                    (Some(p), None) => s.write(&[0xf1, p[0], p[1], p[2]])?,
                    (Some(p), Some(n)) => s.write(&[0xf9, n | 0x80, p[0], p[1], p[2]])?,
                    (None, Some(n)) => s.write(&[0x80 | n])?,
                    (None, None) => return Err("sfx line has neither a note nor a slide".into()),
                }
            }
        }
    }
    if !fallthrough {
        s.write(&[0])?;
    }
    Ok(())
}

/// `write_obj` (`:251-278`). The recorded address is *checked*, not recomputed:
/// the cursor must already be there, except at the three gap starts where the
/// Python hard-seeks.
fn write_obj(s: &mut Serializer, arena: &[Ent], st: &mut [EntState], idx: usize) -> Result<()> {
    let ea = arena[idx].ea;
    if s.addr.is_none() || GAP_START_ADDRS.contains(&ea) {
        s.addr = Some(ea);
    } else if Some(ea) != s.addr {
        return Err(format!(
            "object at {ea:#x} does not follow the previous one, which ended at {:#x}",
            s.addr.unwrap()
        ));
    }
    st[idx].write_addr = s.addr;

    match &arena[idx].body {
        Body::Phrase(patterns) => {
            let patterns = patterns.clone();
            if patterns.len() != 8 {
                return Err(format!("phrase at {ea:#x} has {} patterns", patterns.len()));
            }
            for p in patterns {
                s.write_reloc_entry(p)?;
            }
        }
        Body::Pattern { lines, fallthrough } => {
            let (lines, ft) = (lines.clone(), *fallthrough);
            write_pattern(s, &lines, ft)?;
        }
        Body::Song(phrases) => {
            let phrases = phrases.clone();
            for p in phrases {
                match p {
                    PhraseItem::Loop { loops, jmp } => {
                        let i = s.cur()? as i32 + jmp * 2;
                        s.write(&[loops as u8, 0])?;
                        s.write(&[(i & 0xff) as u8, ((i >> 8) & 0xff) as u8])?;
                    }
                    PhraseItem::Phrase(p) => s.write_reloc_entry(Some(p))?,
                }
            }
            s.write(&[0, 0])?;
        }
        Body::SongList(songs) => {
            let songs = songs.clone();
            for song in songs {
                s.write_reloc_entry(song)?;
            }
        }
        Body::SfxPattern { lines, fallthrough } => {
            let (lines, ft) = (lines.clone(), *fallthrough);
            write_sfx_pattern(s, &lines, ft)?;
        }
        Body::SfxList { patterns, next, echo } => {
            let (patterns, next, echo) = (patterns.clone(), next.clone(), echo.clone());
            for p in patterns {
                s.write_reloc_entry(p)?;
            }
            s.write(&next)?;
            s.write(&echo)?;
        }
    }
    Ok(())
}

/// Stage 14, "Building sound banks". Serialises one bank and returns the
/// `kSoundBank_<song>` payload, having first checked it byte for byte against
/// the image the extract side loaded — `compare_with_orig`
/// (`compile_music.py:386-404`), which diffs against `sound/<song>.spc` where
/// an undefined byte was written as zero.
pub fn build_sound_bank(bank: &Bank, info: Option<&MusicInfo>) -> Result<Vec<u8>> {
    let mut s = Serializer::new();
    let mut st = vec![EntState::default(); bank.arena.len()];

    // `serialize_song`'s intro pre-pass (`:328-368`): samples at 0x4000, then
    // the directory, instrument and sfx-instrument tables written by address.
    if let Some(info) = info {
        s.addr = Some(0x4000);
        // `sample_to_addr`, keyed by the deduplicated sample index.
        let mut sample_to_addr: Vec<Option<u32>> = vec![None; info.samples.len()];
        for (i, sample) in info.samples.iter().enumerate() {
            if sample_to_addr[sample.file].is_none() {
                sample_to_addr[sample.file] = Some(s.cur()?);
                s.write(&sample.brr)?;
            }
            let addr = sample_to_addr[sample.file].expect("just set");
            s.write_word(0x3c00 + i as u32 * 4, addr);
            // Without a repeat point the Python stores the *current* cursor,
            // which is the end of everything written so far, not the end of
            // this sample.
            let second = match sample.repeat {
                Some(rep) => addr + rep / 16 * 9,
                None => s.cur()?,
            };
            s.write_word(0x3c00 + i as u32 * 4 + 2, second);
        }
        for i in 0..6u32 {
            s.write_word(0x3c64 + i * 2, 0xffff);
        }

        for (i, ins) in info.instruments.iter().enumerate() {
            let ea = 0x3d00 + i * 6;
            s.memory[ea] = Some(ins.sample);
            s.memory[ea + 1] = Some(ins.adsr.adsr1());
            s.memory[ea + 2] = Some(ins.adsr.adsr2());
            s.memory[ea + 3] = Some(ins.adsr.vxgain);
            s.memory[ea + 4] = Some((ins.pitch_base >> 8) as u8);
            s.memory[ea + 5] = Some((ins.pitch_base & 0xff) as u8);
        }

        s.write_at(0x3D96, &info.note_gate_off);
        s.write_at(0x3D9e, &info.note_volume);

        for (i, ins) in info.sfx_instruments.iter().enumerate() {
            let ea = 0x3e00 + i as u32 * 9;
            s.memory[ea as usize] = Some(ins.voll);
            s.memory[ea as usize + 1] = Some(ins.volr);
            s.write_word(ea + 2, ins.pitch as u32);
            s.memory[ea as usize + 4] = Some(ins.sample);
            s.memory[ea as usize + 5] = Some(ins.adsr.adsr1());
            s.memory[ea as usize + 6] = Some(ins.adsr.adsr2());
            s.memory[ea as usize + 7] = Some(ins.adsr.vxgain);
            s.memory[ea as usize + 8] = Some(ins.pitch_base);
        }
    }

    // `serialize_song:370`. The pre-pass left the cursor at the end of the BRR
    // block; dropping it makes the first entity seek to its own address.
    s.addr = None;

    // The emission order. `compile_music.print_song:437` re-sorts the
    // concatenation of the music file's entities and the sfx file's by address,
    // with Python's *stable* `sorted`, so two entities sharing an address keep
    // music-before-sfx order. `bank.emitted` is already in that concatenation
    // order, so a stable sort here is the same operation.
    let mut order = bank.emitted.clone();
    order.sort_by_key(|&i| bank.arena[i].ea);

    for &i in &order {
        st[i].defined = true;
        st[i].seen = true;
    }
    // `types_for_name` also holds every object merely *referenced* by an
    // emitted one, and `serialize_song:380-382` raises if any of them was never
    // defined. Objects the extractor created but never printed and never
    // referenced are simply absent from the Python's namespace.
    for &i in &order {
        match &bank.arena[i].body {
            Body::Song(ps) => {
                for p in ps {
                    if let PhraseItem::Phrase(p) = p {
                        st[*p].seen = true;
                    }
                }
            }
            Body::SongList(v) | Body::Phrase(v) | Body::SfxList { patterns: v, .. } => {
                for p in v.iter().flatten() {
                    st[*p].seen = true;
                }
            }
            Body::Pattern { lines, .. } => {
                for l in lines {
                    if let PLine::Call { target, .. } = l {
                        st[*target].seen = true;
                    }
                }
            }
            Body::SfxPattern { .. } => {}
        }
    }

    for &i in &order {
        write_obj(&mut s, &bank.arena, &mut st, i)?;
    }

    // `serialize_song:374-377`: the indoor bank's song list points at
    // `Song_0x2880`, which lives in another bank. It is marked defined with a
    // fixed address so it resolves as a reloc target without being written.
    if bank.name == "indoor" {
        let mut found = false;
        for (i, e) in bank.arena.iter().enumerate() {
            if e.kind == Kind::Song && e.ea == 0x2880 {
                st[i].defined = true;
                st[i].write_addr = Some(0x2880);
                found = true;
            }
        }
        if !found {
            return Err("the indoor bank has no Song_0x2880 to fix up".into());
        }
    }

    for (i, state) in st.iter().enumerate() {
        if state.seen && !state.defined {
            return Err(format!(
                "symbol {:?}_0x{:x} not defined",
                bank.arena[i].kind, bank.arena[i].ea
            ));
        }
    }

    // `process_relocs`.
    for &(p, r) in &s.relocs {
        let a = st[r].write_addr.ok_or_else(|| {
            format!("reloc at {p:#x} targets an object with no address")
        })?;
        s.memory[p as usize] = Some((a & 0xff) as u8);
        s.memory[p as usize + 1] = Some((a >> 8) as u8);
    }

    // `compare_with_orig`: the serialised image must agree with the one the
    // loader produced, wherever the serialiser defined a byte.
    for i in 0..65536usize {
        if let Some(got) = s.memory[i] {
            let want = bank.memory.bytes[i].unwrap_or(0);
            if got != want {
                return Err(format!(
                    "{}: serialised byte {i:#x} is {got:#x}, the SPC image has {want:#x}",
                    bank.name
                ));
            }
        }
    }

    Ok(produce_loadable_seq(&s))
}

/// Stage 14 over all three banks, in `print_sound_banks` order.
pub fn build_sound_banks(banks: &[Bank], info: &MusicInfo) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::with_capacity(banks.len());
    for bank in banks {
        // Only `intro` gets the sample and instrument pre-pass.
        let i = if bank.name == "intro" { Some(info) } else { None };
        out.push(build_sound_bank(bank, i)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------

/// Assets 0-2: `kSoundBank_intro`, `kSoundBank_indoor`, `kSoundBank_ending`,
/// added in that order. `print_sound_banks` (`compile_resources.py:751-754`).
pub fn add_all(rom: &Rom, a: &mut Assets) -> Result<()> {
    let mut banks = read_music_banks(rom)?;
    decode_music(&mut banks)?;
    let info = read_instruments(&banks[0].memory)?;
    add_sound_banks(&banks, &info, a)
}

/// Assets 0-2 from already-decoded banks, so the reading and decoding stages
/// can be separate phases from the building one.
pub fn add_sound_banks(banks: &[Bank], info: &MusicInfo, a: &mut Assets) -> Result<()> {
    let data = build_sound_banks(banks, info)?;
    for (bank, payload) in banks.iter().zip(data.into_iter()) {
        let name = match bank.name {
            "intro" => "kSoundBank_intro",
            "indoor" => "kSoundBank_indoor",
            "ending" => "kSoundBank_ending",
            other => return Err(format!("unknown sound bank {other}")),
        };
        // The store may already hold the key as a placeholder (a standalone
        // test store does not), so fill in place when it does.
        if a.get(name).is_some() {
            a.fill(name, crate::pack::Kind::Uint8, payload)?;
        } else {
            a.add_uint8(name, &payload)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_table_is_the_python_one() {
        assert_eq!(EFFECT_BYTE_LENGTH.len(), 27);
        // 'Call' is 0xef, the one effect with a relocated operand.
        assert_eq!(EFFECT_CALL, 0xef - 0xe0);
        assert_eq!(EFFECT_BYTE_LENGTH[EFFECT_CALL as usize], 3);
    }

    #[test]
    fn duplicate_samples_fold_the_way_the_python_dict_does() {
        assert_eq!(dup_sample(10), 9);
        assert_eq!(dup_sample(20), 19);
        assert_eq!(dup_sample(9), 9);
        assert_eq!(dup_sample(24), 24);
    }

    #[test]
    fn adsr_round_trip_forces_the_high_bit() {
        // The extract side drops bit 7 of adsr1 into nothing and the compile
        // side puts a 1 back, so a byte with bit 7 clear would not survive.
        let mem = SpcMemory { bytes: vec![Some(0x7f), Some(0xff), Some(0x42)], entry_point: 0 };
        let a = Adsr::read(&mem, 0).unwrap();
        assert_eq!(a.decay, 7);
        assert_eq!(a.attack, 0xf);
        assert_eq!(a.adsr1(), 0xff);
        assert_eq!(a.adsr2(), 0xff);
        assert_eq!(a.vxgain, 0x42);
    }

    #[test]
    fn loadable_seq_skips_undefined_and_terminates() {
        let mut s = Serializer::new();
        s.addr = Some(0x10);
        s.write(&[1, 2, 3]).unwrap();
        s.addr = Some(0x20);
        s.write(&[4, 5]).unwrap();
        assert_eq!(
            produce_loadable_seq(&s),
            vec![3, 0, 0x10, 0, 1, 2, 3, 2, 0, 0x20, 0, 4, 5, 0, 0]
        );
    }

    #[test]
    fn the_serialiser_refuses_to_overwrite() {
        let mut s = Serializer::new();
        s.addr = Some(0x100);
        s.write(&[1]).unwrap();
        s.addr = Some(0x100);
        assert!(s.write(&[2]).is_err());
    }
}

/// Tests that need the real cartridge, skipped unless `ZELDA3_ROM` is set:
///
/// ```sh
/// ZELDA3_ROM="/path/to/zelda3.sfc" cargo test -- --ignored
/// ```
#[cfg(test)]
mod rom_tests {
    use super::*;

    fn load() -> Option<Rom> {
        Some(Rom::new(std::fs::read(std::env::var("ZELDA3_ROM").ok()?).ok()?))
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn sound_bank_payload_sizes_match_the_reference_build() {
        let Some(rom) = load() else { return };
        let mut banks = read_music_banks(&rom).unwrap();
        decode_music(&mut banks).unwrap();
        let info = read_instruments(&banks[0].memory).unwrap();
        let data = build_sound_banks(&banks, &info).unwrap();
        assert_eq!(
            data.iter().map(|d| d.len()).collect::<Vec<_>>(),
            vec![50066, 12756, 8354]
        );
    }

    /// The end-to-end check: build a .dat holding only assets 0-2 and diff it
    /// against the Python's `zelda3_assets.dat` with `compare.mjs`. The other
    /// 162 keys report MISSING, which is the point — this slice owns three.
    ///
    /// ```sh
    /// ZELDA3_ROM=... ZELDA3_ORACLE=.../zelda3_assets.dat cargo test -- --ignored
    /// ```
    #[test]
    #[ignore = "needs ZELDA3_ROM and ZELDA3_ORACLE"]
    fn assets_0_to_2_match_the_oracle() {
        let Some(rom) = load() else { return };
        let Ok(oracle) = std::env::var("ZELDA3_ORACLE") else { return };

        let mut a = crate::pack::Assets::new();
        for (name, kind, _) in crate::assets::ASSET_TABLE.iter().take(3) {
            a.add_placeholder(name, *kind).unwrap();
        }
        add_all(&rom, &mut a).unwrap();

        let dir = std::env::temp_dir().join("zelda3-music-slice");
        std::fs::create_dir_all(&dir).unwrap();
        let ours = dir.join("music_only.dat");
        std::fs::write(&ours, a.serialise()).unwrap();

        let out = std::process::Command::new("node")
            .args(["compare.mjs", &oracle, ours.to_str().unwrap(), "--all"])
            .output()
            .expect("node compare.mjs");
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        println!("{text}");
        for name in ["kSoundBank_intro", "kSoundBank_indoor", "kSoundBank_ending"] {
            let line = text
                .lines()
                .find(|l| l.starts_with(name))
                .unwrap_or_else(|| panic!("no row for {name}"));
            assert!(line.ends_with("  ok"), "{line}");
        }
    }
}
