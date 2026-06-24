/*
 * x86_64_basic.c — the "hello world" of embedding RAX.
 *
 * Open a 64-bit x86 engine, load a tiny machine-code snippet that computes a
 * value in registers and halts, run it, then read the result back.
 *
 * Build (from the repo root, after `cargo build -p rax-capi`):
 *   cc -std=c11 -I capi/include capi/examples/x86_64_basic.c \
 *      -L target/debug -lrax -o x86_64_basic
 *   DYLD_LIBRARY_PATH=target/debug ./x86_64_basic   # (LD_LIBRARY_PATH on Linux)
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>

#define CHECK(e) do { rax_status _s = (e); if (_s != RAX_OK) { \
    fprintf(stderr, "%s: %s\n", #e, rax_strerror(_s)); return 1; } } while (0)

int main(void) {
    rax_engine *engine = NULL;
    CHECK(rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &engine));

    /*
     *   mov rax, 0x1337     48 C7 C0 37 13 00 00
     *   mov rcx, 1          48 C7 C1 01 00 00 00
     *   add rax, rcx        48 01 C8
     *   hlt                 F4
     */
    const unsigned char code[] = {
        0x48, 0xC7, 0xC0, 0x37, 0x13, 0x00, 0x00,
        0x48, 0xC7, 0xC1, 0x01, 0x00, 0x00, 0x00,
        0x48, 0x01, 0xC8,
        0xF4,
    };
    const uint64_t entry = 0x1000;
    CHECK(rax_mem_write(engine, entry, code, sizeof(code)));

    /* Give the program a stack and run from `entry` until it halts. */
    CHECK(rax_reg_write_u64(engine, RAX_X86_REG_RSP, 0x8000));
    CHECK(rax_emu_start(engine, entry, RAX_NO_ADDR, /*timeout*/ 0, /*count*/ 0));

    rax_exit ex;
    CHECK(rax_emu_last_exit(engine, &ex));

    uint64_t rax = 0;
    CHECK(rax_reg_read_u64(engine, RAX_X86_REG_RAX, &rax));

    printf("stopped: reason=%d after %" PRIu64 " instructions\n", ex.reason, rax_emu_icount(engine));
    printf("RAX = 0x%" PRIx64 " (expected 0x1338)\n", rax);

    int ok = (ex.reason == RAX_STOP_HLT) && (rax == 0x1338);
    rax_engine_close(engine);
    puts(ok ? "OK" : "FAILED");
    return ok ? 0 : 1;
}
