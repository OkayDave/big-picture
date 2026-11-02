# Image behavior and quality

## Choose a filter

| Filter | Good starting point | Tradeoff |
| --- | --- | --- |
| Nearest | Pixel art, sprites, hard grids | Repeats pixels; no smoothing. Use integer scales for even blocks. |
| Bilinear | Soft edges or fast drafts | Smooth and relatively soft. |
| Catmull–Rom | Crisp illustration and screenshots | Can ring around high-contrast edges. |
| Mitchell | Balanced general artwork | Slightly softer than sharp cubic filters. |
| Lanczos 3 | Photography and fine detail | Detailed conventional interpolation; may ring around edges. |

No filter recreates information that is missing from the input. Upscaling an
already compressed JPEG can enlarge compression artifacts. Save to PNG, TIFF,
or lossless WebP to avoid another lossy encode. Use mild sharpening; high
amounts exaggerate noise and halos.

## Alpha and color

The filter guidance describes the local engine. Optional desktop AI Enhancement
uses generative editing and may change or invent content, colours and alpha.
Transparent sources request transparent PNG output, without a guarantee that
every original alpha value or feature survives. Its final resizing and local
encoding use the same engine. See [the desktop guide](GUI.md#ai-enhancement-mode)
for cloud uploads, costs, size constraints and preservation limits.

Filtering uses premultiplied alpha internally, then restores straight alpha.
This prevents invisible color from contaminating partially transparent edges.
Sharpening preserves alpha and uses an alpha-aware color blur. PNG, WebP and
TIFF preserve transparency; JPEG and BMP flatten it onto the selected RGB matte
(white initially). Encoded-space compositing is used for these mattes.

The working and output depth is **8 bits per channel**. Higher-bit-depth inputs
are accepted but normalized; this version is not an archival HDR/16-bit tool.
Output is treated as sRGB. Embedded ICC profiles are not transformed or copied.
EXIF orientation is applied before sizing, and EXIF, XMP, ICC and other source
metadata are stripped. Do not use the app for a color-critical wide-gamut or
print workflow that requires profile preservation. It does not perform color
management, DPI conversion, or print-size calculation.

By default filters operate on encoded sRGB values. **Linear light** converts
assumed-sRGB RGB channels through the sRGB transfer function before filtering
and converts back afterward; alpha remains linear. This reduces dark bands at
bright/dark transitions but uses larger float buffers. It does not turn an
arbitrary profiled input into color-managed sRGB.

## Formats

| Format | Input | Output |
| --- | --- | --- |
| PNG | Still image, including 16-bit input | Lossless RGBA8; source metadata not copied |
| JPEG | EXIF orientation applied | RGB8, quality 1–100; alpha matte |
| WebP | Still images; animated WebP rejected | Lossless RGBA8 |
| TIFF | First image of multi-image TIFF; orientation applied | Single image, RGBA8 |
| BMP | Still images | RGB8; alpha matte |

GIF, SVG, AVIF, HEIC, RAW and HDR inputs are not supported. PNG decoding uses
the still/default image of APNG; APNG animation frames and multi-page TIFF
sequences are not processed. The app is for still images, not animation/video.
Converting inputs to a supported still-image format is a separate step.

## Large images and files

The engine validates dimensions and overflow before output allocation. CLI and
Advanced defaults limit source/output to 100 million pixels and estimate 512 MiB
of working memory. Simple does not apply these app-level pixel or memory caps;
available machine memory and codec dimension limits still apply.
The estimate includes decoded input, filter/alpha workspaces, encode buffers,
and optional sharpening; codec internals, allocator overhead, the GUI and OS
are outside this estimate. Decoder allocation limits are also set, but some
codecs only enforce them on a best-effort basis. These controls are safeguards,
not a sandbox or a strict process-memory cap.

One image is processed at a time, avoiding an entire batch in memory. SIMD
acceleration is selected by the resizing library where supported. This version
does not use GPU compute, tiling, or concurrent image jobs. If a legitimate image
exceeds a limit, raise the limit deliberately and ensure the machine has enough
RAM. If an image fails, its existing output stays intact and staging files are
cleaned up under normal error/cancellation handling.
