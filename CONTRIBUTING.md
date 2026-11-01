# Contributing

Big Picture is Rust 2024, minimum Rust 1.88. The committed Cargo.lock is the
application dependency baseline. Prefer small, permissively licensed libraries;
avoid network services and telemetry in the image-processing path.

## Build prerequisites

- Windows: Rust's MSVC target and Visual Studio C++ build tools with Windows SDK.
- macOS: Rust and Xcode command-line tools.
- Linux: a C compiler, pkg-config, OpenGL/EGL and X11/Wayland development libraries.
  On Ubuntu, CI installs `libx11-dev libxcursor-dev libxi-dev libxrandr-dev
  libxkbcommon-dev libwayland-dev libgl1-mesa-dev libegl1-mesa-dev`. Native file
  dialogs use an XDG desktop portal at runtime; a portal backend must be available.

```sh
cargo build --locked --bins
cargo run --bin big-picture-gui
cargo run -- upscale --help
cargo build --no-default-features --locked --bin big-picture
```

## Required checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo test --no-default-features --locked
cargo doc --no-deps --all-features --locked
cargo +1.88.0 check --all-targets --all-features --locked
cargo deny --all-features check licenses bans sources
```

Install cargo-deny 0.20.2 with `cargo install cargo-deny --version 0.20.2 --locked`.
CI runs the checks on Windows, Linux and macOS. GUI tests that don't require a
display run automatically; full interactive verification remains a manual task.
See [VALIDATION.md](docs/VALIDATION.md) for what has actually been exercised.

Tests generate small images in temporary folders. They need no downloads,
fixtures from users, credentials, or large committed binaries. Test behavior
that matters: pixels/alpha, dimensions, codec round trips, errors, disk safety
and CLI contracts. Use tolerances where float filtering or lossy codecs warrant
them. Do not change the metadata/depth policy silently.

## Dependency policy and distribution notices

`deny.toml` permits selected permissive code licenses and the bundled font
licenses; new licenses fail review. Font-license conditions concern the fonts
in the app, not processed-image pixels. Inspect the exact dependency licenses
when updating Cargo.lock. No AI models are bundled.

Install cargo-about 0.9.3, then generate distribution notices:

```sh
cargo install cargo-about --version 0.9.3 --locked
cargo about generate --all-features --locked docs/licenses.hbs -o THIRD_PARTY_NOTICES.html
```

The generated HTML includes the resolved license texts and dependency inventory.
Commit refreshed notices with dependency updates and include them in binary
archives. Review warnings rather than silently ignoring missing licenses.
The selected fonts have additional notices under `docs/font-licenses`; those
are copied verbatim from the pinned `epaint_default_fonts` crate and must also
ship with binary archives.

## Local release packaging

```powershell
./scripts/package.ps1
```

The Windows script builds both release binaries, generates notices, copies the
README, application license and font notices, and writes a portable zip plus
SHA-256 checksum under `dist`. It needs cargo-about on PATH. The tag-triggered
GitHub workflow performs equivalent packaging for Windows, Linux and macOS and
uploads CI artifacts; it does not publish a GitHub Release or upload to crates.io.
Review artifacts and platform checks before public distribution. Windows/macOS
binaries are currently unsigned. No installers are provided yet.

## Reporting problems

Include OS, app version, command/options, codec and input/output dimensions.
Use a minimal input you are entitled to share. Do not post private images,
EXIF location data or secrets in public issues. For crashes on unusual images,
include whether a dry run passes and the resource limits used.
