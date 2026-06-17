# CultureAndArts

A terminal UI that probes your desktop environment, finds the ideal
[Google Arts & Culture](https://artsandculture.google.com) artwork to use as a
wallpaper, previews candidates inline using the
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/),
and downloads your pick to `~/Pictures` at the maximum available resolution.

## What it does

1. **Probes your desktop** — screen resolution, scale, compositor/DE, default
   font, cursor theme, and which bars/panels are running (`hyprctl`, `swaymsg`,
   `wlr-randr`, `xrandr`, `fc-match`, `pgrep`). Only signals it can actually
   read are used; nothing is invented.
2. **Discovers artworks** — crawls the Google Arts & Culture *related-artwork
   graph* starting from seed artworks. (Google Arts & Culture has no public
   search API, and its category pages are JavaScript-only, so the related-graph
   crawl is the reliable discovery path. Steer it toward a movement or era by
   pasting any artwork URL from it with `--seed`.)
3. **Ranks candidates** against your screen — aspect-ratio fit, resolution
   adequacy, and orientation.
4. **Filters** by era, minimum megapixels, and dominant-colour palette.
5. **Downloads at full resolution** by re-tooling the
   [dezoomify-rs](https://github.com/lovasoa/dezoomify-rs) tile engine: it
   resolves the tile pyramid, signs each tile request (HMAC-SHA1), decrypts the
   AES-128-CBC tiles, and stitches them into one image saved to `~/Pictures`.

## Usage

```sh
cargo build --release

./target/release/culture-and-arts --probe              # print detected desktop profile only
./target/release/culture-and-arts                      # launch the TUI
./target/release/culture-and-arts --era renaissance --min-mp 8
./target/release/culture-and-arts --color "#1b3a6b" --color-threshold 80
./target/release/culture-and-arts --seed https://artsandculture.google.com/asset/<slug>/<id>
```

Keys: `↑`/`↓` (or `k`/`j`) browse · `Enter` download the selected artwork ·
`q` quit.

Filters: `--era ancient|medieval|renaissance|baroque|romantic|modern|contemporary|unknown`
(repeatable), `--min-mp <megapixels>`, `--color "#rrggbb"`,
`--color-threshold <0-255>`, `--limit <N>`, `--seed <url>` (repeatable).

## Requirements

- A terminal that implements the kitty graphics protocol (kitty, Ghostty,
  WezTerm, Konsole) for true image previews; otherwise it falls back to
  half-block rendering.
- For the desktop probe: any of `hyprctl` / `swaymsg` / `wlr-randr` / `xrandr`,
  plus `fc-match` for font detection.

## Licence

GPL-3.0-or-later. The full-resolution download engine
(`src/download/`) is a port of the Google Arts & Culture backend from
[dezoomify-rs](https://github.com/lovasoa/dezoomify-rs), which is GPLv3;
re-tooling it places this project under the same licence.
