/*
 * mem_and_context.c — sparse mapping, region enumeration, and snapshots.
 *
 * Demonstrates mapping memory at an arbitrary high address, host read/write,
 * enumerating the region table, and a context save/restore round-trip.
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(e) do { rax_status _s = (e); if (_s != RAX_OK) { \
    fprintf(stderr, "%s: %s\n", #e, rax_strerror(_s)); return 1; } } while (0)

int main(void) {
    rax_engine *engine = NULL;
    CHECK(rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &engine));

    /* Map a fresh region far away from the default low RAM. */
    const uint64_t hi = 0x4000000000ULL; /* 256 GiB */
    CHECK(rax_mem_map(engine, hi, 0x2000, RAX_PROT_READ | RAX_PROT_WRITE));

    unsigned char pattern[8] = { 1, 2, 3, 4, 5, 6, 7, 8 };
    CHECK(rax_mem_write(engine, hi, pattern, sizeof(pattern)));
    unsigned char back[8] = { 0 };
    CHECK(rax_mem_read(engine, hi, back, sizeof(back)));
    if (memcmp(pattern, back, sizeof(pattern)) != 0) {
        fprintf(stderr, "high-address read/write mismatch\n");
        return 1;
    }

    /* Enumerate the region table (two-call: count then fill). */
    size_t n = 0;
    CHECK(rax_mem_regions(engine, NULL, &n));
    rax_mem_region *regs = (rax_mem_region *)calloc(n, sizeof(*regs));
    CHECK(rax_mem_regions(engine, regs, &n));
    printf("%zu region(s):\n", n);
    for (size_t i = 0; i < n; i++) {
        printf("  [%zu] base=0x%" PRIx64 " size=0x%" PRIx64 " perms=%u\n",
               i, regs[i].base, regs[i].size, regs[i].perms);
    }
    free(regs);

    /* Set a register, snapshot, clobber it, restore, verify it returns. */
    CHECK(rax_reg_write_u64(engine, RAX_X86_REG_RBX, 0xCAFEF00DULL));

    size_t need = 0;
    CHECK(rax_context_save(engine, NULL, 0, &need));
    void *blob = malloc(need);
    CHECK(rax_context_save(engine, blob, need, &need));
    printf("context blob = %zu bytes\n", need);

    CHECK(rax_reg_write_u64(engine, RAX_X86_REG_RBX, 0)); /* clobber */
    CHECK(rax_context_restore(engine, blob, need));

    uint64_t rbx = 0;
    CHECK(rax_reg_read_u64(engine, RAX_X86_REG_RBX, &rbx));
    free(blob);
    printf("RBX after restore = 0x%" PRIx64 " (expected 0xcafef00d)\n", rbx);

    int ok = (rbx == 0xCAFEF00DULL);
    rax_engine_close(engine);
    puts(ok ? "OK" : "FAILED");
    return ok ? 0 : 1;
}
