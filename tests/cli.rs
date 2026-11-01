use big_picture::cli::Cli;
use clap::Parser;
use std::process::{Command, Output};
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_big-picture"))
        .args(args)
        .output()
        .unwrap()
}
fn source(path: &std::path::Path) {
    image::RgbaImage::from_pixel(3, 2, image::Rgba([20, 80, 140, 255]))
        .save(path)
        .unwrap();
}
fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stdout)))
}

#[test]
fn help_version_and_completion_are_available() {
    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["upscale", "--help"],
        vec!["completions", "bash"],
        vec!["completions", "powershell"],
    ] {
        let output = cli(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
    }
}
#[test]
fn command_structure_is_valid() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
#[test]
fn conflicting_arguments_are_parse_errors() {
    for args in [
        vec![
            "big-picture",
            "upscale",
            "a.png",
            "--scale",
            "2",
            "--width",
            "10",
        ],
        vec![
            "big-picture",
            "upscale",
            "a.png",
            "--stretch",
            "--width",
            "10",
        ],
        vec![
            "big-picture",
            "upscale",
            "a.png",
            "-o",
            "x.png",
            "--output-dir",
            "out",
        ],
    ] {
        assert!(Cli::try_parse_from(args).is_err());
    }
}
#[test]
fn dry_run_is_machine_readable_and_does_not_create_output_directory() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    source(&input);
    let out = dir.path().join("never-created");
    let result = cli(&[
        "--json",
        "upscale",
        input.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
        "--dry-run",
        "--scale",
        "3",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let value = json(&result);
    assert_eq!(value["completed"], 1);
    assert_eq!(value["items"][0]["width"], 9);
    assert_eq!(value["items"][0]["status"], "planned");
    assert!(!out.exists());
}
#[test]
fn cli_exports_correct_image_and_json_report() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("with spaces.png");
    source(&input);
    let output = dir.path().join("output.webp");
    let result = cli(&[
        "upscale",
        input.to_str().unwrap(),
        "-o",
        output.to_str().unwrap(),
        "--width",
        "12",
        "--filter",
        "mitchell",
        "--color-space",
        "linear",
        "--json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert_eq!(json(&result)["items"][0]["report"]["height"], 8);
    assert_eq!(image::image_dimensions(&output).unwrap(), (12, 8));
    assert!(result.stderr.is_empty());
}
#[test]
fn keep_going_reports_partial_failure_with_nonzero_exit() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("a-bad.png");
    let good = dir.path().join("b-good.png");
    source(&good);
    std::fs::write(&bad, b"bad").unwrap();
    let out = dir.path().join("out");
    let result = cli(&[
        "upscale",
        dir.path().to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
        "--keep-going",
        "--json",
    ]);
    assert_eq!(result.status.code(), Some(1));
    let value = json(&result);
    assert_eq!(value["failed"], 1);
    assert_eq!(value["completed"], 1);
    assert_eq!(value["planned"], 2);
    assert!(out.join("b-good-upscaled.png").exists());
    assert!(!out.join("a-bad-upscaled.png").exists());
}
#[test]
fn default_stops_on_first_image_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.png"), b"bad").unwrap();
    source(&dir.path().join("b.png"));
    let result = cli(&["--json", "upscale", dir.path().to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(json(&result)["items"].as_array().unwrap().len(), 1);
    assert!(!dir.path().join("b-upscaled.png").exists());
}
#[test]
fn info_and_license_commands_support_json() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    source(&input);
    let info = cli(&["info", input.to_str().unwrap(), "--json"]);
    assert!(info.status.success());
    assert_eq!(json(&info)[0]["width"], 3);
    let licenses = cli(&["licenses", "--json"]);
    assert!(licenses.status.success());
    assert_eq!(json(&licenses)["license"], "MIT");
}
#[test]
fn preflight_errors_are_json_and_nonzero() {
    let result = cli(&["--json", "upscale", "this-does-not-exist.png"]);
    assert_eq!(result.status.code(), Some(1));
    assert!(
        json(&result)["error"]
            .as_str()
            .unwrap()
            .contains("no matches")
    );
}
#[test]
fn invalid_quality_scale_and_sharpen_are_rejected_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    source(&input);
    for (flag, value) in [("--quality", "0"), ("--scale", "NaN"), ("--sharpen", "4")] {
        let output = cli(&["--json", "upscale", input.to_str().unwrap(), flag, value]);
        assert!(!output.status.success());
        assert!(json(&output)["error"].is_string());
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn explicit_downscale_accepts_fractional_multiplier() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.png");
    source(&input);
    let result = cli(&[
        "--json",
        "upscale",
        input.to_str().unwrap(),
        "--scale",
        "0.5",
        "--allow-downscale",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert_eq!(json(&result)["items"][0]["width"], 2);
}
