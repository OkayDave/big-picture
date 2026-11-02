//! Export the bundled photograph of Merlin for trying the app.
fn main() -> anyhow::Result<()> {
    let destination = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "sample.png".into());
    let image = image::load_from_memory(include_bytes!("../docs/samples/merlin.jpg"))?;
    image.save(&destination)?;
    println!("{}", destination.display());
    Ok(())
}
