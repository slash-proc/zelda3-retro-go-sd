//! The ordered asset list, from PORTING-MAP.md section 2.
//!
//! Order is the whole contract: `compile_resources.assets` is a plain dict, so
//! insertion order fixes the size array and fixes `key_sig`, whose SHA-256 goes
//! into the container magic. Nothing may be reordered, and the list is checked
//! against the recorded hash by a test in this module.
//!
//! `bytes` is the payload length the reference US build produces. It is not
//! used to build anything -- it is the target each porting slice is aiming at,
//! and `compare.mjs` checks the real thing against the real oracle.

use crate::pack::Kind;

/// (key name, element type, payload bytes in the reference US build).
pub const ASSET_TABLE: &[(&str, Kind, usize)] = &[
    ("kSoundBank_intro", Kind::Uint8, 50066),
    ("kSoundBank_indoor", Kind::Uint8, 12756),
    ("kSoundBank_ending", Kind::Uint8, 8354),
    ("kDungeonRoom", Kind::Uint8, 50381),
    ("kDungeonRoomOffs", Kind::Uint16, 640),
    ("kDungeonRoomDoorOffs", Kind::Uint16, 640),
    ("kDungeonRoomHeaders", Kind::Uint8, 3104),
    ("kDungeonRoomHeadersOffs", Kind::Uint16, 640),
    ("kDungeonRoomChests", Kind::Uint8, 504),
    ("kDungeonRoomTeleMsg", Kind::Uint16, 640),
    ("kDungeonPitsHurtPlayer", Kind::Uint16, 114),
    ("kEntranceData_rooms", Kind::Uint16, 266),
    ("kEntranceData_relativeCoords", Kind::Uint8, 1064),
    ("kEntranceData_scrollX", Kind::Uint16, 266),
    ("kEntranceData_scrollY", Kind::Uint16, 266),
    ("kEntranceData_playerX", Kind::Uint16, 266),
    ("kEntranceData_playerY", Kind::Uint16, 266),
    ("kEntranceData_cameraX", Kind::Uint16, 266),
    ("kEntranceData_cameraY", Kind::Uint16, 266),
    ("kEntranceData_blockset", Kind::Uint8, 133),
    ("kEntranceData_floor", Kind::Int8, 133),
    ("kEntranceData_palace", Kind::Int8, 133),
    ("kEntranceData_doorwayOrientation", Kind::Uint8, 133),
    ("kEntranceData_startingBg", Kind::Uint8, 133),
    ("kEntranceData_quadrant1", Kind::Uint8, 133),
    ("kEntranceData_quadrant2", Kind::Uint8, 133),
    ("kEntranceData_doorSettings", Kind::Uint16, 266),
    ("kEntranceData_musicTrack", Kind::Uint8, 133),
    ("kStartingPoint_rooms", Kind::Uint16, 14),
    ("kStartingPoint_relativeCoords", Kind::Uint8, 56),
    ("kStartingPoint_scrollX", Kind::Uint16, 14),
    ("kStartingPoint_scrollY", Kind::Uint16, 14),
    ("kStartingPoint_playerX", Kind::Uint16, 14),
    ("kStartingPoint_playerY", Kind::Uint16, 14),
    ("kStartingPoint_cameraX", Kind::Uint16, 14),
    ("kStartingPoint_cameraY", Kind::Uint16, 14),
    ("kStartingPoint_blockset", Kind::Uint8, 7),
    ("kStartingPoint_floor", Kind::Int8, 7),
    ("kStartingPoint_palace", Kind::Int8, 7),
    ("kStartingPoint_doorwayOrientation", Kind::Uint8, 7),
    ("kStartingPoint_startingBg", Kind::Uint8, 7),
    ("kStartingPoint_quadrant1", Kind::Uint8, 7),
    ("kStartingPoint_quadrant2", Kind::Uint8, 7),
    ("kStartingPoint_doorSettings", Kind::Uint16, 14),
    ("kStartingPoint_entrance", Kind::Uint8, 7),
    ("kStartingPoint_musicTrack", Kind::Uint8, 7),
    ("kDungeonRoomDefault", Kind::Uint8, 646),
    ("kDungeonRoomDefaultOffs", Kind::Uint16, 16),
    ("kDungeonRoomOverlay", Kind::Uint8, 566),
    ("kDungeonRoomOverlayOffs", Kind::Uint16, 38),
    ("kDungeonSecrets", Kind::Uint8, 2887),
    ("kDungAttrsForTile_Offs", Kind::Uint16, 42),
    ("kDungAttrsForTile", Kind::Uint8, 1024),
    ("kMovableBlockDataInit", Kind::Uint16, 396),
    ("kTorchDataInit", Kind::Uint16, 288),
    ("kTorchDataJunk", Kind::Uint16, 96),
    ("kEnemyDamageData", Kind::Uint8, 1728),
    ("kLinkGraphics", Kind::Uint8, 28672),
    ("kDungeonSprites", Kind::Uint8, 4965),
    ("kDungeonSpriteOffs", Kind::Uint16, 640),
    ("kMap32ToMap16_0", Kind::Uint8, 13308),
    ("kMap32ToMap16_1", Kind::Uint8, 13308),
    ("kMap32ToMap16_2", Kind::Uint8, 13308),
    ("kMap32ToMap16_3", Kind::Uint8, 13308),
    ("kSprGfx", Kind::Packed, 133479),
    ("kBgGfx", Kind::Packed, 119899),
    ("kOverworldMapGfx", Kind::Uint8, 16384),
    ("kLightOverworldTilemap", Kind::Uint8, 4096),
    ("kDarkOverworldTilemap", Kind::Uint8, 1024),
    ("kPredefinedTileData", Kind::Uint16, 12876),
    ("kMap16ToMap8", Kind::Uint16, 30016),
    ("kGeneratedWishPondItem", Kind::Uint8, 256),
    ("kGeneratedBombosArr", Kind::Uint8, 256),
    ("kGeneratedEndSequence15", Kind::Uint8, 256),
    ("kEnding_Credits_Text", Kind::Uint8, 1989),
    ("kEnding_Credits_Offs", Kind::Uint16, 788),
    ("kEnding_MapData", Kind::Uint16, 320),
    ("kEnding0_Offs", Kind::Uint16, 34),
    ("kEnding0_Data", Kind::Uint8, 917),
    ("kPalette_DungBgMain", Kind::Uint16, 3600),
    ("kPalette_MainSpr", Kind::Uint16, 240),
    ("kPalette_ArmorAndGloves", Kind::Uint16, 150),
    ("kPalette_Sword", Kind::Uint16, 24),
    ("kPalette_Shield", Kind::Uint16, 24),
    ("kPalette_SpriteAux3", Kind::Uint16, 168),
    ("kPalette_MiscSprite_Indoors", Kind::Uint16, 154),
    ("kPalette_SpriteAux1", Kind::Uint16, 336),
    ("kPalette_OverworldBgMain", Kind::Uint16, 420),
    ("kPalette_OverworldBgAux12", Kind::Uint16, 840),
    ("kPalette_OverworldBgAux3", Kind::Uint16, 196),
    ("kPalette_PalaceMapBg", Kind::Uint16, 192),
    ("kPalette_PalaceMapSpr", Kind::Uint16, 42),
    ("kHudPalData", Kind::Uint16, 128),
    ("kOverworldMapPaletteData", Kind::Uint16, 512),
    ("kDialogue", Kind::Packed, 37233),
    ("kDialogueFont", Kind::Packed, 4201),
    ("kDialogueMap", Kind::Packed, 11),
    ("kDungMap_FloorLayout", Kind::Packed, 1503),
    ("kDungMap_Tiles", Kind::Packed, 214),
    ("kBgTilemap_0", Kind::Uint8, 1115),
    ("kBgTilemap_1", Kind::Uint8, 1467),
    ("kBgTilemap_2", Kind::Uint8, 177),
    ("kBgTilemap_3", Kind::Uint8, 81),
    ("kBgTilemap_4", Kind::Uint8, 233),
    ("kBgTilemap_5", Kind::Uint8, 661),
    ("kOverworld_Hibytes_Comp", Kind::Packed, 25696),
    ("kOverworld_Lobytes_Comp", Kind::Packed, 35667),
    ("kOverworldMapIsSmall", Kind::Uint8, 192),
    ("kOverworldAuxTileThemeIndexes", Kind::Uint8, 128),
    ("kOverworldBgPalettes", Kind::Uint8, 136),
    ("kOverworld_SignText", Kind::Uint16, 256),
    ("kOwMusicSets", Kind::Uint8, 256),
    ("kOwMusicSets2", Kind::Uint8, 96),
    ("kBirdTravel_ScreenIndex", Kind::Uint16, 34),
    ("kBirdTravel_Map16LoadSrcOff", Kind::Uint16, 34),
    ("kBirdTravel_ScrollX", Kind::Uint16, 34),
    ("kBirdTravel_ScrollY", Kind::Uint16, 34),
    ("kBirdTravel_LinkXCoord", Kind::Uint16, 34),
    ("kBirdTravel_LinkYCoord", Kind::Uint16, 34),
    ("kBirdTravel_CameraXScroll", Kind::Uint16, 34),
    ("kBirdTravel_CameraYScroll", Kind::Uint16, 34),
    ("kBirdTravel_Unk1", Kind::Int8, 17),
    ("kBirdTravel_Unk3", Kind::Int8, 17),
    ("kWhirlpoolAreas", Kind::Uint16, 16),
    ("kOverworld_Entrance_Area", Kind::Uint16, 258),
    ("kOverworld_Entrance_Pos", Kind::Uint16, 258),
    ("kOverworld_Entrance_Id", Kind::Uint8, 129),
    ("kFallHole_Area", Kind::Uint16, 38),
    ("kFallHole_Pos", Kind::Uint16, 38),
    ("kFallHole_Entrances", Kind::Uint8, 19),
    ("kExitData_ScreenIndex", Kind::Uint8, 79),
    ("kExitDataRooms", Kind::Uint16, 158),
    ("kExitData_Map16LoadSrcOff", Kind::Uint16, 158),
    ("kExitData_ScrollX", Kind::Uint16, 158),
    ("kExitData_ScrollY", Kind::Uint16, 158),
    ("kExitData_XCoord", Kind::Uint16, 158),
    ("kExitData_YCoord", Kind::Uint16, 158),
    ("kExitData_CameraXScroll", Kind::Uint16, 158),
    ("kExitData_CameraYScroll", Kind::Uint16, 158),
    ("kExitData_NormalDoor", Kind::Uint16, 158),
    ("kExitData_FancyDoor", Kind::Uint16, 158),
    ("kExitData_Unk1", Kind::Int8, 79),
    ("kExitData_Unk3", Kind::Int8, 79),
    ("kSpExit_Top", Kind::Uint16, 32),
    ("kSpExit_Bottom", Kind::Uint16, 32),
    ("kSpExit_Left", Kind::Uint16, 32),
    ("kSpExit_Right", Kind::Uint16, 32),
    ("kSpExit_Tab4", Kind::Int16, 32),
    ("kSpExit_Tab5", Kind::Int16, 32),
    ("kSpExit_Tab6", Kind::Int16, 32),
    ("kSpExit_Tab7", Kind::Int16, 32),
    ("kSpExit_LeftEdgeOfMap", Kind::Uint16, 32),
    ("kSpExit_Dir", Kind::Uint8, 16),
    ("kSpExit_SprGfx", Kind::Uint8, 16),
    ("kSpExit_AuxGfx", Kind::Uint8, 16),
    ("kSpExit_PalBg", Kind::Uint8, 16),
    ("kSpExit_PalSpr", Kind::Uint8, 16),
    ("kOverworldSecrets_Offs", Kind::Uint16, 256),
    ("kOverworldSecrets", Kind::Uint8, 1187),
    ("kOverworldSpriteOffs", Kind::Uint16, 864),
    ("kOverworldSprites", Kind::Uint8, 2797),
    ("kOverworldSpriteGfx", Kind::Uint8, 256),
    ("kOverworldSpritePalettes", Kind::Uint8, 256),
    ("kMap8DataToTileAttr", Kind::Uint8, 512),
    ("kSomeTileAttr", Kind::Uint8, 3824),
];

/// SHA-256 of the NUL-joined key blob for the full 165-key list, measured from
/// the reference build (PORTING-MAP.md section 1).
pub const KEY_SIG_SHA256: &str =
    "1baee92d4aaefc32311b99c51b2bd8c58465ada9246c0f9bb0a93983ae6533cf";

/// Length of that blob.
pub const KEY_SIG_LEN: usize = 3252;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::key_sig_of;
    use crate::hash::sha256;

    fn hex_lower(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn the_list_has_165_entries_and_no_duplicates() {
        assert_eq!(ASSET_TABLE.len(), 165);
        for (i, (n, _, _)) in ASSET_TABLE.iter().enumerate() {
            assert!(
                !ASSET_TABLE[..i].iter().any(|(m, _, _)| m == n),
                "duplicate key {n}"
            );
        }
    }

    /// The milestone: the key blob validates the header before any payload
    /// work starts. If this fails, the order or a name is wrong.
    #[test]
    fn key_blob_matches_the_reference_hash() {
        let blob = key_sig_of(ASSET_TABLE.iter().map(|(n, _, _)| *n));
        assert_eq!(blob.len(), KEY_SIG_LEN);
        assert_eq!(hex_lower(&sha256(&blob)), KEY_SIG_SHA256);
    }
}
