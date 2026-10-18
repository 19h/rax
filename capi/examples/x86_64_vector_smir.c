/* Legacy XMM operands are distinct from scalar architectural registers. */
#include "rax.h"
#include <stdio.h>
#include <string.h>

int main(void)
{
    const unsigned char movq[] = {0x66, 0x48, 0x0f, 0x6e, 0xc2};
    const unsigned char aligned_store[] = {0x0f, 0x29, 0x01};
    rax_smir_instruction instruction;
    rax_smir_op ops[16];
    if (rax_smir_lift_x64(0x1000, movq, sizeof(movq), &instruction, ops, 16) != RAX_OK ||
        instruction.op_count != 1 || ops[0].opcode != RAX_SMIR_X86_MOVDQ ||
        ops[0].dst.kind != RAX_SMIR_OPERAND_XMM || ops[0].dst.id != 0 || ops[0].a.kind != 2 ||
        ops[0].a.id != 2 || ops[0].width_bits != 64)
        return 1;
    memset(ops, 0x5a, sizeof(ops));
    if (rax_smir_lift_x64(0x1000, aligned_store, sizeof(aligned_store), &instruction, ops, 1) !=
            RAX_ERR_BOUNDS ||
        instruction.op_count != 0 || ops[0].opcode != 0x5a5a5a5au)
        return 1;
    if (rax_smir_lift_x64(0x1000, aligned_store, sizeof(aligned_store), &instruction, ops, 16) !=
            RAX_OK ||
        instruction.op_count != 2 || ops[0].opcode != RAX_SMIR_CHECK_ALIGNMENT ||
        ops[0].auxiliary != 16 || ops[1].opcode != RAX_SMIR_VSTORE || ops[1].width_bits != 128)
        return 1;
    puts("legacy XMM SMIR and atomic capacity rejection: OK");
    return 0;
}
