//! Optional desktop-only OpenAI image editing. Credentials never enter app settings;
//! optional OS credential storage is handled separately. No image cache is persisted.
use crate::{Options, Size};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

pub(crate) const MODEL: &str = "gpt-image-2.5-sunburst";
const ENDPOINT: &str = "https://api.openai.com/v1/images/edits";
const MAX_RESPONSE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Strength {
    Off,
    #[default]
    Light,
    Strong,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Colours {
    #[default]
    Original,
    Natural,
    Vibrant,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Enhancement {
    #[default]
    Subtle,
    Balanced,
    Creative,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Quality {
    #[default]
    Standard,
    High,
}
impl Quality {
    fn api_value(self) -> &'static str {
        match self {
            Self::Standard => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub scale: f64,
    pub cleanup: Strength,
    pub detail: Strength,
    pub colours: Colours,
    pub exposure: bool,
    pub enhancement: Enhancement,
    pub quality: Quality,
    // Prompts can contain private information; keep them in memory for this session only.
    #[serde(skip)]
    pub prompt: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            scale: 2.0,
            cleanup: Strength::Light,
            detail: Strength::Light,
            colours: Colours::Original,
            exposure: false,
            enhancement: Enhancement::Subtle,
            quality: Quality::Standard,
            prompt: String::new(),
        }
    }
}
impl Settings {
    pub fn prompt(&self) -> Result<String> {
        ensure!(
            self.prompt.chars().count() <= 4000,
            "additional instructions must be at most 4000 characters"
        );
        let cleanup = match self.cleanup {
            Strength::Off => "Do not denoise or remove compression artefacts.",
            Strength::Light => {
                "Gently reduce noise and compression artefacts, keeping natural texture."
            }
            Strength::Strong => {
                "Thoroughly reduce noise and compression artefacts, avoiding waxy surfaces."
            }
        };
        let detail = match self.detail {
            Strength::Off => "Do not add sharpening or invent fine details.",
            Strength::Light => "Subtly clarify existing fine details and edges without halos.",
            Strength::Strong => {
                "Strongly clarify fine details and sharpen edges while avoiding halos."
            }
        };
        let colours = match self.colours {
            Colours::Original => "Preserve the original colours and white balance.",
            Colours::Natural => "Improve colour balance naturally; retain realistic skin tones.",
            Colours::Vibrant => {
                "Enhance colour vibrancy while avoiding clipped or oversaturated colours."
            }
        };
        let exposure = if self.exposure {
            "Improve exposure and contrast, protecting highlight and shadow detail."
        } else {
            "Preserve the original exposure and contrast."
        };
        let enhancement = match self.enhancement {
            Enhancement::Subtle => {
                "Use subtle, conservative restoration. Prioritize fidelity over new detail."
            }
            Enhancement::Balanced => {
                "Use balanced restoration with moderate detail reconstruction."
            }
            Enhancement::Creative => {
                "Allow creative detail reconstruction while preserving the subject and scene."
            }
        };
        Ok(format!(
            "Enhance the supplied image. Preserve its composition, framing, original style, subject identity, facial features, lettering, logos and background transparency. Do not add or remove objects, crop, reframe or add borders. These preservation requirements apply throughout.\n{enhancement}\n{cleanup}\n{detail}\n{colours}\n{exposure}\nAdditional user instructions:\n{}",
            self.prompt.trim()
        ))
    }
}

pub(crate) fn options() -> Options {
    Options {
        memory_mib: 2048,
        max_pixels: 64_000_000,
        allow_downscale: true,
        ..Options::default()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Plan {
    pub target: (u32, u32),
    pub api: (u32, u32),
}
pub(crate) fn plan(source: (u32, u32), settings: &Settings) -> Result<Plan> {
    ensure!(
        settings.scale.is_finite() && (0.25..=8.0).contains(&settings.scale),
        "AI scale must be between 0.25× and 8×"
    );
    settings.prompt()?;
    let target = Options {
        size: Size::Scale(settings.scale),
        ..options()
    }
    .target(source.0, source.1)?;
    let ratio = f64::from(source.0) / f64::from(source.1);
    ensure!(
        (1.0 / 3.0..=3.0).contains(&ratio),
        "AI enhancement supports aspect ratios from 1:3 to 3:1. Crop this image first or use a local mode."
    );
    // Use documented non-experimental sizes, in multiples of 16. Prefer the
    // requested area, bounded to a useful minimum and 2560×1440's pixel count.
    let area = (f64::from(target.0) * f64::from(target.1)).clamp(1_048_576.0, 3_686_400.0);
    let mut best = None;
    for width in (256..=2560).step_by(16) {
        let height = ((f64::from(width) / ratio / 16.0).round() as u32) * 16;
        if !(256..=2560).contains(&height) {
            continue;
        }
        let pixels = width * height;
        let actual_ratio = f64::from(width) / f64::from(height);
        if !(655_360..=3_686_400).contains(&pixels) || !(1.0 / 3.0..=3.0).contains(&actual_ratio) {
            continue;
        }
        let score = (f64::from(pixels) / area).ln().abs() + 8.0 * (actual_ratio / ratio).ln().abs();
        if best.is_none_or(|(_, old)| score < old) {
            best = Some(((width, height), score));
        }
    }
    Ok(Plan {
        target,
        api: best.context("no supported AI output size")?.0,
    })
}

pub(crate) struct Generated {
    pub source: PathBuf,
    pub source_dimensions: (u32, u32),
    pub before: DynamicImage,
    pub pixels: RgbaImage,
    pub plan: Plan,
    pub settings: Settings,
}

pub(crate) fn generate(
    input: &Path,
    key: &str,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<Generated> {
    generate_at(input, key, settings, cancel, ENDPOINT)
}

fn generate_at(
    input: &Path,
    key: &str,
    settings: &Settings,
    cancel: &AtomicBool,
    endpoint: &str,
) -> Result<Generated> {
    crate::engine::check_cancel(cancel)?;
    ensure!(
        !key.trim().is_empty() && !key.trim().chars().any(char::is_whitespace),
        "Enter a valid OpenAI API key (without spaces)."
    );
    // Validate dimensions, prompt and memory before uploading or incurring a charge.
    let info = crate::inspect(input, &options())?;
    let plan = plan((info.width, info.height), settings)?;
    let source = crate::load(input, &options())?;
    let source = DynamicImage::ImageRgba8(source.into_rgba8());
    let before = source.thumbnail(1400, 1400);
    let transparent = source
        .as_rgba8()
        .is_some_and(|rgba| rgba.pixels().any(|p| p[3] != 255));
    let mut upload = Cursor::new(Vec::new());
    source.write_to(&mut upload, image::ImageFormat::Png)?;
    drop(source);
    let upload = upload.into_inner();
    ensure!(
        upload.len() < 50_000_000,
        "Normalized input exceeds OpenAI's 50 MB image limit; use a smaller source."
    );
    crate::engine::check_cancel(cancel)?;
    let body = reqwest::blocking::multipart::Form::new()
        .text("model", MODEL)
        .text("prompt", settings.prompt()?)
        .text("size", format!("{}x{}", plan.api.0, plan.api.1))
        .text("quality", settings.quality.api_value())
        .text("n", "1")
        .text("output_format", "png")
        .text(
            "background",
            if transparent { "transparent" } else { "opaque" },
        )
        .part(
            "image",
            reqwest::blocking::multipart::Part::bytes(upload)
                .file_name("image.png")
                .mime_str("image/png")?,
        );
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .context("could not initialize secure connection")?;
    // Exactly one request, with no application retries. Never expose raw provider
    // errors, response bodies, headers, or credentials in user-visible errors.
    let response = client.post(endpoint).bearer_auth(key.trim()).multipart(body).send()
        .map_err(|error| if error.is_timeout() {
            anyhow::anyhow!("OpenAI request timed out. It may still be billed; check your usage before generating again.")
        } else { anyhow::anyhow!("Could not reach OpenAI. Check your connection. The request may have reached the service; check usage before retrying.") })?;
    crate::engine::check_cancel(cancel)?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE + 1)
        .read_to_end(&mut bytes)
        .context("could not read OpenAI response; check usage before retrying")?;
    ensure!(
        bytes.len() as u64 <= MAX_RESPONSE,
        "OpenAI response exceeded the 64 MiB limit"
    );
    ensure!(
        (200..300).contains(&status),
        "{}",
        api_error(status, &bytes)
    );
    let pixels = decode_response(&bytes)?;
    crate::engine::check_cancel(cancel)?;
    let pixels = if (pixels.width(), pixels.height()) == plan.target {
        pixels.to_rgba8()
    } else {
        crate::resize(
            &pixels,
            &Options {
                size: Size::Exact {
                    width: plan.target.0,
                    height: plan.target.1,
                },
                ..options()
            },
        )?
    };
    crate::engine::check_cancel(cancel)?;
    Ok(Generated {
        source: input.into(),
        source_dimensions: (info.width, info.height),
        before,
        pixels,
        plan,
        settings: settings.clone(),
    })
}

fn api_error(status: u16, bytes: &[u8]) -> &'static str {
    let value: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    let code = value["error"]["code"].as_str().unwrap_or("");
    match (status, code) {
        (_, "insufficient_quota" | "billing_hard_limit_reached") => {
            "OpenAI quota or billing limit reached. Check your API billing and usage limits."
        }
        (_, "moderation_blocked" | "content_policy_violation") => {
            "OpenAI could not process this image or prompt. Adjust the image or instructions before trying again."
        }
        (401, _) => "OpenAI rejected the API key. Check or replace it.",
        (403 | 404, _) => {
            "Your OpenAI project cannot access this image model. Check model access and account verification."
        }
        (429, _) => {
            "OpenAI rate limit reached. Wait before generating again; also check your quota."
        }
        (400 | 413 | 422, _) => {
            "OpenAI rejected the image or instructions. Check the input and prompt before trying again."
        }
        _ => {
            "OpenAI could not complete the request. Check service status and your usage before generating again."
        }
    }
}

#[derive(Deserialize)]
struct ImageResponse {
    data: Vec<ImageData>,
}
#[derive(Deserialize)]
struct ImageData {
    b64_json: Option<String>,
}
fn decode_response(bytes: &[u8]) -> Result<DynamicImage> {
    let response: ImageResponse =
        serde_json::from_slice(bytes).context("OpenAI returned an invalid image response")?;
    ensure!(
        response.data.len() == 1,
        "OpenAI did not return exactly one image"
    );
    let encoded = response.data[0]
        .b64_json
        .as_deref()
        .context("OpenAI returned no image pixels")?;
    let decoded = STANDARD
        .decode(encoded)
        .context("OpenAI returned invalid image encoding")?;
    let (width, height) = image::ImageReader::new(Cursor::new(&decoded))
        .with_guessed_format()?
        .into_dimensions()?;
    let mut reader = image::ImageReader::new(Cursor::new(decoded)).with_guessed_format()?;
    ensure!(
        reader.format() == Some(image::ImageFormat::Png),
        "OpenAI returned an unexpected image format"
    );
    ensure!(
        width > 0
            && height > 0
            && width <= 3840
            && height <= 3840
            && u64::from(width) * u64::from(height) <= 8_294_400,
        "OpenAI image exceeds supported dimensions"
    );
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().context("could not decode the OpenAI image")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_respect_api_limits_and_keep_requested_final_dimensions() {
        for source in [(1, 1), (100, 300), (300, 100), (110, 73), (1600, 1200)] {
            for scale in [0.25, 1.0, 2.0] {
                let settings = Settings {
                    scale,
                    ..Settings::default()
                };
                let p = plan(source, &settings).unwrap();
                assert_eq!(
                    p.target,
                    Size::Scale(scale).dimensions(source.0, source.1).unwrap()
                );
                assert_eq!((p.api.0 % 16, p.api.1 % 16), (0, 0));
                assert!((655_360..=3_686_400).contains(&(p.api.0 * p.api.1)));
                assert!(p.api.0 <= 2560 && p.api.1 <= 2560);
                assert!((1.0 / 3.0..=3.0).contains(&(f64::from(p.api.0) / f64::from(p.api.1))));
            }
        }
        assert!(plan((400, 100), &Settings::default()).is_err());
        assert!(
            plan(
                (8000, 8000),
                &Settings {
                    scale: 8.0,
                    ..Settings::default()
                }
            )
            .is_err()
        );
        assert!(
            plan(
                (10, 10),
                &Settings {
                    scale: f64::NAN,
                    ..Settings::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn controls_become_instructions_and_private_prompt_is_not_persisted() {
        let settings = Settings {
            cleanup: Strength::Strong,
            detail: Strength::Off,
            colours: Colours::Vibrant,
            exposure: true,
            prompt: "Keep the blue boat".into(),
            ..Settings::default()
        };
        let prompt = settings.prompt().unwrap();
        assert!(prompt.contains("Thoroughly reduce"));
        assert!(prompt.contains("Do not add sharpening"));
        assert!(prompt.contains("colour vibrancy"));
        assert!(prompt.contains("Improve exposure"));
        assert!(prompt.contains("Keep the blue boat"));
        assert!(
            !serde_json::to_string(&settings)
                .unwrap()
                .contains("blue boat")
        );
        assert!(
            Settings {
                prompt: "x".repeat(4001),
                ..settings
            }
            .prompt()
            .is_err()
        );
    }
    #[test]
    fn errors_are_actionable_and_never_echo_provider_content() {
        let bytes = br#"{"error":{"code":"insufficient_quota","message":"SECRET"}}"#;
        assert!(api_error(429, bytes).contains("billing"));
        assert!(api_error(401, b"SECRET").contains("API key"));
        assert!(!api_error(500, bytes).contains("SECRET"));
        assert!(decode_response(br#"{"data":[]}"#).is_err());
        assert!(decode_response(br#"{"data":[{"b64_json":"bad"}]}"#).is_err());
    }
    #[test]
    fn mock_edit_sends_one_request_and_returns_exact_size() {
        use std::{io::Write, net::TcpListener, sync::atomic::Ordering};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/images/edits", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&request);
            assert!(text.contains("Bearer test-key"));
            assert!(text.contains(MODEL));
            assert!(text.contains("name=\"image\"; filename=\"image.png\""));
            assert!(text.contains("name=\"quality\"\r\n\r\nmedium"));
            assert!(!text.contains("private-filename"));
            let mut image = Cursor::new(Vec::new());
            DynamicImage::new_rgba8(32, 32)
                .write_to(&mut image, image::ImageFormat::Png)
                .unwrap();
            let body =
                serde_json::json!({"data":[{"b64_json": STANDARD.encode(image.into_inner())}]})
                    .to_string();
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("private-filename.png");
        DynamicImage::new_rgba8(12, 8).save(&input).unwrap();
        let cancel = AtomicBool::new(false);
        let generated =
            generate_at(&input, "test-key", &Settings::default(), &cancel, &endpoint).unwrap();
        assert_eq!(
            (generated.pixels.width(), generated.pixels.height()),
            (24, 16)
        );
        server.join().unwrap();
        cancel.store(true, Ordering::Relaxed);
        assert!(generate_at(&input, "test-key", &Settings::default(), &cancel, &endpoint).is_err());
    }
}
