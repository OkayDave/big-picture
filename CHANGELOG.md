# Changelog

## 0.1.0 — Unreleased

- Replace the landscape sample with Dave's photograph of Merlin, prepared as a
  1200×900 JPEG without source metadata. The sample exporter now uses that image.
- Add optional single-image AI Enhancement desktop mode powered by OpenAI,
  with cleanup, detail, colour, exposure, prompt, scale and quality controls.
  Keys are session-only by default, with optional OS credential-store persistence;
  prompts remain session-only and cloud/billing consent is explicit.
  Generate retains a full-resolution result for comparison and repeated local
  saves without further API calls, using shared atomic/source-protected encoding.
- Add Remember/Update/Load/Forget controls for the AI API key. Saved keys load on
  launch without enabling cloud processing. No keys go into app preferences.
- Make AI export format explicit (PNG/JPEG/WebP), match extensions to that format,
  and increase AI JPEG quality to 100. PNG and WebP exports remain lossless.
- Add the Expand logo to the desktop header, About window and README, with
  native window and Windows executable icons and an updated app screenshot.
- Default Simple desktop mode with 2× scaling, a destination that resets to the
  system Photos/Pictures folder on launch (with fallbacks), all-algorithm
  quality-100 JPEG exports, timestamped names, and high-quality filter recipes.
- Retain queued images and the selected preview after export; add **Open folder**
  to open the actual export location in the default file manager.
- Add 0.25× and 0.5× scaling presets in both desktop modes and support Simple
  custom ratios down to 0.25×.
- Advanced mode retains the existing individual export and preview controls.
- Remove application-level pixel and memory caps from Simple mode, including
  source loading and export; its resource budgets are independent of Advanced.
- Shared offline Rust engine for conventional image upscaling.
- Five filters, scale/bounded/exact sizing, alpha handling, optional linear-light
  filtering and sharpening, five input/output format families.
- Batch CLI with recursive discovery, glob expansion, dry runs, JSON, completions,
  source protection, cancellation and atomic per-file output writes.
- Native desktop queue, worker-based previews/exports, comparison wipe, zoom,
  native dialogs, saved preferences, resource controls and status reporting.
- Regression tests, user/developer documentation, dependency license policy,
  CI checks and portable archive packaging.
