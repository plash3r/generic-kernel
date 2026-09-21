bits 16
org 0x7c00

%define RELOC 0x0600
%define DELTA (RELOC - 0x7c00)
%define INNER_DISK_LBA 63
%define SPT 63
%define HEADS 255
%define SPC (SPT * HEADS)

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    cld
    mov [drive], dl

    mov si, 0x7c00
    mov di, RELOC
    mov cx, 256
    rep movsw
    jmp 0:(relocated + DELTA)

relocated:
    ; SeaBIOS exposes only legacy INT 13h commands for an El Torito
    ; hard-disk-emulation drive. Keep a tiny resident shim that translates the
    ; bootloader's AH=42 LBA reads into AH=02 CHS reads.
    xor ax, ax
    mov es, ax
    mov bx, 0x13 * 4
    mov ax, [es:bx]
    mov [cs:old13 + DELTA], ax
    mov ax, [es:bx + 2]
    mov [cs:old13 + DELTA + 2], ax
    mov word [es:bx], int13 + DELTA
    mov word [es:bx + 2], 0
    sti

    mov dl, [drive + DELTA]
    mov si, dap + DELTA
    mov ah, 0x42
    int 0x13
    jc hang

    mov dl, [drive + DELTA]
    jmp 0:0x7c00

hang:
    cli
    hlt
    jmp hang

int13:
    cmp ah, 0x42
    jne chain
    cmp dl, [cs:drive + DELTA]
    jne chain

    push bp
    mov bp, sp
    pushad
    push ds
    push es

    cmp byte [ds:si], 0x10
    jb fail
    mov ax, [ds:si + 2]
    test ax, ax
    jz success
    mov [cs:count + DELTA], ax
    mov ax, [ds:si + 4]
    mov [cs:bufoff + DELTA], ax
    mov ax, [ds:si + 6]
    mov [cs:bufseg + DELTA], ax
    mov eax, [ds:si + 12]
    test eax, eax
    jnz fail
    mov eax, [ds:si + 8]
    mov [cs:lba + DELTA], eax

next:
    mov eax, [cs:lba + DELTA]
    xor edx, edx
    mov ecx, SPC
    div ecx
    cmp eax, 1023
    ja fail
    mov [cs:cyl + DELTA], ax

    mov eax, edx
    xor edx, edx
    mov ecx, SPT
    div ecx
    cmp eax, HEADS - 1
    ja fail
    mov [cs:head + DELTA], al
    inc dl
    mov [cs:sector + DELTA], dl

    mov ax, [cs:cyl + DELTA]
    mov ch, al
    shr ax, 2
    and al, 0xc0
    mov cl, [cs:sector + DELTA]
    or cl, al
    mov dh, [cs:head + DELTA]
    mov dl, [cs:drive + DELTA]
    mov bx, [cs:bufoff + DELTA]
    mov ax, [cs:bufseg + DELTA]
    mov es, ax
    mov ax, 0x0201
    pushf
    call far [cs:old13 + DELTA]
    jc fail

    inc dword [cs:lba + DELTA]
    add word [cs:bufoff + DELTA], 512
    dec word [cs:count + DELTA]
    jnz next

success:
    and word [ss:bp + 6], 0xfffe
    jmp done
fail:
    or word [ss:bp + 6], 1
done:
    pop es
    pop ds
    popad
    pop bp
    iret

chain:
    jmp far [cs:old13 + DELTA]

drive:  db 0
head:   db 0
sector: db 0
align 2
old13:  dw 0, 0
count:  dw 0
bufoff: dw 0
bufseg: dw 0
cyl:    dw 0
lba:    dd 0

align 4
dap:
    db 0x10, 0
    dw 1
    dw 0x7c00, 0
    dq INNER_DISK_LBA

times 446 - ($ - $$) db 0
times 64 db 0
dw 0xaa55
