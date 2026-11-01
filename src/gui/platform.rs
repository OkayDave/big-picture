//! Desktop paths and integration with the operating system.
use std::path::PathBuf;

pub(super) fn default_folder() -> PathBuf {
    dirs_next::picture_dir()
        .or_else(|| dirs_next::home_dir().map(|home| home.join("Pictures")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::env::temp_dir().join("big-picture"))
}

pub(super) fn default_output_dir() -> String {
    default_folder().to_string_lossy().into_owned()
}
