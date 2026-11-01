//! Deterministic input discovery and preflight checks, before any output is written.
use crate::OutputFormat;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp"];
pub fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Resolve existing ancestors as well as lexical `..`, including output folders that don't exist yet.
pub fn resolved(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component),
        }
    }
    if normalized.exists() {
        return Ok(normalized.canonicalize()?);
    }
    let parent = normalized.parent().context("path has no parent")?;
    Ok(resolved(parent)?.join(normalized.file_name().context("path has no filename")?))
}

/// Expand files, directories, and quoted glob patterns. Directory symlinks are not followed.
/// Output folders are excluded from directory traversal, preventing recursive reprocessing.
pub fn discover(
    inputs: &[PathBuf],
    recursive: bool,
    exclude: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    let exclude = exclude.map(resolved).transpose()?;
    let mut found = Vec::new();
    for input in inputs {
        let candidates = if input.exists() {
            vec![input.clone()]
        } else {
            let pattern = input.to_str().context("glob pattern must be Unicode")?;
            glob::glob(pattern)?.collect::<std::result::Result<Vec<_>, _>>()?
        };
        ensure!(
            !candidates.is_empty(),
            "input does not exist or pattern has no matches: {}",
            input.display()
        );
        for path in candidates {
            if path.is_dir() {
                let root = path.canonicalize()?;
                ensure!(
                    exclude.as_ref() != Some(&root),
                    "input and output directories must differ"
                );
                let walker = walkdir::WalkDir::new(&root)
                    .follow_links(false)
                    .max_depth(if recursive { usize::MAX } else { 1 });
                for entry in walker.into_iter().filter_entry(|entry| {
                    exclude
                        .as_ref()
                        .is_none_or(|e| !entry.path().starts_with(e))
                }) {
                    let entry = entry?;
                    if entry.file_type().is_file() && supported(entry.path()) {
                        found.push(entry.path().canonicalize()?);
                    }
                }
            } else {
                ensure!(path.is_file(), "input is not a file: {}", path.display());
                ensure!(
                    supported(&path),
                    "unsupported input extension: {}",
                    path.display()
                );
                found.push(path.canonicalize()?);
            }
        }
    }
    found.sort();
    found.dedup();
    ensure!(!found.is_empty(), "no supported images found");
    Ok(found)
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub input: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
    pub output: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub format: Option<OutputFormat>,
    pub overwrite: bool,
}

/// Batch output is flat with `-upscaled` suffix; ambiguous names are rejected, never silently replaced.
pub fn plan(inputs: &[PathBuf], options: &PlanOptions) -> Result<Vec<Job>> {
    ensure!(!inputs.is_empty(), "no input images");
    ensure!(
        options.output.is_none() || options.output_dir.is_none(),
        "choose output or output-dir, not both"
    );
    ensure!(
        options.output.is_none() || inputs.len() == 1,
        "a single output file requires exactly one input"
    );
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for input in inputs {
        let output = if let Some(output) = &options.output {
            let inferred = OutputFormat::from_path(output)?;
            ensure!(
                options.format.is_none_or(|f| f == inferred),
                "format conflicts with output extension"
            );
            output.clone()
        } else {
            let format = options.format.unwrap_or(OutputFormat::Png);
            let mut name = input
                .file_stem()
                .context("input has no filename")?
                .to_os_string();
            name.push(format!("-upscaled.{}", format.extension()));
            options
                .output_dir
                .as_deref()
                .unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")))
                .join(name)
        };
        let output = resolved(&output)?;
        #[cfg(target_os = "windows")]
        let key = output.as_os_str().to_string_lossy().to_lowercase();
        #[cfg(not(target_os = "windows"))]
        let key = output.clone();
        ensure!(
            seen.insert(key),
            "multiple inputs map to {}: use separate batches or output folders",
            output.display()
        );
        for source in inputs {
            ensure!(
                resolved(source)? != output,
                "output would replace an input: {}",
                output.display()
            );
            if output.exists() {
                ensure!(
                    !same_file::is_same_file(source, &output)?,
                    "output is a link to an input"
                );
            }
        }
        crate::engine::validate_destination(input, &output, options.overwrite)?;
        jobs.push(Job {
            input: input.clone(),
            output,
        });
    }
    Ok(jobs)
}
