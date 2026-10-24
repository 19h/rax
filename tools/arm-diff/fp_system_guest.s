.syntax unified
.arm
.global _start
.type _start,%function
_start:
    mrc p15,0,r0,c1,c0,2
    orr r0,r0,#0xf00000
    mcr p15,0,r0,c1,c0,2
    isb
    mov r0,#0x40000000
    vmsr fpexc,r0
    mov r0,#1
    ldr r1,=open_args
    svc #0x123456
    cmp r0,#-1
    beq failed_open
    mov r8,r0
    ldr r4,=cases
    ldr r5,=CASE_COUNT
case_loop:
    ldr r9,[r4]
    ldr r0,[r4,#4]
    cmp r9,#0
    adreq r1,arm_slot
    adrne r1,thumb_slot
    rorne r0,r0,#16
    str r0,[r1]
    orrne r1,r1,#1
    dsb sy
    isb
    add r0,r4,#12
    vldmia r0!,{d0-d15}
    vldmia r0!,{d16-d31}
    ldr r0,[r4,#8]
    vmsr fpscr,r0
    mov r10,r1
    ldr r0,[r4,#268]
    ldr r1,[r4,#272]
    ldr r2,[r4,#276]
    msr APSR_nzcvq,r2
    adr lr,after_slot
    bx r10
after_slot:
    mrs r3,cpsr
    ldr r2,=output
    ldr r12,=0xf8000000
    and r3,r3,r12
    str r3,[r2,#268]
    str r0,[r2,#260]
    str r1,[r2,#264]
    mov r0,r2
    vstmia r0!,{d0-d15}
    vstmia r0!,{d16-d31}
    vmrs r1,fpscr
    str r1,[r0]
    ldr r1,=write_args
    str r8,[r1]
    mov r0,#5
    svc #0x123456
    cmp r0,#0
    bne failed_write
    add r4,r4,#280
    subs r5,r5,#1
    bne case_loop
    ldr r1,=close_args
    str r8,[r1]
    mov r0,#2
    svc #0x123456
    cmp r0,#0
    bne failed_close
    ldr r1,=success
    mov r2,#0
    b finish
failed_open:
    mov r2,#1
    b failure
failed_write:
    mov r2,#2
    b failure
failed_close:
    mov r2,#3
failure:
    ldr r1,=failed
finish:
    mov r0,#4
    svc #0x123456
    ldr r1,=exit_args
    str r2,[r1,#4]
    mov r0,#0x20
    svc #0x123456
    b .
.align 2
arm_slot:
    .word 0xe1a00000
    bx lr
thumb_slot:
    .word 0xbf00bf00
    .hword 0x4770,0xbf00
.ltorg
success:
    .asciz "RAX_A32_CORPUS_OK\n"
failed:
    .asciz "RAX_A32_CORPUS_FAILED\n"
filename:
    .byte @@FILENAME_BYTES@@
filename_end:
    .byte 0
.align 2
open_args:
    .word filename,5,filename_end-filename
write_args:
    .word 0,output,272
close_args:
    .word 0
exit_args:
    .word 0x20026,0
output:
    .space 272
.include "cases.inc"
