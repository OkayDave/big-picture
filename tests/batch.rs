use big_picture::{OutputFormat, PlanOptions, discover, plan};
use std::path::PathBuf;

fn file(path: &std::path::Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"header not needed for planning").unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn discover_is_sorted_deduplicated_and_recursive_only_when_requested() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.PNG"));
    let b = file(&dir.path().join("nested/b.jpg"));
    file(&dir.path().join("skip.txt"));
    assert_eq!(
        discover(&[dir.path().into(), a.clone()], false, None).unwrap(),
        vec![a.clone()]
    );
    let all = discover(&[dir.path().into()], true, None).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.contains(&b));
}
#[test]
fn recursive_discovery_excludes_output_tree() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.png"));
    file(&dir.path().join("out/nested/generated.png"));
    assert_eq!(
        discover(&[dir.path().into()], true, Some(&dir.path().join("out"))).unwrap(),
        vec![a]
    );
    assert!(discover(&[dir.path().into()], true, Some(dir.path())).is_err());
}
#[test]
fn patterns_expand_and_empty_patterns_fail() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.png"));
    assert_eq!(
        discover(&[dir.path().join("*.png")], false, None).unwrap(),
        vec![a]
    );
    assert!(discover(&[dir.path().join("*.jpg")], false, None).is_err());
}
#[test]
fn unsupported_and_missing_inputs_fail_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let input = file(&dir.path().join("a.txt"));
    assert!(discover(&[input], false, None).is_err());
    assert!(discover(&[dir.path().join("nope")], false, None).is_err());
    assert!(discover(&[dir.path().join("empty")], false, None).is_err());
}
#[test]
fn output_defaults_to_png_with_safe_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.jpg"));
    let jobs = plan(&[a], &PlanOptions::default()).unwrap();
    assert_eq!(jobs[0].output.file_name().unwrap(), "a-upscaled.png");
    assert!(!jobs[0].output.exists());
}
#[test]
fn collision_is_rejected_before_creating_destination_directory() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("one/a.png"));
    let b = file(&dir.path().join("two/a.jpg"));
    let out = dir.path().join("out");
    assert!(
        plan(
            &[a, b],
            &PlanOptions {
                output_dir: Some(out.clone()),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(!out.exists());
}
#[test]
fn multiple_inputs_cannot_share_explicit_output() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.png"));
    let b = file(&dir.path().join("b.png"));
    assert!(
        plan(
            &[a, b],
            &PlanOptions {
                output: Some(dir.path().join("out.png")),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn output_format_and_extension_must_agree() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.png"));
    assert!(
        plan(
            std::slice::from_ref(&a),
            &PlanOptions {
                output: Some(dir.path().join("out.jpg")),
                format: Some(OutputFormat::Png),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        plan(
            &[a],
            &PlanOptions {
                output: Some(dir.path().join("out.unsupported")),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn no_job_may_overwrite_any_source_in_batch() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.png"));
    let b = file(&dir.path().join("a-upscaled.png"));
    assert!(
        plan(
            &[a, b],
            &PlanOptions {
                overwrite: true,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn existing_output_requires_overwrite_and_relative_segments_are_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir.path().join("a.png"));
    file(&dir.path().join("a-upscaled.png"));
    assert!(plan(std::slice::from_ref(&a), &PlanOptions::default()).is_err());
    assert!(
        plan(
            &[a],
            &PlanOptions {
                overwrite: true,
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert_eq!(
        big_picture::batch::resolved(&dir.path().join("missing/../next/file.png")).unwrap(),
        dir.path().canonicalize().unwrap().join("next/file.png")
    );
}
