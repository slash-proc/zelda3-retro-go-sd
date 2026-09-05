//! Direct port of the ROM-access half of `tables/util.py`.
//!
//! # The three address arithmetics
//!
//! The Python walks ROM addresses in three mutually incompatible ways, and
//! mixing them silently corrupts output. They are given three distinct names
//! here and nothing else in the crate is allowed to open-code the arithmetic:
//!
//! 1. [`snes_to_offset`] — `LoadedRom.get_byte` (`util.py:97-100`). Folds a
//!    24-bit LoROM address to a flat file offset:
//!    `((ea >> 16) & 0x7f) * 0x8000 + (ea & 0x7fff)`, after asserting bit 15 is
//!    set. This is a *pure mapping*; it never advances anything.
//!
//! 2. [`advance_mapped`] — the wrap rule inside `LoadedRom.get_bytes` /
//!    `get_words` (`util.py:105,113`). After stepping the address it tests
//!    `(addr & 0x8000) == 0` and if so adds `0x8000`, i.e. it skips the
//!    unmapped low half of the *next* bank. Note `get_words` steps by 2 and
//!    applies the test only once per word, so a word can straddle the boundary
//!    exactly as the Python lets it.
//!
//! 3. [`advance_reader`] — `Reader.next` (`util.py:169-174`). Steps by one and
//!    tests `(ea & 0xffff) == 0`, i.e. it corrects only when the low 16 bits
//!    reach zero. Different predicate, different field, and not
//!    interchangeable: for an address that starts in the unmapped low half,
//!    `advance_mapped` pulls it up by `0x8000` on every step while
//!    `advance_reader` leaves it there. They agree only at a bank boundary.
//!
//! And the length a decompressor reports is a fourth rule again:
//! [`decomp_length`], `(end - start) & 0x7fff` (`util.py:181`), which is *not*
//! the number of bytes consumed once a bank was crossed. It is reproduced
//! because the compressed-length values it yields are what get stored.

use crate::hash::{hex_upper, sha1};

pub type Result<T> = core::result::Result<T, String>;

/// SHA-1 of the US cartridge ROM (`util.ZELDA3_SHA1_US`).
pub const ZELDA3_SHA1_US: &str = "6D4F10A8B10E10DBE624CB23CF03B88BB8252973";

/// `util.ZELDA3_SHA1`: every ROM the Python recognises, as
/// (sha1, language code, description). Two releases share the `redux` code;
/// that is how the Python has it, and the code, not the hash, is what the
/// converter uses downstream.
pub const KNOWN_ROMS: &[(&str, &str, &str)] = &[
    (ZELDA3_SHA1_US, "us", "Legend of Zelda, The - A Link to the Past (USA)"),
    ("2E62494967FB0AFDF5DA1635607F9641DF7C6559", "de", "Legend of Zelda, The - A Link to the Past (Germany)"),
    ("229364A1B92A05167CD38609B1AA98F7041987CC", "fr", "Legend of Zelda, The - A Link to the Past (France)"),
    ("C1C6C7F76FFF936C534FF11F87A54162FC0AA100", "fr-c", "Legend of Zelda, The - A Link to the Past (Canada)"),
    ("7C073A222569B9B8E8CA5FCB5DFEC3B5E31DA895", "en", "Legend of Zelda, The - A Link to the Past (Europe)"),
    ("461FCBD700D1332009C0E85A7A136E2A8E4B111E", "es", "Spanish translation"),
    ("3C4D605EEFDA1D76F101965138F238476655B11D", "pl", "Polish translation"),
    ("D0D09ED41F9C373FE6AFDCCAFBF0DA8C88D3D90D", "pt", "Portuguese translation"),
    ("B2A07A59E64C498BC1B2F28728F9BF4014C8D582", "redux", "English Redux"),
    ("9325C22EB0A2A1F0017157C8B620BC3A605CEDE1", "redux", "English Redux"),
    ("FA8ADFDBA2697C9A54D583A1284A22AC764C7637", "nl", "Dutch translation"),
    ("43CD3438469B2C3FE879EA2F410B3EF3CB3F1CA4", "sv", "Swedish translation"),
];

