bits 16
org 0x7c00

%define RELOC 0x0600
%define DELTA (RELOC - 0x7c00)
%define INNER_DISK_LBA 63

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    cld

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
    mov [drive + DELTA], dl
    mov si, dap + DELTA
    mov ah, 0x42
    int 0x13
    jc boot_error

    mov dl, [drive + DELTA]
    jmp 0x0000:0x7c00

boot_error:
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

message:
    db "Generic CD BIOS chainload failed", 0

times 446 - ($ - $$) db 0

; tools/uefi_iso.py writes the single El Torito hard-disk-emulation partition
; entry here. The embedded Generic MBR starts at LBA 63.
times 64 db 0
dw 0xaa55
