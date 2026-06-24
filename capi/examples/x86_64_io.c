/*
 * x86_64_io.c — servicing guest port I/O with hooks.
 *
 * The guest writes a byte to port 0xE9 (OUT) and then reads a byte back (IN).
 * Without devices, RAX would simply stop on the I/O exit; here we install I/O
 * hooks that observe the OUT value and supply the IN value, letting execution
 * continue all the way to HLT.
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>

#define CHECK(e) do { rax_status _s = (e); if (_s != RAX_OK) { \
    fprintf(stderr, "%s: %s\n", #e, rax_strerror(_s)); return 1; } } while (0)

static uint64_t observed_out = 0;

static void on_out(rax_engine *e, uint32_t port, uint32_t size, uint64_t value, void *user) {
    (void)e; (void)user;
    observed_out = value;
    printf("OUT port=0x%x size=%u value=0x%" PRIx64 "\n", port, size, value);
}

static uint64_t on_in(rax_engine *e, uint32_t port, uint32_t size, void *user) {
    (void)e; (void)user;
    printf("IN  port=0x%x size=%u -> supplying 0x42\n", port, size);
    return 0x42;
}

int main(void) {
    rax_engine *engine = NULL;
    CHECK(rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &engine));

    /*
     *   mov al, 0x41     B0 41
     *   out 0xE9, al     E6 E9
     *   in  al, 0xE9     E4 E9
     *   hlt              F4
     */
    const unsigned char code[] = { 0xB0, 0x41, 0xE6, 0xE9, 0xE4, 0xE9, 0xF4 };
    const uint64_t entry = 0x1000;
    CHECK(rax_mem_write(engine, entry, code, sizeof(code)));

    uint32_t in_id = 0, out_id = 0;
    CHECK(rax_hook_add_io_out(engine, on_out, NULL, &out_id));
    CHECK(rax_hook_add_io_in(engine, on_in, NULL, &in_id));

    CHECK(rax_emu_start(engine, entry, RAX_NO_ADDR, 0, 0));

    rax_exit ex;
    CHECK(rax_emu_last_exit(engine, &ex));
    uint64_t al = 0;
    CHECK(rax_reg_read_u64(engine, RAX_X86_REG_RAX, &al));
    al &= 0xFF;
    printf("reason=%d AL after IN = 0x%" PRIx64 "\n", ex.reason, al);

    int ok = (ex.reason == RAX_STOP_HLT) && (observed_out == 0x41) && (al == 0x42);
    rax_engine_close(engine);
    puts(ok ? "OK" : "FAILED");
    return ok ? 0 : 1;
}
