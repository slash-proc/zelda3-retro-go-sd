//! Port of `util.decomp` (`tables/util.py:176-227`), the only decompressor the
//! reachable pipeline needs.
//!
//! There is deliberately no compressor: nothing in the extract-to-compile path
//! calls one (`util.encode_brr_generic` and `compile_resources.compress_store`
//! have no callers, and `decode_music.py` is dead), and the compressed streams
//! that reach the .dat are copied verbatim out of the ROM.
//!
//! Two things must be reproduced literally or the output diverges:
//!
//! * The `copy` command reads from the *output buffer being built*, and its
//!   source range may overlap bytes it is writing in the same command. It has
//!   to stay a byte-at-a-time loop; a slice copy gives different bytes.
//! * `offset_is_be` selects between two byte orders for the copy offset. The
//!   overworld and enemy-damage streams are big-endian, the graphics tilesets
//!   little-endian (`compile_resources.py:107,116,195`).

use crate::rom::{decomp_length, Reader, Result, Rom};

/// Byte order of the `copy` command's source offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OffsetOrder {
    /// `offset_is_be = True` — overworld, enemy damage.
    Big,
    /// `offset_is_be = False` — the sprite and background tilesets.
    Little,
}

/// The result of a decompression: the bytes, plus the length the Python's
/// `return_length=True` path reports. The length is what callers store for the
/// *compressed* assets (`kSprGfx`, `kBgGfx`, the overworld byte streams), where
/// the decompressed bytes are thrown away and only the measurement is kept.
pub struct Decompressed {
    pub data: Vec<u8>,
    /// `(reader.ea - ea) & 0x7fff`, see [`decomp_length`].
    pub compressed_len: u32,
}

/// `util.decomp(ea, rb, offset_is_be)`.
pub fn decomp(rom: &Rom, ea: u32, order: OffsetOrder) -> Result<Decompressed> {
    let mut reader: Reader = rom.reader(ea);
    let mut result: Vec<u8> = Vec::new();

    loop {
        let b = reader.next()? as u32;
        if b == 0xff {
            return Ok(Decompressed {
                data: result,
                compressed_len: decomp_length(ea, reader.ea),
            });
        }

        // The long-form escape: a top nibble of 0b111 re-reads the command from
        // bits 4..2 and takes a 10-bit length. `(b << 3) & 0xe0` is the
        // Python's expression, kept as-is.
        let (cmd, mut lx) = if (b & 0xe0) != 0xe0 {
            (b & 0xe0, b & 0x1f)
        } else {
            let lo = reader.next()? as u32;
            ((b << 3) & 0xe0, ((b & 3) << 8) | lo)
        };
        lx += 1;

        if cmd == 0x00 {
            // 000 - literal run
            for _ in 0..lx {
                let v = reader.next()?;
                result.push(v);
            }
        } else if cmd & 0x80 != 0 {
            // 1xx - copy from the output produced so far. Byte at a time: the
            // source may trail the destination by less than `lx`, so later
            // iterations legitimately read bytes this same command wrote.
            let hi = reader.next()? as u32;
            let lo = reader.next()? as u32;
            let mut offs = (hi << 8) | lo;
            if order == OffsetOrder::Little {
                offs = ((offs >> 8) | (offs << 8)) & 0xffff;
            }
            for _ in 0..lx {
                let v = *result.get(offs as usize).ok_or_else(|| {
                    format!("decomp at {ea:#x}: copy source {offs:#x} is past the output")
                })?;
                result.push(v);
                offs += 1;
            }
        } else if cmd & 0x40 == 0 {
            // 00x - memset
            let v = reader.next()?;
            for _ in 0..lx {
                result.push(v);
            }
        } else if cmd & 0x20 == 0 {
            // 010 - 16-bit memset. The Python's loop breaks *before* writing
            // the second byte when one byte is left, so an odd length ends on
            // b1; and it decrements by two, so the trailing `lx == 1` test is
            // the only odd-length exit.
            let b1 = reader.next()?;
            let b2 = reader.next()?;
            let mut n = lx;
            while n > 0 {
                result.push(b1);
                if n == 1 {
                    break;
                }
                result.push(b2);
                n -= 2;
            }
        } else {
            // 011 - incrementing run
            let mut v = reader.next()?;
            for _ in 0..lx {
                result.push(v);
                v = v.wrapping_add(1);
            }
        }
    }
}

