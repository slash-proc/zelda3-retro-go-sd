/*
 * Zelda 3 — custom linker script (not sdk/ld/core_ram_emu.ld).
 *
 * Bulk .rodata is linked at a fake VMA (0xCAFE0000) and shipped as the
 * sibling SD file /homebrews/zelda3.ro. At launch, main_zelda3.c maps
 * that file into flash and rewrites every pointer in RAM_EMU that still
 * points into the 0xCAFE range (PatchCodeRodataOffset) — same split as
 * the classic firmware overlay / cores/zelda3/zelda3_core.ld.
 *
 * main_zelda3.o (entry + patcher) keeps its own .rodata in RAM_EMU and is
 * excluded from the rewrite walk via _ZELDA3_MAIN_CODE_START/_END.
 * .noreloc tables stay with code in RAM_EMU (classic layout).
 *
 * Object paths use wildcards so BUILD_DIR (build/homebrew/…) does not need
 * hardcoding.
 */

INCLUDE gnw_ram_emu.ld

MEMORY
{
  RAM_EMU (xrw) : ORIGIN = __RAM_EMU_START__, LENGTH = __RAM_EMU_LENGTH__
  /* Sentinel VMA only — payload is extracted to zelda3.ro, not packed in GWHB. */
  RODATA  (x)   : ORIGIN = 0xCAFE0000,         LENGTH = 0xFFFFFFFF
}

PHDRS
{
  text PT_LOAD FLAGS(5); /* PF_R | PF_X */
  data PT_LOAD FLAGS(6); /* PF_R | PF_W */
  /* Separate LOAD so the 0xCAFE VMA does not inflate the RAM_EMU segment. */
  rodata PT_LOAD FLAGS(4); /* PF_R */
}

/* Zelda game objects whose .rodata goes to the sidecar (basenames).
 * Bridge / main_zelda3 keep .rodata in RAM_EMU. */
SECTIONS
{
  .core_entry __RAM_EMU_START__ :
  {
    KEEP(*(.core_entry))
    KEEP(*(.gw_core_bridge_probe))
    . = ALIGN(4);

    _ZELDA3_MAIN_CODE_START = .;
    *main_zelda3.o (.data .data* .text .text* .rodata .rodata*)
    . = ALIGN(4);
    _ZELDA3_MAIN_CODE_END = .;

    *(.text*)
    *(.glue_7)
    *(.glue_7t)
    *(.noreloc)
    *(.data*)

    /* Bridge / entry helpers keep .rodata here; game tables → .rodata_zelda3. */
    EXCLUDE_FILE(
      *zelda_rtl.o
      *misc.o
      *nmi.o
      *poly.o
      *attract.o
      *ppu.o
      *dma.o
      *spc_player.o
      *util.o
      *audio.o
      *overworld.o
      *ending.o
      *select_file.o
      *dungeon.o
      *messaging.o
      *hud.o
      *load_gfx.o
      *ancilla.o
      *player.o
      *sprite.o
      *player_oam.o
      *dsp.o
      *sprite_main.o
      *tagalong.o
      *opus_decoder_amalgam.o
      *tile_detect.o
      *overlord.o
    ) *(.rodata*)

    . = ALIGN(4);
    __CORE_INIT_ARRAY_START__ = .;
    KEEP(*(SORT(.init_array.*)))
    KEEP(*(.init_array*))
    __CORE_INIT_ARRAY_END__ = .;
  } >RAM_EMU :text

  /* May be empty — data claimed above. No leading ALIGN (see core_ram_emu.ld). */
  .data :
  {
  } >RAM_EMU :data

  __CORE_CODE_END__ = .;

  .bss (NOLOAD) :
  {
    . = ALIGN(8);
    *(.bss*)
    *(COMMON)
    . = ALIGN(8);
  } >RAM_EMU :data

  __CORE_BSS_END__ = .;

  ASSERT(__CORE_BSS_END__ <= __RAM_EMU_END__,
         "Error: Zelda3 core code+data+bss overflows RAM_EMU")

  .rodata_zelda3 :
  {
    . = ALIGN(4);
    __rodata_zelda3_start__ = .;
    *(.rodata*)
    __rodata_zelda3_end__ = .;
  } >RODATA :rodata

  __exidx_start = .;
  __exidx_end = .;

  /DISCARD/ :
  {
    *(.fini_array*)
    *(.preinit_array*)
    *(.ARM.exidx*)
    *(.ARM.extab*)
    *(.comment)
    *(.ARM.attributes)
  }
}
