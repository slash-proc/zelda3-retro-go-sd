/*
 * Host stubs for symbols the device build drops via --gc-sections
 * (dma ↔ snes bus helpers unused in HEADLESS Zelda3).
 */
#include <stdint.h>
#include <stddef.h>
#include "odroid_display.h"

struct Snes;

uint8_t snes_readBBus(struct Snes *snes, uint8_t adr)
{
    (void)snes;
    (void)adr;
    return 0;
}

void snes_writeBBus(struct Snes *snes, uint8_t adr, uint8_t val)
{
    (void)snes;
    (void)adr;
    (void)val;
}

uint8_t snes_read(struct Snes *snes, uint32_t adr)
{
    (void)snes;
    (void)adr;
    return 0;
}

void snes_write(struct Snes *snes, uint32_t adr, uint8_t val)
{
    (void)snes;
    (void)adr;
    (void)val;
}

odroid_display_scaling_t odroid_display_get_scaling_mode(void)
{
    return ODROID_DISPLAY_SCALING_FULL;
}
