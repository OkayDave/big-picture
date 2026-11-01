use big_picture::{
    ColorSpace, Filter, Options, OutputFormat, Size, inspect, load, process, resize,
};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader, Rgba, RgbaImage};
use std::{path::Path, sync::atomic::AtomicBool};

fn fixture(path: &Path) {
    RgbaImage::from_fn(6, 4, |x, y| {
        Rgba([
            (x * 40) as u8,
            (y * 60) as u8,
            100,
            if x == 0 { 0 } else { 255 },
        ])
    })
    .save(path)
    .unwrap();
}
fn opts(size: Size) -> Options {
    Options {
        size,
        ..Default::default()
    }
}

#[test]
fn target_scale_and_fractional_scale() {
    assert_eq!(Size::Scale(2.0).dimensions(13, 7).unwrap(), (26, 14));
    assert_eq!(Size::Scale(2.5).dimensions(13, 7).unwrap(), (33, 18));
    assert_eq!(Size::Scale(0.25).dimensions(1, 7).unwrap(), (1, 2));
    assert_eq!(Size::Scale(0.5).dimensions(3, 1).unwrap(), (2, 1));
}
#[test]
fn fit_preserves_aspect_and_does_not_crop() {
    for (size, expected) in [
        (
            Size::Fit {
                width: Some(120),
                height: None,
            },
            (120, 80),
        ),
        (
            Size::Fit {
                width: None,
                height: Some(80),
            },
            (120, 80),
        ),
        (
            Size::Fit {
                width: Some(200),
                height: Some(80),
            },
            (120, 80),
        ),
        (
            Size::Exact {
                width: 200,
                height: 80,
            },
            (200, 80),
        ),
    ] {
        assert_eq!(size.dimensions(30, 20).unwrap(), expected);
    }
}
#[test]
fn fit_never_exceeds_bounds_for_many_aspect_ratios() {
    for w in 1..35 {
        for h in 1..35 {
            let (x, y) = Size::Fit {
                width: Some(103),
                height: Some(79),
            }
            .dimensions(w, h)
            .unwrap();
            assert!(x <= 103 && y <= 79 && x > 0 && y > 0);
        }
    }
}
#[test]
fn invalid_sizes_and_overflows_are_rejected() {
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        assert!(Size::Scale(scale).dimensions(2, 2).is_err());
    }
    assert!(Size::Scale(2.0).dimensions(u32::MAX, 2).is_err());
    assert!(Size::Scale(2.0).dimensions(0, 1).is_err());
    assert!(
        Size::Fit {
            width: None,
            height: None
        }
        .dimensions(1, 1)
        .is_err()
    );
    assert!(
        Size::Fit {
            width: Some(0),
            height: None
        }
        .dimensions(1, 1)
        .is_err()
    );
    assert!(
        Size::Exact {
            width: 0,
            height: 1
        }
        .dimensions(1, 1)
        .is_err()
    );
}
#[test]
fn downscaling_requires_explicit_opt_in() {
    let mut options = opts(Size::Scale(0.5));
    assert!(options.target(20, 10).is_err());
    options.allow_downscale = true;
    assert_eq!(options.target(20, 10).unwrap(), (10, 5));
}
#[test]
fn limits_and_bad_options_fail_before_allocation() {
    let mut options = Options {
        max_pixels: 10,
        ..Default::default()
    };
    assert!(options.target(2, 2).is_err());
    options.max_pixels = 100_000_000;
    options.memory_mib = 1;
    assert!(options.target(400, 300).is_err());
    for quality in [0, 101, 255] {
        options.quality = quality;
        assert!(options.validate().is_err());
    }
    options.quality = 92;
    for sharpen in [-1.0, 4.0, f32::NAN] {
        options.sharpen = sharpen;
        assert!(options.validate().is_err());
    }
    options.sharpen = 0.0;
    options.memory_mib = u64::MAX;
    assert!(options.validate().is_err());
}
#[test]
fn nearest_repeats_exact_pixel_blocks() {
    let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(3, 2, |x, y| {
        Rgba([(x * 70) as u8, (y * 130) as u8, 20, 255])
    }));
    let result = resize(
        &source,
        &Options {
            filter: Filter::Nearest,
            ..Default::default()
        },
    )
    .unwrap();
    let input = source.to_rgba8();
    for (x, y, pixel) in result.enumerate_pixels() {
        assert_eq!(pixel, input.get_pixel(x / 2, y / 2));
    }
}
#[test]
fn filters_preserve_constant_opaque_color_in_both_spaces() {
    let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(5, 3, Rgba([63, 127, 190, 255])));
    for filter in Filter::ALL {
        for color_space in [ColorSpace::Srgb, ColorSpace::Linear] {
            let result = resize(
                &source,
                &Options {
                    filter,
                    color_space,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(result.dimensions(), (10, 6));
            for pixel in result.pixels() {
                for (a, b) in pixel.0.into_iter().zip([63, 127, 190, 255]) {
                    assert!(a.abs_diff(b) <= 1, "{filter:?} {color_space:?}: {pixel:?}");
                }
            }
        }
    }
}
#[test]
fn alpha_is_filtered_without_hidden_color_bleed() {
    let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 0])
        } else {
            Rgba([0, 0, 255, 255])
        }
    }));
    for filter in [
        Filter::Bilinear,
        Filter::CatmullRom,
        Filter::Mitchell,
        Filter::Lanczos3,
    ] {
        for color_space in [ColorSpace::Srgb, ColorSpace::Linear] {
            for sharpen in [0.0, 1.0] {
                let result = resize(
                    &source,
                    &Options {
                        size: Size::Scale(4.0),
                        filter,
                        color_space,
                        sharpen,
                        ..Default::default()
                    },
                )
                .unwrap();
                for p in result.pixels().filter(|p| p[3] > 32) {
                    assert!(p[0] < 4 && p[2] > 250, "{filter:?} {color_space:?}: {p:?}");
                }
            }
        }
    }
}
#[test]
fn linear_light_is_brighter_at_black_white_transition() {
    let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(2, 1, |x, _| {
        Rgba([
            if x == 0 { 0 } else { 255 },
            if x == 0 { 0 } else { 255 },
            if x == 0 { 0 } else { 255 },
            255,
        ])
    }));
    let mut options = Options {
        filter: Filter::Bilinear,
        ..Default::default()
    };
    let srgb = resize(&source, &options).unwrap();
    options.color_space = ColorSpace::Linear;
    let linear = resize(&source, &options).unwrap();
    assert!(linear.get_pixel(1, 0)[0] > srgb.get_pixel(1, 0)[0] + 30);
}
#[test]
fn sharpening_preserves_alpha() {
    let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(4, 4, |x, y| {
        Rgba([(x * 60) as u8, (y * 50) as u8, 80, ((x + y) * 35) as u8])
    }));
    let ordinary = resize(&source, &Options::default()).unwrap();
    let sharp = resize(
        &source,
        &Options {
            sharpen: 1.5,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        ordinary
            .pixels()
            .zip(sharp.pixels())
            .all(|(a, b)| a[3] == b[3])
    );
}
#[test]
fn every_output_codec_round_trips_dimensions_and_signature() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.png");
    fixture(&input);
    for format in OutputFormat::ALL {
        let output = dir.path().join(format!("out.{}", format.extension()));
        let report = process(
            &input,
            &output,
            &Options::default(),
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((report.width, report.height), (12, 8));
        assert!(report.bytes > 0);
        let reader = ImageReader::open(&output)
            .unwrap()
            .with_guessed_format()
            .unwrap();
        let actual = format!("{:?}", reader.format().unwrap()).to_lowercase();
        assert_eq!(
            actual,
            if format == OutputFormat::Jpeg {
                "jpeg"
            } else {
                format.extension()
            }
        );
        assert_eq!(image::image_dimensions(&output).unwrap(), (12, 8));
        let decoded = image::open(&output).unwrap().to_rgba8();
        if matches!(
            format,
            OutputFormat::Png | OutputFormat::Webp | OutputFormat::Tiff
        ) {
            assert_eq!(decoded.get_pixel(0, 0)[3], 0);
        } else {
            assert_eq!(decoded.get_pixel(0, 0)[3], 255);
        }
    }
}
#[test]
fn jpeg_and_bmp_flatten_alpha_to_background() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.png");
    RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 0]))
        .save(&input)
        .unwrap();
    let options = Options {
        background: [20, 80, 140],
        ..Default::default()
    };
    for ext in ["jpg", "bmp"] {
        let output = dir.path().join(format!("out.{ext}"));
        process(&input, &output, &options, false, &AtomicBool::new(false)).unwrap();
        let p = image::open(&output).unwrap().to_rgb8().get_pixel(0, 0).0;
        assert!(
            p.into_iter()
                .zip(options.background)
                .all(|(a, b)| a.abs_diff(b) <= 3)
        );
    }
}
#[test]
fn no_overwrite_and_source_protection_leave_existing_bytes_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.png");
    fixture(&input);
    let original = std::fs::read(&input).unwrap();
    assert!(
        process(
            &input,
            &input,
            &Options::default(),
            true,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&input).unwrap(), original);
    let output = dir.path().join("out.png");
    std::fs::write(&output, b"existing").unwrap();
    assert!(
        process(
            &input,
            &output,
            &Options::default(),
            false,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    process(
        &input,
        &output,
        &Options::default(),
        true,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(image::image_dimensions(&output).unwrap(), (12, 8));
}
#[test]
fn hard_links_to_sources_are_protected() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    fixture(&input);
    std::fs::hard_link(&input, &output).unwrap();
    assert!(
        process(
            &input,
            &output,
            &Options::default(),
            true,
            &AtomicBool::new(false)
        )
        .is_err()
    );
}
#[test]
fn cancelled_operation_has_no_output_or_directory_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    fixture(&input);
    let output = dir.path().join("new/out.png");
    assert!(
        process(
            &input,
            &output,
            &Options::default(),
            false,
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(!output.parent().unwrap().exists());
}
#[test]
fn corrupt_input_does_not_damage_output_or_leave_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    std::fs::write(&input, b"not an image").unwrap();
    std::fs::write(&output, b"original output").unwrap();
    assert!(
        process(
            &input,
            &output,
            &Options::default(),
            true,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"original output");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn successful_write_cleans_staging_file_and_creates_parents() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    fixture(&input);
    let output = dir.path().join("nested/out.png");
    process(
        &input,
        &output,
        &Options::default(),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_dir(output.parent().unwrap()).unwrap().count(),
        1
    );
}
#[test]
fn inspect_enforces_pixel_limit_without_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    fixture(&input);
    assert!(
        inspect(
            &input,
            &Options {
                max_pixels: 10,
                ..Default::default()
            }
        )
        .is_err()
    );
    let info = inspect(&input, &Options::default()).unwrap();
    assert_eq!((info.width, info.height), (6, 4));
}
#[test]
fn sixteen_bit_input_is_accepted_and_normalized_to_eight_bit_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("16.png");
    let output = dir.path().join("8.png");
    image::ImageBuffer::from_pixel(2, 2, image::Rgba([65535u16, 32768, 0, 65535]))
        .save(&input)
        .unwrap();
    process(
        &input,
        &output,
        &Options::default(),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    let decoder = ImageReader::open(&output).unwrap().into_decoder().unwrap();
    assert_eq!(decoder.color_type(), image::ColorType::Rgba8);
}
#[test]
fn exif_rotation_is_applied_and_metadata_is_not_copied() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("rotated.jpg");
    let output = dir.path().join("out.jpg");
    // Little-endian TIFF IFD with Orientation=6 (90° clockwise).
    let exif = vec![
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut encoder = image::codecs::jpeg::JpegEncoder::new(std::fs::File::create(&input).unwrap());
    encoder.set_exif_metadata(exif).unwrap();
    encoder
        .encode_image(&image::RgbImage::from_pixel(
            6,
            4,
            image::Rgb([30, 80, 120]),
        ))
        .unwrap();
    let info = inspect(&input, &Options::default()).unwrap();
    assert_eq!((info.width, info.height), (4, 6));
    let loaded = load(&input, &Options::default()).unwrap();
    assert_eq!((loaded.width(), loaded.height()), (4, 6));
    process(
        &input,
        &output,
        &Options::default(),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut decoder = ImageReader::open(&output).unwrap().into_decoder().unwrap();
    assert_eq!(decoder.dimensions(), (8, 12));
    assert!(decoder.exif_metadata().unwrap().is_none());
}
#[test]
fn rgb_background_parser_is_strict_and_unicode_safe() {
    use big_picture::settings::parse_background;
    assert_eq!(parse_background("#aB10ff").unwrap(), [171, 16, 255]);
    for value in ["", "fff", "#gg0000", "💚ab", "abcdef00"] {
        assert!(parse_background(value).is_err());
    }
}
