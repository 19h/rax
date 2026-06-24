/*
 * x86_64_memhook.c — per-access memory watchpoint hooks.
 *
 * Installs a memory hook that fires on every data read and write the guest
 * makes, reporting the address, size, and value. (Add RAX_HOOK_MEM_FETCH to
 * also observe instruction fetches.)
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>

#define CHECK(e) do { rax_status _s = (e); if (_s != RAX_OK) { \
    fprintf(stderr, "%s: %s\n", #e, rax_strerror(_s)); return 1; } } while (0)

static int g_reads = 0, g_writes = 0;

static void on_mem(rax_engine *e, int kind, uint64_t addr, uint32_t size,
                   uint64_t value, void *user) {
    (void)e; (void)user;
    const char *k = kind == RAX_MEM_READ ? "READ "
                  : kind == RAX_MEM_WRITE ? "WRITE"
                  : "FETCH";
    if (kind == RAX_MEM_READ)  g_reads++;
    if (kind == RAX_MEM_WRITE) g_writes++;
    printf("  %s addr=0x%" PRIx64 " size=%u value=0x%" PRIx64 "\n", k, addr, size, value);
}

int main(void) {
    rax_engine *engine = NULL;
    CHECK(rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &engine));

    /*
     *   mov rax, 0x11223344         48 C7 C0 44 33 22 11
     *   mov [0x2000], rax           48 89 04 25 00 20 00 00
     *   mov rbx, [0x2000]           48 8B 1C 25 00 20 00 00
     *   hlt                         F4
     */
    const unsigned char code[] = {
        0x48, 0xC7, 0xC0, 0x44, 0x33, 0x22, 0x11,
        0x48, 0x89, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00,
        0x48, 0x8B, 0x1C, 0x25, 0x00, 0x20, 0x00, 0x00,
        0xF4,
    };
    const uint64_t entry = 0x1000;
    CHECK(rax_mem_write(engine, entry, code, sizeof(code)));

    uint32_t id = 0;
    CHECK(rax_hook_add_mem(engine, RAX_HOOK_MEM_READ | RAX_HOOK_MEM_WRITE,
                           1, 0, /* begin>end => all addresses */
                           on_mem, NULL, &id));

    puts("memory accesses:");
    CHECK(rax_emu_start(engine, entry, RAX_NO_ADDR, 0, 0));

    uint64_t rbx = 0;
    CHECK(rax_reg_read_u64(engine, RAX_X86_REG_RBX, &rbx));
    printf("reads=%d writes=%d RBX=0x%" PRIx64 "\n", g_reads, g_writes, rbx);

    int ok = (g_writes == 1) && (g_reads == 1) && (rbx == 0x11223344);
    rax_engine_close(engine);
    puts(ok ? "OK" : "FAILED");
    return ok ? 0 : 1;
}
