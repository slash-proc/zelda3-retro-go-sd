//! Port of the asset-container half of `tables/compile_resources.py`:
//! the `add_asset_*` family, `pack_arrays` and `write_assets_to_file`.
//!
//! The container is decided entirely by *insertion order*: it fixes the size
//! array and it fixes `key_sig`, whose SHA-256 goes into the magic. So the
//! store is a `Vec`, never a map, and adding a duplicate name is an error the
//! way the Python's `assert name not in assets` is.

use crate::hash::sha256;

pub type Result<T> = core::result::Result<T, String>;

/// The 16-byte container magic (`compile_resources.py:795`).
pub const MAGIC: &[u8; 16] = b"Zelda3_v0     \n\0";

/// The element-type tag. It never reaches the .dat — it only drives the
/// generated C header — but it is carried so that `--print-assets-header` can
/// be added later without changing the store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Uint8,
    Int8,
    Uint16,
    Int16,
    Packed,
}

impl Kind {
    pub fn c_name(self) -> &'static str {
        match self {
            Kind::Uint8 => "uint8",
            Kind::Int8 => "int8",
            Kind::Uint16 => "uint16",
            Kind::Int16 => "int16",
            Kind::Packed => "packed",
        }
    }
}

pub struct Asset {
    pub name: String,
    pub kind: Kind,
    pub data: Vec<u8>,
}

/// The ordered asset store. `assets` in the Python.
pub struct Assets {
    items: Vec<Asset>,
}

impl Default for Assets {
    fn default() -> Self {
        Self::new()
    }
}

