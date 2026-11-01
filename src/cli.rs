use crate::{
    ColorSpace, Filter, Options, OutputFormat, PlanOptions, Size, discover, inspect, plan, process,
};
use anyhow::{Context, Result, ensure};
use clap::{CommandFactory, Parser, Subcommand};
use serde::Serialize;
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Offline image upscaling. Your images stay on your computer.",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Emit machine-readable JSON on stdout (progress stays on stderr).
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Upscale files, folders or quoted glob patterns.
    Upscale(Upscale),
    /// Inspect oriented dimensions and codec without decoding pixels.
    Info {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
    },
    /// Generate shell completions to stdout.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Show licensing and output-image policy.
    Licenses,
    /// Open the desktop application (requires the gui feature).
    #[cfg(feature = "gui")]
    Gui { inputs: Vec<PathBuf> },
}

#[derive(Debug, clap::Args)]
pub struct Upscale {
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
    /// Single output file; extension selects codec.
    #[arg(short, long, conflicts_with = "output_dir")]
    pub output: Option<PathBuf>,
    /// Flat batch output directory; filenames gain -upscaled.
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    /// Search subdirectories; symlink directories are not followed.
    #[arg(short, long)]
    pub recursive: bool,
    /// Multiplicative scale (default 2); conflicts with width/height.
    #[arg(short, long, conflicts_with_all = ["width", "height"])]
    pub scale: Option<f64>,
    /// Fit within this width, preserving aspect ratio unless --stretch.
    #[arg(long)]
    pub width: Option<u32>,
    /// Fit within this height, preserving aspect ratio unless --stretch.
    #[arg(long)]
    pub height: Option<u32>,
    /// Distort to exact width AND height.
    #[arg(long, requires_all = ["width", "height"])]
    pub stretch: bool,
    #[arg(long, value_enum, default_value = "lanczos3")]
    pub filter: Filter,
    #[arg(long, value_enum, default_value = "srgb")]
    pub color_space: ColorSpace,
    /// Unsharp-mask amount from 0 (off) to 3.
    #[arg(long, default_value_t = 0.0)]
    pub sharpen: f32,
    /// Default PNG; explicit -o selects format from its extension.
    #[arg(long, value_enum)]
    pub format: Option<OutputFormat>,
    /// JPEG quality, 1–100; WebP is always lossless.
    #[arg(long, default_value_t = 92)]
    pub quality: u8,
    /// JPEG/BMP alpha matte as six hex digits.
    #[arg(long, default_value = "ffffff", value_parser = crate::settings::parse_background)]
    pub background: [u8; 3],
    #[arg(long)]
    pub allow_downscale: bool,
    /// Replace existing outputs; source images are always protected.
    #[arg(long)]
    pub overwrite: bool,
    /// Continue processing other files after an image fails.
    #[arg(long)]
    pub keep_going: bool,
    /// Validate headers, sizes and destinations without writing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// Maximum pixels in either source or output.
    #[arg(long, default_value_t = 100_000_000)]
    pub max_pixels: u64,
    /// Estimated working-memory limit in MiB (not an OS quota).
    #[arg(long, default_value_t = 512)]
    pub memory_mib: u64,
}
impl Upscale {
    pub fn options(&self) -> Result<Options> {
        let size = if self.stretch {
            Size::Exact {
                width: self.width.context("stretch needs width")?,
                height: self.height.context("stretch needs height")?,
            }
        } else if self.width.is_some() || self.height.is_some() {
            Size::Fit {
                width: self.width,
                height: self.height,
            }
        } else {
            Size::Scale(self.scale.unwrap_or(2.0))
        };
        let options = Options {
            size,
            filter: self.filter,
            color_space: self.color_space,
            sharpen: self.sharpen,
            quality: self.quality,
            background: self.background,
            allow_downscale: self.allow_downscale,
            max_pixels: self.max_pixels,
            memory_mib: self.memory_mib,
        };
        options.validate()?;
        Ok(options)
    }
}

#[derive(Serialize)]
struct Item {
    input: PathBuf,
    output: PathBuf,
    status: &'static str,
    width: Option<u32>,
    height: Option<u32>,
    report: Option<crate::ProcessReport>,
    error: Option<String>,
}
#[derive(Serialize)]
struct Summary {
    dry_run: bool,
    planned: usize,
    completed: usize,
    failed: usize,
    cancelled: bool,
    items: Vec<Item>,
}

