# Fonts

The panel draws text with fonts from [U8g2](https://github.com/olikraus/u8g2)
through the `u8g2-fonts` crate (MIT/Apache-2.0). Each font keeps its own license;
only permissive ones are used (checked against U8g2's `LICENSE` file and its font
group pages, 2026-06-13).

| Font | Used for | License |
|---|---|---|
| FreeUniversal Bold `fub42_tf`, `fub30_tf` | temperature numerals | SIL Open Font License 1.1 (FreeUniversal © Stephen Wilson 2009, from SIL Sophia © SIL International) |
| FreeUniversal Regular `fur20_tf`, `fur14_tf`, `fur11_tf` | conditions, values, labels | SIL Open Font License 1.1 (as above) |
| Adobe Helvetica `helvR10_tf`, `helvR08_tf` (X11 BDF) | status line, times, captions | Adobe/X11 permissive license ("Permission to use, copy, modify, distribute and sell this software…") |

Logisoso, a common choice for numerals, is not used: U8g2 lists it
as OFL, but its own COPYING file says GPL v2 with a font exception.

`_tf` variants hold Latin-1, so `°`, `·`, `µ`, `³` and accented names render;
characters outside it are skipped rather than failing the draw.
