//! Native desktop workflow. All decoding, filtering, and file writes run on a worker.
use crate::{ColorSpace, Filter, Options, OutputFormat, PlanOptions, Size};
use eframe::egui::{self, Color32, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

mod ai_panel;
mod branding;
mod credentials;
mod platform;
mod simple;

const ACCENT: Color32 = Color32::from_rgb(112, 224, 192);
const MUTED: Color32 = Color32::from_rgb(156, 168, 181);
const SETTINGS_KEY: &str = "big-picture.settings.v1";
const SCALE_PRESETS: [f64; 6] = [0.25, 0.5, 2.0, 3.0, 4.0, 8.0];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    simple: bool,
    ai_mode: bool,
    ai: crate::ai::Settings,
    remember_ai_key: bool,
    ai_format: OutputFormat,
    simple_scale: f64,
    options: Options,
    format: OutputFormat,
    // Always resolve the OS's current Pictures location on launch, including
    // when loading preferences written by older versions.
    #[serde(skip, default = "platform::default_output_dir")]
    output_dir: String,
    size_mode: usize,
    scale: f64,
    width: u32,
    height: u32,
    recursive: bool,
    overwrite: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            simple: true,
            ai_mode: false,
            ai: crate::ai::Settings::default(),
            remember_ai_key: false,
            ai_format: OutputFormat::Png,
            simple_scale: 2.0,
            options: Options::default(),
            format: OutputFormat::Png,
            output_dir: platform::default_output_dir(),
            size_mode: 0,
            scale: 2.0,
            width: 3840,
            height: 2160,
            recursive: false,
            overwrite: false,
        }
    }
}
impl Preferences {
    fn export_folder(&self) -> anyhow::Result<PathBuf> {
        if self.output_dir.trim().is_empty() {
            Ok(platform::default_folder())
        } else {
            Ok(PathBuf::from(&self.output_dir))
        }
    }

    fn preview_options(&self) -> Options {
        if self.ai_mode {
            Options {
                size: Size::Scale(1.0),
                ..crate::ai::options()
            }
        } else if self.simple {
            simple::options(self.simple_scale, Filter::Lanczos3)
        } else {
            self.effective_options()
        }
    }

    fn effective_options(&self) -> Options {
        let mut options = self.options.clone();
        options.size = match self.size_mode {
            1 => Size::Fit {
                width: Some(self.width),
                height: None,
            },
            2 => Size::Fit {
                width: None,
                height: Some(self.height),
            },
            3 => Size::Fit {
                width: Some(self.width),
                height: Some(self.height),
            },
            4 => Size::Exact {
                width: self.width,
                height: self.height,
            },
            _ => Size::Scale(self.scale),
        };
        options
    }
}

#[derive(Clone, Default)]
struct QueueItem {
    path: PathBuf,
    status: String,
    output: Option<PathBuf>,
    saved: usize,
    failures: Vec<String>,
}
struct Preview {
    before: egui::ColorImage,
    after: Option<egui::ColorImage>,
    source: (u32, u32),
    target: (u32, u32),
}
enum Event {
    AiGenerated(Result<Box<crate::ai::Generated>, String>),
    AiSaved(Result<PathBuf, String>),
    Discovered(Result<Vec<PathBuf>, String>),
    Preview(Result<Preview, String>),
    Started(usize, Filter),
    Finished(usize, Result<crate::ProcessReport, String>),
    End,
    Failed(String),
}

pub struct Desktop {
    ai: ai_panel::State,
    logo: egui::TextureHandle,
    prefs: Preferences,
    queue: Vec<QueueItem>,
    selected: usize,
    receiver: Option<Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    exporting: bool,
    before: Option<egui::TextureHandle>,
    after: Option<egui::TextureHandle>,
    dimensions: Option<((u32, u32), (u32, u32))>,
    message: String,
    error: Option<String>,
    wipe: f32,
    zoom: f32,
    comparison: bool,
    show_about: bool,
    completed: usize,
    failed: usize,
    total_exports: usize,
    last_export_folder: Option<PathBuf>,
    close_pending: bool,
    initial_inputs: Option<Vec<PathBuf>>,
}
impl Desktop {
    fn new(cc: &eframe::CreationContext<'_>, inputs: Vec<PathBuf>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(12.0, 8.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(22, 28, 35);
        style.visuals.window_fill = Color32::from_rgb(28, 35, 43);
        style.visuals.selection.bg_fill = Color32::from_rgb(40, 94, 83);
        style.visuals.selection.stroke.color = ACCENT;
        cc.egui_ctx.set_style(style);
        let prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        Self {
            ai: ai_panel::State::default(),
            logo: branding::logo(&cc.egui_ctx),
            prefs,
            queue: vec![],
            selected: 0,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            exporting: false,
            before: None,
            after: None,
            dimensions: None,
            message: "Ready when you are. Add an image to begin.".into(),
            error: None,
            wipe: 0.5,
            zoom: 1.0,
            comparison: true,
            show_about: false,
            completed: 0,
            failed: 0,
            total_exports: 0,
            last_export_folder: None,
            close_pending: false,
            initial_inputs: Some(inputs),
        }
    }
    fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    fn clear_preview(&mut self) {
        self.before = None;
        self.after = None;
        self.dimensions = None;
    }

