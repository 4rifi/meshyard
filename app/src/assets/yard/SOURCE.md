# Dion's lobby map

`lobby-source.png` was provided by Dion Arifi on 2026-11-05 as his original,
AI-assisted artwork for Meshyard. The file is preserved byte for byte from
`Pixel-Art-Lobby mit Kristallbrunnen.png`. No existing game's assets are used.
Its source SHA-256 and exact processing parameters are recorded in processing.json.
The locally bundled Pixelify Sans font remains under its adjacent OFL license.

Reproduce with Pillow 11.3.0 in the APFS tooling environment:

```sh
source /Volumes/TokensAbuse/env.sh
/Volumes/TokensAbuse/tooling/image-tools/bin/python scripts/process-lobby-map.py
```

The selected 418×235 image samples a four-pixel source grid with nearest neighbour,
cropping only the final source row, and reduces to 96 median-cut colours without
dithering. The source has mixed AI-assisted block sizes rather than one exact
lattice. Review images compare unquantized nearest sampling and the selected
palette at the same scale. Native image generation never modifies the source.

`lobby-map.json` stores eleven entrance/feet-slot/label anchors and a drawn-path
waypoint graph. The plaza route follows an inner walkable ring, avoiding the crystal and
bench footprints. The robot monitor is decoration, never a station. Its tightly cropped native pixels form an occluder above moving avatars on the marked Fix transit edge, below HTML labels. No stopping slots occupy its mask. Coordinates
are logical native pixels. Review overlays and comparisons are generated under
`/Volumes/TokensAbuse/dist/map-review/`, separately from the shipped bitmap.
