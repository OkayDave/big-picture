# Validation record

This file records executed checks, separately from the checks configured in CI.

## Live Merlin AI enhancement — 2 November 2026

- With explicit user authorization, made exactly one live OpenAI image-edit
  request using the bundled 1200×900 Merlin sample and the saved Windows
  credential. The credential was loaded internally, never displayed or logged.
  Used Standard quality (`medium`), 2× output, Light cleanup/detail, Original
  colours, preserved exposure, Subtle enhancement and no extra prompt.
- The real `ai::generate` backend requested a 2176×1632 image from
  `gpt-image-2.5-sunburst`, then locally resized to 2400×1800. Saved a 3,603,470-byte
  JPEG through the same `engine::save_pixels` path used by the GUI, with quality
  100 verified from the encoded quantization tables and the JPEG file signature.
- Visually reviewed the result against the source: fur and blanket detail are
  clearer; the subject, pose, framing and overall colours look consistent. This
  is an example of generative enhancement, not proof of exact pixel/identity
  preservation on all inputs.
- UI automation was interrupted by concurrent-input detection before generation,
  so the live check used a temporary local harness calling the existing backend
  and encoder. The harness was removed after success. This verifies the saved-key
  to OpenAI to JPEG path; a complete live Generate/Save journey through native
  controls was not exercised. No additional paid requests were made or retried.
- The JPEG was saved locally as `merlin-ai-enhanced-standard-2026-11-02.jpg`;
  the generated output and credentials are not included in the repository.
- Final Windows checks passed: all 70 ordinary tests, 43 CLI-only tests,
  formatting, Clippy with warnings denied, documentation, Rust 1.88 all-target/
  all-feature checking, and dependency licenses/bans/sources. The existing native
  credential smoke test also passed earlier with a disposable dummy entry.

## Merlin sample — 2 November 2026

- Replaced the generated pixel landscape with the supplied Merlin photograph in
  `docs/samples/merlin.jpg`. Used the local engine for an aspect-preserving
  1200×900 quality-100 JPEG, applying source orientation and omitting metadata.
  The original photo's SHA-256 was unchanged after preparation. No AI API call
  or upload was made.
- The existing `generate_sample` example now exports the bundled photo. Ran it
  successfully with the CLI-only feature set to refresh `target/sample.png`.
  Verified actual JPEG/PNG signatures and 1200×900 dimensions; inspected the
  sample visually and confirmed the bundled JPEG has no EXIF/XMP/ICC/comment
  segments. Formatting and all-target/all-feature Clippy checks passed.
- Existing portable packaging already includes the complete `docs` directory,
  so future packages include Merlin under `docs/samples`. Existing archives and
  the historical screenshots/validation records were not replaced.

## Remembered API keys and explicit AI export format — 2 November 2026

- Added opt-in OS credential storage through keyring 3.6.3: Windows Credential
  Manager, macOS Keychain and Linux Secret Service. Keys remain outside eframe
  preferences; only the remember flag is persisted. Startup reload does not enable
  upload consent. Update, Load and Forget actions run on a separate worker, with
  sanitized errors and a session-only fallback when secure storage is unavailable.
- AI export now has a PNG/JPEG/WebP selector. The filename extension is matched
  to the selected format before source protection and collision checks. JPEG is
  quality 100; PNG and WebP remain lossless. The user-supplied export was inspected
  read-only and has a real PNG signature, so a JPEG/PNG byte mismatch was not
  reproduced. No existing user image was changed.
- All 70 ordinary tests passed (27 library/GUI/AI, 10 batch, 11 CLI, 22 engine),
  using `cargo test --all-targets --all-features --locked --profile ai-credentials
  --config 'profile.ai-credentials.inherits="dev"'`. One native-store test is
  ignored by default so ordinary tests cannot access the user's vault.
- Explicitly ran that native test on Windows: a unique disposable dummy
  credential was saved, read through fresh handles, removed and confirmed absent.
  No real API key was read or written. Mock-store tests cover replacement, absent
  entries and backend errors containing private bytes; GUI state tests cover
  remember/load/forget failures and ensure secrets never enter preferences/status.
- Export regressions inspect actual PNG/JPEG/WebP signatures and JPEG quantization
  tables at quality 100, including mismatched requested extensions and collisions
  at the corrected destination. Saving continues to work with no API key.
- CLI-only tests (43), formatting, Clippy with warnings denied, documentation and
  Rust 1.88 checks passed. Dependency license/bans/sources checks passed without
  changing the allowlist; distribution notices were regenerated. Linux build/CI
  prerequisites now include libdbus-1-dev.
- Both review executables were built under `target/ai-credentials`. No paid API
  calls were made. Linux/macOS credential stores have not been exercised locally.

## Single-image AI Enhancement — 2 November 2026

- Added a third desktop mode with session-only masked key entry, explicit
  cloud/billing acknowledgement, enhancement controls and separate Generate/Save.
  Default model: `gpt-image-2.5-sunburst`, verified against OpenAI's image-edit API
  documentation. No paid API call or user image upload was performed for validation.
- All 66 tests passed (23 library/GUI/AI, 10 batch, 11 CLI, 22 engine), using
  `cargo test --all-targets --all-features --locked --profile ai-update
  --config 'profile.ai-update.inherits="dev"'`. A separate profile avoids replacing
  existing running executables. Review binaries are under `target/ai-update`.
