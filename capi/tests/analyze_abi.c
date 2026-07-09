/* Strict C11 compile-time consumer for the rax 1.3 analysis ABI. */
#include <stddef.h>
#include <stdint.h>

#include "rax.h"

#ifdef __cplusplus
#  define RAX_TEST_STATIC_ASSERT(c, m) static_assert((c), m)
#else
#  define RAX_TEST_STATIC_ASSERT(c, m) _Static_assert((c), m)
#endif

RAX_TEST_STATIC_ASSERT(RAX_API_MAJOR == 1u, "unexpected ABI major");
RAX_TEST_STATIC_ASSERT(RAX_API_MINOR >= 3u, "analysis requires ABI 1.3+");
RAX_TEST_STATIC_ASSERT(RAX_ANALYSIS_ABI_VERSION == 1u, "unexpected analysis version");
RAX_TEST_STATIC_ASSERT(sizeof(rax_decoded) == 40u, "rax_decoded ABI drift");
RAX_TEST_STATIC_ASSERT(sizeof(rax_analysis) == 112u, "rax_analysis ABI drift");
RAX_TEST_STATIC_ASSERT(offsetof(rax_analysis, decoded) == 8u, "summary decoded offset");
RAX_TEST_STATIC_ASSERT(offsetof(rax_analysis, flags) == 48u, "summary flags offset");
RAX_TEST_STATIC_ASSERT(offsetof(rax_analysis, _reserved) == 80u, "summary reserve offset");
RAX_TEST_STATIC_ASSERT(sizeof(rax_analysis_effect) == 88u, "effect ABI drift");
RAX_TEST_STATIC_ASSERT(offsetof(rax_analysis_effect, access) == 8u, "effect access offset");
RAX_TEST_STATIC_ASSERT(offsetof(rax_analysis_effect, value) == 48u, "effect value offset");
RAX_TEST_STATIC_ASSERT(offsetof(rax_analysis_effect, _reserved) == 72u, "effect reserve offset");

static rax_status consume(const uint8_t *bytes, size_t length)
{
    rax_analysis summary;
    rax_analysis_effect effects[8];
    size_t required = 0u;
    rax_status status = rax_analyze(RAX_ARCH_X86, RAX_MODE_64, UINT64_C(0x1000),
                                    bytes, length, &summary, NULL, 0u, &required);
    if (status != RAX_OK || required > (sizeof(effects) / sizeof(effects[0]))) {
        return status;
    }
    return rax_analyze(RAX_ARCH_X86, RAX_MODE_64, UINT64_C(0x1000),
                       bytes, length, &summary, effects, required, &required);
}

int main(void)
{
    static const uint8_t instruction[] = { 0x48u, 0x89u, 0xd8u };
    return consume(instruction, sizeof(instruction)) == RAX_OK ? 0 : 1;
}