/// Return an exit status: 0 success, 1 processing/preflight error, 130 cancellation.
pub fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::Completions { shell } => {
            ensure!(!cli.json, "completions cannot be combined with --json");
            clap_complete::generate(shell, &mut Cli::command(), "big-picture", &mut io::stdout());
            Ok(0)
        }
        Command::Licenses => {
            let text = include_str!("../docs/LICENSING.md");
            if cli.json {
                println!("{}", serde_json::json!({"license":"MIT", "policy":text}));
            } else {
                println!("{text}");
            }
            Ok(0)
        }
        Command::Info { inputs } => {
            let paths = discover(&inputs, false, None)?;
            let infos = paths
                .iter()
                .map(|p| inspect(p, &Options::default()))
                .collect::<Result<Vec<_>>>()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&infos)?);
            } else {
                for i in infos {
                    println!(
                        "{}  {} × {}  {}",
                        i.path.display(),
                        i.width,
                        i.height,
                        i.format
                    );
                }
            }
            Ok(0)
        }
        #[cfg(feature = "gui")]
        Command::Gui { inputs } => {
            ensure!(!cli.json, "GUI cannot be combined with --json");
            crate::gui::launch(inputs).map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(0)
        }
        Command::Upscale(args) => {
            let options = args.options()?;
            let inputs = discover(&args.inputs, args.recursive, args.output_dir.as_deref())?;
            let jobs = plan(
                &inputs,
                &PlanOptions {
                    output: args.output,
                    output_dir: args.output_dir,
                    format: args.format,
                    overwrite: args.overwrite,
                },
            )?;
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = cancel.clone();
            ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
            let mut summary = Summary {
                dry_run: args.dry_run,
                planned: jobs.len(),
                completed: 0,
                failed: 0,
                cancelled: false,
                items: vec![],
            };
            for (index, job) in jobs.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    summary.cancelled = true;
                    break;
                }
                let mut item = Item {
                    input: job.input.clone(),
                    output: job.output.clone(),
                    status: "failed",
                    width: None,
                    height: None,
                    report: None,
                    error: None,
                };
                let result = if args.dry_run {
                    inspect(&job.input, &options)
                        .and_then(|info| options.target(info.width, info.height))
                        .map(|(w, h)| {
                            item.width = Some(w);
                            item.height = Some(h);
                        })
                } else {
                    if !cli.json {
                        eprintln!("[{}/{}] {}", index + 1, jobs.len(), job.input.display());
                    }
                    process(&job.input, &job.output, &options, args.overwrite, &cancel).map(
                        |report| {
                            item.width = Some(report.width);
                            item.height = Some(report.height);
                            item.report = Some(report);
                        },
                    )
                };
                match result {
                    Ok(()) => {
                        item.status = if args.dry_run { "planned" } else { "completed" };
                        summary.completed += 1;
                        if !cli.json {
                            println!(
                                "{} → {} ({} × {})",
                                item.status,
                                item.output.display(),
                                item.width.unwrap_or(0),
                                item.height.unwrap_or(0)
                            );
                        }
                    }
                    Err(error) => {
                        if cancel.load(Ordering::Relaxed) {
                            item.status = "cancelled";
                            summary.cancelled = true;
                        } else {
                            summary.failed += 1;
                        }
                        if !cli.json {
                            eprintln!("{}: {error:#}", job.input.display());
                        }
                        item.error = Some(format!("{error:#}"));
                    }
                }
                summary.items.push(item);
                if summary.cancelled || (summary.failed > 0 && !args.keep_going) {
                    break;
                }
            }
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                eprintln!(
                    "{} / {} {} · {} failed{}",
                    summary.completed,
                    summary.planned,
                    if args.dry_run { "planned" } else { "completed" },
                    summary.failed,
                    if summary.cancelled {
                        " · cancelled"
                    } else {
                        ""
                    }
                );
            }
            Ok(if summary.cancelled {
                130
            } else {
                i32::from(summary.failed > 0)
            })
        }
    }
}
