# Architecture

One Rust package exports a library and two binaries. The optional `gui` feature
is enabled by default. CLI-only builds do not compile eframe or native dialogs.

```text
CLI / clap ─────────┐
                   ├── settings → discovery → plan → inspect → load → resize → encode → commit
Desktop / eframe ──┘
                       shared Rust library
```

- `settings.rs`: serializable options, filter mapping, size calculation,
  validation and resource estimates.
- `batch.rs`: deterministic discovery, exclusion of output trees, flat output
  names, destination collisions, and protection of every batch source.
- `engine.rs`: signature-based codec discovery, header inspection, orientation,
  decode limits, alpha-aware/linear-light resize, sharpening, encoding and commit.
- `cli.rs`: argument parsing, JSON/text presentation, signal handling, dry runs,
  completion generation and batch policy.
- `gui.rs`: persisted preferences, queue state, native dialogs, comparison UI,
  background workers and cancellation. No codec work runs in the paint callback.
- `tests/`: public-engine, batch/filesystem, and executable-level CLI regression
  tests. GUI preference/thumbnail tests live beside the GUI code.

## Filesystem semantics

Each output is encoded into a `NamedTempFile` in the destination directory,
flushed and synced, then committed by rename/persist. With overwrite off,
`persist_noclobber` prevents a late-created destination from being replaced.
With overwrite on, file replacement is atomic on supported local filesystems.
Existing input/output identity is checked with `same-file`, including hard links.
Batch planning additionally checks an output against every batch source.

This is atomic **per file**, not transactional across a batch. Earlier completed
files remain after later failures or cancellation. Directories created during
an encoding failure can remain empty. Network filesystems can have different
rename guarantees. Parent directories are not fsynced, so the implementation
does not promise whole-system crash/power-loss durability. A hard process kill
can leave a staging file. File permissions/metadata of replaced outputs are
not preserved.

Destination identity is rechecked before commit. These checks prevent normal
accidental overwrites, not hostile concurrent mutation of source paths or
directories. The application is not an adversarial filesystem sandbox.

## Workers and cancellation

The desktop uses bounded channels to deliver discovery, preview, per-file and
completion events. It allows one active worker at a time and processes batch
images sequentially. The UI owns textures; workers send bounded thumbnails,
not GPU resources. CLI processing is also sequential. An `AtomicBool` cancellation
flag is checked between expensive stages and immediately before committing.
Filter kernels and codecs run to completion once entered.

## Future scope

AI super-resolution is deliberately deferred by the initial product decision.
Any future backend must separately review runtime and exact weight licenses,
model provenance, output terms, offline model integrity, tiling and hardware
requirements. Other candidates are true 1:1/cropped previews, color management,
16-bit output, installer signing, measured performance benchmarks and automatic
tree-preserving batch naming. These are not claimed as features of 0.1.0.
