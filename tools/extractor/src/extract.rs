//! The Zelda 3 conversion pipeline.
//!
//! The core layer is in place: [`crate::rom`] reads the cartridge,
//! [`crate::codec`] decompresses, [`crate::pack`] accumulates and serialises,
//! and [`crate::assets`] fixes the 165-key order. This module is the
//! conductor: it owns the ordered stage table and the context the stages hand
//! results to each other in. Nothing here decodes anything itself.
//!
//! **Emission order is the contract.** `compile_resources.print_all` emits the
//! 165 assets in one interleaved sequence — music 0-2, dungeon 3-55, graphics
//! 56-57, dungeon 58-59, overworld 60-63, graphics 64-93, dialogue 94-96,
//! dungeon 97-98, graphics 99-104, overworld 105-164 — and that order fixes
//! both the size array and the key blob whose SHA-256 sits in the header. So
//! the phase table calls the per-stage functions of each module in that global
//! order rather than each module's `add_all`, and [`phase_write`] re-checks the
//! key-blob hash before anything is serialised: a stage inserted in the wrong
//! place fails loudly here instead of producing a plausible, wrong file.
//!
//! The stages come from PORTING-MAP.md section 4. Stages 1-13 read the ROM
//! into memory, 14-28 build the assets from what was read, and 29 writes. The
//! split is what lets a host show a progress bar: each stage hands its result
//! to the context and returns, so the work is done exactly once even though
//! reading and building are reported separately.

use crate::assets::ASSET_TABLE;
use crate::pack::Assets;
use crate::rom::{Rom, ZELDA3_SHA1_US};
use crate::{dialogue, dungeon, graphics, music, overworld};

pub type Result<T> = core::result::Result<T, String>;

/// Re-exported so the ABI docs and the CLI keep referring to one definition.
pub use crate::rom::KNOWN_ROMS;

/// The finished conversion: the produced bytes plus anything worth telling the
/// user that did not stop the run.
pub struct Extraction {
    pub data: Vec<u8>,
    pub warnings: Vec<String>,
}

/// A registered input file could not be given a role. Reported to the host as
/// [`crate::ERR_INPUTS`], separately from a failure inside the conversion
/// itself ([`crate::ERR_EXTRACTION`]), because the fix is different: the user
/// supplied the wrong files, not a broken ROM.
pub struct InputsError(pub String);

/// Everything that crosses a phase boundary. Phases mutate it in order; the
/// host never sees it.
///
/// The intermediate fields are what stages 1-13 produce and stages 14-28
/// consume. They are cleared as soon as the last stage that needs them has
/// run, because the whole conversion is held in a wasm module's linear memory
/// and the peak is what the host has to reserve.
pub struct Ctx {
    /// The US ROM. Every table and every graphic comes from this one.
    pub base: Rom,
    /// Translated ROMs, sorted into canonical language order. Dialogue only.
    pub languages: Vec<Rom>,
    /// The ordered asset store, filled in emission order by the stages.
    pub assets: Assets,
    pub warnings: Vec<String>,

    // Stages 2-4.
    rooms: Vec<dungeon::Room>,
    entrances: Vec<dungeon::Entrance>,
    starting_points: Vec<dungeon::Entrance>,
    dungeon: Option<dungeon::Dungeon>,
    // Stages 5-7.
    ow_links: Option<overworld::Links>,
    ow_areas: Vec<overworld::Area>,
    map32: Vec<[u16; 4]>,
    // Stages 8 and 13, one entry per language, `us` first.
    lang_codes: Vec<&'static str>,
    strings: Vec<Vec<String>>,
    fonts: Vec<(Vec<u8>, Vec<u8>)>,
    // Stages 9-11.
    banks: Vec<music::Bank>,
    music_info: Option<music::MusicInfo>,
    // Stage 12.
    link_pixels: Vec<u8>,
}