/// The bytes of a compressed stream, exactly as stored in the ROM, together
/// with the decompressed output. `kSprGfx`/`kBgGfx` and the overworld byte
/// streams keep the *compressed* form, and the only way to know how long it is
/// is to decompress and measure.
pub fn compressed_bytes(rom: &Rom, ea: u32, order: OffsetOrder) -> Result<Vec<u8>> {
    let d = decomp(rom, ea, order)?;
    rom.get_bytes(ea, d.compressed_len as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::Rom;

    /// Builds a ROM whose bank 0x80 starts with `bytes` at 0x808000.
    fn rom_with(bytes: &[u8]) -> Rom {
        let mut data = vec![0u8; 0x8000];
        data[..bytes.len()].copy_from_slice(bytes);
        Rom { data, sha1: String::new(), language: None }
    }

    #[test]
    fn literal_then_end() {
        let r = rom_with(&[0x02, 1, 2, 3, 0xff]);
        let d = decomp(&r, 0x808000, OffsetOrder::Big).unwrap();
        assert_eq!(d.data, vec![1, 2, 3]);
        assert_eq!(d.compressed_len, 5);
    }

    #[test]
    fn memset_and_incr() {
        // 0x20|3 -> memset 4 of 0xaa ; 0x60|1 -> incr 2 from 0x10
        let r = rom_with(&[0x23, 0xaa, 0x61, 0x10, 0xff]);
        let d = decomp(&r, 0x808000, OffsetOrder::Big).unwrap();
        assert_eq!(d.data, vec![0xaa, 0xaa, 0xaa, 0xaa, 0x10, 0x11]);
    }

    #[test]
    fn memset16_odd_length_ends_on_the_first_byte() {
        // 0x40|2 -> length 3 of the pair (1,2): 1,2,1
        let r = rom_with(&[0x42, 1, 2, 0xff]);
        let d = decomp(&r, 0x808000, OffsetOrder::Big).unwrap();
        assert_eq!(d.data, vec![1, 2, 1]);
    }

    #[test]
    fn copy_overlaps_its_own_output() {
        // literal 0xaa, then copy 3 bytes from offset 0 -- the source catches
        // up with the destination, which a slice copy would get wrong.
        let r = rom_with(&[0x00, 0xaa, 0x82, 0x00, 0x00, 0xff]);
        let d = decomp(&r, 0x808000, OffsetOrder::Big).unwrap();
        assert_eq!(d.data, vec![0xaa, 0xaa, 0xaa, 0xaa]);
    }

    #[test]
    fn little_endian_offset_swaps_the_bytes() {
        let src = [0x03u8, 1, 2, 3, 4]; // literal 1,2,3,4
        let mut prog = src.to_vec();
        prog.extend_from_slice(&[0x81, 0x02, 0x00]); // copy 2 from offs 0x0002 LE-swapped
        prog.push(0xff);
        let r = rom_with(&prog);
        let d = decomp(&r, 0x808000, OffsetOrder::Little).unwrap();
        assert_eq!(d.data, vec![1, 2, 3, 4, 3, 4]);
    }

    #[test]
    fn long_form_length() {
        // 0xe0 -> cmd (0xe0<<3)&0xe0 = 0x00 (literal), lx = (0&3)<<8 | 4, +1 = 5
        let r = rom_with(&[0xe0, 0x04, 1, 2, 3, 4, 5, 0xff]);
        let d = decomp(&r, 0x808000, OffsetOrder::Big).unwrap();
        assert_eq!(d.data, vec![1, 2, 3, 4, 5]);
    }
}

/// Tests that need the real cartridge. They are skipped unless `ZELDA3_ROM`
/// points at a US ROM, so `cargo test` stays green in a checkout without one:
///
/// ```sh
/// ZELDA3_ROM="/path/to/zelda3.sfc" cargo test -- --ignored
/// ```
#[cfg(test)]
mod rom_tests {
    use super::*;
    use crate::rom::Rom;

    fn load() -> Option<Rom> {
        let path = std::env::var("ZELDA3_ROM").ok()?;
        Some(Rom::new(std::fs::read(path).ok()?))
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn enemy_damage_table_decompresses() {
        let Some(rom) = load() else { return };
        assert_eq!(rom.language, Some("us"));
        // kEnemyDamageData, asset 56: big-endian copy offsets, 1728 bytes out.
        let d = decomp(&rom, 0x83e800, OffsetOrder::Big).unwrap();
        assert_eq!(d.data.len(), 1728);
    }

    #[test]
    #[ignore = "needs ZELDA3_ROM"]
    fn background_tilesets_measure_the_way_the_python_does() {
        let Some(rom) = load() else { return };
        // Four kCompBgPtrs entries with their measured (compressed,
        // decompressed) lengths. These use *little-endian* copy offsets; with
        // the big-endian rule the streams decode to the wrong bytes and the
        // measured length changes, so this pins the flag as well as the codec.
        for &(ea, comp, raw) in &[
            (0x11b800u32, 1250usize, 1536usize),
            (0x11bce2, 1149, 1536),
            (0x13a619, 1298, 1536),
            (0x18b953, 1634, 2048),
        ] {
            let d = decomp(&rom, ea, OffsetOrder::Little).unwrap();
            assert_eq!(d.data.len(), raw, "decompressed length at {ea:#x}");
            assert_eq!(d.compressed_len as usize, comp, "compressed length at {ea:#x}");
            assert_eq!(compressed_bytes(&rom, ea, OffsetOrder::Little).unwrap().len(), comp);
        }
    }
}
