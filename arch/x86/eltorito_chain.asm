bits 16
org 0x7c00

%define RELOC 0x0600
%define DELTA (RELOC - 0x7c00)
%define INNER_DISK_LBA 63
%define COM1 0x3f8
%define SECTORS_PER_TRACK 63
%define HEADS 255
%define SECTORS_PER_CYLINDER (SECTORS_PER_TRACK * HEADS)

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    cld

    mov [drive], dl
    call serial_init
    mov si, trace_entry
    call serial_puts

    ; Relocate below the conventional MBR address. The compatibility INT 13h
    ; shim installed below must remain resident while the bootloader stages use
    ; BIOS disk services.
    mov si, 0x7c00
    mov di, RELOC
    mov cx, 256
    rep movsw
    jmp 0x0000:(relocated + DELTA)

relocated:
    ; SeaBIOS deliberately exposes only old-style INT 13h commands for an
    ; El Torito emulated hard disk. bootloader 0.11 uses AH=42 extended reads,
    ; so translate those requests to CHS reads for this emulated drive.
    cli
    xor ax, ax
    mov es, ax
    mov bx, 0x13 * 4
    mov ax, [es:bx]
    mov [cs:old_int13 + DELTA], ax
    mov ax, [es:bx + 2]
    mov [cs:old_int13 + DELTA + 2], ax
    mov word [es:bx], int13_handler + DELTA
    mov word [es:bx + 2], 0
    sti

    mov si, trace_shim + DELTA
    call serial_puts

    ; Load the original Generic BIOS MBR through the same compatibility path
    ; that its later stages will use.
    mov dl, [drive + DELTA]
    mov si, dap + DELTA
    mov ah, 0x42
    int 0x13
    jc boot_error

    mov si, trace_read + DELTA
    call serial_puts
    mov dl, [drive + DELTA]
    jmp 0x0000:0x7c00

boot_error:
    mov si, trace_error + DELTA
    call serial_puts
.hang:
    cli
    hlt
    jmp .hang

; INT 13h AH=42 compatibility for the El Torito emulated hard disk. Other
; commands and other drives are chained directly to the firmware handler.
int13_handler:
    cmp ah, 0x42
    jne .chain
    cmp dl, [cs:drive + DELTA]
    jne .chain

    push bp
    mov bp, sp
    pushad
    push ds
    push es

    cmp byte [ds:si], 0x10
    jb .fail
    mov ax, [ds:si + 2]
    test ax, ax
    jz .success
    mov [cs:req_count + DELTA], ax
    mov ax, [ds:si + 4]
    mov [cs:req_offset + DELTA], ax
    mov ax, [ds:si + 6]
    mov [cs:req_segment + DELTA], ax
    mov eax, [ds:si + 12]
    test eax, eax
    jnz .fail
    mov eax, [ds:si + 8]
    mov [cs:req_lba + DELTA], eax

.next_sector:
    mov eax, [cs:req_lba + DELTA]
    xor edx, edx
    mov ecx, SECTORS_PER_CYLINDER
    div ecx
    cmp eax, 1023
    ja .fail
    mov [cs:req_cylinder + DELTA], ax

    mov eax, edx
    xor edx, edx
    mov ecx, SECTORS_PER_TRACK
    div ecx
    cmp eax, HEADS - 1
    ja .fail
    mov [cs:req_head + DELTA], al
    inc dl
    mov [cs:req_sector + DELTA], dl

    mov ax, [cs:req_cylinder + DELTA]
    mov ch, al
    shr ax, 2
    and al, 0xc0
    mov cl, [cs:req_sector + DELTA]
    or cl, al
    mov dh, [cs:req_head + DELTA]
    mov dl, [cs:drive + DELTA]
    mov bx, [cs:req_offset + DELTA]
    mov ax, [cs:req_segment + DELTA]
    mov es, ax
    mov ax, 0x0201
    pushf
    call far [cs:old_int13 + DELTA]
    jc .fail

    add dword [cs:req_lba + DELTA], 1
    add word [cs:req_offset + DELTA], 512
    dec word [cs:req_count + DELTA]
    jnz .next_sector

.success:
    and word [ss:bp + 6], 0xfffe
    jmp .done
.fail:
    or word [ss:bp + 6], 1
.done:
    pop es
    pop ds
    popad
    pop bp
    iret

.chain:
    jmp far [cs:old_int13 + DELTA]

serial_init:
    push ax
    push dx
    mov dx, COM1 + 1
    xor al, al
    out dx, al
    mov dx, COM1 + 3
    mov al, 0x80
    out dx, al
    mov dx, COM1
    mov al, 3
    out dx, al
    mov dx, COM1 + 1
    xor al, al
    out dx, al
    mov dx, COM1 + 3
    mov al, 0x03
    out dx, al
    mov dx, COM1 + 2
    mov al, 0xc7
    out dx, al
    mov dx, COM1 + 4
    mov al, 0x0b
    out dx, al
    pop dx
    pop ax
    ret

serial_puts:
    push ax
    push dx
.next:
    lodsb
    test al, al
    jz .done
    mov ah, al
.wait:
    mov dx, COM1 + 5
    in al, dx
    test al, 0x20
    jz .wait
    mov dx, COM1
    mov al, ah
    out dx, al
    jmp .next
.done:
    pop dx
    pop ax
    ret

drive:
    db 0
req_head:
    db 0
req_sector:
    db 0
align 2
old_int13:
    dw 0, 0
req_count:
    dw 0
req_offset:
    dw 0
req_segment:
    dw 0
req_cylinder:
    dw 0
req_lba:
    dd 0

align 4
dap:
    db 0x10
    db 0
    dw 1
    dw 0x7c00
    dw 0
    dq INNER_DISK_LBA

trace_entry:
    db "GENERIC CD: entry", 13, 10, 0
trace_shim:
    db "GENERIC CD: INT13 shim", 13, 10, 0
trace_read:
    db "GENERIC CD: MBR", 13, 10, 0
trace_error:
    db "GENERIC CD: read failed", 13, 10, 0

times 446 - ($ - $$) db 0

; A single partition makes the image valid for El Torito hard-disk emulation.
; SeaBIOS derives 1024/255/63 CHS geometry from this entry.
times 64 db 0
dw 0xaa55
