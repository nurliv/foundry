use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock drift")
        .as_nanos();
    let pid = std::process::id();
    std::env::temp_dir().join(format!("foundry-test-{pid}-{ts}"))
}

fn run_foundry(workdir: &Path, args: &[&str]) -> std::process::Output {
    let bin = assert_cmd::cargo::cargo_bin!("foundry");
    Command::new(bin)
        .args(args)
        .current_dir(workdir)
        .output()
        .expect("failed to run foundry binary")
}

#[test]
fn init_creates_meta_json_and_lint_passes() {
    let root = unique_temp_dir();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");

    fs::write(spec_dir.join("01-example.md"), "# Example\n\ncontent").expect("write markdown");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let meta_path = spec_dir.join("01-example.meta.json");
    assert!(meta_path.exists(), "meta file should be generated");

    let meta_raw = fs::read_to_string(&meta_path).expect("read meta file");
    assert!(meta_raw.contains("\"id\": \"SPC-"));
    assert!(meta_raw.contains("\"body_md_path\": \"spec/01-example.md\""));
    let mut meta_json: serde_json::Value = serde_json::from_str(&meta_raw).expect("parse meta");
    meta_json["type"] = serde_json::Value::String("product_goal".to_string());
    fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta_json).expect("serialize meta") + "\n",
    )
    .expect("write updated meta");

    let lint = run_foundry(&root, &["spec", "lint"]);
    assert!(
        lint.status.success(),
        "lint failed: {}\n{}",
        String::from_utf8_lossy(&lint.stdout),
        String::from_utf8_lossy(&lint.stderr)
    );

    fs::remove_dir_all(&root).expect("cleanup temp dir");
}

#[test]
fn link_add_and_remove_updates_meta() {
    let root = unique_temp_dir();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A").expect("write a");
    fs::write(spec_dir.join("b.md"), "# B").expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let add = run_foundry(
        &root,
        &[
            "spec",
            "link",
            "add",
            "--from",
            "SPC-001",
            "--to",
            "SPC-002",
            "--type",
            "depends_on",
            "--rationale",
            "a depends on b",
        ],
    );
    assert!(add.status.success(), "add failed");

    let a_meta = fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a meta");
    assert!(a_meta.contains("\"to\": \"SPC-002\""));
    assert!(a_meta.contains("\"type\": \"depends_on\""));

    let remove = run_foundry(
        &root,
        &[
            "spec",
            "link",
            "remove",
            "--from",
            "SPC-001",
            "--to",
            "SPC-002",
            "--type",
            "depends_on",
        ],
    );
    assert!(remove.status.success(), "remove failed");

    let a_meta_after =
        fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a meta after remove");
    assert!(!a_meta_after.contains("\"to\": \"SPC-002\""));

    fs::remove_dir_all(&root).expect("cleanup temp dir");
}
