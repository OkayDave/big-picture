//! Fixed-quality recipes and preflight for the simple desktop workflow.
use crate::{ColorSpace, Filter, Job, Options, OutputFormat, PlanOptions, Size};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub(super) fn algorithm_name(filter: Filter) -> &'static str {
    match filter {
        Filter::Nearest => "nearest",
        Filter::Bilinear => "bilinear",
        Filter::CatmullRom => "catmull-rom",
        Filter::Mitchell => "mitchell",
        Filter::Lanczos3 => "lanczos3",
    }
}

pub(super) fn options(scale: f64, filter: Filter) -> Options {
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
        allow_downscale: scale < 1.0,
        // Simple has no user-configured resource caps. Use the largest valid
        // budgets; the engine still checks dimension and allocation arithmetic.
        memory_mib: u64::MAX / (1024 * 1024),
        max_pixels: u64::MAX,
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
) -> Result<Vec<ExportJob>> {
    ensure!(!inputs.is_empty(), "no input images");
    ensure!(
        scale.is_finite() && (0.25..=64.0).contains(&scale),
        "scale ratio must be between 0.25 and 64"
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
                options: options(scale, filter),
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
        let jobs = plan(std::slice::from_ref(&input), dir.path(), 2.5, 123).unwrap();
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
        assert!(plan(&[input], dir.path(), 2.5, 123).is_err());
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
        assert!(plan(&inputs, &folder, 2.0, 123).is_err());
        assert!(!folder.exists());
        for ratio in [0.0, 0.24, f64::NAN, f64::INFINITY, 65.0] {
            assert!(plan(&inputs[..1], &folder, ratio, 123).is_err());
        }
    }

    #[test]
    fn large_images_pass_simple_header_and_working_memory_checks_without_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("large.bmp");
        // Header only: inspecting this 400 MP source needs no pixel allocation.
        let mut header = [0u8; 54];
        header[..2].copy_from_slice(b"BM");
        header[2..6].copy_from_slice(&1_200_000_054u32.to_le_bytes());
        header[10..14].copy_from_slice(&54u32.to_le_bytes());
        header[14..18].copy_from_slice(&40u32.to_le_bytes());
        header[18..22].copy_from_slice(&20_000i32.to_le_bytes());
        header[22..26].copy_from_slice(&20_000i32.to_le_bytes());
        header[26..28].copy_from_slice(&1u16.to_le_bytes());
        header[28..30].copy_from_slice(&24u16.to_le_bytes());
        std::fs::write(&input, header).unwrap();
        assert!(crate::inspect(&input, &Options::default()).is_err());
        for job in plan(&[input], dir.path(), 2.0, 123).unwrap() {
            let info = crate::inspect(&job.job.input, &job.options).unwrap();
            // All five recipes accept 1.6 GP output, including a working-memory
            // estimate well beyond the previous 8 GiB Simple budget.
            assert_eq!(
                job.options.target(info.width, info.height).unwrap(),
                (40_000, 40_000)
            );
        }
    }
}