    fn add(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if paths.is_empty() || self.busy() {
            return;
        }
        let recursive = !self.prefs.simple && !self.prefs.ai_mode && self.prefs.recursive;
        let exclude = self.prefs.export_folder().ok();
        let (tx, rx) = mpsc::sync_channel(2);
        self.receiver = Some(rx);
        self.error = None;
        self.message = "Finding images…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::discover(&paths, recursive, exclude.as_deref())
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::Discovered(result));
            ctx.request_repaint();
        });
    }

    fn preview(&mut self, ctx: &egui::Context) {
        if self.busy() || self.queue.is_empty() {
            return;
        }
        let path = self.queue[self.selected].path.clone();
        if let Some(result) = self.ai.result.clone() {
            if result.source != path {
                self.ai.result = None;
            } else if self.prefs.ai_mode {
                self.show_ai_result(result, ctx);
                return;
            }
        }
        let options = self.prefs.preview_options();
        let simple_mode = self.prefs.simple || self.prefs.ai_mode;
        let (tx, rx) = mpsc::sync_channel(2);
        self.receiver = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.message = if simple_mode {
            "Loading selected image…"
        } else {
            "Building a full-resolution preview…"
        }
        .into();
        self.error = None;
        self.clear_preview();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Preview> {
                let info = crate::inspect(&path, &options)?;
                let target = if simple_mode {
                    options.size.dimensions(info.width, info.height)?
                } else {
                    options.target(info.width, info.height)?
                };
                crate::engine::check_cancel(&cancel)?;
                let image = crate::load(&path, &options)?;
                crate::engine::check_cancel(&cancel)?;
                if simple_mode {
                    return Ok(Preview {
                        before: thumbnail(&image),
                        after: None,
                        source: (info.width, info.height),
                        target,
                    });
                }
                let resized = crate::resize(&image, &options)?;
                crate::engine::check_cancel(&cancel)?;
                Ok(Preview {
                    before: thumbnail(&image),
                    after: Some(thumbnail(&image::DynamicImage::ImageRgba8(resized))),
                    source: (info.width, info.height),
                    target,
                })
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::Preview(result));
            ctx.request_repaint();
        });
    }

    fn export(&mut self, ctx: &egui::Context) {
        if self.busy() || self.queue.is_empty() || self.prefs.ai_mode {
            return;
        }
        let folder = match self.prefs.export_folder() {
            Ok(folder) => folder,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let paths = self
            .queue
            .iter()
            .map(|i| i.path.clone())
            .collect::<Vec<_>>();
        let options = self.prefs.effective_options();
        let simple_mode = self.prefs.simple;
        let scale = self.prefs.simple_scale;
        let plan_options = PlanOptions {
            output: None,
            output_dir: Some(folder.clone()),
            format: Some(self.prefs.format),
            overwrite: self.prefs.overwrite,
        };
        let (tx, rx) = mpsc::sync_channel(2);
        self.receiver = Some(rx);
        self.exporting = true;
        self.completed = 0;
        self.failed = 0;
        self.total_exports = self.queue.len() * if simple_mode { Filter::ALL.len() } else { 1 };
        self.error = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        for item in &mut self.queue {
            item.status = "Waiting".into();
            item.output = None;
            item.saved = 0;
            item.failures.clear();
        }
        self.message = "Checking output destinations…".into();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let planned = if simple_mode {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                simple::plan(&paths, &folder, scale, timestamp)
            } else {
                crate::plan(&paths, &plan_options).map(|jobs| {
                    jobs.into_iter()
                        .enumerate()
                        .map(|(index, job)| simple::ExportJob {
                            index,
                            job,
                            options: options.clone(),
                        })
                        .collect()
                })
            };
            let jobs = match planned {
                Ok(jobs) => jobs,
                Err(error) => {
                    let _ = tx.send(Event::Failed(format!("{error:#}")));
                    ctx.request_repaint();
                    return;
                }
            };
            for export in &jobs {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let index = export.index;
                let job = &export.job;
                let _ = tx.send(Event::Started(index, export.options.filter));
                ctx.request_repaint();
                let result = crate::process(
                    &job.input,
                    &job.output,
                    &export.options,
                    !simple_mode && plan_options.overwrite,
                    &cancel,
                )
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send(Event::Finished(index, result));
                ctx.request_repaint();
            }
            let _ = tx.send(Event::End);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        if let Some(rx) = &self.receiver {
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if events.is_empty() {
                            events.push(Event::Failed(
                                "The background worker stopped unexpectedly. Retry the operation."
                                    .into(),
                            ));
                        }
                        break;
                    }
                }
            }
        }
        for event in events {
            match event {
                Event::AiGenerated(result) => self.ai_generated(result, ctx),
                Event::AiSaved(result) => {
                    self.receiver = None;
                    match result {
                        Ok(path) => {
                            self.last_export_folder = path.parent().map(PathBuf::from);
                            self.message = format!(
                                "AI result saved to {}. No additional API request was made.",
                                path.display()
                            );
                            if let Some(item) = self.queue.get_mut(self.selected) {
                                item.status = "Saved AI enhancement".into();
                                item.output = Some(path);
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.message =
                                "Save stopped. The generated result is still available.".into();
                        }
                    }
                }
                Event::Discovered(result) => {
                    self.receiver = None;
                    match result {
                        Ok(paths) => {
                            for path in paths {
                                if !self.queue.iter().any(|i| i.path == path) {
                                    self.queue.push(QueueItem {
                                        path,
                                        status: "Ready".into(),
                                        output: None,
                                        saved: 0,
                                        failures: vec![],
                                    });
                                }
                            }
                            self.message = format!(
                                "{} images in queue. Build a preview or export the batch.",
                                self.queue.len()
                            );
                            if self.before.is_none() && !self.queue.is_empty() {
                                self.preview(ctx);
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::Preview(result) => {
                    self.receiver = None;
                    match result {
                        Ok(p) => {
                            self.dimensions = Some((p.source, p.target));
                            self.before = Some(ctx.load_texture(
                                "source-preview",
                                p.before,
                                egui::TextureOptions::NEAREST,
                            ));
                            self.after = p.after.map(|after| {
                                ctx.load_texture(
                                    "output-preview",
                                    after,
                                    egui::TextureOptions::LINEAR,
                                )
                            });
                            self.message = if self.prefs.ai_mode {
                                "Image ready. Generate enhances only the selected image using OpenAI."
                            } else if self.prefs.simple {
                                "Image ready. Choose a ratio and export all algorithms."
                            } else {
                                "Preview ready. Drag the divider to compare. Export saves the full-resolution image."
                            }.into();
                        }
                        Err(error) => {
                            self.message = "Preview stopped.".into();
                            self.error = Some(error);
                        }
                    }
                }
                Event::Started(i, filter) => {
                    self.queue[i].status =
                        format!("Processing {}…", simple::algorithm_name(filter));
                    self.message = format!(
                        "Processing export {} of {}",
                        self.completed + self.failed + 1,
                        self.total_exports
                    );
                }
                Event::Finished(i, result) => match result {
                    Ok(report) => {
                        self.queue[i].saved += 1;
                        self.queue[i].status = if self.prefs.simple {
                            format!(
                                "Saved {} of {} algorithms",
                                self.queue[i].saved,
                                Filter::ALL.len()
                            )
                        } else {
                            format!("Saved · {} × {}", report.width, report.height)
                        };
                        self.last_export_folder = report.output.parent().map(PathBuf::from);
                        self.queue[i].output = Some(report.output);
                        self.completed += 1;
                    }
                    Err(error) => {
                        if self.cancel.load(Ordering::Relaxed) {
                            self.queue[i].status = "Cancelled".into();
                        } else {
                            self.queue[i].status = format!("Failed: {error}");
                            self.queue[i].failures.push(error);
                            self.failed += 1;
                        }
                    }
                },
                Event::End => {
                    self.receiver = None;
                    self.exporting = false;
                    let cancelled = self.cancel.load(Ordering::Relaxed);
                    if cancelled {
                        for item in &mut self.queue {
                            if item.status == "Waiting" {
                                item.status = "Cancelled".into();
                            }
                        }
                    }
                    if self.prefs.simple {
                        for item in &mut self.queue {
                            item.status = format!(
                                "{} saved · {} failed{}",
                                item.saved,
                                item.failures.len(),
                                if cancelled { " · cancelled" } else { "" }
                            );
                            if let Some(error) = item.failures.first() {
                                item.status.push_str(&format!(": {error}"));
                            }
                        }
                    }
                    self.message = format!(
                        "{} saved · {} failed{}",
                        self.completed,
                        self.failed,
                        if cancelled { " · cancelled" } else { "" }
                    );
                }
                Event::Failed(error) => {
                    self.receiver = None;
                    self.exporting = false;
                    self.error = Some(error);
                    self.message =
                        "Background operation stopped. Any completed outputs remain available."
                            .into();
                    for item in &mut self.queue {
                        item.status = "Ready".into();
                    }
                }
            }
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Make room for detail");
        ui.label(RichText::new("OUTPUT SETTINGS").small().color(MUTED));
        ui.add_space(5.0);
        let old = serde_json::to_string(&self.prefs.effective_options()).ok();
        egui::ComboBox::from_label("Size by")
            .selected_text(
                [
                    "Scale",
                    "Width",
                    "Height",
                    "Fit within",
                    "Exact size (stretch)",
                ][self.prefs.size_mode.min(4)],
            )
            .show_ui(ui, |ui| {
                for (i, label) in [
                    "Scale",
                    "Width",
                    "Height",
                    "Fit within",
                    "Exact size (stretch)",
                ]
                .iter()
                .enumerate()
                {
                    ui.selectable_value(&mut self.prefs.size_mode, i, *label);
                }
            });
        if self.prefs.size_mode == 0 {
            ui.horizontal_wrapped(|ui| {
                for scale in SCALE_PRESETS {
                    if ui
                        .selectable_value(&mut self.prefs.scale, scale, format!("{scale}×"))
                        .clicked()
                        && scale < 1.0
                    {
                        self.prefs.options.allow_downscale = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Custom scale");
                ui.add(
                    egui::DragValue::new(&mut self.prefs.scale)
                        .range(0.01..=64.0)
                        .speed(0.05)
                        .suffix("×"),
                );
            });
        } else {
            if self.prefs.size_mode != 2 {
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(
                        egui::DragValue::new(&mut self.prefs.width)
                            .range(1..=100_000)
                            .suffix(" px"),
                    );
                });
            }
            if self.prefs.size_mode != 1 {
                ui.horizontal(|ui| {
                    ui.label("Height");
                    ui.add(
                        egui::DragValue::new(&mut self.prefs.height)
                            .range(1..=100_000)
                            .suffix(" px"),
                    );
                });
            }
            ui.label(
                RichText::new(if self.prefs.size_mode == 4 {
                    "Exact size may distort the image."
                } else {
                    "Aspect ratio preserved. No crop or padding."
                })
                .small()
                .color(MUTED),
            );
        }
        ui.separator();
        egui::ComboBox::from_label("Filter")
            .selected_text(self.prefs.options.filter.label())
            .show_ui(ui, |ui| {
                for filter in Filter::ALL {
                    ui.selectable_value(&mut self.prefs.options.filter, filter, filter.label());
                }
            });
        ui.checkbox(
            &mut self.prefs.options.allow_downscale,
            "Allow smaller outputs",
        );
        let mut linear = self.prefs.options.color_space == ColorSpace::Linear;
        if ui
            .checkbox(&mut linear, "Filter in linear light")
            .on_hover_text("Avoids dark transitions in assumed-sRGB images. Uses more memory.")
            .changed()
        {
            self.prefs.options.color_space = if linear {
                ColorSpace::Linear
            } else {
                ColorSpace::Srgb
            };
        }
        ui.add(egui::Slider::new(&mut self.prefs.options.sharpen, 0.0..=3.0).text("Sharpen"));
        ui.separator();
        egui::ComboBox::from_label("Format")
            .selected_text(self.prefs.format.extension().to_uppercase())
            .show_ui(ui, |ui| {
                for format in OutputFormat::ALL {
                    ui.selectable_value(
                        &mut self.prefs.format,
                        format,
                        format.extension().to_uppercase(),
                    );
                }
            });
        if self.prefs.format == OutputFormat::Jpeg {
            ui.add(egui::Slider::new(&mut self.prefs.options.quality, 1..=100).text("Quality"));
        }
        if matches!(self.prefs.format, OutputFormat::Jpeg | OutputFormat::Bmp) {
            ui.horizontal(|ui| {
                ui.label("Alpha background");
                ui.color_edit_button_srgb(&mut self.prefs.options.background);
            });
        } else {
            ui.label(
                RichText::new("Lossless · transparency preserved")
                    .small()
                    .color(MUTED),
            );
        }
        ui.label("Output folder");
        ui.add(
            egui::TextEdit::singleline(&mut self.prefs.output_dir)
                .desired_width(f32::INFINITY)
                .hint_text("Choose a folder…"),
        );
        if ui.button("Choose output folder…").clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.prefs.output_dir = path.to_string_lossy().into_owned();
        }
        ui.checkbox(&mut self.prefs.overwrite, "Replace existing outputs")
            .on_hover_text("Source files are always protected.");
        egui::CollapsingHeader::new("Resource limits").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Memory MiB");
                ui.add(egui::DragValue::new(&mut self.prefs.options.memory_mib).range(64..=65_536));
            });
            ui.horizontal(|ui| {
                ui.label("Max pixels");
                ui.add(
                    egui::DragValue::new(&mut self.prefs.options.max_pixels)
                        .range(1..=1_000_000_000)
                        .speed(100_000),
                );
            });
            ui.label(
                RichText::new("Conservative estimates, not an OS memory quota.")
                    .small()
                    .color(MUTED),
            );
        });
        if serde_json::to_string(&self.prefs.effective_options()).ok() != old {
            self.after = None;
            self.dimensions = self.dimensions.map(|(src, _)| {
                (
                    src,
                    self.prefs
                        .effective_options()
                        .size
                        .dimensions(src.0, src.1)
                        .unwrap_or(src),
                )
            });
            self.message = "Settings changed. Build a preview to see the result.".into();
        }
    }

    fn simple_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Choose. Resize. Export.");
        ui.label("Scale ratio");
        let old = self.prefs.simple_scale;
        ui.horizontal_wrapped(|ui| {
            for scale in SCALE_PRESETS {
                ui.selectable_value(&mut self.prefs.simple_scale, scale, format!("{scale}×"));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Custom ratio");
            ui.add(
                egui::DragValue::new(&mut self.prefs.simple_scale)
                    .range(0.25..=64.0)
                    .speed(0.05)
                    .suffix("×"),
            );
        });
        if self.prefs.simple_scale != old {
            self.dimensions = self.dimensions.map(|(src, _)| {
                (
                    src,
                    Size::Scale(self.prefs.simple_scale)
                        .dimensions(src.0, src.1)
                        .unwrap_or(src),
                )
            });
        }
        ui.separator();
        ui.label("Destination folder (optional)");
        let default = platform::default_folder().display().to_string();
        ui.add(
            egui::TextEdit::singleline(&mut self.prefs.output_dir)
                .desired_width(f32::INFINITY)
                .hint_text(default),
        );
        if ui.button("Choose destination…").clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.prefs.output_dir = path.to_string_lossy().into_owned();
        }
        ui.label(
            RichText::new("Starts in your Photos/Pictures folder each time you open the app.")
                .small()
                .color(MUTED),
        );
        ui.separator();
        ui.label(format!(
            "{} JPEGs per image · quality 100",
            Filter::ALL.len()
        ));
        ui.label(
            RichText::new("Nearest · Bilinear · Catmull–Rom · Mitchell · Lanczos 3")
                .small()
                .color(MUTED),
        );
        ui.label(RichText::new("Linear-light filtering and gentle sharpening for smooth filters. Transparent areas become white.").small().color(MUTED));
        ui.label(RichText::new("Images stay in the queue after export. Remove them or clear the queue when you're finished.").small().color(MUTED));
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("THE BIG PICTURE").small().color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.prefs.simple || self.prefs.ai_mode {
                    ui.checkbox(&mut self.comparison, "Comparison wipe");
                }
                ui.add(
                    egui::Slider::new(&mut self.zoom, 1.0..=4.0)
                        .text("Zoom")
                        .show_value(false),
                );
            });
        });
        ui.separator();
        if let Some(before) = &self.before {
            if let Some((src, dst)) = self.dimensions {
                ui.horizontal(|ui| {
                    ui.label(format!("{} × {}", src.0, src.1));
                    ui.label(RichText::new("→").color(ACCENT));
                    ui.label(RichText::new(format!("{} × {}", dst.0, dst.1)).color(ACCENT));
                    ui.label(
                        RichText::new("  Preview fitted to screen")
                            .small()
                            .color(MUTED),
                    );
                });
            }
            let available = ui.available_size() - Vec2::new(20.0, 35.0);
            let ratio = before.size_vec2();
            let factor = (available.x / ratio.x).min(available.y / ratio.y).max(0.01) * self.zoom;
            let image_size = ratio * factor;
            egui::ScrollArea::both()
                .id_salt("preview-scroll")
                .show(ui, |ui| {
                    let (rect, response) = ui.allocate_exact_size(image_size, egui::Sense::drag());
                    let painter = ui.painter_at(rect);
                    // Checkerboard reveals transparency.
                    let tile = 20.0;
                    for y in 0..((rect.height() / tile).ceil() as usize).min(250) {
                        for x in 0..((rect.width() / tile).ceil() as usize).min(250) {
                            let square = egui::Rect::from_min_size(
                                rect.min + Vec2::new(x as f32 * tile, y as f32 * tile),
                                Vec2::splat(tile),
                            )
                            .intersect(rect);
                            painter.rect_filled(
                                square,
                                0.0,
                                if (x + y) % 2 == 0 {
                                    Color32::from_gray(43)
                                } else {
                                    Color32::from_gray(57)
                                },
                            );
                        }
                    }
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    if let Some(after) = &self.after {
                        painter.image(after.id(), rect, uv, Color32::WHITE);
                        if self.comparison {
                            if response.dragged()
                                && let Some(pos) = response.interact_pointer_pos()
                            {
                                self.wipe = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                            }
                            let divider = rect.left() + rect.width() * self.wipe;
                            let clip = egui::Rect::from_min_max(
                                rect.min,
                                egui::pos2(divider, rect.bottom()),
                            )
                            .intersect(ui.clip_rect());
                            painter.with_clip_rect(clip).image(
                                before.id(),
                                rect,
                                uv,
                                Color32::WHITE,
                            );
                            painter.line_segment(
                                [
                                    egui::pos2(divider, rect.top()),
                                    egui::pos2(divider, rect.bottom()),
                                ],
                                egui::Stroke::new(2.0, ACCENT),
                            );
                            painter.circle_filled(
                                egui::pos2(divider, rect.center().y),
                                9.0,
                                ACCENT,
                            );
                        }
                    } else {
                        painter.image(before.id(), rect, uv, Color32::WHITE);
                    }
                });
            ui.horizontal(|ui| {
                ui.label(RichText::new("ORIGINAL").small().color(MUTED));
                if self.after.is_some() {
                    ui.label(
                        RichText::new(if self.prefs.ai_mode {
                            "/ AI ENHANCED"
                        } else {
                            "/ UPSCALED"
                        })
                        .small()
                        .color(ACCENT),
                    );
                }
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.26).max(20.0));
                ui.label(
                    RichText::new("A little image.\nA bigger possibility.")
                        .size(32.0)
                        .color(Color32::from_gray(225)),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new(if self.prefs.simple {
                        "Drop an image here, or use Choose image.\nPNG · JPEG · WebP · TIFF · BMP"
                    } else {
                        "Drop images here, or use Add images.\nPNG · JPEG · WebP · TIFF · BMP"
                    })
                    .color(MUTED),
                );
                ui.add_space(12.0);
                ui.label(
                    RichText::new(if self.prefs.ai_mode {
                        "AI enhancement uploads your selected image to OpenAI."
                    } else {
                        "Local processing. No uploads in Simple or Advanced."
                    })
                    .small()
                    .color(ACCENT),
                );
            });
        }
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_credentials(ctx);
        self.poll(ctx);
        if let Some(inputs) = self.initial_inputs.take()
            && !inputs.is_empty()
        {
            self.add(inputs, ctx);
        }
        if !self.busy() {
            let dropped = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect::<Vec<_>>()
            });
            if !dropped.is_empty() {
                self.add(dropped, ctx);
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_pending = true;
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                branding::show_logo(ui, &self.logo, 64.0);
                ui.label(RichText::new("  /  IMAGE UPSCALER").small().color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("About & licenses").clicked() {
                        self.show_about = true;
                    }
                    let old = (self.prefs.simple, self.prefs.ai_mode);
                    let mut mode = if self.prefs.ai_mode {
                        2
                    } else if self.prefs.simple {
                        0
                    } else {
                        1
                    };
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        ui.selectable_value(&mut mode, 2, "AI Enhancement");
                        ui.selectable_value(&mut mode, 1, "Advanced");
                        ui.selectable_value(&mut mode, 0, "Simple");
                    });
                    self.prefs.simple = mode == 0;
                    self.prefs.ai_mode = mode == 2;
                    if old != (self.prefs.simple, self.prefs.ai_mode) {
                        self.clear_preview();
                        if !self.queue.is_empty() {
                            self.preview(ctx);
                        }
                    }
                });
            });
            ui.add_space(8.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if self.busy() {
                    ui.spinner();
                }
                ui.label(&self.message);
                if !self.exporting
                    && let Some(folder) = &self.last_export_folder
                    && ui
                        .button("Open folder")
                        .on_hover_text(folder.display().to_string())
                        .clicked()
                    && let Err(error) = open::that_detached(folder)
                {
                    self.error = Some(format!("Could not open export folder: {error}"));
                }
                if self.busy() && ui.button("Cancel").clicked() {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.message = "Cancelling after the current processing stage…".into();
                }
            });
            if self.exporting {
                ui.add(
                    egui::ProgressBar::new(
                        (self.completed + self.failed) as f32 / self.total_exports.max(1) as f32,
                    )
                    .show_percentage(),
                );
            }
            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(255, 158, 140), error);
            }
            ui.add_space(6.0);
        });
        egui::SidePanel::left("queue")
            .resizable(true)
            .default_width(235.0)
            .min_width(185.0)
            .show(ctx, |ui| {
                ui.heading(format!("Your images  {}", self.queue.len()));
                let busy = self.busy();
                ui.add_enabled_ui(!busy, |ui| {
                    if ui
                        .button(if self.prefs.simple {
                            "Choose image…"
                        } else {
                            "+ Add images…"
                        })
                        .clicked()
                        && let Some(paths) = rfd::FileDialog::new()
                            .add_filter("Images", crate::batch::EXTENSIONS)
                            .pick_files()
                    {
                        self.add(paths, ctx);
                    }
                    if !self.prefs.simple
                        && !self.prefs.ai_mode
                        && ui.button("+ Add folder…").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        self.add(vec![path], ctx);
                    }
                    if !self.prefs.simple && !self.prefs.ai_mode {
                        ui.checkbox(&mut self.prefs.recursive, "Include subfolders");
                    }
                });
                ui.separator();
                let mut select = None;
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - 100.0).max(40.0))
                    .show(ui, |ui| {
                        for (i, item) in self.queue.iter().enumerate() {
                            ui.add_enabled_ui(!busy, |ui| {
                                let name =
                                    item.path.file_name().unwrap_or_default().to_string_lossy();
                                if ui
                                    .selectable_label(i == self.selected, name)
                                    .on_hover_text(item.path.display().to_string())
                                    .clicked()
                                {
                                    select = Some(i);
                                }
                                ui.label(RichText::new(&item.status).small().color(
                                    if item.status.starts_with("Saved") {
                                        ACCENT
                                    } else {
                                        MUTED
                                    },
                                ));
                                if let Some(output) = &item.output {
                                    ui.label(RichText::new(output.display().to_string()).small())
                                        .on_hover_text(output.display().to_string());
                                }
                            });
                            ui.separator();
                        }
                    });
                if let Some(i) = select
                    && (self.selected != i || self.before.is_none())
                {
                    self.selected = i;
                    self.clear_preview();
                    self.preview(ctx);
                }
                ui.add_enabled_ui(!busy && !self.queue.is_empty(), |ui| {
                    if ui.button("Remove selected").clicked() {
                        self.ai.result = None;
                        self.queue.remove(self.selected);
                        self.selected = self.selected.min(self.queue.len().saturating_sub(1));
                        self.clear_preview();
                    }
                    if ui.button("Clear queue").clicked() {
                        self.ai.result = None;
                        self.queue.clear();
                        self.selected = 0;
                        self.clear_preview();
                    }
                });
            });
        egui::SidePanel::right("settings")
            .default_width(305.0)
            .min_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                let busy = self.busy();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if self.prefs.ai_mode {
                            self.ai_settings(ui, ctx);
                            return;
                        }
                        if self.prefs.simple {
                            self.simple_settings(ui);
                        } else {
                            self.settings(ui);
                        }
                        ui.separator();
                        if !self.prefs.simple
                            && ui
                                .add_enabled(
                                    !self.prefs.simple && !self.queue.is_empty(),
                                    egui::Button::new("Build preview")
                                        .min_size(Vec2::new(ui.available_width(), 36.0)),
                                )
                                .clicked()
                        {
                            self.preview(ctx);
                        }
                        let export = egui::Button::new(
                            RichText::new(format!(
                                "Export {} {}",
                                self.queue.len()
                                    * if self.prefs.simple {
                                        Filter::ALL.len()
                                    } else {
                                        1
                                    },
                                if self.prefs.simple {
                                    "JPEGs"
                                } else if self.queue.len() == 1 {
                                    "image"
                                } else {
                                    "images"
                                }
                            ))
                            .strong()
                            .color(Color32::from_rgb(15, 37, 31)),
                        )
                        .fill(ACCENT)
                        .min_size(Vec2::new(ui.available_width(), 42.0));
                        if ui.add_enabled(!self.queue.is_empty(), export).clicked() {
                            self.export(ctx);
                        }
                        ui.label(
                            RichText::new(if self.prefs.simple {
                                "Filenames: timestamp-original-algorithm-ratio.jpeg"
                            } else {
                                "Files gain -upscaled. Metadata is stripped. Originals stay safe."
                            })
                            .small()
                            .color(MUTED),
                        );
                    });
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.preview_ui(ui);
        });
        if self.show_about {
            egui::Window::new("About Big Picture")
                .open(&mut self.show_about)
                .default_width(620.0)
                .show(ctx, |ui| {
                    branding::show_logo(ui, &self.logo, 96.0);
                    ui.label(concat!("Version ", env!("CARGO_PKG_VERSION")));
                    ui.label("Local upscaling and optional AI enhancement.");
                    ui.separator();
                    ui.label("MIT licensed. The app adds no royalties, attribution requirement, or commercial-use restriction to processed images. Your rights to the input still apply.");
                    ui.label("Simple and Advanced process locally. AI Enhancement sends the selected image and instructions directly to OpenAI using your API key, with API charges and provider terms. AI can alter or invent details.");
                    ui.horizontal(|ui| {
                        ui.hyperlink_to("Image codecs", "https://github.com/image-rs/image");
                        ui.hyperlink_to("Resize engine", "https://github.com/Cykooz/fast_image_resize");
                        ui.hyperlink_to("Desktop toolkit", "https://github.com/emilk/egui");
                    });
                    egui::ScrollArea::vertical()
                        .max_height(450.0)
                        .show(ui, |ui| {
                            ui.label(include_str!("../docs/LICENSING.md"));
                        });
                });
        }
        if self.close_pending {
            if !self.busy() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                egui::Window::new("Processing is active")
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.label("Cancel processing and close after the current stage?");
                        if ui.button("Cancel and close").clicked() {
                            self.cancel.store(true, Ordering::Relaxed);
                        }
                        if ui.button("Keep working").clicked() {
                            self.close_pending = false;
                        }
                    });
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.prefs);
    }
}