// ---------------------------------------------------------------------------
// The three address arithmetics, as free functions so they cannot blur.
// ---------------------------------------------------------------------------

/// Arithmetic 1: `LoadedRom.get_byte`'s LoROM fold. Errors instead of
/// asserting, but the condition is the Python's `assert (ea & 0x8000)`.
#[inline]
pub fn snes_to_offset(ea: u32) -> Result<usize> {
    if ea & 0x8000 == 0 {
        return Err(format!("bad effective address {ea:#x}: bit 15 clear (not LoROM)"));
    }
    Ok((((ea >> 16) & 0x7f) * 0x8000 + (ea & 0x7fff)) as usize)
}

/// Arithmetic 2: the wrap rule of `LoadedRom.get_bytes`/`get_words`. `step` is
/// 1 for bytes and 2 for words; the test is applied once, after the step.
#[inline]
pub fn advance_mapped(addr: u32, step: u32) -> u32 {
    let a = addr + step;
    if a & 0x8000 == 0 {
        a + 0x8000
    } else {
        a
    }
}

/// Arithmetic 3: `Reader.next`'s wrap rule. Different predicate, different
/// field: it fires only when the low 16 bits reach zero.
#[inline]
pub fn advance_reader(ea: u32) -> u32 {
    let a = ea + 1;
    if a & 0xffff == 0 {
        a + 0x8000
    } else {
        a
    }
}

/// Arithmetic 4: the length `util.decomp` reports, `(end - start) & 0x7fff`.
#[inline]
pub fn decomp_length(start: u32, end: u32) -> u32 {
    end.wrapping_sub(start) & 0x7fff
}

// ---------------------------------------------------------------------------

/// A loaded, header-stripped ROM plus the identity the SHA-1 table gives it.
pub struct Rom {
    pub data: Vec<u8>,
    pub sha1: String,
    /// Language code from [`KNOWN_ROMS`], or `None` for an unrecognised ROM.
    pub language: Option<&'static str>,
}

impl Rom {
    /// Mirrors `LoadedRom.__init__`: strip a 512-byte SMC copier header if the
    /// length says one is present, identify by SHA-1, then apply the Swedish
    /// broken-size workaround. Identification never gates loading here — the
    /// caller decides whether an unknown ROM is acceptable, because the wasm
    /// ABI has a bypass flag the Python does not.
    pub fn new(mut data: Vec<u8>) -> Rom {
        if (data.len() & 0xfffff) == 0x200 {
            data.drain(..0x200);
        }
        let sha1 = hex_upper(&sha1(&data));
        let language = KNOWN_ROMS
            .iter()
            .find(|(h, _, _)| *h == sha1)
            .map(|(_, lang, _)| *lang);
        // `util.py:92-93`: the Swedish release ships 0x200 bytes long and the
        // length test above does not catch it.
        if language == Some("sv") && data.len() == 0x10083b {
            data.drain(..0x200);
        }
        Rom { data, sha1, language }
    }