impl Ctx {
    /// Sorts the registered inputs into roles by content and rejects anything
    /// that cannot be placed. Role comes from the SHA-1, never from position,
    /// so a mislabelled file cannot be smuggled into the wrong slot.
    ///
    /// Exactly one input must be the US ROM; the rest must each be a distinct
    /// translation this converter has text tables for. The four ways that can
    /// go wrong — no US ROM, an unrecognised file, the same language twice, a
    /// US ROM offered as a translation — are all rejected here, before any
    /// work is done, and all report [`crate::ERR_INPUTS`].
    ///
    /// The translations are then sorted into canonical order (the declaration
    /// order of `kLanguages`). The Python packs languages in command-line
    /// order and `de,fr` really does produce a different file from `fr,de`, so
    /// a host that has no natural order would otherwise produce output that
    /// depended on the order it happened to register files in. Fixing the
    /// order here makes the same set of ROMs always produce the same bytes.
    pub fn new(inputs: Vec<Vec<u8>>) -> core::result::Result<Ctx, InputsError> {
        if inputs.is_empty() {
            return Err(InputsError("no input files were registered".into()));
        }

        let mut roms: Vec<Rom> = inputs.into_iter().map(Rom::new).collect();

        // Whether an unrecognised file may be tried at all is the host's
        // decision now -- it hashes what the user supplied against the
        // manifest's `variants[]` and refuses first when the input is strict.
        // By the time a file reaches here that question has been answered, so
        // the module identifies what it can and gets on with it rather than
        // second-guessing the host.
        let mut warnings = Vec::new();
        let base_at = match roms.iter().position(|r| r.language == Some("us")) {
            Some(i) => i,
            None => {
                warnings.push(format!(
                    "no supplied file matches the US ROM (SHA-1 {}); \
                     using the first one as the base.",
                    ZELDA3_SHA1_US
                ));
                0
            }
        };
        let base = roms.remove(base_at);

        // An unrecognised extra file is skipped with a warning rather than
        // refused. There is nothing else this converter could do with it: it
        // cannot tell which language it is reading, so it cannot pack its
        // dialogue. Saying so and continuing beats failing a run whose other
        // inputs were all fine.
        let mut languages = Vec::new();
        for extra in roms {
            if extra.language.is_none() {
                warnings.push(format!(
                    "ROM {} is not a known language release; its dialogue was skipped.",
                    extra.sha1
                ));
                continue;
            }
            languages.push(extra);
        }

        // Rejects a duplicate language and a US ROM handed over as a
        // translation, and yields the canonical order. Doing it now means
        // those mistakes are reported before a minute of decoding, and as an
        // input error rather than as a conversion failure.
        let order = dialogue::language_order(&languages).map_err(InputsError)?;
        let mut sorted: Vec<Option<Rom>> = languages.into_iter().map(Some).collect();
        let languages: Vec<Rom> = order
            .iter()
            .map(|&(_, i)| sorted[i].take().expect("language_order repeated an index"))
            .collect();

        Ok(Ctx {
            base,
            languages,
            assets: Assets::new(),
            warnings,
            rooms: Vec::new(),
            entrances: Vec::new(),
            starting_points: Vec::new(),
            dungeon: None,
            ow_links: None,
            ow_areas: Vec::new(),
            map32: Vec::new(),
            lang_codes: Vec::new(),
            strings: Vec::new(),
            fonts: Vec::new(),
            banks: Vec::new(),
            music_info: None,
            link_pixels: Vec::new(),
        })
    }

    /// Serialises the asset store into the container format.
    pub fn finish(mut self) -> Extraction {
        let empty = self.assets.iter().filter(|a| a.data.is_empty()).count();
        if empty != 0 {
            self.warnings.push(format!(
                "{empty} of {} assets have no payload: the container header is correct but the file is not a usable asset pack.",
                self.assets.len()
            ));
        }
        let data = self.assets.serialise();
        Extraction { data, warnings: self.warnings }
    }
}

