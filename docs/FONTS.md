# Console fonts

Generic's framebuffer console supports runtime font switching.

## Built-in presets

The default is `noto16`:

    font
    font list
    font set noto16
    font set noto20
    font set noto24
    font set bold16
    font set bold20
    font reset

Changing a preset immediately recalculates the console grid and clears the
framebuffer so that wrapping, scrolling and backspace use the new cell size.

## Custom PSF2 fonts

Generic can load a PSF2 bitmap font directly from VFS:

    font load /mnt/fonts/myfont.psf

or from the initramfs root:

    font load /fonts/myfont.psf

The PSF2 parser validates the header, dimensions, glyph count, per-glyph byte
size and backing data before the renderer switches fonts. Glyph dimensions are
currently limited to 64x64 pixels to keep malformed inputs from causing
unreasonable rendering work.

Generic supports the optional PSF2 Unicode table. When no Unicode mapping is
present, glyph indices are treated as direct character values, which is the
common layout for basic ASCII PSF fonts.

Custom PSF2 rendering is one-bit bitmap rendering. The built-in Noto Sans Mono
presets remain anti-aliased because their rasters store 8-bit pixel intensity.

## Putting a font into Generic

For a persistent font, place the PSF2 file somewhere on the GenericFS volume
mounted at `/mnt`.

For a font shipped with the OS image, add it under `initramfs/fonts/` before
building. The initramfs generator includes ordinary files recursively, so the
font becomes available at `/fonts/<name>.psf` on boot.

The current setting is runtime-only. A future user configuration layer can
store the selected preset or PSF2 path and apply it automatically at login.
