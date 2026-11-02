use crate::{ColorSpace, Options, OutputFormat};
use anyhow::{Context, Result, ensure};
use image::{DynamicImage, ImageDecoder, ImageReader, Rgba, RgbaImage};
use serde::Serialize;
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

/// Header information; dimensions reflect EXIF orientation, without decoding pixels.
#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub decoded_bytes: u64,
}

fn decoder(path: &Path, options: &Options) -> Result<(impl ImageDecoder, String)> {
    options.validate()?;
    let mut reader = ImageReader::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .with_guessed_format()?;
    let format = reader.format().context("unrecognized image format")?;
    // Do not silently flatten animations.
    if format == image::ImageFormat::WebP {
        let webp = image::codecs::webp::WebPDecoder::new(BufReader::new(File::open(path)?))?;
        ensure!(
            !webp.has_animation(),
            "animated WebP is not supported; export a still frame first"
        );
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(options.memory_mib * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .with_context(|| format!("read image header {}", path.display()))?;
    let (w, h) = decoder.dimensions();
    ensure!(
        u64::from(w) * u64::from(h) <= options.max_pixels,
        "input exceeds pixel limit"
    );
    ensure!(
        decoder.total_bytes() <= options.memory_mib * 1024 * 1024,
        "decoded input exceeds memory limit"
    );
    Ok((decoder, format!("{format:?}")))
}

pub fn inspect(path: &Path, options: &Options) -> Result<ImageInfo> {
    let (mut decoder, format) = decoder(path, options)?;
    let (mut width, mut height) = decoder.dimensions();
    use image::metadata::Orientation::*;
    if matches!(
        decoder.orientation()?,
        Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH
    ) {
        std::mem::swap(&mut width, &mut height);
    }
    Ok(ImageInfo {
        path: path.into(),
        width,
        height,
        format,
        decoded_bytes: decoder.total_bytes(),
    })
}

/// Decode within configured limits and apply EXIF orientation. Working/output depth is 8-bit sRGB.
pub fn load(path: &Path, options: &Options) -> Result<DynamicImage> {
    let (mut decoder, _) = decoder(path, options)?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)
        .with_context(|| format!("decode {}", path.display()))?;
    image.apply_orientation(orientation);
    Ok(image)
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Resize with alpha premultiplication handled by fast_image_resize; optional linear-light filtering.
pub fn resize(source: &DynamicImage, options: &Options) -> Result<RgbaImage> {
    let (w, h) = options.target(source.width(), source.height())?;
    let config = fast_image_resize::ResizeOptions::new().resize_alg(options.filter.algorithm());
    let mut resizer = fast_image_resize::Resizer::new();
    let mut output = if options.color_space == ColorSpace::Linear {
        let mut src = source.to_rgba32f();
        for pixel in src.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = to_linear(*channel);
            }
        }
        let mut dst = image::Rgba32FImage::new(w, h);
        resizer.resize(&src, &mut dst, &config)?;
        RgbaImage::from_fn(w, h, |x, y| {
            let p = dst.get_pixel(x, y).0;
            Rgba([
                quantize(to_srgb(p[0].max(0.0))),
                quantize(to_srgb(p[1].max(0.0))),
                quantize(to_srgb(p[2].max(0.0))),
                quantize(p[3]),
            ])
        })
    } else {
        let src = source.to_rgba8();
        let mut dst = RgbaImage::new(w, h);
        resizer.resize(&src, &mut dst, &config)?;
        dst
    };
    if options.sharpen > 0.0 {
        // Blur premultiplied color so invisible RGB cannot leak into visible edges.
        let premultiplied = image::Rgba32FImage::from_fn(w, h, |x, y| {
            let p = output.get_pixel(x, y).0;
            let a = f32::from(p[3]) / 255.0;
            image::Rgba([
                f32::from(p[0]) * a,
                f32::from(p[1]) * a,
                f32::from(p[2]) * a,
                a,
            ])
        });
        let blurred = image::imageops::blur(&premultiplied, 1.0);
        for (pixel, blur) in output.pixels_mut().zip(blurred.pixels()) {
            if pixel[3] > 0 && blur[3] > 0.00001 {
                for c in 0..3 {
                    let original = f32::from(pixel[c]);
                    pixel[c] = (original + options.sharpen * (original - blur[c] / blur[3]))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    Ok(output)
}
fn quantize(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A successful, atomically committed output.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessReport {
    pub input: PathBuf,
    pub output: PathBuf,
    pub input_width: u32,
    pub input_height: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub elapsed_ms: u128,
}

pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "cancelled");
    Ok(())
}

/// Protect source images, write to a sibling temporary file, then commit atomically.
/// Cancellation is checked between decoding, resizing and encoding; kernels are not interrupted.
pub fn process(
    input: &Path,
    output: &Path,
    options: &Options,
    overwrite: bool,
    cancel: &AtomicBool,
) -> Result<ProcessReport> {
    let start = Instant::now();
    check_cancel(cancel)?;
    OutputFormat::from_path(output)?;
    validate_destination(input, output, overwrite)?;
    let info = inspect(input, options)?;
    options.target(info.width, info.height)?;
    let source = load(input, options)?;
    check_cancel(cancel)?;
    let result = resize(&source, options)?;
    drop(source);
    check_cancel(cancel)?;
    let bytes = save_pixels(input, output, &result, options, overwrite, cancel)?;
    Ok(ProcessReport {
        input: input.into(),
        output: output.into(),
        input_width: info.width,
        input_height: info.height,
        width: result.width(),
        height: result.height(),
        bytes,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

/// Save already-processed pixels without decoding, resizing, or contacting a service.
/// Uses the same source protection and atomic commit as conventional processing.
pub fn save_pixels(
    input: &Path,
    output: &Path,
    result: &RgbaImage,
    options: &Options,
    overwrite: bool,
    cancel: &AtomicBool,
) -> Result<u64> {
    options.validate()?;
    check_cancel(cancel)?;
    let format = OutputFormat::from_path(output)?;
    validate_destination(input, output, overwrite)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::new(temp.as_file_mut());
        if format == OutputFormat::Jpeg || format == OutputFormat::Bmp {
            let rgb = image::RgbImage::from_fn(result.width(), result.height(), |x, y| {
                let p = result.get_pixel(x, y).0;
                let a = u32::from(p[3]);
                image::Rgb(std::array::from_fn(|i| {
                    ((u32::from(p[i]) * a + u32::from(options.background[i]) * (255 - a) + 127)
                        / 255) as u8
                }))
            });
            if format == OutputFormat::Jpeg {
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, options.quality)
                    .encode_image(&rgb)?;
            } else {
                DynamicImage::ImageRgb8(rgb).write_to(&mut writer, format.image_format())?;
            }
        } else {
            DynamicImage::ImageRgba8(result.clone())
                .write_to(&mut writer, format.image_format())?;
        }
        writer.flush()?;
    }
    temp.as_file().sync_all()?;
    let bytes = temp.as_file().metadata()?.len();
    check_cancel(cancel)?;
    // Recheck source identity in case paths changed while processing.
    validate_destination(input, output, overwrite)?;
    if overwrite {
        temp.persist(output).map_err(|e| e.error)?;
    } else {
        temp.persist_noclobber(output).map_err(|e| e.error)?;
    }
    Ok(bytes)
}

pub fn validate_destination(input: &Path, output: &Path, overwrite: bool) -> Result<()> {
    if output.exists() {
        ensure!(
            !same_file::is_same_file(input, output)?,
            "output would replace a source image: {}",
            output.display()
        );
        ensure!(
            overwrite,
            "output exists (enable overwrite): {}",
            output.display()
        );
        ensure!(
            output.is_file(),
            "output is not a regular file: {}",
            output.display()
        );
    }
    Ok(())
}
