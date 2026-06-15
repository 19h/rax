/* Linker script for the bare-metal ARMv6 (ARM1176JZF-S / S3C64xx) microkernel.
 *
 * The rax VMM ("armv7a" machine = SMDK6410) loads a flat A32 binary at RAM base
 * + TEXT_OFFSET = 0x5000_8000 (=entry), MMU off (VA==PA), 128 MiB of RAM at
 * [0x5000_0000, 0x5800_0000), the Samsung UART0 at 0x7F00_5000, and a synthetic
 * power-off port at 0x7E00_E000. The device tree is placed at 0x5300_0000 and an
 * (unused) initrd window at 0x5400_0000; the stack/heap sit above both so they
 * never collide. */

ENTRY(_start)

SECTIONS
{
    . = 0x50008000;

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

    /* Stack grows down from 0x5700_0000 (above DTB@0x5300_0000 / initrd@0x5400_0000). */
    . = 0x57000000;
    __stack_top = .;

    /* 24 MiB heap at [0x5500_0000, 0x5680_0000). */
    . = 0x55000000;
    __heap_start = .;
    . = 0x56800000;
    __heap_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.ARM.exidx*)
        *(.ARM.attributes)
    }
}
