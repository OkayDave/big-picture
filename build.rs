fn main() {
    println!("cargo:rerun-if-changed=docs/branding/big-picture-app-icon.ico");
    #[cfg(feature = "gui")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("docs/branding/big-picture-app-icon.ico")
            .set("ProductName", "Big Picture")
            .set(
                "FileDescription",
                "Big Picture — image upscaler and AI enhancement",
            )
            .compile()
            .expect("compile the bundled Big Picture Windows icon and version information");
    }
}