    /// Human-readable name from the SHA-1 table, for messages only.
    pub fn description(&self) -> Option<&'static str> {
        KNOWN_ROMS
            .iter()
            .find(|(h, _, _)| *h == self.sha1)
            .map(|(_, _, d)| *d)
    }

    #[inline]
    pub fn get_byte(&self, ea: u32) -> Result<u8> {
        let off = snes_to_offset(ea)?;
        self.data
            .get(off)
            .copied()
            .ok_or_else(|| format!("read past end of ROM at {ea:#x} (offset {off:#x})"))
    }

    #[inline]
    pub fn get_word(&self, ea: u32) -> Result<u32> {
        Ok(self.get_byte(ea)? as u32 + self.get_byte(ea + 1)? as u32 * 256)
    }

    #[inline]
    pub fn get_24(&self, ea: u32) -> Result<u32> {
        Ok(self.get_byte(ea)? as u32
            + self.get_byte(ea + 1)? as u32 * 256
            + self.get_byte(ea + 2)? as u32 * 65536)
    }

    /// `util.get_int8`: the byte reinterpreted as two's complement.
    #[inline]
    pub fn get_int8(&self, ea: u32) -> Result<i32> {
        let b = self.get_byte(ea)? as i32;
        Ok(if b & 0x80 != 0 { b - 256 } else { b })
    }

    /// `util.get_int16`.
    #[inline]
    pub fn get_int16(&self, ea: u32) -> Result<i32> {
        let b = self.get_word(ea)? as i32;
        Ok(if b & 0x8000 != 0 { b - 65536 } else { b })
    }

    /// `LoadedRom.get_bytes`. Uses [`advance_mapped`] with a step of 1.
    pub fn get_bytes(&self, addr: u32, n: usize) -> Result<Vec<u8>> {
        let mut addr = addr;
        let mut r = Vec::with_capacity(n);
        for _ in 0..n {
            r.push(self.get_byte(addr)?);
            addr = advance_mapped(addr, 1);
        }
        Ok(r)
    }

    /// `LoadedRom.get_words`. Uses [`advance_mapped`] with a step of 2 — note
    /// this is *not* two applications of the step-1 rule.
    pub fn get_words(&self, addr: u32, n: usize) -> Result<Vec<u16>> {
        let mut addr = addr;
        let mut r = Vec::with_capacity(n);
        for _ in 0..n {
            r.push(self.get_word(addr)? as u16);
            addr = advance_mapped(addr, 2);
        }
        Ok(r)
    }

    /// A byte-at-a-time cursor with [`advance_reader`] semantics, as used by
    /// the decompressor. Kept separate from `get_bytes` on purpose.
    pub fn reader(&self, ea: u32) -> Reader<'_> {
        Reader { rom: self, ea }
    }
}

/// `util.Reader`.
pub struct Reader<'a> {
    rom: &'a Rom,
    pub ea: u32,
}

impl Reader<'_> {
    #[inline]
    pub fn next(&mut self) -> Result<u8> {
        let r = self.rom.get_byte(self.ea)?;
        self.ea = advance_reader(self.ea);
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_arithmetics_disagree() {
        // At a bank boundary the two agree: both land in the mapped half of
        // the next bank.
        assert_eq!(advance_mapped(0x82ffff, 1), 0x838000);
        assert_eq!(advance_reader(0x82ffff), 0x838000);
        // Anywhere in the unmapped low half they do not. `advance_mapped`
        // pulls every such address up by 0x8000; `advance_reader` leaves it
        // alone, because its test is on the low 16 bits, not on bit 15.
        assert_eq!(advance_mapped(0x820100, 1), 0x828101);
        assert_eq!(advance_reader(0x820100), 0x820101);
        // And the word walker is not two byte steps: from 0x82fffe it steps
        // straight past the boundary in one go.
        assert_eq!(advance_mapped(0x82fffe, 2), 0x838000);
        assert_eq!(advance_mapped(advance_mapped(0x82fffe, 1), 1), 0x838000);
        assert_eq!(advance_mapped(0x82fffd, 2), 0x82ffff);
    }

    #[test]
    fn lorom_fold() {
        assert_eq!(snes_to_offset(0x808000).unwrap(), 0);
        assert_eq!(snes_to_offset(0x818000).unwrap(), 0x8000);
        assert_eq!(snes_to_offset(0x8effff).unwrap(), 0xe * 0x8000 + 0x7fff);
        assert!(snes_to_offset(0x800000).is_err());
    }

    #[test]
    fn decomp_length_masks_to_15_bits() {
        assert_eq!(decomp_length(0x828000, 0x828010), 0x10);
        // Crossing a bank makes the reported length wrap; the Python does the
        // same and downstream depends on the wrapped value.
        assert_eq!(decomp_length(0x82fff0, 0x838000), 0x10);
    }
}
