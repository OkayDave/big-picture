#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() -> eframe::Result {
    big_picture::gui::launch(
        std::env::args_os()
            .skip(1)
            .map(std::path::PathBuf::from)
            .collect(),
    )
}
