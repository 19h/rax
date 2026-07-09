/*
 * decode_flow.c — static, stateless single-instruction decode (rax_decode).
 *
 * rax_decode disassembles ONE instruction with no engine, no memory map and no
 * execution, projecting it to a small control-flow summary (length, flow class,
 * direct/indirect, resolved target). This example decodes a handful of x86-64
 * and AArch64 instructions and checks the classification, so `make test`
 * catches any regression in the decode surface.
 *
 * Build (from the repo root, after `cargo build -p rax-capi`):
 *   cc -std=c11 -I capi/include capi/examples/decode_flow.c \
 *      -L target/release -lrax -o decode_flow
 *   DYLD_LIBRARY_PATH=target/release ./decode_flow   # (LD_LIBRARY_PATH on Linux)
 */
#include "rax.h"
#include <inttypes.h>
#include <stdio.h>

static int failures = 0;

/* Decode `bytes` and assert the resulting fields. Prints and counts mismatches. */
static void expect(const char *name, int arch, uint32_t mode, uint64_t pc,
                   const unsigned char *bytes, size_t len, uint32_t exp_valid,
                   uint32_t exp_size, int32_t exp_flow, uint32_t exp_indirect,
                   uint32_t exp_has_target, uint64_t exp_target) {
    rax_decoded d;
    rax_status st = rax_decode(arch, mode, pc, bytes, len, &d);
    if (st != RAX_OK) {
        fprintf(stderr, "%s: rax_decode returned %s\n", name, rax_strerror(st));
        failures++;
        return;
    }
    int ok = d.valid == exp_valid && d.size == exp_size && d.flow == exp_flow &&
             d.is_indirect == exp_indirect && d.has_target == exp_has_target &&
             (!exp_has_target || d.target == exp_target);
    printf("%-18s valid=%u size=%u flow=%d indirect=%u has_target=%u target=0x%" PRIx64
           "  [%s]\n",
           name, d.valid, d.size, d.flow, d.is_indirect, d.has_target, d.target,
           ok ? "OK" : "FAILED");
    if (!ok) failures++;
}

int main(void) {
    /* Report the ABI version this library exposes (decode is since API 1.2). */
    uint32_t major = 0, minor = 0, patch = 0;
    rax_version(&major, &minor, &patch);
    printf("rax ABI %u.%u.%u — %s\n", major, minor, patch, rax_version_string());

    /* --- x86-64 (pc = 0x1000) --- */
    const unsigned char call_rel32[] = {0xE8, 0x00, 0x00, 0x00, 0x00};
    expect("x86 call rel32", RAX_ARCH_X86, RAX_MODE_64, 0x1000, call_rel32,
           sizeof(call_rel32), 1, 5, RAX_FLOW_CALL, 0, 1, 0x1005);

    const unsigned char icall[] = {0xFF, 0xD0}; /* call rax */
    expect("x86 indirect call", RAX_ARCH_X86, RAX_MODE_64, 0x1000, icall,
           sizeof(icall), 1, 2, RAX_FLOW_INDIRECT_CALL, 1, 0, 0);

    const unsigned char jmp[] = {0xEB, 0xFE}; /* jmp .-2 */
    expect("x86 jmp", RAX_ARCH_X86, RAX_MODE_64, 0x1000, jmp, sizeof(jmp), 1, 2,
           RAX_FLOW_BRANCH, 0, 1, 0x1000);

    const unsigned char je[] = {0x74, 0x05}; /* je .+5 */
    expect("x86 cond branch", RAX_ARCH_X86, RAX_MODE_64, 0x1000, je, sizeof(je),
           1, 2, RAX_FLOW_COND_BRANCH, 0, 1, 0x1007);

    const unsigned char ret[] = {0xC3};
    expect("x86 ret", RAX_ARCH_X86, RAX_MODE_64, 0x1000, ret, sizeof(ret), 1, 1,
           RAX_FLOW_RETURN, 0, 0, 0);

    const unsigned char ijmp[] = {0xFF, 0xE0}; /* jmp rax */
    expect("x86 indirect jmp", RAX_ARCH_X86, RAX_MODE_64, 0x1000, ijmp,
           sizeof(ijmp), 1, 2, RAX_FLOW_INDIRECT_JUMP, 1, 0, 0);

    const unsigned char nop[] = {0x90};
    expect("x86 nop", RAX_ARCH_X86, RAX_MODE_64, 0x1000, nop, sizeof(nop), 1, 1,
           RAX_FLOW_FALLTHROUGH, 0, 0, 0);

    /* --- AArch64 (little-endian, pc = 0x1000) --- */
    const unsigned char bl[] = {0x00, 0x00, 0x00, 0x94}; /* bl #0 */
    expect("arm64 bl", RAX_ARCH_ARM64, 0, 0x1000, bl, sizeof(bl), 1, 4,
           RAX_FLOW_CALL, 0, 1, 0x1000);

    const unsigned char blr[] = {0x00, 0x00, 0x3F, 0xD6}; /* blr x0 */
    expect("arm64 blr", RAX_ARCH_ARM64, 0, 0x1000, blr, sizeof(blr), 1, 4,
           RAX_FLOW_INDIRECT_CALL, 1, 0, 0);

    const unsigned char aret[] = {0xC0, 0x03, 0x5F, 0xD6}; /* ret */
    expect("arm64 ret", RAX_ARCH_ARM64, 0, 0x1000, aret, sizeof(aret), 1, 4,
           RAX_FLOW_RETURN, 0, 0, 0);

    const unsigned char b[] = {0x00, 0x00, 0x00, 0x14}; /* b #0 */
    expect("arm64 b", RAX_ARCH_ARM64, 0, 0x1000, b, sizeof(b), 1, 4,
           RAX_FLOW_BRANCH, 0, 1, 0x1000);

    const unsigned char anop[] = {0x1F, 0x20, 0x03, 0xD5}; /* nop */
    expect("arm64 nop", RAX_ARCH_ARM64, 0, 0x1000, anop, sizeof(anop), 1, 4,
           RAX_FLOW_FALLTHROUGH, 0, 0, 0);

    /* --- Argument validation --- */
    rax_decoded d;
    if (rax_decode(RAX_ARCH_X86, RAX_MODE_64, 0x1000, NULL, 4, &d) != RAX_ERR_ARG) {
        fprintf(stderr, "NULL bytes should return RAX_ERR_ARG\n");
        failures++;
    }
    if (rax_decode(99, RAX_MODE_64, 0x1000, nop, 1, &d) != RAX_ERR_ARCH) {
        fprintf(stderr, "bad arch should return RAX_ERR_ARCH\n");
        failures++;
    }

    printf("%s (%d failure%s)\n", failures ? "FAILED" : "OK", failures,
           failures == 1 ? "" : "s");
    return failures ? 1 : 0;
}
