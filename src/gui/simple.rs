//! Fixed-quality recipes and preflight for the simple desktop workflow.
use crate::{ColorSpace, Filter, Job, Options, OutputFormat, PlanOptions, Size};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub(super) fn default_folder() -> Result<PathBuf> {
    Ok(std::env::home_dir()
        .context("Could not locate your home folder. Choose an export folder.")?
        .join("Pictures")
        .join("big-picture"))
}

pub(super) fn algorithm_name(filter: Filter) -> &'static str {
    match filter {
        Filter::Nearest => "nearest",
        Filter::Bilinear => "bilinear",
        Filter::CatmullRom => "catmull-rom",
        Filter::Mitchell => "mitchell",
        Filter::Lanczos3 => "lanczos3",
    }
}

pub(super) fn options(scale: f64, filter: Filter, limits: &Options) -> Options {
    Options {
        size: Size::Scale(scale),
        filter,
        color_space: if filter == Filter::Nearest {
            ColorSpace::Srgb
        } else {
            ColorSpace::Linear
        },
        // Gentle sharpening for smooth interpolation; retain pixel-art edges exactly.
        sharpen: if filter == Filter::Nearest { 0.0 } else { 0.15 },
        quality: 100,
        // Full-resolution linear-light processing needs more headroom than the
        // advanced defaults. This is an estimate, not an up-front allocation.
        memory_mib: limits.memory_mib.max(8192),
        max_pixels: limits.max_pixels,
        ..Options::default()
    }
}

pub(super) struct ExportJob {
    pub index: usize,
    pub job: Job,
    pub options: Options,
}

pub(super) fn plan(
    inputs: &[PathBuf],
    folder: &Path,
    scale: f64,
    timestamp: u128,
    limits: &Options,
) -> Result<Vec<ExportJob>> {
    ensure!(!inputs.is_empty(), "no input images");
    ensure!(
        scale.is_finite() && (1.0..=64.0).contains(&scale),
        "upscale ratio must be between 1 and 64"
    );
    let sources = inputs
        .iter()
        .map(|p| crate::batch::resolved(p))
        .collect::<Result<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        for filter in Filter::ALL {
            let mut name = std::ffi::OsString::from(format!("{timestamp}-"));
            name.push(input.file_stem().context("input has no filename")?);
            name.push(format!("-{}-{scale}x.jpeg", algorithm_name(filter)));
            let mut planned = crate::plan(
                std::slice::from_ref(input),
                &PlanOptions {
                    output: Some(folder.join(name)),
                    format: Some(OutputFormat::Jpeg),
                    ..Default::default()
                },
            )?;
            let job = planned.remove(0);
            #[cfg(target_os = "windows")]
            let key = job.output.as_os_str().to_string_lossy().to_lowercase();
            #[cfg(not(target_os = "windows"))]
            let key = job.output.clone();
            ensure!(
                seen.insert(key),
                "multiple inputs map to {}: use separate batches or output folders",
                job.output.display()
            );
            for source in &sources {
                ensure!(
                    *source != job.output,
                    "output would replace an input: {}",
                    job.output.display()
                );
            }
            jobs.push(ExportJob {
                index,
                job,
                options: options(scale, filter, limits),
            });
        }
    }
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_plan_names_fractional_ratios_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("original.name.png");
        std::fs::write(&input, b"source").unwrap();
        let jobs = plan(
            std::slice::from_ref(&input),
            dir.path(),
            2.5,
            123,
            &Options::default(),
        )
        .unwrap();
        assert_eq!(jobs.len(), Filter::ALL.len());
        for (export, filter) in jobs.iter().zip(Filter::ALL) {
            assert_eq!(export.index, 0);
            assert_eq!(
                export.job.output.file_name().unwrap(),
                format!("123-original.name-{}-2.5x.jpeg", algorithm_name(filter)).as_str()
            );
            assert_eq!(export.options.target(8, 6).unwrap(), (20, 15));
            assert_eq!(export.options.quality, 100);
            assert_eq!(export.options.background, [255; 3]);
            if filter == Filter::Nearest {
                assert_eq!(export.options.sharpen, 0.0);
                assert_eq!(export.options.color_space, ColorSpace::Srgb);
            } else {
                assert_eq!(export.options.color_space, ColorSpace::Linear);
                assert!(export.options.sharpen > 0.0 && export.options.sharpen <= 0.25);
            }
        }
        std::fs::write(&jobs[4].job.output, b"existing").unwrap();
        assert!(plan(&[input], dir.path(), 2.5, 123, &Options::default()).is_err());
        assert_eq!(std::fs::read(&jobs[4].job.output).unwrap(), b"existing");
        assert!(!jobs[0].job.output.exists());
    }

    #[test]
    fn simple_preflight_rejects_duplicate_stems_before_any_export() {
        let dir = tempfile::tempdir().unwrap();
        let inputs = [dir.path().join("same.png"), dir.path().join("same.jpeg")];
        for input in &inputs {
            std::fs::write(input, b"source").unwrap();
        }
        let folder = dir.path().join("out");
        assert!(plan(&inputs, &folder, 2.0, 123, &Options::default()).is_err());
        assert!(!folder.exists());
        for ratio in [0.0, 0.5, f64::NAN, f64::INFINITY, 65.0] {
            assert!(plan(&inputs[..1], &folder, ratio, 123, &Options::default()).is_err());
        }
    }
}
