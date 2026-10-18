/* Scalar SMIR C ABI: a BTC operation and atomic buffer-capacity rejection. */
#include <stdio.h>
#include <string.h>
#include "rax.h"

int main(void)
{
    const unsigned char bytes[] = {0x48, 0x0f, 0xba, 0xf8, 0x19};
    rax_smir_instruction summary;
    rax_smir_op operations[16];
    if (rax_smir_lift_x64(0x1000, bytes, sizeof(bytes), &summary, NULL, 0) != RAX_OK ||
        summary.required_op_count > 16)
        return 1;
    memset(operations, 0x5a, sizeof(operations));
    if (rax_smir_lift_x64(0x1000, bytes, sizeof(bytes), &summary, operations, 0) !=
            RAX_ERR_BOUNDS ||
        summary.op_count != 0 || operations[0].opcode != 0x5a5a5a5au)
        return 1;
    if (rax_smir_lift_x64(0x1000, bytes, sizeof(bytes), &summary, operations, 16) != RAX_OK ||
        summary.flags != RAX_SMIR_COMPLETE)
        return 1;
    for (unsigned i = 0; i < summary.op_count; ++i)
        if (operations[i].opcode == RAX_SMIR_BTC)
        {
            if (operations[i].width_bits != 64 || operations[i].flags_mask != 1 ||
                operations[i].b.kind != 3 || operations[i].b.value != 0x19)
                return 1;
            puts("BTC scalar SMIR: OK");
            return 0;
        }
    return 1;
}
