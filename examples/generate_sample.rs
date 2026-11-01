//! Generate an original, redistributable pixel landscape for trying the app.
fn main() -> anyhow::Result<()> {
    let destination = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "sample.png".into());
    let image = image::RgbImage::from_fn(240, 160, |x, y| {
        let sky = [35 + (y / 4) as u8, 62 + (y / 3) as u8, 92 + (y / 3) as u8];
        let sun = (i64::from(x) - 170).pow(2) + (i64::from(y) - 45).pow(2) < 18 * 18;
        let far = 95 - ((f64::from(x) / 31.0).sin() * 22.0) as i32;
        let near = 122 - ((f64::from(x) / 24.0 + 1.5).sin() * 18.0) as i32;
        let color = if y as i32 > near {
            [24, 57, 64]
        } else if y as i32 > far {
            [58, 112, 118]
        } else if sun {
            [245, 195, 119]
        } else {
            sky
        };
        image::Rgb(color)
    });
    image.save(&destination)?;
    println!("{}", destination.display());
    Ok(())
}