impl Assets {
    pub fn new() -> Assets {
        Assets { items: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Asset> {
        self.items.iter()
    }

    pub fn get(&self, name: &str) -> Option<&Asset> {
        self.items.iter().find(|a| a.name == name)
    }

    /// The shared tail of every `add_asset_*`, including the duplicate check.
    fn add(&mut self, name: &str, kind: Kind, data: Vec<u8>) -> Result<()> {
        if self.items.iter().any(|a| a.name == name) {
            return Err(format!("asset {name} was added twice"));
        }
        self.items.push(Asset { name: name.to_string(), kind, data });
        Ok(())
    }

    /// `add_asset_uint8` — `array.array('B', data)`.
    pub fn add_uint8(&mut self, name: &str, data: &[u8]) -> Result<()> {
        self.add(name, Kind::Uint8, data.to_vec())
    }

    /// `add_asset_int8` — `array.array('b', data)`. Python raises on a value
    /// outside -128..127 rather than truncating, so this does too.
    pub fn add_int8(&mut self, name: &str, data: &[i32]) -> Result<()> {
        let mut out = Vec::with_capacity(data.len());
        for (i, &v) in data.iter().enumerate() {
            if !(-128..=127).contains(&v) {
                return Err(format!("{name}[{i}] = {v} does not fit in an int8"));
            }
            out.push(v as i8 as u8);
        }
        self.add(name, Kind::Int8, out)
    }

    /// `add_asset_uint16` — `array.array('H', data)`, little-endian.
    pub fn add_uint16(&mut self, name: &str, data: &[u16]) -> Result<()> {
        let mut out = Vec::with_capacity(data.len() * 2);
        for v in data {
            out.extend_from_slice(&v.to_le_bytes());
        }
        self.add(name, Kind::Uint16, out)
    }

    /// `add_asset_int16` — `array.array('h', data)`.
    pub fn add_int16(&mut self, name: &str, data: &[i32]) -> Result<()> {
        let mut out = Vec::with_capacity(data.len() * 2);
        for (i, &v) in data.iter().enumerate() {
            if !(-32768..=32767).contains(&v) {
                return Err(format!("{name}[{i}] = {v} does not fit in an int16"));
            }
            out.extend_from_slice(&(v as i16).to_le_bytes());
        }
        self.add(name, Kind::Int16, out)
    }

    /// `add_asset_packed` — the entries run through [`pack_arrays`].
    pub fn add_packed(&mut self, name: &str, entries: &[Vec<u8>]) -> Result<()> {
        let v = pack_arrays(entries);
        self.add(name, Kind::Packed, v)
    }

    /// Registers a name with no payload yet. Not a Python equivalent: it is how
    /// a partially-ported build keeps the key order, count and key-blob hash
    /// correct while individual assets are still missing. A finished build has
    /// none of these.
    pub fn add_placeholder(&mut self, name: &str, kind: Kind) -> Result<()> {
        self.add(name, kind, Vec::new())
    }

    /// Replaces a placeholder's bytes in place, keeping its position. Lets a
    /// slice fill in one asset without disturbing the order the header depends
    /// on.
    pub fn fill(&mut self, name: &str, kind: Kind, data: Vec<u8>) -> Result<()> {
        let a = self
            .items
            .iter_mut()
            .find(|a| a.name == name)
            .ok_or_else(|| format!("no asset named {name} to fill"))?;
        a.kind = kind;
        a.data = data;
        Ok(())
    }

    /// The NUL-terminated, NUL-joined key blob (`key_sig`). Every key name in
    /// insertion order, each followed by a zero byte, so the blob ends with a
    /// trailing NUL.
    pub fn key_sig(&self) -> Vec<u8> {
        key_sig_of(self.items.iter().map(|a| a.name.as_str()))
    }

    /// `write_assets_to_file`. Everything is little-endian.
    ///
    /// ```text
    ///   0   16     magic
    ///   16  32     sha256(key_sig)
    ///   48  32     zero
    ///   80  4      u32 asset count
    ///   84  4      u32 len(key_sig)
    ///   88  4*N    u32 size[i]
    ///   ..  ..     key_sig
    ///   ..  ..     payloads, each preceded by 0-3 NULs so it starts aligned
    /// ```
    ///
    /// The padding is driven by the length of the file *so far*, not by the
    /// payload's own size, and there is no padding after the last payload.
    pub fn serialise(&self) -> Vec<u8> {
        let key_sig = self.key_sig();
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&sha256(&key_sig));
        out.extend_from_slice(&[0u8; 32]);
        out.extend_from_slice(&(self.items.len() as u32).to_le_bytes());
        out.extend_from_slice(&(key_sig.len() as u32).to_le_bytes());
        for a in &self.items {
            out.extend_from_slice(&(a.data.len() as u32).to_le_bytes());
        }
        out.extend_from_slice(&key_sig);
        for a in &self.items {
            while out.len() & 3 != 0 {
                out.push(0);
            }
            out.extend_from_slice(&a.data);
        }
        out
    }
}

/// The key blob for an arbitrary name sequence, so the ordered list from the
/// porting map can be hashed without building a store.
pub fn key_sig_of<'a>(names: impl Iterator<Item = &'a str>) -> Vec<u8> {
    let mut v = Vec::new();
    for n in names {
        v.extend_from_slice(n.as_bytes());
        v.push(0);
    }
    v
}

