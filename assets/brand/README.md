<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroTERM brand

FerroTERM is the official name (Ferro for the Rust family shared with FerroEHR,
TERM for terminology). All of it is under the Business Source License 1.1 with the rest of the
repository.

## The mark

The mark is a concept graph: one concept at the centre with its related concepts
around it, joined by edges. It reads the same for any code system the server
handles, SNOMED CT, LOINC, or another, because every terminology is concepts and
the relationships between them. The top node is the cyan accent; the centre node
is the largest.

## Palette

| Token | Hex | Use |
|---|---|---|
| teal | `#0d9488` | primary mark, edges, accents |
| teal-deep | `#0f766e` | nodes, hover, pressed |
| cyan | `#22d3ee` | the accent node, highlights |
| ink | `#0f172a` | text on light |
| mist | `#f1f5f9` | text on dark |
| tile | `#0b1220` | dark tile background |

The values live in `tokens.css` as custom properties. On a dark tile the mark
brightens to teal `#14b8a6` edges and `#2dd4bf` nodes so it holds contrast.

## Contrast

WCAG 2.2 asks **4.5:1** for body text and **3:1** for a graphical object or
large text ([contrast minimum](https://www.w3.org/TR/WCAG22/#contrast-minimum),
[non-text contrast](https://www.w3.org/TR/WCAG22/#non-text-contrast)). Every
token is measured against both grounds this palette draws on, the light surface
and the dark tile. "Graphics" in the last column means the mark, a rule, or an
icon, and never a label.

| Token | Value | On #f8fafc | On #0b1220 | Safe for |
|---|---|---|---|---|
| `--ferroterm-teal` | `#0d9488` | 3.58 | 5.00 | light: graphics, dark: text |
| `--ferroterm-teal-deep` | `#0f766e` | 5.23 | 3.42 | light: text, dark: graphics |
| `--ferroterm-cyan` | `#22d3ee` | 1.73 | 10.36 | light: neither, dark: text |
| `--ferroterm-ink` | `#0f172a` | 17.06 | 1.05 | light: text, dark: neither |
| `--ferroterm-mist` | `#f1f5f9` | 1.05 | 17.09 | light: neither, dark: text |
| `--ferroterm-tile` | `#0b1220` | 17.89 | 1.00 | light: text, dark: neither |
| `--ferroterm-surface` | `#f8fafc` | 1.00 | 17.89 | light: neither, dark: text |

Three of these are safe for the mark and not for text, which is the distinction
the table exists to make:

- **Teal is a mark colour on light.** At 3.58:1 it clears the 3:1 a graphic
  needs and falls short of the 4.5:1 a label needs. The role token
  `--ferroterm-brand` is teal because the mark is teal; text in the brand colour
  takes `--ferroterm-brand-text`, which resolves to deep teal on light and to
  teal on the dark tile, where each reaches 4.5:1.
- **Deep teal is the mirror image.** It carries text on light at 5.23:1 and
  drops to 3.42:1 on the tile, where it is a graphic only.
- **Cyan is a highlight, not a colour to write in.** At 1.73:1 on the light
  surface it misses even the graphics threshold, so on light it is safe for
  neither; on the dark tile it reaches 10.36:1.

The mark itself is unchanged. It is a graphic, it passes 3:1 on both grounds,
and the same artwork is used by FerroHEALTH.

`scripts/checks/brand-contrast.sh` measures the values in `tokens.css` and fails
when this table or a token's own `safe:` comment stops matching what it
measures, so the figures here are checked rather than remembered. The `brand`
CI job runs it on every pull request.

## Files

| File | What it is |
|---|---|
| `ferroterm-icon.svg` | primary icon, full colour, transparent background |
| `ferroterm-icon-mono.svg` | one-colour icon, inherits `currentColor` |
| `ferroterm-icon-dark.svg` | the icon on a dark rounded tile |
| `ferroterm-lockup-light.svg` | icon and "FerroTERM" wordmark for light backgrounds |
| `ferroterm-lockup-dark.svg` | the lockup for dark backgrounds |
| `ferroterm-lockup-auto.svg` | the lockup that follows `prefers-color-scheme` |
| `favicon.svg` | the mark on a teal tile, for browser tabs |
| `favicon-32.png`, `favicon-16.png`, `favicon.ico` | raster favicons |
| `ferroterm-social.svg`, `ferroterm-social.png` | 1200x630 social card |
| `tokens.css` | the palette as CSS custom properties |

The wordmark is set in Bricolage Grotesque (700). Body copy pairs with IBM Plex
Sans, and data and labels with IBM Plex Mono. The lockup SVGs reference these
with a system-sans fallback; outline the wordmark to paths before any print use.

## Intrinsic size

Each SVG here carries two sizes that do different jobs. The `viewBox` is the
coordinate space the artwork is drawn in, and it stays as drawn. The `width` and
`height` attributes are the file's natural size, which is what a consumer that
rasterizes the file takes when nothing tells it how big to render. At a natural
size of 48 a package registry, a listing header, or an icon cache that wants
several hundred pixels stores a 48-pixel bitmap and shows it blurry. The artwork
is vector and reads at any size, so the two attributes are the only cap.

| File | `viewBox` | Natural size |
|---|---|---|
| `ferroterm-icon.svg` | `0 0 48 48` | 512 x 512 |
| `ferroterm-icon-mono.svg` | `0 0 48 48` | 512 x 512 |
| `ferroterm-icon-dark.svg` | `0 0 48 48` | 512 x 512 |
| `favicon.svg` | `0 0 32 32` | 32 x 32 |
| the three lockups | `0 0 184 56` | 184 x 56 |

The square icons ship at 512 so a rasterizing consumer gets a sharp bitmap.
`favicon.svg` keeps a small natural size, because a browser tab asks for a small
icon. The lockups keep their own natural width, because they are drawn to a
fixed aspect, so a consumer scales them by picking one dimension and letting the
other follow. Any square icon added here starts at 512.

## Usage

- Keep clear space around the mark equal to the diameter of one node.
- The smallest the mark reads is 16 px; below that use the favicon.
- Put the colour mark on light or quiet surfaces, and `ferroterm-icon-dark.svg` (the
  tile) on busy or light-photographic backgrounds.
- Use `ferroterm-icon-mono.svg` where one colour is required; it takes the
  surrounding text colour.
- Do not recolour the mark outside the palette, stretch it, add effects, or
  rebuild the wordmark in another typeface.

## Regenerating the rasters

The PNG and ICO files derive from the SVGs:

```bash
rsvg-convert -w 32 -h 32 favicon.svg -o favicon-32.png
rsvg-convert -w 16 -h 16 favicon.svg -o favicon-16.png
magick favicon-32.png favicon-16.png favicon.ico
rsvg-convert -w 1200 -h 630 ferroterm-social.svg -o ferroterm-social.png
```
