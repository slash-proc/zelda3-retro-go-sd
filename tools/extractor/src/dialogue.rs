//! Assets 94-96: `kDialogue`, `kDialogueFont`, `kDialogueMap`.
//!
//! Port of `text_compression.py` (decode + greedy re-encode + dictionary
//! encode), the font half of `sprite_sheets.py` (`kFontTypes`, `decode_font`,
//! `encode_font_from_png`) and `compile_resources.print_dialogue` (`:121-146`).
//!
//! # Shape
//!
//! Each of the three assets is a packed array with **one entry per language**,
//! languages being `us` plus whatever translated ROMs were supplied. The US
//! entry always comes from the base ROM; every other entry comes from that
//! language's own cartridge, which contributes *only* dialogue and font.
//!
//! # The two hazards
//!
//! 1. [`encode_greedy`] is **first-match, not longest-match**, over an
//!    insertion-ordered dictionary (`text_compression.py:474-478`: `rev` is a
//!    dict of dicts, iterated in insertion order). The tables happen to be
//!    written longest-first, which is the only reason it behaves like a
//!    longest match. The dictionary is therefore a slice, scanned in order,
//!    and must never become a hash or an ordered-by-key map.
//!
//! 2. Alphabets are indexed by **character**, not by byte: several entries are
//!    non-ASCII (`"ö"`, `"…"`) and several are multi-character (`"[1HeartL]"`).
//!    Everything here works on `Vec<char>`.
//!
//! The `dialogue.txt` / `font_XX.png` intermediates the Python writes between
//! its two invocations are not reproduced. The text file is a pure `"%d: %s"`
//! round trip (`extract_resources.py:242`, `compile_resources.py:69-74`), and
//! the PNG round trip is the identity for every language but `pt`, asserted by
//! `sprite_sheets.py:199` itself.

#[path = "dialogue_tables.rs"]
mod tables;

pub use tables::{Font, Lang, FONTS, LANGS};

use crate::pack::{pack_arrays, Assets, Kind};
use crate::rom::Rom;

pub type Result<T> = core::result::Result<T, String>;

/// The language table entry for a code, e.g. `"fr"`.
pub fn lang(code: &str) -> Result<&'static Lang> {
    LANGS
        .iter()
        .find(|l| l.code == code)
        .ok_or_else(|| format!("no text tables for language {code:?}"))
}

/// The font table entry for a code.
pub fn font_info(code: &str) -> Result<&'static Font> {
    FONTS
        .iter()
        .find(|f| f.code == code)
        .ok_or_else(|| format!("no font tables for language {code:?}"))
}