fn thumbnail(image: &image::DynamicImage) -> egui::ColorImage {
    let rgba = image.thumbnail(1400, 1400).to_rgba8();
    egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    )
}

pub fn launch(inputs: Vec<PathBuf>) -> eframe::Result {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([980.0, 660.0])
            .with_icon(branding::icon())
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Big Picture",
        native,
        Box::new(move |cc| Ok(Box::new(Desktop::new(cc, inputs)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn assert_jpeg_quality_100(bytes: &[u8]) {
        let tables = bytes
            .windows(2)
            .enumerate()
            .filter(|(_, w)| *w == [0xff, 0xdb])
            .collect::<Vec<_>>();
        assert!(!tables.is_empty());
        for (offset, _) in tables {
            assert!(
                bytes[offset + 5..offset + 69].iter().all(|v| *v == 1),
                "JPEG quantization table does not match quality 100"
            );
        }
    }

    #[test]
    fn remembered_key_state_never_serializes_secrets_and_handles_failures() {
        let (mut app, _) = app();
        app.ai.key.push_str("private-key");
        app.ai.consent = true;
        app.ai.key_edited = true;
        app.credentials_finished(Err(credentials::Error::Unavailable), "save");
        assert!(!app.prefs.remember_ai_key);
        assert!(app.ai.key.as_str() == "private-key");
        app.credentials_finished(Ok(credentials::Outcome::Saved), "save");
        assert!(app.prefs.remember_ai_key);
        assert!(!app.ai.key_edited);
        let stored = serde_json::to_string(&app.prefs).unwrap();
        assert!(!stored.contains("private-key"));
        let restored: Preferences = serde_json::from_str(&stored).unwrap();
        assert!(restored.remember_ai_key);
        app.credentials_finished(Err(credentials::Error::Unavailable), "forget");
        assert!(app.prefs.remember_ai_key); // Never claim an unsuccessful delete succeeded.
        assert!(app.ai.vault_message.contains("may still be stored"));
        app.credentials_finished(
            Ok(credentials::Outcome::Loaded(Some(zeroize::Zeroizing::new(
                "reloaded-key".into(),
            )))),
            "load",
        );
        assert!(app.ai.key.as_str() == "reloaded-key");
        assert!(!app.ai.consent); // Reloading a key must not consent to an upload.
        app.credentials_finished(Ok(credentials::Outcome::Forgotten), "forget");
        assert!(!app.prefs.remember_ai_key);
        assert!(app.ai.key.is_empty());
        app.credentials_finished(Ok(credentials::Outcome::Loaded(None)), "load");
        assert!(!app.prefs.remember_ai_key);
        assert!(app.ai.vault_message.contains("No saved key"));
    }

    #[test]
    fn credential_worker_result_is_applied_without_using_image_worker_or_logging_secret() {
        let (mut app, ctx) = app();
        let (tx, rx) = mpsc::sync_channel(1);
        app.ai.vault_receiver = Some(rx);
        app.ai.vault_operation = Some("load");
        tx.send(Ok(credentials::Outcome::Loaded(Some(
            zeroize::Zeroizing::new("hidden-key".into()),
        ))))
        .unwrap_or_else(|_| panic!("mock channel disconnected"));
        app.poll_credentials(&ctx);
        assert!(app.ai.vault_receiver.is_none());
        assert!(app.ai.key.as_str() == "hidden-key");
        assert!(!app.ai.vault_message.contains("hidden-key"));
        assert!(!app.busy());
        let (tx, rx) = mpsc::sync_channel(1);
        app.ai.vault_receiver = Some(rx);
        app.ai.vault_operation = Some("forget");
        drop(tx);
        app.poll_credentials(&ctx);
        assert!(app.prefs.remember_ai_key);
        assert!(app.ai.vault_message.contains("Could not remove"));
    }

    #[test]
    fn ai_export_selected_format_corrects_extension_and_checks_the_final_destination() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("original.png");
        image::RgbaImage::new(8, 6).save(&input).unwrap();
        let (mut app, ctx) = app();
        app.prefs.ai_mode = true;
        app.queue.push(QueueItem {
            path: input.clone(),
            ..Default::default()
        });
        app.ai_generated(Ok(ai_fixture(&app, input)), &ctx);
        app.prefs.ai_format = OutputFormat::Jpeg;
        app.save_ai(dir.path().join("chosen.png"), &ctx);
        settle(&mut app, &ctx);
        let jpeg = dir.path().join("chosen.jpg");
        assert!(!dir.path().join("chosen.png").exists());
        let bytes = std::fs::read(&jpeg).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert_jpeg_quality_100(&bytes);
        assert_eq!(app.queue[0].output.as_ref(), Some(&jpeg));
        // A collision at the corrected path is checked before staging any file.
        app.save_ai(dir.path().join("chosen.png"), &ctx);
        assert!(!app.busy());
        assert!(app.error.is_some());
        assert_eq!(std::fs::read(jpeg).unwrap(), bytes);
        app.prefs.ai_format = OutputFormat::Png;
        app.save_ai(dir.path().join("lossless.jpg"), &ctx);
        settle(&mut app, &ctx);
        assert!(!dir.path().join("lossless.jpg").exists());
        assert_eq!(
            image::guess_format(&std::fs::read(dir.path().join("lossless.png")).unwrap()).unwrap(),
            image::ImageFormat::Png
        );
    }
    #[test]
    fn ai_credentials_and_prompt_are_never_saved_and_legacy_modes_still_load() {
        let (mut app, _) = app();
        app.ai.key.push_str("private-api-key");
        app.ai.consent = true;
        app.prefs.ai_mode = true;
        app.prefs.ai.prompt = "private-prompt".into();
        app.prefs.ai.scale = 1.0;
        let stored = serde_json::to_string(&app.prefs).unwrap();
        assert!(!stored.contains("private-api-key"));
        assert!(!stored.contains("private-prompt"));
        assert!(!stored.contains("consent"));
        let restored: Preferences = serde_json::from_str(&stored).unwrap();
        assert!(restored.ai_mode);
        assert_eq!(restored.ai.scale, 1.0);
        assert!(restored.ai.prompt.is_empty());
        for simple in [true, false] {
            let old: Preferences =
                serde_json::from_value(serde_json::json!({"simple":simple})).unwrap();
            assert_eq!(old.simple, simple);
            assert!(!old.ai_mode);
        }
    }

    fn ai_fixture(app: &Desktop, input: PathBuf) -> Box<crate::ai::Generated> {
        let settings = app.prefs.ai.clone();
        let plan = crate::ai::plan((8, 6), &settings).unwrap();
        Box::new(crate::ai::Generated {
            source: input,
            source_dimensions: (8, 6),
            before: image::DynamicImage::new_rgba8(8, 6),
            pixels: image::RgbaImage::from_pixel(
                plan.target.0,
                plan.target.1,
                image::Rgba([50, 100, 150, 128]),
            ),
            plan,
            settings,
        })
    }

    #[test]
    fn ai_selection_only_loads_source_and_never_generates_without_consent() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.png");
        image::RgbaImage::new(8, 6).save(&input).unwrap();
        let (mut app, ctx) = app();
        app.prefs.ai_mode = true;
        app.ai.key.push_str("not-a-real-key");
        app.add(vec![input], &ctx);
        settle(&mut app, &ctx);
        assert!(app.before.is_some());
        assert!(app.after.is_none());
        assert!(app.ai.result.is_none());
        app.generate_ai(&ctx);
        assert!(!app.busy());
        // Even a direct call to local Export cannot accidentally process AI mode.
        app.export(&ctx);
        assert!(!app.busy());
    }

    #[test]
    fn ai_saves_cached_full_pixels_without_a_key_and_protects_all_sources() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.png");
        let other = dir.path().join("other.png");
        image::RgbaImage::new(8, 6).save(&input).unwrap();
        image::RgbaImage::new(8, 6).save(&other).unwrap();
        let original = std::fs::read(&input).unwrap();
        let (mut app, ctx) = app();
        app.prefs.ai_mode = true;
        app.queue = vec![
            QueueItem {
                path: input.clone(),
                ..Default::default()
            },
            QueueItem {
                path: other.clone(),
                ..Default::default()
            },
        ];
        app.ai_generated(Ok(ai_fixture(&app, input.clone())), &ctx);
        assert!(app.after.is_some());
        assert_eq!(app.dimensions.unwrap().1, (16, 12));
        // Control edits must not change or regenerate a paid result on Save.
        app.prefs.ai.scale = 4.0;
        for ext in ["png", "jpeg", "webp"] {
            let output = dir.path().join(format!("saved.{ext}"));
            app.prefs.ai_format = OutputFormat::from_path(&output).unwrap();
            app.save_ai(output.clone(), &ctx);
            settle(&mut app, &ctx);
            assert!(app.error.is_none(), "{:?}", app.error);
            assert_eq!(image::image_dimensions(&output).unwrap(), (16, 12));
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(
                image::guess_format(&bytes).unwrap(),
                app.prefs.ai_format.image_format()
            );
            if ext == "jpeg" {
                assert_jpeg_quality_100(&bytes);
            }
            if ext == "png" {
                assert_eq!(
                    image::open(&output).unwrap().to_rgba8().get_pixel(0, 0)[3],
                    128
                );
            }
        }
        assert!(app.ai.key.is_empty());
        assert!(app.ai.result.is_some());
        app.prefs.ai_format = OutputFormat::Png;
        app.save_ai(other.clone(), &ctx);
        assert!(!app.busy());
        assert!(app.error.is_some());
        let output = dir.path().join("saved.png");
        let saved = std::fs::read(&output).unwrap();
        app.save_ai(output.clone(), &ctx);
        assert!(app.error.is_some());
        assert_eq!(std::fs::read(output).unwrap(), saved);
        assert_eq!(std::fs::read(input).unwrap(), original);
        assert_eq!(app.queue.len(), 2);
    }

    #[test]
    fn ai_failure_cancellation_and_mode_switch_preserve_the_previous_result() {
        let (mut app, ctx) = app();
        let input = PathBuf::from("source.png");
        app.prefs.ai_mode = true;
        app.queue.push(QueueItem {
            path: input.clone(),
            ..Default::default()
        });
        app.ai_generated(Ok(ai_fixture(&app, input.clone())), &ctx);
        let previous = app.ai.result.clone().unwrap();
        app.ai_generated(Err("mock failure".into()), &ctx);
        assert!(Arc::ptr_eq(&previous, app.ai.result.as_ref().unwrap()));
        app.cancel.store(true, Ordering::Relaxed);
        app.ai_generated(Ok(ai_fixture(&app, input)), &ctx);
        assert!(Arc::ptr_eq(&previous, app.ai.result.as_ref().unwrap()));
        app.clear_preview();
        app.preview(&ctx);
        assert!(!app.busy()); // Cached result is restored without loading or uploading.
        assert!(app.after.is_some());
        assert!(Arc::ptr_eq(&previous, app.ai.result.as_ref().unwrap()));
    }
    #[test]
    fn preferences_round_trip_and_modes() {
        let prefs = Preferences {
            simple: false,
            size_mode: 3,
            width: 100,
            height: 90,
            ..Default::default()
        };
        assert_eq!(
            prefs.effective_options().size.dimensions(10, 20).unwrap(),
            (45, 90)
        );
        let stored = serde_json::to_string(&prefs).unwrap();
        let restored: Preferences = serde_json::from_str(&stored).unwrap();
        assert_eq!(
            restored
                .effective_options()
                .size
                .dimensions(10, 20)
                .unwrap(),
            (45, 90)
        );
    }
    #[test]
    fn preview_thumbnail_is_bounded() {
        let image = image::DynamicImage::new_rgba8(1600, 800);
        assert_eq!(thumbnail(&image).size, [1400, 700]);
    }

    fn app() -> (Desktop, egui::Context) {
        let ctx = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = Desktop::new(&creation, vec![]);
        app.prefs.simple = false;
        app.initial_inputs = None;
        app.ai.vault_checked = true; // Tests never access the user's real credential store.
        (app, ctx)
    }

    fn settle(app: &mut Desktop, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while app.busy() && std::time::Instant::now() < deadline {
            app.poll(ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.busy(), "worker did not finish");
    }

    #[test]
    fn empty_desktop_and_all_size_controls_render_headlessly() {
        use eframe::App;
        let (mut app, ctx) = app();
        for mode in 0..7 {
            app.prefs.ai_mode = mode == 6;
            app.prefs.simple = mode == 5;
            app.prefs.size_mode = mode;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    if mode == 6 {
                        Vec2::new(980.0, 660.0)
                    } else {
                        Vec2::new(1280.0, 820.0)
                    },
                )),
                ..Default::default()
            };
            let result = ctx.run(input, |ctx| {
                app.update(ctx, &mut eframe::Frame::_new_kittest());
            });
            assert!(!result.shapes.is_empty());
        }
    }

    #[test]
    fn desktop_discovery_preview_and_export_use_shared_engine() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("fixture.png");
        image::RgbaImage::from_pixel(8, 6, image::Rgba([30, 80, 140, 255]))
            .save(&input)
            .unwrap();
        let (mut app, ctx) = app();
        app.add(vec![input], &ctx);
        settle(&mut app, &ctx);
        assert_eq!(app.queue.len(), 1);
        assert!(app.before.is_some() && app.after.is_some());
        assert_eq!(app.dimensions, Some(((8, 6), (16, 12))));
        app.prefs.output_dir = dir.path().join("out").to_string_lossy().into_owned();
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert_eq!((app.completed, app.failed), (1, 0));
        assert!(app.queue[0].status.starts_with("Saved"));
        assert_eq!(
            image::image_dimensions(app.queue[0].output.as_ref().unwrap()).unwrap(),
            (16, 12)
        );
    }

    #[test]
    fn desktop_batch_reports_decode_errors_and_keeps_going() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.png");
        let bad = dir.path().join("bad.png");
        image::RgbaImage::new(8, 6).save(&good).unwrap();
        std::fs::write(&bad, b"bad").unwrap();
        let (mut app, ctx) = app();
        app.queue = vec![
            QueueItem {
                path: bad,
                status: "Ready".into(),
                output: None,
                ..Default::default()
            },
            QueueItem {
                path: good,
                status: "Ready".into(),
                output: None,
                ..Default::default()
            },
        ];
        app.prefs.output_dir = dir.path().join("out").to_string_lossy().into_owned();
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert_eq!((app.completed, app.failed), (1, 1));
        assert!(app.queue[0].status.starts_with("Failed"));
        assert!(app.queue[1].status.starts_with("Saved"));
    }

    #[test]
    fn desktop_worker_disconnect_recovers_instead_of_staying_busy() {
        let (mut app, ctx) = app();
        let (tx, rx) = mpsc::sync_channel(1);
        app.receiver = Some(rx);
        drop(tx);
        app.poll(&ctx);
        assert!(!app.busy());
        assert!(app.error.unwrap().contains("worker stopped"));
    }

    #[test]
    fn desktop_preflight_failure_does_not_write_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let existing = out.join("input-upscaled.png");
        std::fs::write(&existing, b"existing").unwrap();
        let (mut app, ctx) = app();
        app.queue.push(QueueItem {
            path: source,
            status: "Ready".into(),
            output: None,
            ..Default::default()
        });
        app.prefs.output_dir = out.to_string_lossy().into_owned();
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert!(app.error.is_some());
        assert_eq!(std::fs::read(&existing).unwrap(), b"existing");
        app.prefs.overwrite = true;
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert!(app.error.is_none());
        assert_eq!(app.completed, 1);
    }

    #[test]
    fn export_folder_resets_on_load_while_other_preferences_survive() {
        let prefs = Preferences::default();
        assert!(prefs.simple);
        assert_eq!(prefs.simple_scale, 2.0);
        assert_eq!(prefs.export_folder().unwrap(), platform::default_folder());
        let old: Preferences =
            serde_json::from_str(r#"{"output_dir":"previous-export","scale":8.0,"format":"Png"}"#)
                .unwrap();
        assert!(old.simple);
        assert_eq!(old.simple_scale, 2.0);
        assert_eq!(old.export_folder().unwrap(), platform::default_folder());
        let stored = Preferences {
            simple: false,
            simple_scale: 3.5,
            output_dir: "custom destination".into(),
            ..old
        };
        assert_eq!(
            stored.export_folder().unwrap(),
            PathBuf::from("custom destination")
        );
        let json = serde_json::to_string(&stored).unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&json)
                .unwrap()
                .get("output_dir")
                .is_none()
        );
        let restored: Preferences = serde_json::from_str(&json).unwrap();
        assert!(!restored.simple);
        assert_eq!(restored.simple_scale, 3.5);
        assert_eq!(
            restored.export_folder().unwrap(),
            platform::default_folder()
        );
        let blank = Preferences {
            output_dir: " ".into(),
            ..stored
        };
        assert_eq!(blank.export_folder().unwrap(), platform::default_folder());
    }

    #[test]
    fn simple_export_writes_every_algorithm_at_quality_100_and_keeps_queue_and_preview() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("my image.png");
        image::RgbaImage::from_fn(8, 6, |x, y| {
            image::Rgba([(x * 30) as u8, (y * 40) as u8, 140, 255])
        })
        .save(&input)
        .unwrap();
        let original = std::fs::read(&input).unwrap();
        let (mut app, ctx) = app();
        app.prefs.simple = true;
        app.prefs.options.quality = 1;
        app.prefs.options.background = [0; 3];
        app.prefs.options.allow_downscale = true;
        app.add(vec![input.clone()], &ctx);
        settle(&mut app, &ctx);
        assert!(app.before.is_some());
        assert!(app.after.is_none()); // Source thumbnail doesn't perform an unnecessary upscale.
        app.prefs.output_dir = dir.path().join("new folder").to_string_lossy().into_owned();
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert_eq!((app.completed, app.failed, app.total_exports), (5, 0, 5));
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue[0].path, input.canonicalize().unwrap());
        assert_eq!(app.queue[0].saved, 5);
        assert_eq!(app.selected, 0);
        assert!(app.before.is_some());
        assert_eq!(
            app.last_export_folder.as_ref().unwrap(),
            &app.queue[0].output.as_ref().unwrap().parent().unwrap()
        );
        let outputs = std::fs::read_dir(app.prefs.export_folder().unwrap())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(outputs.len(), Filter::ALL.len());
        let mut timestamp = None;
        for filter in Filter::ALL {
            let suffix = format!("-my image-{}-2x.jpeg", simple::algorithm_name(filter));
            let output = outputs
                .iter()
                .find(|p| p.file_name().unwrap().to_string_lossy().ends_with(&suffix))
                .unwrap();
            let name = output.file_name().unwrap().to_string_lossy();
            let stamp = name.strip_suffix(&suffix).unwrap().parse::<u128>().unwrap();
            assert_eq!(*timestamp.get_or_insert(stamp), stamp);
            assert_eq!(image::image_dimensions(output).unwrap(), (16, 12));
            assert_eq!(
                image::guess_format(&std::fs::read(output).unwrap()).unwrap(),
                image::ImageFormat::Jpeg
            );
            // Quality 100 encodes both 8-bit JPEG quantization tables with all ones.
            let bytes = std::fs::read(output).unwrap();
            let tables = bytes
                .windows(2)
                .enumerate()
                .filter(|(_, w)| *w == [0xff, 0xdb])
                .collect::<Vec<_>>();
            assert!(!tables.is_empty());
            for (offset, _) in tables {
                assert!(bytes[offset + 5..offset + 69].iter().all(|v| *v == 1));
            }
        }
        assert_eq!(std::fs::read(input).unwrap(), original);
        // Editing the destination after export must not redirect Open folder.
        let exported_folder = app.last_export_folder.clone();
        app.prefs.output_dir = dir.path().join("next batch").to_string_lossy().into_owned();
        assert_eq!(app.last_export_folder, exported_folder);
        // Restarting resets the destination even after a successful export.
        let restored: Preferences =
            serde_json::from_str(&serde_json::to_string(&app.prefs).unwrap()).unwrap();
        assert_eq!(
            restored.export_folder().unwrap(),
            platform::default_folder()
        );
    }

    #[test]
    fn simple_batch_retains_errors_and_retry_clears_them() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.png");
        let bad = dir.path().join("bad.png");
        image::RgbaImage::new(8, 6).save(&good).unwrap();
        std::fs::write(&bad, b"bad").unwrap();
        let (mut app, ctx) = app();
        app.prefs.simple = true;
        app.prefs.output_dir = dir.path().join("out").to_string_lossy().into_owned();
        app.queue = vec![
            QueueItem {
                path: bad.clone(),
                ..Default::default()
            },
            QueueItem {
                path: good,
                ..Default::default()
            },
        ];
        app.selected = 1;
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert_eq!((app.completed, app.failed), (5, 5));
        assert_eq!(app.queue.len(), 2);
        assert_eq!(app.queue[0].path, bad);
        assert!(app.queue[0].status.starts_with("0 saved · 5 failed:"));
        assert_eq!(app.queue[1].saved, 5);
        assert_eq!(app.selected, 1);
        image::RgbaImage::new(8, 6).save(&bad).unwrap();
        app.export(&ctx);
        settle(&mut app, &ctx);
        assert_eq!((app.completed, app.failed), (10, 0));
        assert_eq!(app.queue.len(), 2);
        assert!(
            app.queue
                .iter()
                .all(|item| item.saved == 5 && item.failures.is_empty())
        );
    }

    #[test]
    fn simple_cancel_keeps_partial_and_fully_saved_images() {
        let (mut app, ctx) = app();
        app.prefs.simple = true;
        app.queue = vec![
            QueueItem {
                saved: 5,
                ..Default::default()
            },
            QueueItem {
                saved: 2,
                ..Default::default()
            },
        ];
        app.selected = 1;
        app.cancel.store(true, Ordering::Relaxed);
        let (tx, rx) = mpsc::sync_channel(1);
        app.receiver = Some(rx);
        app.exporting = true;
        tx.send(Event::End).unwrap();
        app.poll(&ctx);
        assert!(!app.busy());
        assert!(!app.exporting);
        assert_eq!(app.queue.len(), 2);
        assert_eq!(app.queue[0].saved, 5);
        assert_eq!(app.queue[1].saved, 2);
        assert!(app.queue[1].status.ends_with("cancelled"));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn simple_fractional_presets_export_smaller_images_with_every_filter() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.png");
        image::RgbaImage::new(16, 8).save(&input).unwrap();
        let (mut app, ctx) = app();
        app.prefs.simple = true;
        // Even extremely restrictive Advanced settings must not affect Simple
        // source loading, preview or exports.
        app.prefs.options.max_pixels = 1;
        app.prefs.options.memory_mib = 1;
        app.prefs.output_dir = dir.path().join("exports").to_string_lossy().into_owned();
        app.add(vec![input], &ctx);
        settle(&mut app, &ctx);
        assert!(app.before.is_some(), "{}", app.message);
        assert!(app.last_export_folder.is_none());
        for (scale, expected) in [(0.25, (4, 2)), (0.5, (8, 4))] {
            app.prefs.simple_scale = scale;
            app.export(&ctx);
            settle(&mut app, &ctx);
            assert_eq!((app.completed, app.failed), (5, 0), "{}", app.message);
            assert_eq!(app.queue.len(), 1);
            let folder = app.last_export_folder.as_ref().unwrap();
            let suffix = format!("-{scale}x.jpeg");
            let outputs: Vec<_> = std::fs::read_dir(folder)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .ends_with(&suffix)
                })
                .collect();
            assert_eq!(outputs.len(), Filter::ALL.len());
            for output in outputs {
                assert_eq!(image::image_dimensions(output).unwrap(), expected);
            }
        }
    }
}
