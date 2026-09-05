# Zelda 3 (A Link to the Past) — Retro-Go SD GWHB homebrew
#
#   make                        — build + pack Zelda 3.bin (+ zelda3.ro)
#   make docker                 — same inside the builder image
#   make host                   — SDL2 desktop preview (zelda3_host)
#
# Sidecars on SD (not in the GWHB):
#   /homebrews/zelda3.ro
#   /homebrews/zelda3_assets.dat
# Build assets from a Zelda3 ROM via external/zelda3 (see README).
# Verbose compiler lines: make V=

#######################################
# Project identity
#######################################
PROJECT_KIND ?= homebrew

CORE_NAME  := zelda3
CORE_ENTRY := app_main

CORE_ZELDA3 := external/zelda3

CORE_C_SOURCES := \
$(CORE_ZELDA3)/zelda_rtl.c \
$(CORE_ZELDA3)/misc.c \
$(CORE_ZELDA3)/nmi.c \
$(CORE_ZELDA3)/poly.c \
$(CORE_ZELDA3)/attract.c \
$(CORE_ZELDA3)/snes/ppu.c \
$(CORE_ZELDA3)/snes/dma.c \
$(CORE_ZELDA3)/spc_player.c \
$(CORE_ZELDA3)/util.c \
$(CORE_ZELDA3)/audio.c \
$(CORE_ZELDA3)/overworld.c \
$(CORE_ZELDA3)/ending.c \
$(CORE_ZELDA3)/select_file.c \
$(CORE_ZELDA3)/dungeon.c \
$(CORE_ZELDA3)/messaging.c \
$(CORE_ZELDA3)/hud.c \
$(CORE_ZELDA3)/load_gfx.c \
$(CORE_ZELDA3)/ancilla.c \
$(CORE_ZELDA3)/player.c \
$(CORE_ZELDA3)/sprite.c \
$(CORE_ZELDA3)/player_oam.c \
$(CORE_ZELDA3)/snes/dsp.c \
$(CORE_ZELDA3)/sprite_main.c \
$(CORE_ZELDA3)/tagalong.c \
$(CORE_ZELDA3)/third_party/opus-1.3.1-stripped/opus_decoder_amalgam.c \
$(CORE_ZELDA3)/tile_detect.c \
$(CORE_ZELDA3)/overlord.c \
src/main_zelda3.c

CORE_C_INCLUDES := \
-Isrc \
-I$(CORE_ZELDA3) \
-Iexternal

# Defaults match firmware Makefile.common classic Zelda3 build.
# FEATURES bit7 = SKIP_INTRO_ON_KEYPRESS (default on).
CORE_C_DEFS := \
-DPROJECT_KIND_HOMEBREW=1 \
-DHEADLESS \
-DLIMIT_30FPS=1 \
-DFASTER_UI=1 \
-DBATTERY_INDICATOR=1 \
-DFEATURES=128

GNW_CORE_SDK ?= sdk
BUILD_DIR ?= build/$(PROJECT_KIND)

CORE_LDSCRIPT := src/zelda3_core.ld

PACKED_BIN := Zelda 3.bin
RO_BIN     := zelda3.ro
HB_NAME    := Zelda 3
COVER_JPG  := $(BUILD_DIR)/cover.jpg
COVER_SRC  := src/assets/cover_src.png

include $(GNW_CORE_SDK)/Makefile

PACK_HOMEBREW := $(GNW_CORE_SDK)/tools/pack_homebrew.py

# Upstream warn suppressions (mirrors cores/zelda3/Makefile).
ZELDA3_WARN_OFF := -Wno-parentheses -Wno-unknown-pragmas -Wno-incompatible-pointer-types \
	-Wno-unused-const-variable -Wno-strict-aliasing -Wno-comment -Wno-unused-function \
	-Wno-stack-usage -Wno-int-in-bool-context -Wno-unused-variable -Wno-unused-but-set-variable
CFLAGS += $(ZELDA3_WARN_OFF) -std=gnu11
ASFLAGS += -std=gnu11

#######################################
# Packed header version
#######################################
CORE_VERSION ?= $(shell git describe --tags --dirty 2>/dev/null || echo NOTAG)

