.section .multiboot,"a"
.align 4
.long 0x1badb002,0,-0x1badb002
.section .text
.code32
.global _start
_start:
    cli
    lgdt gdt_ptr
    ljmp $8,$protected
protected:
    movw $16,%ax
    movw %ax,%ds
    movw %ax,%es
    movw %ax,%ss
    movl $stack_top,%esp
    movl $1,stage
    movl $1,%eax
    cpuid
    testl $0x04000000,%ecx
    jz fail
    movl $2,stage
    movl $13,%eax
    xorl %ecx,%ecx
    cpuid
    movl %eax,observed
    movl %edx,observed+4
    cmpl $7,%eax
    jne fail
    testl %edx,%edx
    jnz fail
    movl $13,%eax
    movl $2,%ecx
    cpuid
    cmpl $256,%eax
    jne fail
    cmpl $576,%ebx
    jne fail
    movl $ud_handler,%eax
    movl $6,%ecx
    call install_gate
    movl $gp_handler,%eax
    movl $13,%ecx
    call install_gate
    lidt idt_ptr
    movl $cases,%esi
    movl $records,%edi
next_case:
    movl $3,stage
    movl %cr4,%eax
    orl $0x40000,%eax
    movl %eax,%cr4
    movl $3,%eax
    xorl %edx,%edx
    xorl %ecx,%ecx
    xsetbv
    movl $0,fault_vector
    movl $0,fault_error
    movl %cr4,%eax
    andl $0xfffbffff,%eax
    orl 0(%esi),%eax
    movl %eax,%cr4
    movl 4(%esi),%ecx
    movl 8(%esi),%eax
    movl 12(%esi),%edx
    movl $4,stage
instruction:
    xsetbv
after_instruction:
    movl $1,%eax
    cpuid
    andl $0x08000000,%ecx
    movl %ecx,16(%edi)
    movl %cr4,%eax
    orl $0x40000,%eax
    movl %eax,%cr4
    xorl %ecx,%ecx
    xgetbv
    movl %eax,8(%edi)
    movl %edx,12(%edi)
    movl $13,%eax
    xorl %ecx,%ecx
    cpuid
    movl %ebx,20(%edi)
    movl fault_vector,%eax
    movl %eax,0(%edi)
    movl fault_error,%eax
    movl %eax,4(%edi)
    addl $16,%esi
    addl $24,%edi
    incl completed
    cmpl $@CASE_COUNT@,completed
    jne next_case
    movl $header,%esi
    movl $12,%ecx
    call output
    movl $records,%esi
    movl $(@CASE_COUNT@*24),%ecx
    call output
    movl $trailer,%esi
    movl $8,%ecx
    call output
    movl $0x10,%eax
    jmp finish
fail:
    movl $stage,%esi
    movl $16,%ecx
    call output
    movl $0x11,%eax
finish:
    movw $0xf4,%dx
    outl %eax,%dx
    hlt
    jmp finish
output:
    movw $0xe9,%dx
1:
    lodsb
    outb %al,%dx
    loop 1b
    ret
install_gate:
    leal idt(,%ecx,8),%ebx
    movw %ax,0(%ebx)
    movw $8,2(%ebx)
    movw $0x8e00,4(%ebx)
    shrl $16,%eax
    movw %ax,6(%ebx)
    ret
ud_handler:
    pushl $0
    pushl $6
    jmp fault
gp_handler:
    pushl $13
fault:
    pushal
    cmpl $instruction,40(%esp)
    jne fail
    movl 32(%esp),%eax
    movl %eax,fault_vector
    movl 36(%esp),%eax
    movl %eax,fault_error
    movl $after_instruction,40(%esp)
    popal
    addl $8,%esp
    iret
.align 8
gdt:
    .quad 0,0x00cf9a000000ffff,0x00cf92000000ffff
gdt_ptr:
    .word 23
    .long gdt
idt_ptr:
    .word 2047
    .long idt
.section .rodata
header:
    .ascii "RAXXCR01"
    .long @CASE_COUNT@
trailer:
    .ascii "XCR0DONE"
.align 4
cases:
    .incbin "cases.bin"
.section .bss
.align 16
records:
    .skip @CASE_COUNT@*24
fault_vector:
    .skip 4
fault_error:
    .skip 4
completed:
    .skip 4
stage:
    .skip 4
observed:
    .skip 8
reserved_debug:
    .skip 4
idt:
    .skip 2048
.align 16
stack:
    .skip 16384
stack_top:
