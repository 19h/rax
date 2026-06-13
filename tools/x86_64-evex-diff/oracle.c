#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define WIRE_MAGIC 0x58455645u /* 'E','V','E','X' */

enum {
    CASE_VMOVAPS_ZMM0 = 0,
    CASE_VMOVAPS_ZMM8 = 1,
    CASE_VMOVAPS_ZMM16 = 2,
    CASE_VMOVAPS_ZMM24 = 3,
    CASE_VADDPS_ZMM16 = 4,
    CASE_VADDPS_ZMM24 = 5,
};

struct in_case {
    uint32_t id;
    uint32_t reserved;
    uint64_t zmm0[8];
    uint64_t zmm2[8];
    uint64_t zmm8[8];
    uint64_t zmm16[8];
    uint64_t zmm24[8];
};

struct out_case {
    uint32_t id;
    uint32_t valid;
    uint64_t result[8];
};

#define RUN_OP(OP)                                                                     \
    __asm__ volatile(                                                                  \
        "vmovups (%[z0]), %%zmm0\n\t"                                                  \
        "vmovups (%[z2]), %%zmm2\n\t"                                                  \
        "vmovups (%[z8]), %%zmm8\n\t"                                                  \
        "vmovups (%[z16]), %%zmm16\n\t"                                                \
        "vmovups (%[z24]), %%zmm24\n\t"                                                \
        OP "\n\t"                                                                      \
        "vmovups %%zmm1, (%[res])\n\t"                                                 \
        "vzeroupper\n\t"                                                              \
        :                                                                              \
        : [z0] "r"(zmm0), [z2] "r"(zmm2), [z8] "r"(zmm8), [z16] "r"(zmm16),          \
          [z24] "r"(zmm24), [res] "r"(result)                                         \
        : "zmm0", "zmm1", "zmm2", "zmm8", "zmm16", "zmm24", "memory")

static void execute_case(const struct in_case *in, struct out_case *out) {
    _Alignas(64) uint64_t zmm0[8];
    _Alignas(64) uint64_t zmm2[8];
    _Alignas(64) uint64_t zmm8[8];
    _Alignas(64) uint64_t zmm16[8];
    _Alignas(64) uint64_t zmm24[8];
    _Alignas(64) uint64_t result[8] = {0};

    memcpy(zmm0, in->zmm0, sizeof(zmm0));
    memcpy(zmm2, in->zmm2, sizeof(zmm2));
    memcpy(zmm8, in->zmm8, sizeof(zmm8));
    memcpy(zmm16, in->zmm16, sizeof(zmm16));
    memcpy(zmm24, in->zmm24, sizeof(zmm24));

    out->id = in->id;
    out->valid = 1;

    switch (in->id) {
    case CASE_VMOVAPS_ZMM0:
        RUN_OP("vmovaps %%zmm0, %%zmm1");
        break;
    case CASE_VMOVAPS_ZMM8:
        RUN_OP("vmovaps %%zmm8, %%zmm1");
        break;
    case CASE_VMOVAPS_ZMM16:
        RUN_OP("vmovaps %%zmm16, %%zmm1");
        break;
    case CASE_VMOVAPS_ZMM24:
        RUN_OP("vmovaps %%zmm24, %%zmm1");
        break;
    case CASE_VADDPS_ZMM16:
        RUN_OP("vaddps %%zmm16, %%zmm2, %%zmm1");
        break;
    case CASE_VADDPS_ZMM24:
        RUN_OP("vaddps %%zmm24, %%zmm2, %%zmm1");
        break;
    default:
        out->valid = 0;
        break;
    }

    memcpy(out->result, result, sizeof(result));
}

int main(void) {
    uint32_t header[2];
    if (fread(header, sizeof(header), 1, stdin) != 1) {
        return 2;
    }
    if (header[0] != WIRE_MAGIC) {
        return 3;
    }

    if (fwrite(header, sizeof(header), 1, stdout) != 1) {
        return 4;
    }

    for (uint32_t i = 0; i < header[1]; i++) {
        struct in_case in;
        struct out_case out;
        if (fread(&in, sizeof(in), 1, stdin) != 1) {
            return 5;
        }
        execute_case(&in, &out);
        if (fwrite(&out, sizeof(out), 1, stdout) != 1) {
            return 6;
        }
    }

    return 0;
}
