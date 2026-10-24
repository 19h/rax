.syntax unified
.arm
.global _start
_start:
    mrc p15,0,r0,c1,c0,2
    orr r0,r0,#0xf00000
    mcr p15,0,r0,c1,c0,2
    isb
    mov r0,#0x40000000
    vmsr fpexc,r0
    ldr r5,=output
    vmrs r0,mvfr0
    vmrs r1,mvfr1
    vmrs r2,mvfr2
    stmia r5!,{r0-r2}
    mov r7,#0
state:
    adr r9,arm_slot
    cmp r7,#0
    adrne r9,thumb_slot
    orrne r9,r9,#1
    ldr r4,=inputs
    ldr r6,=@@COUNT@@
next:
    ldr r0,[r4],#4
    blx r9
    stmia r5!,{r0-r2}
    subs r6,r6,#1
    bne next
    add r7,r7,#1
    cmp r7,#2
    bne state
    mov r0,#1
    ldr r1,=open_args
    svc #0x123456
    cmp r0,#-1
    beq failed
    ldr r1,=close_args
    str r0,[r1]
    ldr r1,=write_args
    str r0,[r1]
    mov r0,#5
    svc #0x123456
    cmp r0,#0
    bne failed
    mov r0,#2
    ldr r1,=close_args
    svc #0x123456
    cmp r0,#0
    bne failed
    mov r2,#0
    ldr r1,=success
    b finish
failed:
    mov r2,#1
    ldr r1,=failure
finish:
    mov r0,#4
    svc #0x123456
    ldr r1,=exit_args
    str r2,[r1,#4]
    mov r0,#0x20
    svc #0x123456
    b .
arm_slot:
    mov r12,#0x40000000
    vmsr fpexc,r12
    vmsr fpscr,r0
    vmrs r1,fpscr
    vmsr fpexc,r0
    vmrs r2,fpexc
    vmsr fpexc,r12
    bx lr
.thumb
thumb_slot:
    mov r12,#0x40000000
    vmsr fpexc,r12
    vmsr fpscr,r0
    vmrs r1,fpscr
    vmsr fpexc,r0
    vmrs r2,fpexc
    vmsr fpexc,r12
    bx lr
.arm
.ltorg
success:
    .asciz "RAX_FP_REGISTER_PROFILE_COMPLETE\n"
failure:
    .asciz "RAX_FP_REGISTER_PROFILE_FAILED\n"
filename:
    .asciz "results.bin"
.align 2
open_args:
    .word filename,5,11
write_args:
    .word 0,output,@@OUTPUT_BYTES@@
close_args:
    .word 0
exit_args:
    .word 0x20026,0
inputs:
    .incbin "input.bin"
.align 2
output:
    .space @@OUTPUT_BYTES@@
