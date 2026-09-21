.section .text
.global _start
.type _start,@function
_start:
    mov $1, %rax
    int $0x80
    mov %rax, %rdi
    mov $0, %rax
    int $0x80
    ud2
.size _start, .-_start
