# Desktop guide

Start `big-picture-gui` or `big-picture gui`. Both accept image paths. On Windows,
the GUI executable runs without a console window.

## Simple mode

The app opens in **Simple** mode by default. Use **Choose image…** (or drop an
image onto the window), choose an upscale ratio, optionally choose a destination,
then click **Export 5 JPEGs**. Multiple selected images are also supported; the
button shows the total number of exports.

- The initial ratio is **2×**. Presets and custom ratios from 1× to 64× are available.
- The destination uses the saved export folder, or `~/Pictures/big-picture` if
  none has been chosen. Missing folders are created when exporting. Leaving the
  destination field empty restores the home-folder default.
- Every image is exported once with each available algorithm: Nearest,
  Bilinear, Catmull–Rom, Mitchell, and Lanczos 3.
- All outputs are **JPEG at quality 100**, with transparency composited on white.
  Smooth filters use linear light and gentle sharpening (amount 0.15); Nearest
  retains exact pixel edges without sharpening or color-space conversion.
- Names follow `<timestamp>-<original-name>-<algorithm>-<ratio>.jpeg`, such as
  `1793491200000000000-holiday-lanczos3-2x.jpeg`. The timestamp is UTC Unix
  nanoseconds shared across the batch, the original name excludes its extension,
  and ratios include `x` (including fractional values such as `2.5x`).
- Images leave the queue once **all five exports succeed**. Failed or cancelled
  images remain with their saved/failed counts and an error where applicable.
  Retrying exports all five algorithms again with a new timestamp.
- Output collisions are rejected before writing. Simple never replaces existing
  outputs, even if replacement was enabled in Advanced.

The selected image is shown as a source thumbnail; Simple does not run a full
upscale before Export. Processing runs one output at a time. Simple allows an
estimated working-memory budget of at least 8192 MiB for full-resolution,
linear-light processing; it uses the pixel limit from Advanced and honors a
higher Advanced memory budget. Resource checks still reject oversized images.

## Advanced mode

Switch to **Advanced** in the header for the existing preview, individual filter,
format, sizing, and resource controls. Its processing settings are kept separate
from Simple's recipes; the destination folder is shared between modes.

1. Use **Add images**, **Add folder**, or drag files/folders onto the window.
   Enable **Include subfolders** before adding a directory to scan recursively.
2. Choose a queued image. An initial preview is generated on a worker thread.
3. Choose a scale preset or width/height sizing mode, filter, optional linear
   light, and sharpening. Changing processing settings invalidates the old result.
4. Use **Build preview**. Drag the mint divider to compare the source on the
   left with the upscaled result on the right. Turn off **Comparison wipe** to
   show the result alone. Zoom and scroll to inspect the preview.
5. Choose an output format and folder. For JPEG, set quality; for JPEG/BMP,
   choose the transparency background. Output folder text can also be typed.
6. Use **Export N images**. Every queued image uses the current settings.
   The queue shows each saved path or error, and the status bar reports totals.

Queue operations, settings and previews are disabled while a worker is active.
The window keeps repainting and its Cancel button remains usable. Cancellation
finishes the current non-interruptible stage, then discards the unfinished
output. Batch exports continue through individual failures unless cancelled.
Closing during processing asks whether to cancel and close after that stage.

**Remove selected** and **Clear queue** remove queue entries only. They do not
delete source images or outputs. **Replace existing outputs** is off initially;
source images are always protected, even when enabled.

Settings, selected mode and each mode's scale are saved through eframe in the OS
application-data directory. The image queue and image pixels are not persisted.
Output-folder preferences persist, along with processing and resource settings.

## Preview limits

Advanced preview generation runs the full resize, then makes at most 1400×1400
display textures. This tests the actual filter path but the screen view is not a
pixel-exact 1:1 inspector for large images; zoom enlarges these display textures.
The source preview is sampled with nearest-neighbor display interpolation so
the original pixel grid can be compared with the filtered output. The preview
shows the resize before output encoding, including before JPEG compression and
JPEG/BMP background compositing. Export always writes the full target size.

The app currently uses a dark theme. AccessKit support is enabled, and controls
support keyboard focus. Native screen-reader coverage and every DPI/layout
combination still require manual device review. Minimum window size is 980×660;
the settings and queue scroll, and the side panels can be resized.

See [image behavior](IMAGE_BEHAVIOR.md) and [CLI batch naming](CLI.md) for codec,
color, metadata, collision, and resource details shared by both interfaces.