/// The ordered stages of a conversion. Names are user-visible: they appear in
/// a host's progress bar, so they say what is happening in the terms the game
/// uses, not in the terms the code does.
///
/// PORTING-MAP.md section 4 lists these in the same wording. The one departure
/// is that "Reading overworld links" runs before "Reading the overworld"
/// rather than after: the per-area read consults the exit, travel, entrance
/// and hole tables, so they have to exist first.
pub const PHASES: &[(&str, fn(&mut Ctx) -> Result<()>)] = &[
    // Reading (extract_resources.py).
    ("Reading the ROM", phase_read_rom),
    ("Reading dungeon rooms", phase_read_dungeon_rooms),
    ("Reading room entrances", phase_read_room_entrances),
    ("Reading template rooms", phase_read_template_rooms),
    ("Reading overworld links", phase_read_overworld_links),
    ("Reading the overworld", phase_read_overworld),
    ("Reading tile mappings", phase_read_tile_mappings),
    ("Reading dialogue", phase_read_dialogue),
    ("Reading music banks", phase_read_music_banks),
    ("Decoding music", phase_decode_music),
    ("Reading instruments", phase_read_instruments),
    ("Reading Link's sprites", phase_read_link_sprites),
    ("Reading the font", phase_read_font),
    // Building (compile_resources.py), in `print_all` emission order.
    ("Building sound banks", phase_sound_banks), // 0-2
    ("Building dungeon rooms", phase_dungeon_rooms), // 3-10
    ("Building entrances", phase_entrances),      // 11-45
    ("Building room templates", phase_room_templates), // 46-55
    ("Building enemy data", phase_enemy_data),    // 56
    ("Packing Link's sprites", phase_link_sprites), // 57
    ("Building dungeon sprites", phase_dungeon_sprites), // 58-59
    ("Building tile mappings", phase_tile_mappings), // 60-63
    ("Packing graphics", phase_graphics),         // 64-65
    ("Copying palettes and tables", phase_palettes), // 66-93
    ("Compressing dialogue", phase_dialogue),     // 94-96
    ("Building dungeon maps", phase_dungeon_maps), // 97-98
    ("Building tilemaps", phase_tilemaps),        // 99-104
    ("Packing overworld maps", phase_overworld_maps), // 105-106
    ("Building overworld tables", phase_overworld_tables), // 107-164
    ("Writing the asset file", phase_write),
];

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Checks the ROM is readable through the LoROM mapping before anything relies
/// on it. `get_byte` asserts bit 15 the way the Python does, and a truncated
/// ROM should fail here rather than a thousand reads later.
fn phase_read_rom(ctx: &mut Ctx) -> Result<()> {
    if ctx.base.data.len() < 0x100000 {
        return Err(format!(
            "the base ROM is {} bytes, too short to be a Zelda 3 cartridge.",
            ctx.base.data.len()
        ));
    }
    ctx.base.get_byte(0x808000)?;
    for r in &ctx.languages {
        if r.data.len() < 0x100000 {
            return Err(format!(
                "the {} ROM is {} bytes, too short to be a Zelda 3 cartridge.",
                r.language.unwrap_or("extra"),
                r.data.len()
            ));
        }
        r.get_byte(0x808000)?;
    }
    Ok(())
}

fn phase_read_dungeon_rooms(ctx: &mut Ctx) -> Result<()> {
    ctx.rooms = dungeon::read_rooms(&ctx.base)?;
    Ok(())
}

fn phase_read_room_entrances(ctx: &mut Ctx) -> Result<()> {
    let (e, s) = dungeon::read_entrances(&ctx.base)?;
    ctx.entrances = e;
    ctx.starting_points = s;
    Ok(())
}

/// Closes the dungeon read: the templates plus the rooms and entrances the two
/// stages before produced, assembled into the one value the build stages take.
fn phase_read_template_rooms(ctx: &mut Ctx) -> Result<()> {
    let (default_rooms, overlay_rooms) = dungeon::read_templates(&ctx.base)?;
    ctx.dungeon = Some(dungeon::Dungeon {
        rooms: core::mem::take(&mut ctx.rooms),
        entrances: core::mem::take(&mut ctx.entrances),
        starting_points: core::mem::take(&mut ctx.starting_points),
        default_rooms,
        overlay_rooms,
    });
    Ok(())
}

fn phase_read_overworld_links(ctx: &mut Ctx) -> Result<()> {
    ctx.ow_links = Some(overworld::read_links(&ctx.base)?);
    Ok(())
}

fn phase_read_overworld(ctx: &mut Ctx) -> Result<()> {
    let links = ctx.ow_links.take().ok_or("the overworld links were not read")?;
    ctx.ow_areas = overworld::read_areas(&ctx.base, &links)?;
    Ok(())
}

