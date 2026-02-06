use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

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
    let root = tempdir().expect("create temp dir");
    let root = root.path();
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
}

#[test]
fn link_add_and_remove_updates_meta() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
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
}

#[test]
fn impact_supports_depth_and_json_format() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A").expect("write a");
    fs::write(spec_dir.join("b.md"), "# B").expect("write b");
    fs::write(spec_dir.join("c.md"), "# C").expect("write c");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let add_ab = run_foundry(
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
            "a->b",
        ],
    );
    assert!(add_ab.status.success(), "add a->b failed");

    let add_bc = run_foundry(
        &root,
        &[
            "spec",
            "link",
            "add",
            "--from",
            "SPC-002",
            "--to",
            "SPC-003",
            "--type",
            "depends_on",
            "--rationale",
            "b->c",
        ],
    );
    assert!(add_bc.status.success(), "add b->c failed");

    let impact_depth_1 = run_foundry(
        &root,
        &[
            "spec",
            "impact",
            "SPC-001",
            "--depth",
            "1",
            "--format",
            "json",
        ],
    );
    assert!(impact_depth_1.status.success(), "impact depth1 failed");
    let json_depth_1: serde_json::Value =
        serde_json::from_slice(&impact_depth_1.stdout).expect("parse depth1 json");
    let order_depth_1 = json_depth_1["recommended_review_order"]
        .as_array()
        .expect("review order should be an array");
    assert_eq!(order_depth_1.len(), 2);
    assert_eq!(json_depth_1["depth"], 1);

    let impact_depth_2 = run_foundry(
        &root,
        &[
            "spec",
            "impact",
            "SPC-001",
            "--depth",
            "2",
            "--format",
            "json",
        ],
    );
    assert!(impact_depth_2.status.success(), "impact depth2 failed");
    let json_depth_2: serde_json::Value =
        serde_json::from_slice(&impact_depth_2.stdout).expect("parse depth2 json");
    let order_depth_2 = json_depth_2["recommended_review_order"]
        .as_array()
        .expect("review order should be an array");
    assert_eq!(order_depth_2.len(), 3);
    assert_eq!(json_depth_2["depth"], 2);
}

#[test]
fn lint_detects_term_key_drift() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A").expect("write a");
    fs::write(spec_dir.join("b.md"), "# B").expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let mut a_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a"))
            .expect("parse a");
    let mut b_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("b.meta.json")).expect("read b"))
            .expect("parse b");

    a_meta["type"] = serde_json::Value::String("product_goal".to_string());
    a_meta["terms"] = serde_json::json!(["User_ID"]);
    b_meta["terms"] = serde_json::json!(["user-id"]);

    fs::write(
        spec_dir.join("a.meta.json"),
        serde_json::to_string_pretty(&a_meta).expect("serialize a") + "\n",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.meta.json"),
        serde_json::to_string_pretty(&b_meta).expect("serialize b") + "\n",
    )
    .expect("write b");

    let lint = run_foundry(&root, &["spec", "lint"]);
    assert!(!lint.status.success(), "lint should fail on term drift");
    let stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(stdout.contains("term key drift detected"), "{stdout}");
}

#[test]
fn link_propose_creates_proposed_edge() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# Account User").expect("write a");
    fs::write(spec_dir.join("b.md"), "# User Profile").expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let mut a_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a"))
            .expect("parse a");
    let mut b_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("b.meta.json")).expect("read b"))
            .expect("parse b");
    a_meta["type"] = serde_json::Value::String("product_goal".to_string());
    a_meta["terms"] = serde_json::json!(["user_account"]);
    b_meta["terms"] = serde_json::json!(["user-profile"]);
    fs::write(
        spec_dir.join("a.meta.json"),
        serde_json::to_string_pretty(&a_meta).expect("serialize a") + "\n",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.meta.json"),
        serde_json::to_string_pretty(&b_meta).expect("serialize b") + "\n",
    )
    .expect("write b");

    let propose = run_foundry(
        &root,
        &["spec", "link", "propose", "--node", "SPC-001", "--limit", "1"],
    );
    assert!(propose.status.success(), "propose failed");

    let a_after = fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a after");
    assert!(a_after.contains("\"to\": \"SPC-002\""), "{a_after}");
    assert!(a_after.contains("\"status\": \"proposed\""), "{a_after}");
    assert!(a_after.contains("\"type\": \"impacts\""), "{a_after}");
}

#[test]
fn search_index_and_query_json_work() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(
        spec_dir.join("a.md"),
        "# Login Flow\n\nUser logs in with email and password.",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.md"),
        "# Billing Flow\n\nUser updates payment method.",
    )
    .expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let index = run_foundry(&root, &["spec", "search", "index"]);
    assert!(index.status.success(), "index failed");

    let query = run_foundry(
        &root,
        &[
            "spec",
            "search",
            "query",
            "email password",
            "--format",
            "json",
            "--top-k",
            "5",
        ],
    );
    assert!(query.status.success(), "query failed");
    let output: serde_json::Value =
        serde_json::from_slice(&query.stdout).expect("parse query output");
    let hits = output["hits"].as_array().expect("hits should be array");
    assert!(!hits.is_empty(), "search should return at least one hit");
    assert_eq!(hits[0]["id"], "SPC-001");
}

