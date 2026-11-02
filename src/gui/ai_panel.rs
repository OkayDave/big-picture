use super::*;
use crate::ai::{self, Colours, Enhancement, Quality, Strength};
use zeroize::{Zeroize, Zeroizing};

#[derive(Default)]
pub(super) struct State {
    // Deliberately separate from serializable Preferences, even when eframe saves.
    pub key: Zeroizing<String>,
    pub consent: bool,
    pub result: Option<Arc<ai::Generated>>,
    pub vault_checked: bool,
    pub vault_receiver: Option<Receiver<Result<credentials::Outcome, credentials::Error>>>,
    pub vault_operation: Option<&'static str>,
    pub vault_message: String,
    pub key_edited: bool,
}

fn output_path(mut path: PathBuf, format: OutputFormat) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        matches!(
            format,
            OutputFormat::Png | OutputFormat::Jpeg | OutputFormat::Webp
        ),
        "Choose PNG, JPEG or WebP for AI export."
    );
    anyhow::ensure!(
        path.file_name().is_some(),
        "Choose a filename for the AI result."
    );
    if OutputFormat::from_path(&path).ok() != Some(format) {
        path.set_extension(format.extension());
    }
    Ok(path)
}

fn choices<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    ui.label(label);
    ui.horizontal_wrapped(|ui| {
        for &(option, text) in options {
            ui.selectable_value(value, option, text);
        }
    });
}