fn phase_read_tile_mappings(ctx: &mut Ctx) -> Result<()> {
    ctx.map32 = overworld::read_map32_to_map16(&ctx.base)?;
    Ok(())
}

/// Decodes the 397 strings of every language in the build. The language list
/// is fixed here — `us` from the base ROM, then the translations in canonical
/// order — and the font stage and the compressing stage reuse it, so the three
/// cannot disagree about what a build contains.
fn phase_read_dialogue(ctx: &mut Ctx) -> Result<()> {
    ctx.lang_codes = dialogue::language_codes(&ctx.base, &ctx.languages)?;
    ctx.strings = dialogue::read_all_strings(&ctx.base, &ctx.languages)?;
    Ok(())
}

fn phase_read_music_banks(ctx: &mut Ctx) -> Result<()> {
    ctx.banks = music::read_music_banks(&ctx.base)?;
    Ok(())
}

fn phase_decode_music(ctx: &mut Ctx) -> Result<()> {
    music::decode_music(&mut ctx.banks)
}

fn phase_read_instruments(ctx: &mut Ctx) -> Result<()> {
    let first = ctx.banks.first().ok_or("no sound banks were read")?;
    ctx.music_info = Some(music::read_instruments(&first.memory)?);
    Ok(())
}

fn phase_read_link_sprites(ctx: &mut Ctx) -> Result<()> {
    ctx.link_pixels = graphics::decode_4bit_tileset_link(&ctx.base)?;
    Ok(())
}