#[test]
fn search_doctor_reports_ok_after_index() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A\n\ntext").expect("write a");
    fs::write(spec_dir.join("b.md"), "# B\n\ntext").expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");
    let index = run_foundry(&root, &["spec", "search", "index"]);
    assert!(index.status.success(), "index failed");

    let doctor = run_foundry(&root, &["spec", "search", "doctor"]);
    assert!(doctor.status.success(), "doctor command failed");
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(stdout.contains("search doctor: ok"), "{stdout}");
}

#[test]
fn search_hybrid_handles_near_match_query() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(
        spec_dir.join("a.md"),
        "# Authorization Policy\n\nThis spec defines authorization controls.",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.md"),
        "# Billing Rules\n\nInvoice and payment constraints.",
    )
    .expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");
    let index = run_foundry(&root, &["spec", "search", "index"]);
    assert!(index.status.success(), "index failed");

    let lexical = run_foundry(
        &root,
        &[
            "spec",
            "search",
            "query",
            "authorisation policy",
            "--mode",
            "lexical",
            "--format",
            "json",
        ],
    );
    assert!(lexical.status.success(), "lexical query failed");

    let hybrid = run_foundry(
        &root,
        &[
            "spec",
            "search",
            "query",
            "authorisation policy",
            "--mode",
            "hybrid",
            "--format",
            "json",
        ],
    );
    assert!(hybrid.status.success(), "hybrid query failed");
    let output: serde_json::Value =
        serde_json::from_slice(&hybrid.stdout).expect("parse hybrid output");
    assert_eq!(output["mode"], "hybrid");
    let hits = output["hits"].as_array().expect("hits array");
    assert!(!hits.is_empty(), "hybrid should return at least one hit");
    assert_eq!(hits[0]["id"], "SPC-001");
}

#[test]
fn search_lexical_boosts_title_match() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(
        spec_dir.join("a.md"),
        "# Checkout Flow\n\nSimple checkout process.",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.md"),
        "# Payment Domain\n\ncheckout checkout checkout for edge text weight.",
    )
    .expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");
    let index = run_foundry(&root, &["spec", "search", "index"]);
    assert!(index.status.success(), "index failed");

    let query = run_foundry(
        &root,
        &[
            "spec",
            "search",
            "query",
            "checkout flow",
            "--mode",
            "lexical",
            "--format",
            "json",
            "--top-k",
            "2",
        ],
    );
    assert!(query.status.success(), "query failed");
    let output: serde_json::Value =
        serde_json::from_slice(&query.stdout).expect("parse query output");
    let hits = output["hits"].as_array().expect("hits array");
    assert_eq!(hits[0]["id"], "SPC-001");
}

#[test]
fn ask_returns_citations_and_evidence() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(
        spec_dir.join("a.md"),
        "# Auth Flow\n\nAuthentication flow validates user session.",
    )
    .expect("write a");
    fs::write(spec_dir.join("b.md"), "# Billing\n\nPayment update flow.").expect("write b");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");
    let index = run_foundry(&root, &["spec", "search", "index"]);
    assert!(index.status.success(), "index failed");

    let ask = run_foundry(
        &root,
        &[
            "spec",
            "ask",
            "how does auth flow work?",
            "--format",
            "json",
            "--top-k",
            "3",
        ],
    );
    assert!(
        ask.status.success(),
        "ask failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&ask.stdout),
        String::from_utf8_lossy(&ask.stderr)
    );
    let output: serde_json::Value =
        serde_json::from_slice(&ask.stdout).expect("parse ask output");
    assert!(output["answer"].is_string());
    assert!(output["citations"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(output["evidence"].as_array().is_some_and(|a| !a.is_empty()));
}

#[test]
fn ask_reports_gap_when_no_hit() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# Logging\n\nLog retention settings.")
        .expect("write markdown");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");
    let index = run_foundry(&root, &["spec", "search", "index"]);
    assert!(index.status.success(), "index failed");

    let ask = run_foundry(
        &root,
        &["spec", "ask", "zzzz-no-match-token", "--format", "json"],
    );
    assert!(
        ask.status.success(),
        "ask failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&ask.stdout),
        String::from_utf8_lossy(&ask.stderr)
    );
    let output: serde_json::Value =
        serde_json::from_slice(&ask.stdout).expect("parse ask output");
    assert_eq!(output["confidence"], 0.0);
    assert!(output["gaps"].as_array().is_some_and(|a| !a.is_empty()));
}

#[test]
fn ask_includes_neighbor_citations_from_graph_edges() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(
        spec_dir.join("a.md"),
        "# Login Spec\n\nLogin flow with token validation.",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.md"),
        "# Session Dependency\n\nSession lifecycle requirements.",
    )
    .expect("write b");

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
            "login depends on session",
        ],
    );
    assert!(add.status.success(), "link add failed");
    let index = run_foundry(&root, &["spec", "search", "index", "--rebuild"]);
    assert!(index.status.success(), "index failed");

    let ask = run_foundry(
        &root,
        &["spec", "ask", "login flow", "--format", "json", "--top-k", "1"],
    );
    assert!(
        ask.status.success(),
        "ask failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&ask.stdout),
        String::from_utf8_lossy(&ask.stderr)
    );
    let output: serde_json::Value =
        serde_json::from_slice(&ask.stdout).expect("parse ask output");
    let citations = output["citations"].as_array().expect("citations array");
    let ids = citations
        .iter()
        .filter_map(|v| v["id"].as_str().map(ToString::to_string))
        .collect::<Vec<_>>();
    assert!(ids.contains(&"SPC-001".to_string()));
    assert!(ids.contains(&"SPC-002".to_string()));
}
