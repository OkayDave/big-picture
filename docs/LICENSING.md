# Licensing and image rights

Big Picture's application code is MIT licensed. It adds no royalty, attribution,
commercial-use restriction, or claim of ownership to images it processes. You
may use the resulting pixels for commercial or non-commercial purposes to the
extent you hold the necessary rights to the input and its contents. Upscaling
does not remove copyright, trademark, privacy, or other restrictions in an input.
This policy is not a guarantee that a particular input is cleared for use.

Simple, Advanced and the CLI use local mathematical filters. Optional desktop
AI Enhancement uses hosted OpenAI image editing, the user's API key, and paid API
usage. No AI weights are bundled. Provider terms and policies also apply to that
optional service; review them in your OpenAI account before use.

## Dependencies

The selected direct libraries use permissive licenses. Software distribution
obligations apply to distributing the program and its dependencies; these do
not by themselves license somebody else's input image to you.

| Library | Purpose | Upstream license |
| --- | --- | --- |
| [image](https://github.com/image-rs/image) | Codecs, orientation, pixel buffers | MIT OR Apache-2.0 |
| [fast_image_resize](https://github.com/Cykooz/fast_image_resize) | SIMD filters, alpha handling | MIT OR Apache-2.0 |
| [egui / eframe](https://github.com/emilk/egui) | Desktop interface, bundled fonts | MIT OR Apache-2.0 (font assets also have their own notices) |
| [clap / clap_complete](https://github.com/clap-rs/clap) | CLI and completions | MIT OR Apache-2.0 |
| [rfd](https://github.com/PolyMeilex/rfd) | Native file dialogs | MIT |
| [anyhow](https://github.com/dtolnay/anyhow) | Error context | MIT OR Apache-2.0 |
| [serde / serde_json](https://github.com/serde-rs) | Settings and JSON | MIT OR Apache-2.0 |
| [tempfile](https://github.com/Stebalien/tempfile) | Atomic output staging | MIT OR Apache-2.0 |
| [walkdir / same-file](https://github.com/BurntSushi/walkdir) | Discovery and source protection | Unlicense OR MIT |
| [glob](https://github.com/rust-lang/glob) | Input pattern expansion | MIT OR Apache-2.0 |
| [ctrlc](https://github.com/Detegr/rust-ctrlc) | Cancellation | MIT OR Apache-2.0 |
| [dirs-next](https://github.com/xdg-rs/dirs) | System Pictures folder | MIT OR Apache-2.0 |
| [open](https://github.com/Byron/open-rs) | Open the export folder | MIT |
| [winresource](https://github.com/BenjaminRi/winresource) | Build-time Windows icon and version resources | MIT |
| [reqwest](https://github.com/seanmonstar/reqwest) | Optional desktop HTTPS image-edit requests | MIT OR Apache-2.0 |
| [base64](https://github.com/marshallpierce/rust-base64) | Decode AI response images | MIT OR Apache-2.0 |
| [zeroize](https://github.com/RustCrypto/utils/tree/master/zeroize) | Clear application-owned API-key strings on drop | Apache-2.0 OR MIT |
| [keyring](https://github.com/hwchen/keyring-rs) | Optional OS credential-store access | MIT OR Apache-2.0 |

The exact transitive dependency set is pinned in Cargo.lock. The dependency
graph also includes Boost-licensed Windows clipboard support (BSL-1.0), along
with the bundled OFL/Ubuntu/MIT font assets. These are included in the notice set.
The dependency license policy is enforced by cargo-deny in CI. Before distributing binaries,
generate and include THIRD_PARTY_NOTICES.html with cargo-about using about.toml
and docs/licenses.hbs, and review its output (including bundled font notices).
See CONTRIBUTING.md for the commands. Do not assume this short table replaces
the required upstream license texts.
