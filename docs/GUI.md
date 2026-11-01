# Desktop guide

Start `big-picture-gui` or `big-picture gui`. Both accept image paths. On Windows,
the GUI executable runs without a console window.

The Expand logo appears in the header and About window. The native window uses
the matching app icon; Windows GUI-enabled builds also embed the icon and version
information in both executable files. All artwork is bundled for offline use.

## Simple mode

The app opens in **Simple** mode by default. Use **Choose image…** (or drop an
image onto the window), choose a scale ratio, optionally choose a destination,
then click **Export 5 JPEGs**. Multiple selected images are also supported; the
button shows the total number of exports.

- The initial ratio is **2×**. Presets are **0.25×, 0.5×, 2×, 3×, 4×, and 8×**;
  custom ratios from 0.25× to 64× are available. Ratios below 1× make smaller images.
- On every launch, the destination resets to your system **Photos/Pictures**
  folder, honoring Windows known-folder redirection and Linux XDG user-directory
  settings. If it cannot be found, the app falls back to `Pictures` in your home
  folder, then the current folder, then `big-picture` in the system temporary folder.
  Missing folders are created when exporting. You can change the destination for
  the current session; leaving the field empty restores the system default.
- Every image is exported once with each available algorithm: Nearest,
  Bilinear, Catmull–Rom, Mitchell, and Lanczos 3.
- All outputs are **JPEG at quality 100**, with transparency composited on white.
  Smooth filters use linear light and gentle sharpening (amount 0.15); Nearest
  retains exact pixel edges without sharpening or color-space conversion.
- Names follow `<timestamp>-<original-name>-<algorithm>-<ratio>.jpeg`, such as
  `1793491200000000000-holiday-lanczos3-2x.jpeg`. The timestamp is UTC Unix
  nanoseconds shared across the batch, the original name excludes its extension,
  and ratios include `x` (including fractional values such as `2.5x`).
- Images and the selected preview **stay in the queue after export**, with
  saved/failed counts and an error where applicable. Use **Remove selected** or
  **Clear queue** when finished. Exporting again processes all queued images with
  all five algorithms and a new timestamp.
- After an export saves any files, **Open folder** in the status bar opens their
  folder in your default file manager. It remembers the actual export location
  even if you change the destination field afterward.
- Output collisions are rejected before writing. Simple never replaces existing
  outputs, even if replacement was enabled in Advanced.

The selected image is shown as a source thumbnail; Simple does not run a full
upscale before Export. Processing runs one output at a time. Simple does not
apply the app's pixel or memory caps and does not inherit Advanced resource
settings. Image dimensions must still fit the output format and the machine
must have enough available memory for full-resolution processing.

## Advanced mode

Switch to **Advanced** in the header for the existing preview, individual filter,
format, sizing, and resource controls. Its processing settings are kept separate
from Simple's recipes; the destination folder is shared between modes.

1. Use **Add images**, **Add folder**, or drag files/folders onto the window.
   Enable **Include subfolders** before adding a directory to scan recursively.
2. Choose a queued image. An initial preview is generated on a worker thread.
3. Choose a scale preset or width/height sizing mode, filter, optional linear
   light, and sharpening. Changing processing settings invalidates the old result.
   Selecting **0.25×** or **0.5×** also enables **Allow smaller outputs**.
4. Use **Build preview**. Drag the mint divider to compare the source on the
   left with the upscaled result on the right. Turn off **Comparison wipe** to
   show the result alone. Zoom and scroll to inspect the preview.
5. Choose an output format and folder. For JPEG, set quality; for JPEG/BMP,
   choose the transparency background. Output folder text can also be typed.
6. Use **Export N images**. Every queued image uses the current settings.
   The queue shows each saved path or error, and the status bar reports totals.
   Use **Open folder** to open the exported files' folder in your file manager.

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
Processing and resource settings persist; the output folder resets on launch.

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
