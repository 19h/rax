/* Linker script for the bare-metal AArch64 microkernel.
 *
 * The rax VMM loads a flat AArch64 binary at the RAM base 0x4000_0000 (=entry)
 * with the MMU off (VA==PA), 128 MiB of RAM at [0x4000_0000, 0x4800_0000), a
 * PL011 UART at 0x0900_0000, and PSCI-over-HVC for power-off. The device tree
 * is placed low (~0x4020_0000); the kernel ignores it, and the stack/heap below
 * are kept well clear of it. */

ENTRY(_start)

SECTIONS
{
    . = 0x40000000;

    .text : {
        *(.text._start)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* Stack grows down from 0x4400_0000 (64 MiB into RAM). */
    . = 0x44000000;
    __stack_top = .;

    /* 32 MiB heap at [0x4500_0000, 0x4700_0000). */
    . = 0x45000000;
    __heap_start = .;
    . = 0x47000000;
    __heap_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.ARM.exidx*)
        *(.ARM.attributes)
    }
}