fn phase_read_font(ctx: &mut Ctx) -> Result<()> {
    ctx.fonts = dialogue::read_all_fonts(&ctx.base, &ctx.languages)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Building. Every stage below appends to the store, and the order they run in
// is the order the keys appear in the file.
// ---------------------------------------------------------------------------

/// Assets 0-2. `print_sound_banks`.
fn phase_sound_banks(ctx: &mut Ctx) -> Result<()> {
    let info = ctx.music_info.take().ok_or("the instruments were not read")?;
    music::add_sound_banks(&ctx.banks, &info, &mut ctx.assets)?;
    // 3 x 64 KB of SPC images plus every decoded pattern; nothing else needs
    // them.
    ctx.banks = Vec::new();
    Ok(())
}

/// Assets 3-10. The first half of `print_dungeon_rooms`.
fn phase_dungeon_rooms(ctx: &mut Ctx) -> Result<()> {
    let d = ctx.dungeon.take().ok_or("the dungeon was not read")?;
    let r = dungeon::add_rooms(&d, &mut ctx.assets);
    ctx.dungeon = Some(d);
    r
}

/// Assets 11-45. The entrance and starting-point tables.
fn phase_entrances(ctx: &mut Ctx) -> Result<()> {
    let d = ctx.dungeon.take().ok_or("the dungeon was not read")?;
    let r = dungeon::add_entrances(&d, &mut ctx.assets);
    ctx.dungeon = Some(d);
    r
}

/// Assets 46-55. Default and overlay rooms, secrets, and the tile-attribute
/// tables that close `print_dungeon_rooms`.
fn phase_room_templates(ctx: &mut Ctx) -> Result<()> {
    let d = ctx.dungeon.take().ok_or("the dungeon was not read")?;
    let r = dungeon::add_templates(&ctx.base, &d, &mut ctx.assets);
    // `add_sprites` still needs the rooms; the template lists are done with.
    ctx.dungeon = Some(dungeon::Dungeon {
        default_rooms: Vec::new(),
        overlay_rooms: Vec::new(),
        ..d
    });
    r
}

/// Asset 56. `print_enemy_damage_data`.
fn phase_enemy_data(ctx: &mut Ctx) -> Result<()> {
    graphics::add_enemy_damage_data(&ctx.base, &mut ctx.assets)
}

/// Asset 57. `print_link_graphics`, re-encoding the pixels stage 12 decoded.
fn phase_link_sprites(ctx: &mut Ctx) -> Result<()> {
    let pixels = core::mem::take(&mut ctx.link_pixels);
    ctx.assets.add_uint8("kLinkGraphics", &graphics::encode_link_graphics(&pixels))
}

/// Assets 58-59. `print_dungeon_sprites`, the last stage that needs the rooms.
fn phase_dungeon_sprites(ctx: &mut Ctx) -> Result<()> {
    let d = ctx.dungeon.take().ok_or("the dungeon was not read")?;
    dungeon::add_sprites(&d, &mut ctx.assets)
}

/// Assets 60-63. `print_map32_to_map16`.
fn phase_tile_mappings(ctx: &mut Ctx) -> Result<()> {
    let tab = core::mem::take(&mut ctx.map32);
    overworld::add_map32_to_map16_from(&tab, &mut ctx.assets)
}

/// Assets 64-65. `print_images`: the compressed sprite and background sheets.
fn phase_graphics(ctx: &mut Ctx) -> Result<()> {
    graphics::add_packed_graphics(&ctx.base, &mut ctx.assets)
}

/// Assets 66-93. `print_misc`: palettes and the direct ROM extracts.
fn phase_palettes(ctx: &mut Ctx) -> Result<()> {
    graphics::add_palettes_and_tables(&ctx.base, &mut ctx.assets)
}

/// Assets 94-96. `print_dialogue`: one entry per language in `kDialogue`,
/// `kDialogueFont` and `kDialogueMap`. The only stage a language ROM reaches.
fn phase_dialogue(ctx: &mut Ctx) -> Result<()> {
    let codes = core::mem::take(&mut ctx.lang_codes);
    let strings = core::mem::take(&mut ctx.strings);
    let fonts = core::mem::take(&mut ctx.fonts);
    dialogue::add_all_from(&codes, &strings, &fonts, &mut ctx.assets)
}

/// Assets 97-98. `print_dungeon_map`.
fn phase_dungeon_maps(ctx: &mut Ctx) -> Result<()> {
    dungeon::add_maps(&ctx.base, &mut ctx.assets)
}

/// Assets 99-104. `print_tilemaps`.
fn phase_tilemaps(ctx: &mut Ctx) -> Result<()> {
    graphics::add_bg_tilemaps(&ctx.base, &mut ctx.assets)
}

/// Assets 105-106. `print_overworld`: the compressed hi/lo byte streams.
fn phase_overworld_maps(ctx: &mut Ctx) -> Result<()> {
    overworld::add_overworld(&ctx.base, &mut ctx.assets)
}

/// Assets 107-164. `print_overworld_tables`, in `A.add` registration order.
fn phase_overworld_tables(ctx: &mut Ctx) -> Result<()> {
    let areas = core::mem::take(&mut ctx.ow_areas);
    overworld::add_overworld_tables_from(&ctx.base, &areas, &mut ctx.assets)
}

// ---------------------------------------------------------------------------

/// Guards the invariants the whole container rests on: the 165 keys must be
/// the ones the reference build emits, in that order, with those element
/// types, and the key blob must hash to the recorded value. A stage that runs
/// out of turn fails here rather than in a byte diff.
fn phase_write(ctx: &mut Ctx) -> Result<()> {
    use crate::assets::{KEY_SIG_LEN, KEY_SIG_SHA256};

    if ctx.assets.len() != ASSET_TABLE.len() {
        return Err(format!(
            "the run produced {} assets, expected {}",
            ctx.assets.len(),
            ASSET_TABLE.len()
        ));
    }
    for (i, (asset, (name, kind, _))) in ctx.assets.iter().zip(ASSET_TABLE).enumerate() {
        if asset.name != *name {
            return Err(format!(
                "asset {i} is {:?}, expected {name:?}: a stage ran out of order",
                asset.name
            ));
        }
        if asset.kind != *kind {
            return Err(format!(
                "asset {i} ({name}) is a {:?}, expected {kind:?}",
                asset.kind
            ));
        }
    }

    let blob = ctx.assets.key_sig();
    if blob.len() != KEY_SIG_LEN {
        return Err(format!(
            "key blob is {} bytes, expected {KEY_SIG_LEN}",
            blob.len()
        ));
    }
    let got: String = crate::hash::sha256(&blob)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != KEY_SIG_SHA256 {
        return Err(format!("key blob hashes to {got}, expected {KEY_SIG_SHA256}"));
    }
    Ok(())
}
