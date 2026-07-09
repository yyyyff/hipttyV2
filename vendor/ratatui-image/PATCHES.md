# Patches on ratatui-image 11.0.6

Upstream: [ratatui/ratatui-image](https://github.com/ratatui/ratatui-image) `11.0.6` (crates.io).

## hiptty: fix SlicedImage overflow when skip and drop are both non-zero

**Symptom (Windows Terminal, Sixel content images):** When a tall image is partially
scrolled so the top is above the viewport *and* the bottom is clipped, Sixel data
overshoots the render rect into the status bar / chrome. Further scrolls leave ghost pixels.

**Cause:** `SlicedSixelData::bands` computed

```text
take_bands = (size.height - drop) * font_height / 6
```

then `.skip(skip_bands).take(take_bands)`. When `skip > 0`, that still takes more bands
than the visible height (`size - skip - drop`), so the sixel sequence is taller than
`image_area`.

The generic `SlicedProtocol::Sliced` path had the same class of bug:

```text
.skip(skip).take(len - drop)  // wrong when skip > 0
```

**Fix:**

- Sixel: take bands for the visible row count (`area.height` / `size - skip - drop`).
- Sliced (iTerm2-style): `.take(visible_rows)` where `visible_rows == image_area.height`.

**Files:** `src/sliced.rs`

Remove this vendor when upstream ships an equivalent fix and bump the workspace dep.
