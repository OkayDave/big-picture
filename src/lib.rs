//! Offline, alpha-aware image upscaling shared by the command line and desktop UI.
//!
//! See [`Options`], [`inspect`], [`plan`], and [`process`] for the public pipeline.
#[cfg(feature = "gui")]
pub(crate) mod ai;
pub mod batch;
pub mod cli;
pub mod engine;
#[cfg(feature = "gui")]
pub mod gui;
pub mod settings;

pub use batch::{Job, PlanOptions, discover, plan};
pub use engine::{ImageInfo, ProcessReport, inspect, load, process, resize};
pub use settings::{ColorSpace, Filter, Options, OutputFormat, Size};
