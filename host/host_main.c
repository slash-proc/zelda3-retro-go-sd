/*
 * Host SDL entry — opens a window then calls the GWHB app_main().
 */
#include <stdio.h>
#include <stdlib.h>

#include "host_compat.h"
#include "host_platform.h"

void app_main(unsigned char load_state, unsigned char start_paused, signed char save_slot);

int main(int argc, char **argv)
{
    const char *title = "Zelda 3 (host)";
    const char *rom = getenv("HOST_ROM");
    const char *host_sd = getenv("HOST_SD");

    if (argc > 1 && argv[1] && argv[1][0])
        rom = argv[1];

    if (rom)
        host_set_rom_path(rom);

    host_platform_init(title, HOST_SCALE);
    gw_core_bridge_init();

    printf("host: Esc or close window to quit\n");
    printf("host: Arrows=D-pad  Z=B  X=A  Enter=Start  Shift=Select  A/S=Y/X\n");
    printf("host: F1=save state  F2=load state  (./host_saves/)\n");
    printf("host: assets → /homebrews/zelda3_assets.dat");
    if (host_sd && host_sd[0])
        printf(" (HOST_SD=%s)\n", host_sd);
    else
        printf(" (or ./homebrews/zelda3_assets.dat)\n");
    printf("host: HOST_OFW_MARIO=1 for Mario face-button layout\n");
    printf("host: device builds also need /homebrews/zelda3.ro (not used on host)\n");
    if (rom)
        printf("host: ROM/path %s\n", rom);

    app_main(0, 0, -1);

    host_platform_shutdown();
    return 0;
}
