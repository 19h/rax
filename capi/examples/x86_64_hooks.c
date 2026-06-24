/*
 * x86_64_hooks.c — code + block hooks and stopping from a hook.
 *
 * Runs a countdown loop and traces every executed instruction with a code hook.
 * A block hook reports basic-block entries. The code hook also demonstrates
 * cooperative stop: after a budget of instructions it calls rax_emu_stop().
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>

#define CHECK(e) do { rax_status _s = (e); if (_s != RAX_OK) { \
    fprintf(stderr, "%s: %s\n", #e, rax_strerror(_s)); return 1; } } while (0)

struct ctx { int insns; int blocks; int budget; };

static void on_code(rax_engine *e, uint64_t addr, uint32_t size, void *user) {
    struct ctx *c = (struct ctx *)user;
    (void)size;
    c->insns++;
    printf("  insn #%d at 0x%" PRIx64 "\n", c->insns, addr);
    if (c->insns >= c->budget) {
        printf("  budget reached -> stop\n");
        rax_emu_stop(e);
    }
}

static void on_block(rax_engine *e, uint64_t addr, uint32_t size, void *user) {
    struct ctx *c = (struct ctx *)user;
    (void)e; (void)size;
    c->blocks++;
    printf("block entry at 0x%" PRIx64 "\n", addr);
}

int main(void) {
    rax_engine *engine = NULL;
    CHECK(rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &engine));

    /*
     *   mov ecx, 5      B9 05 00 00 00   (rcx = 5, upper bits cleared)
     * loop:
     *   dec rcx         48 FF C9
     *   jnz loop        75 FB
     *   hlt             F4
     */
    const unsigned char code[] = {
        0xB9, 0x05, 0x00, 0x00, 0x00,
        0x48, 0xFF, 0xC9,
        0x75, 0xFB,
        0xF4,
    };
    const uint64_t entry = 0x1000;
    CHECK(rax_mem_write(engine, entry, code, sizeof(code)));

    struct ctx c = { 0, 0, 100 /* generous budget; loop self-terminates first */ };
    uint32_t code_id = 0, block_id = 0;
    CHECK(rax_hook_add_code(engine, 1, 0, on_code, &c, &code_id));   /* begin>end => all */
    CHECK(rax_hook_add_block(engine, 1, 0, on_block, &c, &block_id));

    CHECK(rax_emu_start(engine, entry, RAX_NO_ADDR, 0, 0));

    rax_exit ex;
    CHECK(rax_emu_last_exit(engine, &ex));
    printf("done: reason=%d insns=%d blocks=%d icount=%" PRIu64 "\n",
           ex.reason, c.insns, c.blocks, rax_emu_icount(engine));

    rax_hook_del(engine, code_id);
    rax_hook_del(engine, block_id);

    int ok = (ex.reason == RAX_STOP_HLT) && (c.insns == 12) && (c.blocks >= 2);
    rax_engine_close(engine);
    puts(ok ? "OK" : "FAILED");
    return ok ? 0 : 1;
}