/// `pack_arrays` (`compile_resources.py:89-99`).
///
/// The offset table holds the cumulative lengths of every entry *except the
/// last*, so entry 0's implicit offset of zero is not stored and there are
/// `count - 1` offsets. The width test is on `offs`, which after the loop is
/// the sum of all but the last entry — **not** the total payload size — and
/// that literal reading is what decides u16 versus u32 for the borderline
/// cases. The trailer is `count - 1`, with `8192` added to flag 32-bit offsets.
pub fn pack_arrays(arr: &[Vec<u8>]) -> Vec<u8> {
    if arr.is_empty() {
        return Vec::new();
    }
    let mut all_offs: Vec<usize> = Vec::with_capacity(arr.len().saturating_sub(1));
    let mut offs: usize = 0;
    for entry in &arr[..arr.len() - 1] {
        offs += entry.len();
        all_offs.push(offs);
    }

    let mut out = Vec::new();
    let wide = !(offs < 65536 && arr.len() <= 8192);
    for o in &all_offs {
        if wide {
            out.extend_from_slice(&(*o as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&(*o as u16).to_le_bytes());
        }
    }
    for entry in arr {
        out.extend_from_slice(entry);
    }
    let trailer = if wide { 8192 + arr.len() - 1 } else { arr.len() - 1 };
    out.extend_from_slice(&(trailer as u16).to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pack_is_empty() {
        assert!(pack_arrays(&[]).is_empty());
    }

    #[test]
    fn single_entry_has_no_offsets() {
        // count - 1 == 0 offsets, then the payload, then the trailer 0.
        assert_eq!(pack_arrays(&[vec![1, 2, 3]]), vec![1, 2, 3, 0, 0]);
    }

    #[test]
    fn narrow_layout() {
        let v = pack_arrays(&[vec![1, 2], vec![3], vec![4, 5, 6]]);
        // offsets 2, 3 as u16; payload; trailer 2
        assert_eq!(v, vec![2, 0, 3, 0, 1, 2, 3, 4, 5, 6, 2, 0]);
    }

    #[test]
    fn width_test_ignores_the_last_entry() {
        // All but the last sum to 4, well under 65536, so the layout stays
        // narrow even though the total payload is far larger. Reproducing the
        // Python's literal `offs` is the whole point.
        let arr = vec![vec![0u8; 4], vec![0u8; 70000]];
        let v = pack_arrays(&arr);
        assert_eq!(&v[..2], &[4, 0]); // u16 offset, not u32
        assert_eq!(&v[v.len() - 2..], &[1, 0]); // trailer 1, no 8192 flag
    }

    #[test]
    fn wide_layout_sets_the_trailer_flag() {
        let arr = vec![vec![0u8; 70000], vec![0u8; 1]];
        let v = pack_arrays(&arr);
        assert_eq!(&v[..4], &70000u32.to_le_bytes());
        assert_eq!(&v[v.len() - 2..], &(8192u16 + 1).to_le_bytes());
    }

    #[test]
    fn dialogue_map_shape() {
        // kDialogueMap for the US-only build is 11 bytes:
        // pack([pack([b"us", [0,0,0]])]).
        let inner = pack_arrays(&[b"us".to_vec(), vec![0, 0, 0]]);
        let outer = pack_arrays(&[inner]);
        assert_eq!(outer.len(), 11);
    }

    #[test]
    fn header_layout_and_padding() {
        let mut a = Assets::new();
        a.add_uint8("kA", &[1, 2, 3]).unwrap(); // 3 bytes -> next needs padding
        a.add_uint8("kB", &[9]).unwrap();
        let out = a.serialise();
        assert_eq!(&out[..16], MAGIC);
        assert_eq!(&out[48..80], &[0u8; 32]);
        assert_eq!(&out[80..84], &2u32.to_le_bytes());
        let key_sig = b"kA\0kB\0";
        assert_eq!(&out[84..88], &(key_sig.len() as u32).to_le_bytes());
        assert_eq!(&out[88..92], &3u32.to_le_bytes());
        assert_eq!(&out[92..96], &1u32.to_le_bytes());
        assert_eq!(&out[96..96 + key_sig.len()], key_sig);
        assert_eq!(&out[16..48], &sha256(key_sig));
        // key blob ends at 102, padded to 104, then 3 bytes, padded to 108,
        // then 1 byte -- and nothing after it.
        assert_eq!(out.len(), 109);
        assert_eq!(out[102], 0);
        assert_eq!(&out[104..107], &[1, 2, 3]);
        assert_eq!(out[108], 9);
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let mut a = Assets::new();
        a.add_uint8("k", &[]).unwrap();
        assert!(a.add_uint8("k", &[]).is_err());
    }

    #[test]
    fn int8_range_is_checked_not_truncated() {
        let mut a = Assets::new();
        assert!(a.add_int8("k", &[-128, 127]).is_ok());
        assert!(a.add_int8("j", &[128]).is_err());
    }
}