/// `text_compression.uses_new_format`.
pub fn uses_new_format(code: &str) -> Result<bool> {
    Ok(lang(code)?.new_encoder)
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// `text_compression.decode_strings_generic` followed by the fix-up in
/// `print_strings` (`:460-467`): a PAL decode that yields 396 strings gets a
/// synthetic one spliced in at index 4, so every language ends up with the
/// same 397-entry table.
///
/// The module-level `dict_expansion` statistics list the Python appends to is
/// dead and is not reproduced.
pub fn decode_strings(rom: &Rom, code: &str) -> Result<Vec<String>> {
    let info = lang(code)?;
    let mut texts = decode_strings_raw(rom, info)?;
    if texts.len() == 396 {
        texts.insert(
            4,
            "[Speed 00]0- [Number 00]. 1- [Number 01][2]2- [Number 02]. 3- [Number 03]".to_string(),
        );
    }
    Ok(texts)
}

/// The literal loop of `decode_strings_generic`. Addresses advance by plain
/// addition, exactly as the Python does — this is not one of the four ROM
/// address arithmetics in [`crate::rom`], because the Python never wraps here.
pub fn decode_strings_raw(rom: &Rom, info: &Lang) -> Result<Vec<String>> {
    let mut p = info.rom_addrs[0];
    let mut rom_idx = 1usize;
    let mut result: Vec<String> = Vec::new();
    loop {
        let mut s = String::new();
        loop {
            let c = rom.get_byte(p)?;
            let l = if c >= info.command_start && c < info.switch_bank {
                info.command_lengths[(c - info.command_start) as usize] as u32
            } else {
                1
            };
            p += l;
            if c == 0x7f {
                // EndMessage. Checked before anything is appended, which is
                // why `command_names` may be shorter than `command_lengths`.
                break;
            }
            if c < info.command_start {
                let mut c = c;
                if Some(c) == info.escape {
                    c = rom.get_byte(p)?;
                    p += 1;
                }
                s.push_str(
                    info.alphabet
                        .get(c as usize)
                        .ok_or_else(|| format!("byte {c:#x} is outside the {} alphabet", info.code))?,
                );
            } else if c < info.switch_bank {
                let name = info
                    .command_names
                    .get((c - info.command_start) as usize)
                    .ok_or_else(|| format!("byte {c:#x} has no command name in {}", info.code))?;
                if l == 2 {
                    // `'[%s %.2d]'` -- zero-padded to two digits, wider if the
                    // parameter needs it.
                    s.push_str(&format!("[{} {:02}]", name, rom.get_byte(p - 1)?));
                } else {
                    s.push_str(&format!("[{name}]"));
                }
            } else if c == info.finish {
                return Ok(result);
            } else if c == info.switch_bank {
                p = *info
                    .rom_addrs
                    .get(rom_idx)
                    .ok_or_else(|| "text stream switched bank more than twice".to_string())?;
                rom_idx += 1;
                s.clear();
            } else if c < info.switch_bank + 8 {
                // `assert 0` for everything but pt, which uses the gap.
                if info.code != "pt" {
                    return Err(format!(
                        "unexpected byte {c:#x} in the {} text stream",
                        info.code
                    ));
                }
            } else {
                s.push_str(
                    info.dictionary
                        .get((c - info.dict_base_dec) as usize)
                        .ok_or_else(|| {
                            format!("dictionary index {} out of range in {}", c - info.dict_base_dec, info.code)
                        })?,
                );
            }
        }
        result.push(s);
        if result.len() >= 397 && info.code == "pt" {
            return Ok(result);
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// `text_compression.compress_strings`.
pub fn compress_strings(texts: &[String], code: &str) -> Result<Vec<Vec<u8>>> {
    let info = lang(code)?;
    texts.iter().map(|t| compress_string(t, info)).collect()
}

fn compress_string(s: &str, info: &Lang) -> Result<Vec<u8>> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let (what, num) = encode_greedy(&chars, i, info)?;
        out.extend_from_slice(&what);
        if num == 0 {
            return Err("encoder made no progress".into());
        }
        i += num;
    }
    Ok(out)
}

/// Does `a` start with `w`, comparing character by character?
fn starts_with(a: &[char], w: &str) -> bool {
    let mut it = a.iter();
    for wc in w.chars() {
        match it.next() {
            Some(&c) if c == wc => {}
            _ => return false,
        }
    }
    true
}

/// The last index at which `key` appears in the alphabet, or `None`.
///
/// `a2i = {e: i for i, e in enumerate(alphabet)}` — a later duplicate
/// overwrites an earlier one, so the *last* occurrence wins. `LangSV`'s
/// alphabet really does list `" "` twice.
fn a2i(info: &Lang, key: &str) -> Option<usize> {
    info.alphabet.iter().rposition(|e| *e == key)
}

/// `text_compression.encode_greedy_from_dict` (`:471-487`). Returns the bytes
/// emitted and the number of **characters** consumed.
///
/// The dictionary is scanned in declaration order and the first entry that is
/// a prefix wins. The Python buckets the dictionary by first character first,
/// but a prefix match implies an equal first character, so scanning the whole
/// list in order picks the identical entry.
///
/// The byte value is the *last* index at which that phrase occurs, because
/// `rev[first][phrase] = index` overwrites on a duplicate phrase while keeping
/// its original position in the iteration order. No shipped table has a
/// duplicate; the rule is reproduced anyway so that one would not surprise.
pub fn encode_greedy(chars: &[char], i: usize, info: &Lang) -> Result<(Vec<u8>, usize)> {
    let a = &chars[i..];
    if let Some(w) = info.dictionary.iter().find(|w| starts_with(a, w)) {
        let idx = info.dictionary.iter().rposition(|x| x == w).unwrap();
        return Ok((
            vec![idx as u8 + info.dict_base_enc],
            w.chars().count(),
        ));
    }

    if a[0] == '[' {
        let close = a
            .iter()
            .position(|&c| c == ']')
            .ok_or_else(|| format!("unterminated '[' in {:?}", a.iter().collect::<String>()))?;
        // `cmd = a[1:a.index(']')]`, `cmdlen + 2 == close + 1` characters.
        let token: String = a[..close + 1].iter().collect();
        // A bracketed alphabet entry such as "[1HeartL]" is a *character* and
        // is tried before the token is read as a command.
        if let Some(idx) = a2i(info, &token) {
            return Ok((vec![idx as u8], close + 1));
        }
        let cmd: String = a[1..close].iter().collect();
        let (name, param) = match cmd.split_once(' ') {
            Some((n, p)) => (
                n.to_string(),
                Some(p.trim().parse::<i64>().map_err(|_| {
                    format!("command parameter {p:?} in {token:?} is not an integer")
                })?),
            ),
            None => (cmd, None),
        };
        let bytes = if info.new_encoder {
            new_encoder(&name, param)?
        } else {
            org_encoder(info, &name, param)?
        };
        return Ok((bytes, close + 1));
    }

    let key = a[0].to_string();
    let idx = a2i(info, &key)
        .ok_or_else(|| format!("character {key:?} is not in the {} alphabet", info.code))?;
    Ok((vec![idx as u8], 1))
}

/// `text_compression.org_encoder` (`:146-154`). The command index is the
/// position in this language's `command_names`, and `command_lengths` must
/// agree about whether a parameter is present.
fn org_encoder(info: &Lang, cmd: &str, param: Option<i64>) -> Result<Vec<u8>> {
    let idx = info
        .command_names
        .iter()
        .position(|n| *n == cmd)
        .ok_or_else(|| format!("Invalid cmd {cmd}"))?;
    let want = if param.is_none() { 1 } else { 2 };
    if info.command_lengths[idx] != want {
        return Err(format!("Invalid cmd params {cmd} {param:?}"));
    }
    let b = idx as u8 + info.command_start;
    Ok(match param {
        None => vec![b],
        Some(p) => vec![b, p as u8],
    })
}

/// `text_compression.new_encoder` (`:180-192`) over `kCmdInfo` (`:157-178`).
///
/// Three commands legitimately encode to **zero** bytes (`Window 0`,
/// `Sound 64`, `ScrollSpd 0`), which is why this returns a `Vec` and not a
/// single byte.
fn new_encoder(cmd: &str, param: Option<i64>) -> Result<Vec<u8>> {
    // The parameterless forms: `len(info) <= 1 or isinstance(info[1], int)`
    // returns the whole tuple and rejects any parameter.
    let fixed: Option<&[u8]> = match cmd {
        "Scroll" => Some(&[0x80]),
        "Waitkey" => Some(&[0x81]),
        "1" => Some(&[0x82]),
        "2" => Some(&[0x83]),
        "3" => Some(&[0x84]),
        "Name" => Some(&[0x85]),
        "Choose" => Some(&[0x87, 0x80]),
        "Choose2" => Some(&[0x87, 0x81]),
        "Choose3" => Some(&[0x87, 0x82]),
        "Selchg" => Some(&[0x87, 0x83]),
        "Item" => Some(&[0x87, 0x84]),
        "NextPic" => Some(&[0x87, 0x85]),
        _ => None,
    };
    if let Some(f) = fixed {
        if param.is_some() {
            return Err(format!("Invalid cmd params {cmd} {param:?}"));
        }
        return Ok(f.to_vec());
    }

    let bad = || format!("Invalid cmd params {cmd} {param:?}");
    let p = param.ok_or_else(bad)?;
    // `None` in the mapping means "encodes to nothing".
    let mapped: Option<u8> = match cmd {
        "Wait" if (0..16).contains(&p) => Some(p as u8),
        "Color" if (0..16).contains(&p) => Some(p as u8 + 0x10),
        "Number" if (0..16).contains(&p) => Some(p as u8 + 0x20),
        "Speed" if (0..16).contains(&p) => Some(p as u8 + 0x30),
        "Sound" if p == 45 => Some(0x40),
        "Sound" if p == 64 => return Ok(Vec::new()),
        "Window" if p == 0 => return Ok(Vec::new()),
        "Window" if p == 2 => Some(0x86),
        "Position" if p == 0 => Some(0x87),
        "Position" if p == 1 => Some(0x88),
        "ScrollSpd" if p == 0 => return Ok(Vec::new()),
        "Wait" | "Color" | "Number" | "Speed" | "Sound" | "Window" | "Position" | "ScrollSpd" => {
            return Err(bad())
        }
        _ => return Err(format!("Invalid cmd {cmd}")),
    };
    Ok(vec![0x87, mapped.unwrap()])
}

/// `text_compression.encode_dictionary`: every dictionary phrase re-expressed
/// as alphabet indices. Single characters only — a multi-character alphabet
/// entry can never match here.
pub fn encode_dictionary(code: &str) -> Result<Vec<Vec<u8>>> {
    let info = lang(code)?;
    let mut out = Vec::with_capacity(info.dictionary.len());
    for line in info.dictionary {
        let mut e = Vec::new();
        for c in line.chars() {
            let key = c.to_string();
            let idx = a2i(info, &key).ok_or_else(|| {
                format!("dictionary character {key:?} is not in the {code} alphabet")
            })?;
            e.push(idx as u8);
        }
        out.push(e);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Font
// ---------------------------------------------------------------------------

/// `sprite_sheets.encode_font_from_png`, short-circuited through the ROM.
///
/// `decode_font:199` asserts `(data, W) == encode_font_from_png(lang)` for
/// every language except `pt`, so for those the PNG is provably an identity
/// transport and the two ROM reads are the whole thing: `tiles * 16` bytes of
/// 2bpp glyph data and `chars` width bytes.
///
/// `pt` is the exception and is excluded from that assert: it permutes the
/// tiles through `get_pt_remapper` (`sprite_sheets.py:139-146`) and takes its
/// widths from a third byte of the same table. That path is reproduced here
/// but is not covered by any oracle.
pub fn encode_font(rom: &Rom, code: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let ft = font_info(code)?;
    let data = rom.get_bytes(ft.gfx, ft.tiles * 16)?;
    if code != "pt" {
        let widths = rom.get_bytes(ft.widths_addr, ft.chars)?;
        return Ok((data, widths));
    }

    let b = rom.get_bytes(0x8efc09, 121 * 3)?;
    let mut remap = [0usize; 256];
    for (i, r) in remap.iter_mut().enumerate() {
        *r = i;
    }
    for i in 0..121usize {
        let ch = (i & 0xf) | ((i << 1) & 0xe0);
        remap[ch] = b[i * 3] as usize;
        remap[ch | 0x10] = b[i * 3 + 1] as usize;
    }
    let mut out = vec![0u8; ft.tiles * 16];
    for i in 0..ft.tiles {
        let src = remap[i] * 16;
        out[i * 16..i * 16 + 16].copy_from_slice(&data[src..src + 16]);
    }
    let widths: Vec<u8> = (0..121).map(|i| b[i * 3 + 2]).collect();
    Ok((out, widths[..ft.chars].to_vec()))
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// The canonical position of a language code: its index in `kLanguages`.
fn canonical_rank(code: &str) -> Option<usize> {
    LANGS.iter().position(|l| l.code == code)
}

/// The language list a build covers: `us` first, then the supplied ROMs sorted
/// into **canonical** order (the declaration order of `kLanguages`) rather than
/// the order the host handed them over.
///
/// The Python packs languages in the order they appear on `--languages`, and
/// the order is load-bearing: `de,fr` and `fr,de` produce different files. A
/// host has no natural order to offer, so this fixes one — the same set of
/// ROMs always produces identical bytes. A parity run passes the Python this
/// same order.
pub fn language_order(langs: &[Rom]) -> Result<Vec<(&'static str, usize)>> {
    let mut extra: Vec<(&'static str, usize)> = Vec::new();
    for (i, r) in langs.iter().enumerate() {
        let code = r.language.ok_or_else(|| {
            format!("the ROM with SHA-1 {} is not a language release this converter knows", r.sha1)
        })?;
        if code == "us" {
            return Err("a US ROM was supplied as a translation".into());
        }
        canonical_rank(code).ok_or_else(|| format!("no text tables for language {code:?}"))?;
        if extra.iter().any(|(c, _)| *c == code) {
            // `compile_resources.py:127`: `if a in languages ... raise`.
            return Err(format!("language {code} was supplied twice"));
        }
        extra.push((code, i));
    }
    extra.sort_by_key(|(c, _)| canonical_rank(c).unwrap());
    Ok(extra)
}

/// `compile_resources.print_dialogue` (`:121-146`) for one language: the
/// `kDialogue`, `kDialogueFont` and `kDialogueMap` entries.
pub fn language_entries(
    rom: &Rom,
    code: &str,
    index: usize,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    language_entries_from(code, index, &decode_strings(rom, code)?, &encode_font(rom, code)?)
}

/// [`language_entries`] over already-read strings and font, so the reading
/// stages and the compressing stage can be separate phases without decoding
/// the same ROM twice.
pub fn language_entries_from(
    code: &str,
    index: usize,
    texts: &[String],
    font: &(Vec<u8>, Vec<u8>),
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let dict_packed = pack_arrays(&encode_dictionary(code)?);
    let dialogue_packed = pack_arrays(&compress_strings(texts, code)?);
    let dialogue = pack_arrays(&[dict_packed, dialogue_packed]);

    let font = pack_arrays(&[font.0.clone(), font.1.clone()]);

    let mut flags = u8::from(uses_new_format(code)?);
    if index != 0 {
        flags |= 2;
    }
    let idx = u8::try_from(index).map_err(|_| "too many languages".to_string())?;
    let map = pack_arrays(&[code.as_bytes().to_vec(), vec![idx, idx, flags]]);

    Ok((dialogue, font, map))
}

/// Adds assets 94-96. `langs` may be empty, which is the US-only build.
///
/// Fills the three keys if the store already registered them as placeholders
/// (the partially-ported build keeps all 165 keys in order from the start);
/// otherwise appends them, which is what a standalone test store wants.
pub fn add_all(rom: &Rom, langs: &[Rom], a: &mut Assets) -> Result<()> {
    let codes = language_codes(rom, langs)?;
    let strings = read_all_strings(rom, langs)?;
    let fonts = read_all_fonts(rom, langs)?;
    add_all_from(&codes, &strings, &fonts, a)
}

/// The build's language list: `us` from the base ROM, then the supplied
/// translation ROMs in canonical order, each paired with the ROM to read.
///
/// One place decides which ROM a language comes from, so the strings stage,
/// the font stage and the assembling stage cannot disagree.
fn sources<'a>(rom: &'a Rom, langs: &'a [Rom]) -> Result<Vec<(&'static str, &'a Rom)>> {
    let mut out: Vec<(&'static str, &Rom)> = vec![("us", rom)];
    for (code, src) in language_order(langs)? {
        out.push((code, &langs[src]));
    }
    Ok(out)
}

/// The language codes this build covers, `us` first: exactly the entries
/// [`read_all_strings`] and [`read_all_fonts`] return, in the same order.
pub fn language_codes(rom: &Rom, langs: &[Rom]) -> Result<Vec<&'static str>> {
    Ok(sources(rom, langs)?.into_iter().map(|(c, _)| c).collect())
}

/// Stage "Reading dialogue": the 397 strings of every language in the build,
/// `us` first, then the translations in canonical order.
pub fn read_all_strings(rom: &Rom, langs: &[Rom]) -> Result<Vec<Vec<String>>> {
    sources(rom, langs)?.iter().map(|(code, r)| decode_strings(r, code)).collect()
}

/// Stage "Reading the font": the glyph data and width table of every language
/// in the build, in the same order as [`read_all_strings`].
pub fn read_all_fonts(rom: &Rom, langs: &[Rom]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    sources(rom, langs)?.iter().map(|(code, r)| encode_font(r, code)).collect()
}

/// Stage "Compressing dialogue": assets 94-96 from the already-read strings
/// and fonts, in the order [`language_codes`] fixed.
pub fn add_all_from(
    codes: &[&str],
    strings: &[Vec<String>],
    fonts: &[(Vec<u8>, Vec<u8>)],
    a: &mut Assets,
) -> Result<()> {
    if strings.len() != fonts.len() || strings.len() != codes.len() {
        return Err("dialogue strings, fonts and codes cover different languages".into());
    }
    let mut all_langs = Vec::new();
    let mut all_fonts = Vec::new();
    let mut mappings = Vec::new();

    for (i, (texts, font)) in strings.iter().zip(fonts.iter()).enumerate() {
        let (d, f, m) = language_entries_from(codes[i], i, texts, font)?;
        all_langs.push(d);
        all_fonts.push(f);
        mappings.push(m);
    }

    put(a, "kDialogue", &all_langs)?;
    put(a, "kDialogueFont", &all_fonts)?;
    put(a, "kDialogueMap", &mappings)?;
    Ok(())
}

fn put(a: &mut Assets, name: &str, entries: &[Vec<u8>]) -> Result<()> {
    if a.get(name).is_some() {
        a.fill(name, Kind::Packed, pack_arrays(entries))
    } else {
        a.add_packed(name, entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_scan_is_first_match_not_longest() {
        // 'and ' comes before 'and' and before 'an' in kTextDictionary_US, and
        // 'an' comes before 'at'. If this ever became a longest-match or a
        // key-sorted map the encoder would emit different bytes.
        let us = lang("us").unwrap();
        let pos = |w: &str| us.dictionary.iter().position(|x| *x == w).unwrap();
        assert!(pos("and ") < pos("and"));
        assert!(pos("and") < pos("an"));
        assert!(pos("ain") < pos("an"));
        let chars: Vec<char> = "and the".chars().collect();
        let (b, n) = encode_greedy(&chars, 0, us).unwrap();
        assert_eq!(n, 4); // 'and ', the first entry that is a prefix
        assert_eq!(b, vec![pos("and ") as u8 + 0x88]);
    }

    #[test]
    fn bracketed_alphabet_entries_beat_commands() {
        // "[1HeartL]" is a character in the US alphabet, not a command.
        let us = lang("us").unwrap();
        let chars: Vec<char> = "[1HeartL]".chars().collect();
        let (b, n) = encode_greedy(&chars, 0, us).unwrap();
        assert_eq!(n, 9);
        assert_eq!(b, vec![82]);
        // "[Speed 00]" is a command with a parameter.
        let chars: Vec<char> = "[Speed 00]".chars().collect();
        let (b, n) = encode_greedy(&chars, 0, us).unwrap();
        assert_eq!(n, 10);
        assert_eq!(b, vec![19 + 0x67, 0]);
    }

    #[test]
    fn new_encoder_can_emit_nothing() {
        assert!(new_encoder("Window", Some(0)).unwrap().is_empty());
        assert!(new_encoder("Sound", Some(64)).unwrap().is_empty());
        assert!(new_encoder("ScrollSpd", Some(0)).unwrap().is_empty());
        assert_eq!(new_encoder("Window", Some(2)).unwrap(), vec![0x87, 0x86]);
        assert!(new_encoder("Scroll", Some(1)).is_err());
        assert!(new_encoder("Nope", None).is_err());
    }

    #[test]
    fn canonical_order_is_the_klanguages_order() {
        assert_eq!(canonical_rank("us"), Some(0));
        assert_eq!(canonical_rank("de"), Some(1));
        assert_eq!(canonical_rank("fr"), Some(2));
        assert!(canonical_rank("de").unwrap() < canonical_rank("fr").unwrap());
    }

    #[test]
    fn every_language_has_a_font() {
        for l in LANGS {
            assert!(font_info(l.code).is_ok(), "{}", l.code);
        }
    }

    #[test]
    fn dictionary_encodes_through_the_alphabet() {
        // Round trip: every dictionary phrase must be expressible.
        for l in LANGS {
            encode_dictionary(l.code).unwrap();
        }
    }
}

/// Parity against the Python oracle. Skipped unless the ROM and oracle paths
/// are in the environment, so `cargo test` stays green without them:
///
/// ```sh
/// ZELDA3_ROM=... ZELDA3_ROM_FR=... ZELDA3_ROM_DE=... ZELDA3_ORACLE_DIR=... \
///   cargo test dialogue -- --ignored --nocapture
/// ```
///
/// `ZELDA3_ORACLE_DIR` holds `oracle_us.dat`, `oracle_fr.dat`, `oracle_de.dat`
/// and `oracle_defr.dat`, produced by `restool.py` per ORACLES.md. Note the
/// two-language oracle is the `--languages de,fr` one: this port sorts
/// languages into the canonical `kLanguages` order, so `de` precedes `fr`
/// whatever order the host supplied.
///
/// The build under test contains only assets 94-96; `compare.mjs` reports the
/// other 162 keys as MISSING, which is expected.
#[cfg(test)]
mod oracle_tests {
    use super::*;
    use crate::assets::ASSET_TABLE;

    fn rom(var: &str) -> Option<Rom> {
        Some(Rom::new(std::fs::read(std::env::var(var).ok()?).ok()?))
    }

    fn build(base: &Rom, langs: &[Rom]) -> Vec<u8> {
        let mut a = Assets::new();
        for (name, kind, _) in ASSET_TABLE {
            a.add_placeholder(name, *kind).unwrap();
        }
        add_all(base, langs, &mut a).unwrap();
        a.serialise()
    }

    fn compare(oracle: &str, data: &[u8], label: &str) {
        let dir = std::env::var("ZELDA3_ORACLE_DIR").unwrap();
        let out = std::env::temp_dir().join(format!("zelda3_dialogue_{label}.dat"));
        std::fs::write(&out, data).unwrap();
        let o = std::process::Command::new("node")
            .arg("compare.mjs")
            .arg(format!("{dir}/{oracle}"))
            .arg(&out)
            .output()
            .expect("run node compare.mjs from the crate root");
        let text = String::from_utf8_lossy(&o.stdout).to_string()
            + &String::from_utf8_lossy(&o.stderr);
        println!("--- {label} ({oracle})\n{text}");
        for key in ["kDialogue", "kDialogueFont", "kDialogueMap"] {
            assert!(
                !text.contains(&format!("{key} ")) || text.contains(&format!("{key}: ok")),
                "{label}: {key} did not match"
            );
            assert!(!text.contains(&format!("MISSING {key}")), "{label}: {key} missing");
        }
        // Belt and braces: the three keys must be byte-identical to the oracle.
        let ours = data;
        let theirs = std::fs::read(format!("{dir}/{oracle}")).unwrap();
        for key in ["kDialogue", "kDialogueFont", "kDialogueMap"] {
            assert_eq!(entry(&theirs, key), entry(ours, key), "{label}: {key}");
        }
    }

    /// Minimal independent .dat reader, so the assertion above does not lean on
    /// the writer it is checking.
    fn entry(buf: &[u8], key: &str) -> Vec<u8> {
        let rd = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap()) as usize;
        let count = rd(80);
        let key_len = rd(84);
        let sizes: Vec<usize> = (0..count).map(|i| rd(88 + 4 * i)).collect();
        let keys_at = 88 + 4 * count;
        let blob = &buf[keys_at..keys_at + key_len];
        let names: Vec<&str> = blob
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| core::str::from_utf8(s).unwrap())
            .collect();
        let mut pos = keys_at + key_len;
        for (i, n) in names.iter().enumerate() {
            while pos & 3 != 0 {
                pos += 1;
            }
            if *n == key {
                return buf[pos..pos + sizes[i]].to_vec();
            }
            pos += sizes[i];
        }
        panic!("no key {key}");
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM* and ZELDA3_ORACLE_DIR"]
    fn matches_the_python_oracles() {
        let Some(us) = rom("ZELDA3_ROM") else { return };
        assert_eq!(us.language, Some("us"));
        compare("oracle_us.dat", &build(&us, &[]), "us");

        let fr = rom("ZELDA3_ROM_FR");
        let de = rom("ZELDA3_ROM_DE");
        if let Some(fr) = &fr {
            assert_eq!(fr.language, Some("fr"));
            compare("oracle_fr.dat", &build(&us, core::slice::from_ref(fr)), "fr");
        }
        if let Some(de) = &de {
            assert_eq!(de.language, Some("de"));
            compare("oracle_de.dat", &build(&us, core::slice::from_ref(de)), "de");
        }
        if let (Some(fr), Some(de)) = (fr, de) {
            // Supplied fr-first on purpose: the canonical sort must put de
            // first regardless, matching the Python's `--languages de,fr`.
            let data = build(&us, &[fr, de]);
            compare("oracle_defr.dat", &data, "defr");
        }
    }
}
