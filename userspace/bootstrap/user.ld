ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);
}

SECTIONS
{
    . = 0x400000;
    .text : ALIGN(4096)
    {
        *(.text*)
    } :text
}
