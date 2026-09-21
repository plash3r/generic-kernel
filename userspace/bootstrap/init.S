.section .text
.global _start
.type _start,@function
_start:
    mov $2, %rax
    mov $1, %rdi
    lea user_message(%rip), %rsi
    mov $(user_message_end - user_message), %rdx
    int $0x80

    mov $1, %rax
    int $0x80
    mov %rax, %rdi
    mov $0, %rax
    int $0x80
    ud2

user_message:
    .ascii "GENERIC USER: /bin/init via validated write syscall\n"
user_message_end:
.size _start, .-_start
