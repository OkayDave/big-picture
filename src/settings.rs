use anyhow::{Context, Result, bail, ensure};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Reconstruction filters, ordered from pixel art to photographic detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Filter {
    Nearest,
    Bilinear,
    CatmullRom,
    Mitchell,
    Lanczos3,
}
impl Filter {
    pub const ALL: [Self; 5] = [
        Self::Nearest,
        Self::Bilinear,
        Self::CatmullRom,
        Self::Mitchell,
        Self::Lanczos3,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest · pixel art",
            Self::Bilinear => "Bilinear · smooth",
            Self::CatmullRom => "Catmull–Rom · crisp",
            Self::Mitchell => "Mitchell · balanced",
            Self::Lanczos3 => "Lanczos 3 · detailed",
        }
    }
    pub(crate) fn algorithm(self) -> fast_image_resize::ResizeAlg {
        use fast_image_resize::{FilterType as F, ResizeAlg as A};
        match self {
            Self::Nearest => A::Nearest,
            Self::Bilinear => A::Convolution(F::Bilinear),
            Self::CatmullRom => A::Convolution(F::CatmullRom),
            Self::Mitchell => A::Convolution(F::Mitchell),
            Self::Lanczos3 => A::Convolution(F::Lanczos3),
        }
    }
}

/// sRGB is faster; linear light avoids darkening high-contrast transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum ColorSpace {
    Srgb,
    Linear,
}

/// Output codecs. WebP is lossless; quality applies only to JPEG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum OutputFormat {
    Png,
    Jpeg,
    Webp,
    Tiff,
    Bmp,
}
impl OutputFormat {
    pub const ALL: [Self; 5] = [Self::Png, Self::Jpeg, Self::Webp, Self::Tiff, Self::Bmp];
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
        }
    }
    pub fn from_path(path: &std::path::Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "webp" => Ok(Self::Webp),
            "tif" | "tiff" => Ok(Self::Tiff),
            "bmp" => Ok(Self::Bmp),
            _ => bail!("unsupported output extension: {}", path.display()),
        }
    }
    pub(crate) fn image_format(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Jpeg => image::ImageFormat::Jpeg,
            Self::Webp => image::ImageFormat::WebP,
            Self::Tiff => image::ImageFormat::Tiff,
            Self::Bmp => image::ImageFormat::Bmp,
        }
    }
}

/// Scale both axes, preserve the aspect ratio within bounds, or explicitly stretch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Size {
    Scale(f64),
    Fit {
        width: Option<u32>,
        height: Option<u32>,
    },
    Exact {
        width: u32,
        height: u32,
    },
}
impl Size {
    pub fn validate(self) -> Result<()> {
        match self {
            Self::Scale(scale) => ensure!(
                scale.is_finite() && scale > 0.0,
                "scale must be finite and positive"
            ),
            Self::Fit { width, height } => {
                ensure!(
                    width.is_some() || height.is_some(),
                    "provide a width or height"
                );
                ensure!(
                    width != Some(0) && height != Some(0),
                    "dimensions must be positive"
                );
            }
            Self::Exact { width, height } => {
                ensure!(width > 0 && height > 0, "dimensions must be positive")
            }
        }
        Ok(())
    }
    pub fn dimensions(self, width: u32, height: u32) -> Result<(u32, u32)> {
        self.validate()?;
        ensure!(
            width > 0 && height > 0,
            "source dimensions must be positive"
        );
        let (w, h) = match self {
            Self::Scale(scale) => {
                ensure!(
                    scale.is_finite() && scale > 0.0,
                    "scale must be finite and positive"
                );
                (f64::from(width) * scale, f64::from(height) * scale)
            }
            Self::Fit {
                width: w,
                height: h,
            } => {
                ensure!(w.is_some() || h.is_some(), "provide a width or height");
                ensure!(w != Some(0) && h != Some(0), "dimensions must be positive");
                // Integer ratios keep the limiting axis exact and the other within bounds.
                let (nw, nh) = match (w, h) {
                    (Some(w), Some(h))
                        if u64::from(w) * u64::from(height) > u64::from(h) * u64::from(width) =>
                    {
                        (
                            (u64::from(width) * u64::from(h) / u64::from(height)).max(1),
                            u64::from(h),
                        )
                    }
                    (Some(w), _) => (
                        u64::from(w),
                        (u64::from(height) * u64::from(w) / u64::from(width)).max(1),
                    ),
                    (None, Some(h)) => (
                        (u64::from(width) * u64::from(h) / u64::from(height)).max(1),
                        u64::from(h),
                    ),
                    (None, None) => unreachable!("validated above"),
                };
                return Ok((
                    nw.try_into().context("target width is too large")?,
                    nh.try_into().context("target height is too large")?,
                ));
            }
            Self::Exact { width, height } => (f64::from(width), f64::from(height)),
        };
        ensure!(
            w.is_finite()
                && h.is_finite()
                && w >= 1.0
                && h >= 1.0
                && w.round() <= f64::from(u32::MAX)
                && h.round() <= f64::from(u32::MAX),
            "target dimensions are zero or too large"
        );
        Ok((w.round() as u32, h.round() as u32))
    }
}

