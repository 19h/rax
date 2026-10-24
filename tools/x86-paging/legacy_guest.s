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
    testl $0x30000,%edx
    jnz fail
    movl $pf_handler,%eax
    movw %ax,idt+14*8
    movw $8,idt+14*8+2
    movw $0x8e00,idt+14*8+4
    shrl $16,%eax
    movw %ax,idt+14*8+6
    lidt idt_ptr
    movl $2,stage
    xorl %ecx,%ecx
identity:
    movl %ecx,%eax
    shll $12,%eax
    orl $3,%eax
    movl %eax,identity_pt(,%ecx,4)
    incl %ecx
    cmpl $1024,%ecx
    jne identity
    movl $identity_pt+3,page_directory
    movl $test_pt+7,page_directory+4
    movl $0x00300007,test_pt+12
    movl $0x11223344,0x00300020
    movl $0x55667788,0x00803020
    movl $0x99aabbcc,0x00301020
    movl $page_directory,%eax
    movl %eax,%cr3
    xorl %eax,%eax
    movl %eax,%cr4
    movl %cr0,%eax
    orl $0x80010000,%eax
    movl %eax,%cr0
    movl $3,stage
    cmpl $0x11223344,0x00403020
    jne fail
    testl $0x20,page_directory+4
    jz fail
    testl $0x20,test_pt+12
    jz fail
    testl $0x40,test_pt+12
    jnz fail
    movl $0x12345678,0x00403020
    testl $0x40,test_pt+12
    jz fail
    movl $4,stage
    movl $0x00800087,page_directory+4
    movl $0x10,%eax
    movl %eax,%cr4
    cmpl $0x55667788,0x00403020
    jne fail
    movl $0xaabbccdd,0x00403020
    movl page_directory+4,%eax
    andl $0x60,%eax
    cmpl $0x60,%eax
    jne fail
    movl $0xfe,observed_error
    movl $5,stage
    orl $0x200000,page_directory+4
    invlpg 0x00403020
    # QEMU TCG reports RSVD with P=0; Intel086 requires P=1.
    # This assertion records QEMU behavior, not RAX's required error code.
    movl $8,expected_error
    movl $after_reserved,expected_resume
    movl 0x00403020,%eax
    jmp fail
after_reserved:
    movl $8,stage
    movl $0x00300087,test_pt+12
    movl $test_pt+0x87,page_directory+4
    xorl %eax,%eax
    movl %eax,%cr4
    cmpl $0x12345678,0x00403020
    jne fail
    movl $0xfffff000,test_pt+12
    invlpg 0x00403020
    movl $0,expected_error
    movl $after_absent,expected_resume
    movl 0x00403020,%eax
    jmp fail
after_absent:
    movl $9,stage
    movl $0x00300005,test_pt+12
    invlpg 0x00403020
    movl $3,expected_error
    movl $after_wp,expected_resume
    movl $0xfaceface,0x00403020
    jmp fail
after_wp:
    movl $10,stage
    testl $0x40,test_pt+12
    jnz fail
    movl $0x00300007,test_pt+12
    invlpg 0x00403020
    cmpl $0x12345678,0x00403020
    jne fail
    movl %cr0,%eax
    andl $0x7fffffff,%eax
    movl %eax,%cr0
    movl $0x00301007,test_pt+12
    orl $0x80000000,%eax
    movl %eax,%cr0
    cmpl $0x99aabbcc,0x00403020
    jne fail
    cmpl $3,fault_count
    jne fail
    movl $success,%esi
    movl $0x10,%ebx
    jmp finish
fail:
    movw $0xe9,%dx
    movl stage,%eax
    addb $48,%al
    outb %al,%dx
    movl observed_error,%eax
    andb $15,%al
    addb $48,%al
    outb %al,%dx
    movl fault_count,%eax
    addb $48,%al
    outb %al,%dx
    movl $failure,%esi
    movl $0x11,%ebx
finish:
    movw $0xe9,%dx
print:
    lodsb
    testb %al,%al
    jz exit
    outb %al,%dx
    jmp print
exit:
    movw $0xf4,%dx
    movl %ebx,%eax
    outl %eax,%dx
    hlt
    jmp exit
pf_handler:
    pushal
    movl 32(%esp),%eax
    movl %eax,observed_error
    cmpl expected_error,%eax
    jne fail
    movl expected_resume,%eax
    movl %eax,36(%esp)
    incl fault_count
    popal
    addl $4,%esp
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
success:
    .asciz "RAX_X86_LEGACY_PAGING_OK\n"
failure:
    .asciz "RAX_X86_LEGACY_PAGING_FAILED\n"
.section .bss
.align 4096
page_directory:
    .skip 4096
identity_pt:
    .skip 4096
test_pt:
    .skip 4096
idt:
    .skip 2048
expected_error:
    .skip 4
expected_resume:
    .skip 4
fault_count:
    .skip 4
stage:
    .skip 4
observed_error:
    .skip 4
.align 16
    .skip 16384
stack_top:
