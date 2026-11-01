# Validation record

This file records executed checks, separately from the checks configured in CI.

Local environment: Windows x86_64, Rust 1.95.0, also checked with Rust 1.88.0.
Scope: conventional upscaling, offline. Validation completed 1 November 2026.

## Automated checks

| Check | Result |
| --- | --- |
| `cargo test --all-targets --all-features --locked` | 56 tests passed: 13 GUI state/workflow/planning, 10 batch, 11 CLI, 22 engine |
| `cargo test --no-default-features --locked` | 43 integration tests passed, plus doc-test runner |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | Passed |
| `cargo fmt --check` | Passed |
| `cargo doc --no-deps --all-features --locked` | Passed |
| `cargo +1.88.0 check --all-targets --all-features --locked` | Passed on Windows |
| `cargo deny --all-features check licenses bans sources` (0.20.2) | Passed |
| `cargo about generate --all-features --locked docs/licenses.hbs -o THIRD_PARTY_NOTICES.html` (0.9.3) | Passed; notices generated |
| `cargo build --release --locked --bins` | Both Windows release executables built |

The test suite covers actual codec output and pixel/alpha behavior, all filters
in both color spaces, dimension boundaries and invalid settings, source/hard-link
protection, output collisions, safe writes and cancellation flags, metadata
orientation, 16-bit normalization, CLI subprocess contracts, partial failures,
GUI background discovery/preview/export, and worker-error recovery. Fixtures are
generated locally; no private user images are required.

Simple-mode coverage additionally verifies migration of saved settings, default
and remembered destinations, separate mode/scale persistence, all five actual
quality-100 JPEG outputs (including quantization tables), dimensions, timestamped
filenames and fractional ratios, source preservation, no-overwrite/collision
preflight, successful-image removal, retained errors and retries, and queue
cleanup after partial cancellation. Both modes render headlessly. The license
check and notice generation were rerun with the locally cached 0.20.2/0.9.3 tools;
generated notices match the existing file byte for byte (no dependency changes).

## Interactive Windows review

Launched the actual desktop executable with the original landscape generated
by `examples/generate_sample.rs`. Verified initial preview, selection of the 4×
preset invalidating the old result, rebuilding the comparison, setting an output
folder by keyboard, and exporting through the GUI. The queue reported one saved
image and zero failures. Independent CLI inspection confirmed the exported PNG
was 960×640 from the 240×160 source. Dragging the comparison divider visibly
changed the before/after split. The accessibility tree exposed controls, settings,
dimensions, saved status and output paths.

For Simple, launched the updated Windows app, confirmed the saved destination
and initial 2× ratio, chose the generated sample through the native file picker,
and exported into an isolated destination. The UI reported **5 saved · 0 failed**
and an empty queue. All five algorithm-named `.jpeg` files were present. Simple
shows a source thumbnail and omits Advanced's preview/filter/codec controls.

The Windows portable archive is created by `scripts/package.ps1`; it contains
both executables, application license, generated third-party notices, verbatim
font notices and the user documentation. A SHA-256 sidecar accompanies it.
The archive was extracted into a separate temporary build folder, required
entries and the checksum were checked, and its CLI reported version 0.1.0 and
successfully exported a 720×480 PNG at 3× from the 240×160 sample. The extracted
GUI executable's hash matches the release executable reviewed interactively.

## Remaining platform / device review

Linux/macOS CI and manual native-window review have not run locally. Workflows
are configured but no hosted CI result is claimed. Code-signing, installers,
screen-reader behavior, all display scaling combinations, system-level Ctrl+C
delivery on every terminal, very large real-world images, and network-filesystem
atomicity still need release/device testing. The unit/integration cancellation
checks validate the engine's cancellation contract, not every OS signal path.

See IMAGE_BEHAVIOR.md for intentional still-image, metadata, color-depth,
color-management and preview limitations. No AI quality claim is made.
