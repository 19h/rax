/*
 * x86_64_step.c — single-stepping instruction by instruction.
 *
 * Steps one instruction at a time with rax_emu_step(), printing RIP before each
 * step, until the program halts.
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>

#define CHECK(e) do { rax_status _s = (e); if (_s != RAX_OK) { \
    fprintf(stderr, "%s: %s\n", #e, rax_strerror(_s)); return 1; } } while (0)

int main(void) {
    rax_engine *engine = NULL;
    CHECK(rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &engine));

    if (!rax_engine_supports_stepping(engine)) {
        fprintf(stderr, "backend does not support stepping\n");
        rax_engine_close(engine);
        return 1;
    }

    /*  mov ecx,3 ; loop: dec rcx ; jnz loop ; hlt  */
    const unsigned char code[] = {
        0xB9, 0x03, 0x00, 0x00, 0x00,
        0x48, 0xFF, 0xC9,
        0x75, 0xFB,
        0xF4,
    };
    const uint64_t entry = 0x1000;
    CHECK(rax_mem_write(engine, entry, code, sizeof(code)));
    CHECK(rax_reg_write_u64(engine, RAX_X86_REG_RIP, entry));

    int steps = 0;
    for (;;) {
        uint64_t rip = 0, rcx = 0;
        CHECK(rax_reg_read_u64(engine, RAX_X86_REG_RIP, &rip));
        CHECK(rax_reg_read_u64(engine, RAX_X86_REG_RCX, &rcx));
        printf("step %2d: RIP=0x%" PRIx64 " RCX=%" PRIu64 "\n", steps, rip, rcx);

        uint64_t executed = 0;
        CHECK(rax_emu_step(engine, 1, &executed));
        steps++;

        rax_exit ex;
        CHECK(rax_emu_last_exit(engine, &ex));
        if (ex.reason == RAX_STOP_HLT) {
            printf("halted after %d steps (icount=%" PRIu64 ")\n", steps, rax_emu_icount(engine));
            break;
        }
        if (steps > 1000) { fprintf(stderr, "runaway\n"); rax_engine_close(engine); return 1; }
    }

    rax_engine_close(engine);
    puts("OK");
    return 0;
}