#######################################
# Pack (+ extract .rodata_zelda3 sidecar)
#######################################
.PHONY: pack cover

cover: $(COVER_JPG)

# Must stay ≤ gui.c COVER_MAX (186×100) and COVER_SIZE (10 KiB).
$(COVER_JPG): $(COVER_SRC)
	$(V)$(ECHO) [ COVER ] $(COVER_JPG)
	$(V)mkdir -p $(BUILD_DIR)
	$(V)python3 -c "from pathlib import Path; from PIL import Image; \
img=Image.open('$(COVER_SRC)').convert('RGB'); \
img.thumbnail((186,100)); \
img.save('$(COVER_JPG)', 'JPEG', quality=85, optimize=True); \
sz=Path('$(COVER_JPG)').stat().st_size; \
assert sz <= 10*1024, f'cover too big: {sz}'; \
w,h=img.size; assert w<=186 and h<=100, (w,h)"

$(RO_BIN): $(TARGET_ELF)
	$(V)$(ECHO) [ RO ] $(RO_BIN)
	$(V)$(CP) -O binary --only-section=.rodata_zelda3 $< $@
	$(V)$(SZ) --target=binary $@

pack: $(TARGET_BIN) $(RO_BIN) $(COVER_JPG)
	$(V)$(ECHO) [ PACK GWHB ] "$(PACKED_BIN)" version=$(CORE_VERSION)
	$(V)python3 $(PACK_HOMEBREW) \
		--elf $(TARGET_ELF) --bin $(TARGET_BIN) \
		--name "$(HB_NAME)" --version "$(CORE_VERSION)" \
		--cover $(COVER_JPG) \
		--out "$(PACKED_BIN)"

all: pack

.PHONY: print-PROJECT_KIND print-PACKED_BIN print-RO_BIN print-CORE_NAME print-DOCKER_IMAGE \
	print-TARGET_ELF print-TARGET_MAP print-CORE_VERSION
print-PROJECT_KIND:
	@echo $(PROJECT_KIND)
print-PACKED_BIN:
	@echo $(PACKED_BIN)
print-RO_BIN:
	@echo $(RO_BIN)
print-CORE_NAME:
	@echo $(CORE_NAME)
print-DOCKER_IMAGE:
	@echo $(DOCKER_IMAGE)
print-TARGET_ELF:
	@echo $(TARGET_ELF)
print-TARGET_MAP:
	@echo $(BUILD_DIR)/$(CORE_NAME)_core.map
print-CORE_VERSION:
	@echo $(CORE_VERSION)

clean::
	$(V)rm -f "$(PACKED_BIN)" $(RO_BIN) $(COVER_JPG)

#######################################
# Docker (same image as firmware repo)
#######################################
.PHONY: docker docker_pull docker_shell

RELEASE_VERSION ?= v1.5
DOCKER_REPOSITORY ?= sylverb/retro-go-sd-builder
DOCKER_IMAGE ?= $(DOCKER_REPOSITORY):$(RELEASE_VERSION)

DOCKER_TTY_FLAG := $(shell if [ -t 0 ]; then echo -it; else echo; fi)
DOCKER_USER := $(shell id -u):$(shell id -g)
DOCKER_RUN := docker run --rm $(DOCKER_TTY_FLAG) \
	--user $(DOCKER_USER) \
	-v "$(CURDIR):/opt/workdir" \
	-w /opt/workdir \
	$(DOCKER_IMAGE)

docker:
	$(V)$(ECHO) "[ DOCKER ]" $(DOCKER_IMAGE) "PROJECT_KIND=$(PROJECT_KIND)"
	$(V)$(DOCKER_RUN) make --no-print-directory -j$$(nproc) PROJECT_KIND=$(PROJECT_KIND)

docker_pull:
	$(V)$(ECHO) "[ PULL ]" $(DOCKER_IMAGE)
	$(V)docker pull $(DOCKER_IMAGE)

docker_shell:
	$(DOCKER_RUN) bash

#######################################
# Host SDL (Linux / macOS)
#######################################
include host/Makefile.host
