/* Host: neutralize ELF-only section(".noreloc") attrs (invalid on Mach-O).
 * `__attribute__((section (".noreloc")))` → `__attribute__(())`. */
#pragma once
#define section(x)
