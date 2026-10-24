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
    movl $pdpt+3,pml4
    movl $directories+3,%eax
    movl $pdpt,%edi
    movl $4,%ecx
1:
    movl %eax,(%edi)
    addl $4096,%eax
    addl $8,%edi
    loop 1b
    xorl %ecx,%ecx
2:
    movl %ecx,%eax
    shll $21,%eax
    orl $0x83,%eax
    movl %eax,directories(,%ecx,8)
    incl %ecx
    cmpl $2048,%ecx
    jne 2b
    movl $0x20,%eax
    movl %eax,%cr4
    movl $pml4,%eax
    movl %eax,%cr3
    movl $0xc0000080,%ecx
    rdmsr
    orl $0x100,%eax
    wrmsr
    movl %cr0,%eax
    orl $0x80000001,%eax
    movl %eax,%cr0
    ljmp $24,$long_mode
.code64
long_mode:
    movl $stack_top,%esp
    movl $gp_handler,%eax
    movw %ax,idt+13*16(%rip)
    movw $24,idt+13*16+2(%rip)
    movw $0x8e00,idt+13*16+4(%rip)
    shrl $16,%eax
    movw %ax,idt+13*16+6(%rip)
    lidt idt_ptr(%rip)
    movl $cases,%esi
    movl $records,%edi
    movl $0xfee00080,%ebx
next_case:
    movl $0,fault_vector(%rip)
    movl $0,fault_error(%rip)
    cmpl $0,(%rsi)
    jne read_case
    movl $0xa7,(%rbx)
    movq 8(%rsi),%rax
instruction:
    movq %rax,%cr8
after_instruction:
    movq %cr8,%rax
    jmp record
read_case:
    movq 8(%rsi),%rax
    movl %eax,(%rbx)
    movq %cr8,%rax
record:
    movq %rax,8(%rdi)
    movl (%rbx),%eax
    movl %eax,16(%rdi)
    movl fault_vector(%rip),%eax
    movl %eax,(%rdi)
    movl fault_error(%rip),%eax
    movl %eax,4(%rdi)
    addq $16,%rsi
    addq $24,%rdi
    incl completed(%rip)
    cmpl $@CASE_COUNT@,completed(%rip)
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
gp_handler:
    pushq %rax
    leaq instruction(%rip),%rax
    cmpq %rax,16(%rsp)
    jne fail
    movq 8(%rsp),%rax
    movl %eax,fault_error(%rip)
    movl $13,fault_vector(%rip)
    leaq after_instruction(%rip),%rax
    movq %rax,16(%rsp)
    popq %rax
    addq $8,%rsp
    iretq
.align 8
gdt:
    .quad 0,0x00cf9a000000ffff,0x00cf92000000ffff,0x00af9a000000ffff
gdt_ptr:
    .word 31
    .long gdt
idt_ptr:
    .word 4095
    .long idt,0
.section .rodata
header:
    .ascii "RAXCR801"
    .long @CASE_COUNT@
trailer:
    .ascii "CR8_DONE"
.align 8
cases:
    .incbin "cases.bin"
.section .bss
.align 4096
pml4:
    .skip 4096
pdpt:
    .skip 4096
directories:
    .skip 16384
idt:
    .skip 4096
records:
    .skip @CASE_COUNT@*24
fault_vector:
    .skip 4
fault_error:
    .skip 4
completed:
    .skip 4
.align 16
stack:
    .skip 16384
stack_top:
