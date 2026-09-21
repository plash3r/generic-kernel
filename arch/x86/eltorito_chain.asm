bits 16
org 0x7c00

%define RELOC 0x0600
%define DELTA (RELOC - 0x7c00)
%define INNER_DISK_LBA 63
%define COM1 0x3f8

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    cld

    ; Preserve the El Torito emulated hard-disk number before diagnostics use
    ; DX. The copied byte moves with the rest of this sector during relocation.
    mov [drive], dl
    call serial_init
    mov si, trace_entry
    call serial_puts

    ; Move this wrapper MBR away from 0x7c00 so the original Generic MBR can
    ; be loaded into the conventional BIOS boot address without overwriting
    ; the instructions that perform the load.
    mov si, 0x7c00
    mov di, RELOC
    mov cx, 256
    rep movsw
    jmp 0x0000:(relocated + DELTA)

relocated:
    sti
    mov si, trace_relocated + DELTA
    call serial_puts

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

    mov si, message + DELTA
.print:
    lodsb
    test al, al
    jz .hang
    mov ah, 0x0e
    mov bx, 0x0007
    int 0x10
    jmp .print
.hang:
    cli
    hlt
    jmp .hang

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
trace_relocated:
    db "GENERIC CD: relocated", 13, 10, 0
trace_read:
    db "GENERIC CD: inner MBR loaded", 13, 10, 0
trace_error:
    db "GENERIC CD: INT13 read failed", 13, 10, 0
message:
    db "Generic CD BIOS chainload failed", 0

times 446 - ($ - $$) db 0

; tools/uefi_iso.py writes the single El Torito hard-disk-emulation partition
; entry here. The embedded Generic MBR starts at LBA 63.
times 64 db 0
dw 0xaa55
