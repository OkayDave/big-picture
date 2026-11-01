//! The approved Expand artwork, embedded so the desktop app stays self-contained.
use eframe::egui;

const LOGO: &[u8] = include_bytes!("../../docs/branding/big-picture-logo-dark.png");
const ICON: &[u8] = include_bytes!("../../docs/branding/big-picture-app-icon-256.png");

pub(super) fn logo(ctx: &egui::Context) -> egui::TextureHandle {
    let rgba = image::load_from_memory(LOGO)
        .expect("the bundled Big Picture logo is a valid PNG")
        .into_rgba8();
    ctx.load_texture(
        "big-picture-logo",
        egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    )
}

pub(super) fn show_logo(ui: &mut egui::Ui, logo: &egui::TextureHandle, height: f32) {
    let size = logo.size_vec2() * (height / logo.size_vec2().y);
    ui.add(egui::Image::new((logo.id(), size)).alt_text("Big Picture"));
}

pub(super) fn icon() -> egui::IconData {
    let rgba = image::load_from_memory(ICON)
        .expect("the bundled Big Picture icon is a valid PNG")
        .into_rgba8();
    egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}