- A local HTTP mock exercised the real multipart edit request, PNG upload,
  model/quality selection, response decoding and final dimensions. Regression
  tests cover settings migration, no persisted keys/prompts/consent, source-only
  loading, no generation without consent, local PNG/JPEG/WebP saves without a
  key, retained result after control changes/failure/cancellation/mode switching,
  output collisions and source protection. AI controls render headlessly at
  the minimum 980×660 window size.
- CLI-only tests passed (43 tests), as did formatting, Clippy with warnings
  denied, documentation generation and Rust 1.88 all-target/all-feature checking.
- cargo-deny 0.20.2 passed licenses/bans/sources. Refreshed distribution notices
  with cargo-about 0.9.3. HTTPS uses rustls with native trusted roots; networking
  dependencies are excluded from CLI-only builds. No new license allowlist entry.
- Native Windows review confirmed the third mode and enhancement controls,
  masked key field, explanatory cloud/cost text and disabled Generate/Save before
  setup. Loading the local 240×160 sample showed a source-only preview and a
  480×320 requested output, with a separate 1248×832 AI render plan and no upload.
  Live OpenAI model access, billing and image quality remain unverified.
  Linux/macOS native UI and HTTP operation have not been reviewed locally.
- This is source implementation and a local review build, not a published release
  or a replacement for the earlier portable package. API keys are session-only;
  optional OS-vault storage, batch enhancement and selective editing are deferred.

Local environment: Windows x86_64, Rust 1.95.0, also checked with Rust 1.88.0.
Scope: conventional upscaling, offline. Validation completed 1 November 2026.

## Expand branding and README update — 1 November 2026

- Embedded the approved logo in the desktop header and About window, the PNG
  icon in the native viewport, and ICO/version resources in Windows executables
  for GUI-enabled builds. CLI-only builds do not enable the resource dependency.
- `cargo build --locked --bins` passed. Native Windows review confirmed the
  branded header, matching title-bar icon and working sample-image comparison.
  Executable metadata reports Big Picture and version 0.1.0. The refreshed
  `docs/screenshots/desktop.png` is a real app capture using the original sample
  generated by `examples/generate_sample.rs`.
- All 58 tests passed (15 GUI, 10 batch, 11 CLI, 22 engine). CLI-only tests,
  formatting, Clippy with warnings denied, documentation and the Rust 1.88
  minimum-version check passed with the updated dependency lockfile.
- License/bans/sources checks passed using the cached cargo-deny 0.20.2 binary.
  Distribution notices were regenerated with cached cargo-about 0.9.3.
- README artwork includes light/dark SVG variants with outlined lettering.
  Logo assets remain bundled in the existing portable-package docs tree.
  Linux/macOS native branding has not been reviewed locally.

## Simple resource limits update — 1 November 2026

- Removed Simple's application-level source/output pixel cap, estimated working
  memory cap, and decoder memory budget, using the largest representable valid
  budgets. Simple no longer inherits Advanced resource settings. Dimension and
  arithmetic checks, codec constraints, and source/output protections remain.
- 58 tests passed (15 GUI, 10 batch, 11 CLI, 22 engine) using
  `cargo test --all-targets --all-features --locked --profile simple-update
  --config 'profile.simple-update.inherits="dev"'`. The separate build profile
  avoids replacing the executable currently open in Windows.
- A header-only 400 MP BMP fixture passed Simple header checks and planning for
  1.6 GP output with every filter, without allocating the image. Real small-image
  previews and all-filter exports passed with Advanced set to a one-pixel cap
  and a 1 MiB memory budget. No multi-gigabyte allocation/export is claimed.
- Formatting, Clippy with warnings denied, documentation, Rust 1.88 checks and
  CLI-only tests passed. Updated executables are in `target/simple-update`.

## Folder and scaling update — 1 November 2026

This update supersedes the remembered-destination and automatic-queue-cleanup
behavior in the earlier baseline below.

- `cargo test --all-targets --all-features --locked`: 57 tests passed (14 GUI,
  10 batch, 11 CLI, 22 engine), including actual 0.25× and 0.5× exports with all
  five filters, destination reset when loading old/new preferences, and retained
  queue entries, selection and preview after success, failure/retry and cancellation.
- CLI-only tests, formatting, Clippy with warnings denied, documentation, and
  the Rust 1.88 minimum-version check passed. Both debug executables built.
- Dependency licenses/bans/sources passed with cargo-deny 0.20.2; distribution
  notices were regenerated with cargo-about 0.9.3. New GUI dependencies are
  `dirs-next` for system folder lookup and `open` for the default file manager.
- Native Windows review confirmed the default destination uses the redirected
  OneDrive Pictures folder. A 0.25× GUI export saved five JPEGs at 272×362 from
  a 1086×1448 source, preserved the queue and source preview, and **Open folder**
  opened the actual output directory in Explorer. The CLI independently verified
  all five output formats and dimensions. Test exports are under
  `target/folder-scaling-ui-export`, outside source control.
- This update was reviewed using the debug build. The earlier release binaries
  and portable archive described below are from the previous baseline.

## Earlier baseline

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
