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
    vmrs r0,mvfr0
    ubfx r0,r0,#12,#4
    cmp r0,#0
    bne failure
    mov r6,#0
    mov r7,#0
state:
    adr r9,arm_slot
    cmp r6,#0
    adrne r9,thumb_slot
    orrne r9,r9,#1
    mov r4,#0
controls:
    mov r5,#0
exception_byte:
    and r0,r4,#31
    lsl r0,r0,#22
    and r1,r4,#32
    orr r0,r0,r1,lsl#14
    ldr r1,=0xa800009f
    orr r0,r0,r1
    orr r0,r0,r5,lsl#8
    blx r9
    bic r2,r0,#0xff00
    cmp r1,r2
    bne failure
    add r7,r7,#1
    add r5,r5,#1
    cmp r5,#256
    bne exception_byte
    add r4,r4,#1
    cmp r4,#64
    bne controls
    add r6,r6,#1
    cmp r6,#2
    bne state
    cmp r7,#32768
    bne failure
    mov r2,#0
    ldr r1,=success
    b finish
failure:
    mov r2,#1
    ldr r1,=failed
finish:
    mov r0,#4
    svc #0x123456
    ldr r1,=exit_args
    str r2,[r1,#4]
    mov r0,#0x20
    svc #0x123456
    b .
arm_slot:
    vmsr fpscr,r0
    vmrs r1,fpscr
    bx lr
.thumb
thumb_slot:
    vmsr fpscr,r0
    vmrs r1,fpscr
    bx lr
.arm
.ltorg
success:
    .asciz "RAX_A32_NONTRAPPING_PROFILE_OK_32768\n"
failed:
    .asciz "RAX_A32_NONTRAPPING_PROFILE_FAILED\n"
.align 2
exit_args:
    .word 0x20026,0