/// Validated identically by both frontends. Resource estimates are conservative,
/// not operating-system memory quotas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub size: Size,
    pub filter: Filter,
    pub color_space: ColorSpace,
    /// Unsharp-mask amount, 0 (off) through 3.
    pub sharpen: f32,
    pub quality: u8,
    pub background: [u8; 3],
    pub allow_downscale: bool,
    pub max_pixels: u64,
    pub memory_mib: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            size: Size::Scale(2.0),
            filter: Filter::Lanczos3,
            color_space: ColorSpace::Srgb,
            sharpen: 0.0,
            quality: 92,
            background: [255; 3],
            allow_downscale: false,
            max_pixels: 100_000_000,
            memory_mib: 512,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        self.size.validate()?;
        ensure!(
            self.sharpen.is_finite() && (0.0..=3.0).contains(&self.sharpen),
            "sharpen must be between 0 and 3"
        );
        ensure!(
            (1..=100).contains(&self.quality),
            "JPEG quality must be between 1 and 100"
        );
        ensure!(self.max_pixels > 0, "max-pixels must be positive");
        ensure!(
            self.memory_mib > 0 && self.memory_mib <= u64::MAX / (1024 * 1024),
            "invalid memory limit"
        );
        Ok(())
    }
    pub fn target(&self, width: u32, height: u32) -> Result<(u32, u32)> {
        self.validate()?;
        let (w, h) = self.size.dimensions(width, height)?;
        ensure!(
            self.allow_downscale || (w >= width && h >= height),
            "target would downscale; enable allow-downscale explicitly"
        );
        let src = u64::from(width) * u64::from(height);
        let dst = u64::from(w) * u64::from(h);
        ensure!(
            src <= self.max_pixels && dst <= self.max_pixels,
            "image exceeds the {} pixel limit",
            self.max_pixels
        );
        // Includes decoded 16-bit input, float/alpha workspaces, sharpening and encoding buffers.
        let mut bytes_per_pixel = if self.color_space == ColorSpace::Linear {
            80
        } else {
            48
        };
        if self.sharpen > 0.0 {
            bytes_per_pixel += 48;
        }
        let estimate = src
            .checked_add(dst)
            .and_then(|p| p.checked_mul(bytes_per_pixel));
        ensure!(
            estimate.is_some_and(|n| n <= self.memory_mib * 1024 * 1024),
            "estimated working memory exceeds {} MiB; increase memory-mib or reduce dimensions",
            self.memory_mib
        );
        Ok((w, h))
    }
}

/// Parse an RGB background such as `#ffffff` or `182030`.
pub fn parse_background(value: &str) -> Result<[u8; 3], String> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.is_ascii() {
        return Err("use six hexadecimal digits (e.g. #ffffff)".into());
    }
    let mut rgb = [0; 3];
    for (i, channel) in rgb.iter_mut().enumerate() {
        *channel = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| "invalid RGB background".to_string())?;
    }
    Ok(rgb)
}