impl Desktop {
    pub(super) fn start_credentials(
        &mut self,
        action: credentials::Action,
        operation: &'static str,
        ctx: &egui::Context,
    ) {
        if self.ai.vault_receiver.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.ai.vault_receiver = Some(rx);
        self.ai.vault_operation = Some(operation);
        self.ai.vault_message = format!("Accessing {}…", credentials::store_name());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(credentials::execute(action));
            ctx.request_repaint();
        });
    }

    pub(super) fn poll_credentials(&mut self, ctx: &egui::Context) {
        if !self.ai.vault_checked {
            self.ai.vault_checked = true;
            if self.prefs.remember_ai_key {
                self.start_credentials(credentials::Action::Load, "load", ctx);
            }
        }
        let outcome = self
            .ai
            .vault_receiver
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(credentials::Error::Unavailable)),
            });
        if let Some(outcome) = outcome {
            self.ai.vault_receiver = None;
            let operation = self.ai.vault_operation.take().unwrap_or("access");
            self.credentials_finished(outcome, operation);
        }
        if self.ai.vault_receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(super) fn credentials_finished(
        &mut self,
        outcome: Result<credentials::Outcome, credentials::Error>,
        operation: &str,
    ) {
        match outcome {
            Ok(credentials::Outcome::Loaded(Some(key))) => {
                self.ai.key = key;
                self.ai.key_edited = false;
                self.ai.consent = false;
                self.prefs.remember_ai_key = true;
                self.ai.vault_message =
                    "Saved key loaded. Cloud processing still requires your acknowledgement."
                        .into();
            }
            Ok(credentials::Outcome::Loaded(None)) => {
                self.prefs.remember_ai_key = false;
                self.ai.vault_message =
                    "No saved key was found. Paste a key to use this session.".into();
            }
            Ok(credentials::Outcome::Saved) => {
                self.prefs.remember_ai_key = true;
                self.ai.key_edited = false;
                self.ai.vault_message = format!("API key saved in {}.", credentials::store_name());
            }
            Ok(credentials::Outcome::Forgotten) => {
                self.prefs.remember_ai_key = false;
                self.ai.key.zeroize();
                self.ai.key_edited = false;
                self.ai.consent = false;
                self.ai.vault_message = "Saved key removed and session key cleared.".into();
            }
            Err(credentials::Error::InvalidKey) => {
                self.ai.vault_message =
                    "The key is empty or contains spaces. Enter a valid key before saving.".into();
            }
            Err(credentials::Error::Unavailable) => {
                self.ai.vault_message = if operation == "forget" {
                    "Could not remove the saved key. It may still be stored and load next time. Retry when the credential store is available."
                } else if operation == "save" {
                    "Could not confirm secure storage. The entered key can still be used in this session. Retry saving or check the OS credential store."
                } else {
                    "Could not load the saved key. Unlock your OS credential store and retry, or paste a key for this session."
                }.into();
            }
        }
    }

    pub(super) fn ai_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("AI Enhancement");
        ui.label("Enhances the selected image only.");
        if let Some(item) = self.queue.get(self.selected) {
            ui.label(
                RichText::new(item.path.file_name().unwrap_or_default().to_string_lossy())
                    .color(ACCENT),
            );
        }
        ui.separator();
        ui.label("OpenAI API key");
        ui.add_enabled_ui(self.ai.vault_receiver.is_none(), |ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut *self.ai.key)
                        .password(true)
                        .hint_text("Paste your API key")
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                self.ai.key_edited = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Clear key").clicked() {
                    self.ai.key.zeroize();
                    self.ai.consent = false;
                    self.ai.key_edited = true;
                }
            });
            let mut remember = self.prefs.remember_ai_key;
            if ui
                .add_enabled(
                    !self.ai.key.trim().is_empty() || remember,
                    egui::Checkbox::new(&mut remember, "Remember API key on this device"),
                )
                .changed()
            {
                if remember {
                    self.start_credentials(
                        credentials::Action::Save(self.ai.key.clone()),
                        "save",
                        ctx,
                    );
                } else {
                    self.start_credentials(credentials::Action::Forget, "forget", ctx);
                }
            }
            if self.prefs.remember_ai_key {
                ui.horizontal_wrapped(|ui| {
                    if self.ai.key_edited
                        && ui
                            .add_enabled(
                                !self.ai.key.trim().is_empty(),
                                egui::Button::new("Update saved key"),
                            )
                            .clicked()
                    {
                        self.start_credentials(
                            credentials::Action::Save(self.ai.key.clone()),
                            "save",
                            ctx,
                        );
                    }
                    if ui.button("Forget saved key").clicked() {
                        self.start_credentials(credentials::Action::Forget, "forget", ctx);
                    }
                    if ui.button("Reload saved key").clicked() {
                        self.start_credentials(credentials::Action::Load, "load", ctx);
                    }
                });
            } else if ui.button("Load saved key").clicked() {
                self.start_credentials(credentials::Action::Load, "load", ctx);
            }
        });
        ui.label(
            RichText::new(if self.prefs.remember_ai_key {
                format!(
                    "Saved in {}. {}",
                    credentials::store_name(),
                    if self.ai.key_edited {
                        "Edits stay in this session until you update the saved key."
                    } else {
                        "Loaded automatically on launch."
                    }
                )
            } else {
                "Session only unless you choose Remember. Keys never go in app preferences.".into()
            })
            .small()
            .color(MUTED),
        );
        if !self.ai.vault_message.is_empty() {
            ui.label(RichText::new(&self.ai.vault_message).small());
        }
        ui.hyperlink_to("Get an API key", "https://platform.openai.com/api-keys");
        ui.separator();
        let settings = &mut self.prefs.ai;
        ui.label("Output scale");
        ui.horizontal_wrapped(|ui| {
            for scale in [0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 8.0] {
                ui.selectable_value(&mut settings.scale, scale, format!("{scale}×"));
            }
        });
        ui.add(
            egui::DragValue::new(&mut settings.scale)
                .range(0.25..=8.0)
                .speed(0.05)
                .prefix("Custom: ")
                .suffix("×"),
        );
        let strengths = [
            (Strength::Off, "Off"),
            (Strength::Light, "Light"),
            (Strength::Strong, "Strong"),
        ];
        choices(
            ui,
            "Clean up noise and artefacts",
            &mut settings.cleanup,
            &strengths,
        );
        choices(ui, "Enhance details", &mut settings.detail, &strengths);
        choices(
            ui,
            "Colours",
            &mut settings.colours,
            &[
                (Colours::Original, "Original"),
                (Colours::Natural, "Natural"),
                (Colours::Vibrant, "Vibrant"),
            ],
        );
        ui.checkbox(&mut settings.exposure, "Improve exposure and contrast");
        choices(
            ui,
            "Overall enhancement",
            &mut settings.enhancement,
            &[
                (Enhancement::Subtle, "Subtle"),
                (Enhancement::Balanced, "Balanced"),
                (Enhancement::Creative, "Creative"),
            ],
        );
        ui.label("Additional instructions (optional)");
        ui.add(
            egui::TextEdit::multiline(&mut settings.prompt)
                .desired_rows(3)
                .char_limit(4000)
                .desired_width(f32::INFINITY)
                .hint_text("For example: keep the grain and warm lighting"),
        );
        ui.label(RichText::new("Controls guide the AI; their effects are approximate. AI can change faces, text and other details.").small().color(MUTED));
        choices(
            ui,
            "Generation quality",
            &mut settings.quality,
            &[(Quality::Standard, "Standard"), (Quality::High, "High")],
        );
        ui.label(RichText::new("High uses more API output tokens and generally costs more. Each Generate is a new paid request; the amount depends on image size and usage.").small());
        ui.hyperlink_to(
            "OpenAI pricing and cost calculator",
            "https://developers.openai.com/api/docs/guides/image-generation#calculating-costs",
        );
        ui.label(
            RichText::new(format!("Model: {}", ai::MODEL))
                .small()
                .color(MUTED),
        );
        let mut valid = false;
        if let Some((source, _)) = self.dimensions {
            match ai::plan(source, settings) {
                Ok(plan) => {
                    valid = true;
                    ui.label(format!(
                        "Next output: {} × {}",
                        plan.target.0, plan.target.1
                    ));
                    ui.label(RichText::new(format!("AI render: {} × {}. Local Lanczos resizing reaches the exact output size when needed.", plan.api.0, plan.api.1)).small().color(MUTED));
                }
                Err(error) => {
                    ui.colored_label(Color32::from_rgb(255, 158, 140), error.to_string());
                }
            }
        }
        ui.separator();
        ui.checkbox(&mut self.ai.consent, "Send the selected image and instructions to OpenAI using my API key. API charges apply.");
        ui.label(RichText::new("Generate may take up to 3 minutes. Cancel discards the pending result after the current stage; it cannot guarantee cancellation of OpenAI processing or charges.").small().color(MUTED));
        if ui
            .add_enabled(
                valid
                    && self.ai.consent
                    && !self.ai.key.trim().is_empty()
                    && self.ai.vault_receiver.is_none(),
                egui::Button::new("Generate enhancement · paid")
                    .min_size(Vec2::new(ui.available_width(), 40.0)),
            )
            .clicked()
        {
            self.generate_ai(ctx);
        }
        ui.separator();
        if let Some(result) = &self.ai.result {
            ui.label(
                RichText::new(format!(
                    "Current result: {} × {} · {}× · {:?} quality",
                    result.plan.target.0,
                    result.plan.target.1,
                    result.settings.scale,
                    result.settings.quality
                ))
                .color(ACCENT),
            );
            if result.settings != self.prefs.ai {
                ui.label("Settings changed. Saving keeps the current result; Generate applies the new settings.");
            }
        }
        choices(
            ui,
            "Export format",
            &mut self.prefs.ai_format,
            &[
                (OutputFormat::Png, "PNG"),
                (OutputFormat::Jpeg, "JPEG"),
                (OutputFormat::Webp, "WebP"),
            ],
        );
        ui.label(
            RichText::new(match self.prefs.ai_format {
                OutputFormat::Jpeg => "JPEG · quality 100 · transparent areas become white",
                OutputFormat::Webp => "WebP · lossless · preserves transparency",
                _ => "PNG · lossless · preserves transparency",
            })
            .small()
            .color(MUTED),
        );
        ui.label(
            RichText::new("The filename extension follows the selected export format.")
                .small()
                .color(MUTED),
        );
        if ui
            .add_enabled(
                self.ai.result.is_some(),
                egui::Button::new("Save current result… · no API charge")
                    .min_size(Vec2::new(ui.available_width(), 40.0)),
            )
            .clicked()
        {
            let filename = self
                .ai
                .result
                .as_ref()
                .and_then(|r| r.source.file_stem())
                .unwrap_or_default()
                .to_string_lossy();
            if let Some(output) = rfd::FileDialog::new()
                .set_directory(
                    self.prefs
                        .export_folder()
                        .unwrap_or_else(|_| platform::default_folder()),
                )
                .set_file_name(format!(
                    "{filename}-ai-enhanced.{}",
                    self.prefs.ai_format.extension()
                ))
                .add_filter(
                    self.prefs.ai_format.extension().to_uppercase(),
                    match self.prefs.ai_format {
                        OutputFormat::Jpeg => &["jpg", "jpeg"][..],
                        OutputFormat::Webp => &["webp"][..],
                        _ => &["png"][..],
                    },
                )
                .save_file()
            {
                self.save_ai(output, ctx);
            }
        }
        ui.label(RichText::new("Save exports the full result locally. Existing files are protected. Results stay in memory until you choose a different image, remove it, clear the queue or close the app.").small().color(MUTED));
    }

    pub(super) fn generate_ai(&mut self, ctx: &egui::Context) {
        if self.busy()
            || !self.prefs.ai_mode
            || !self.ai.consent
            || self.ai.key.trim().is_empty()
            || self.ai.vault_receiver.is_some()
        {
            return;
        }
        let Some(item) = self.queue.get(self.selected) else {
            return;
        };
        let input = item.path.clone();
        let key = self.ai.key.clone();
        let settings = self.prefs.ai.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.receiver = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.error = None;
        self.message = "Generating one AI enhancement with OpenAI… API charges apply.".into();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = ai::generate(&input, &key, &settings, &cancel)
                .map(Box::new)
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::AiGenerated(result));
            ctx.request_repaint();
        });
    }

    pub(super) fn ai_generated(
        &mut self,
        result: Result<Box<ai::Generated>, String>,
        ctx: &egui::Context,
    ) {
        self.receiver = None;
        match result {
            Ok(result) if !self.cancel.load(Ordering::Relaxed) => {
                self.show_ai_result(Arc::from(result), ctx);
            }
            Ok(_) => {
                self.message =
                    "AI result discarded after cancellation. OpenAI charges may still apply.".into()
            }
            Err(error) => {
                self.error = Some(error);
                self.message = "AI generation stopped. Any previous result is still available; check API usage before retrying.".into();
            }
        }
    }

    pub(super) fn show_ai_result(&mut self, result: Arc<ai::Generated>, ctx: &egui::Context) {
        self.dimensions = Some((result.source_dimensions, result.plan.target));
        self.before = Some(ctx.load_texture(
            "ai-before",
            thumbnail(&result.before),
            egui::TextureOptions::NEAREST,
        ));
        // Make only a bounded thumbnail, retaining full pixels for Save.
        let after = image::imageops::thumbnail(&result.pixels, 1400, 1400);
        self.after = Some(ctx.load_texture(
            "ai-after",
            egui::ColorImage::from_rgba_unmultiplied(
                [after.width() as usize, after.height() as usize],
                after.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        ));
        self.ai.result = Some(result);
        self.comparison = true;
        self.message = "AI result ready. Compare, then save without another API request.".into();
    }

    pub(super) fn save_ai(&mut self, output: PathBuf, ctx: &egui::Context) {
        if self.busy() || !self.prefs.ai_mode {
            return;
        }
        let Some(result) = self.ai.result.clone() else {
            return;
        };
        let output = match output_path(output, self.prefs.ai_format) {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        if self
            .queue
            .get(self.selected)
            .is_none_or(|item| item.path != result.source)
        {
            self.error =
                Some("Choose the image belonging to the current AI result before saving.".into());
            return;
        }
        // Protect every queued input, including aliases and hard links.
        let validation = self
            .queue
            .iter()
            .try_for_each(|item| crate::engine::validate_destination(&item.path, &output, false));
        if let Err(error) = validation {
            self.error = Some(format!("{error:#}"));
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.receiver = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.error = None;
        self.message = "Saving the existing AI result locally…".into();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let saved = crate::engine::save_pixels(
                &result.source,
                &output,
                &result.pixels,
                &Options {
                    quality: 100,
                    ..ai::options()
                },
                false,
                &cancel,
            )
            .map(|_| output)
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::AiSaved(saved));
            ctx.request_repaint();
        });
    }
}
