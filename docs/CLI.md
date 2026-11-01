# Command-line guide

Examples below assume the binary is installed or `target/release` is on PATH.
Alternatively prefix arguments with `cargo run --release --`.

```sh
big-picture --help
big-picture upscale --help
big-picture info photo.jpg
big-picture info "images/*.png" --json
big-picture gui photo.jpg
big-picture licenses
big-picture completions bash > big-picture.bash
big-picture completions powershell > big-picture.ps1
```

Completions support Bash, Elvish, Fish, PowerShell and Zsh. The `gui` subcommand
is available only in GUI-enabled builds; the separate GUI executable also
accepts file/folder paths.

## Upscale

`big-picture upscale <INPUT>... [OPTIONS]`

An input can be a supported image, a directory, or a quoted glob pattern. Quote
paths with spaces and quote glob patterns so the app expands them consistently
across shells. Globs must match at least one input. Explicit unsupported inputs
are errors; directory scans skip unsupported extensions. File signatures, not
just extensions, determine decoding. Supported filename extensions are `.png`,
`.jpg`, `.jpeg`, `.webp`, `.tif`, `.tiff`, and `.bmp`, case-insensitively.

| Option | Behavior / default |
| --- | --- |
| `-s, --scale <NUMBER>` | Positive finite multiplier; defaults to 2. Conflicts with width/height. |
| `--width <PX>` | Preserve aspect ratio, limit width. |
| `--height <PX>` | Preserve aspect ratio, limit height. |
| `--width ... --height ...` | Largest image fitting inside the box, no crop or padding. |
| `--stretch` | Exact width AND height; both required. May distort. |
| `--filter <FILTER>` | `nearest`, `bilinear`, `catmull-rom`, `mitchell`, `lanczos3` (default). |
| `--color-space <SPACE>` | `srgb` (default) or `linear`; assumes sRGB input. |
| `--sharpen <AMOUNT>` | 0–3, default 0; preserves alpha. |
| `-o, --output <FILE>` | One input only; extension selects output codec. |
| `--output-dir <DIR>` | Batch output directory, created as needed. Conflicts with `-o`. |
| `--format <FORMAT>` | `png`, `jpeg`, `webp`, `tiff`, `bmp`; default PNG. Must agree with `-o` extension if both given. |
| `--quality <1-100>` | JPEG quality, default 92; does not affect other codecs. |
| `--background <RRGGBB>` | Alpha matte for JPEG/BMP, default `ffffff`; `#` is optional and should be quoted in shells. |
| `-r, --recursive` | Include directory descendants. Symlink directories are not followed. |
| `--allow-downscale` | Allow either dimension to decrease; otherwise smaller outputs fail. |
| `--overwrite` | Replace existing output files. Never permits replacing any source. |
| `--keep-going` | Continue after an individual image fails; final exit remains nonzero. |
| `--dry-run` | Check headers, dimensions, estimated resources and destinations; create no outputs or directories. |
| `--max-pixels <COUNT>` | Source AND output pixel limit, default 100,000,000. |
| `--memory-mib <MiB>` | Estimated working-memory limit, default 512 MiB. |
| `--json` | Machine-readable stdout; usable before or after subcommands. |

Fractional scale dimensions round to nearest integer, with a minimum of one
pixel per axis. Bounding-box dimensions
round down, with a minimum of one pixel. A scale of 1 is allowed. It is possible
to normalize a codec or remove metadata without changing dimensions.

## Batch naming and preflight

Without `-o`, output names are `<stem>-upscaled.<extension>`. The default
location is beside each source, or the requested output directory. Output
directories are flat, including recursive batches. If two inputs would map to
the same output (such as `a.jpg` and `a.png`), the entire plan fails before
writing. Use separate output folders or separate invocations for those inputs.

Input discovery is sorted and deduplicated. An output directory inside a source
directory is excluded from traversal. Source and output directories must differ
when scanning a folder. Outputs written beside inputs may be discovered on a
later directory scan; a separate output directory is recommended for batches.

Destinations, collisions and source identities are checked before any job
starts. Each image's header/size/decode is then checked when that job runs;
earlier successful outputs remain if a later job fails. A dry run validates
headers but is not a full corruption check; it does not decode the pixel stream.

## Automation and exit codes

```sh
big-picture upscale "photos/*.jpg" --output-dir large --scale 2 --json
big-picture upscale logo.png --scale 4 --background 182030 -o logo.jpg
big-picture upscale photo.png --color-space linear --sharpen 0.5 --memory-mib 1024 -o large.tiff
```

Upscale JSON contains `dry_run`, `planned`, `completed`, `failed`, `cancelled`
and `items`. Each item has paths, dimensions, `status`, optional `report`, and
optional `error`. A successful report includes input/output dimensions, file
bytes, and elapsed milliseconds. In dry runs, `completed` counts validated plans
and the item status is `planned`; no report is produced. On an early stop,
unvisited jobs have no item entry; `planned` still gives the whole batch size.
Discovery or global preflight errors produce `{"error":"..."}`. `info --json`
returns an array of header records. CLI parsing errors use clap's normal stderr
help/error text, including when `--json` was present.

| Exit | Meaning |
| --- | --- |
| 0 | All requested jobs succeeded (or dry-run plans validated). |
| 1 | Discovery, validation, decoding, encoding, or file operation failed. |
| 2 | Invalid CLI syntax or incompatible options. |
| 130 | Cancelled by Ctrl+C. |

Ctrl+C sets a cancellation flag. Processing stops between decode, resize,
encode and commit stages, not halfway through a codec or filter kernel. A
cancelled job has no committed output; earlier successful jobs remain. Do not
force-kill a process if you want this cleanup behavior.
