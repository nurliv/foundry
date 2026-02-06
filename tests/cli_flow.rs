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
fn write_creates_markdown_and_meta_with_defaults() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    fs::create_dir_all(root.join("spec")).expect("create spec dir");

    let write = run_foundry(
        &root,
        &[
            "spec",
            "write",
            "--path",
            "spec/10-auth.md",
            "--body",
            "# Auth Requirement\n\nusers can sign in",
            "--type",
            "feature_requirement",
            "--status",
            "draft",
            "--term",
            "auth",
            "--term",
            "signin",
        ],
    );
    assert!(
        write.status.success(),
        "write failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let md_path = root.join("spec/10-auth.md");
    let meta_path = root.join("spec/10-auth.meta.json");
    assert!(md_path.exists(), "markdown should exist");
    assert!(meta_path.exists(), "meta should exist");

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(meta_path).expect("read meta")).expect("parse");
    assert_eq!(meta["title"], "Auth Requirement");
    assert_eq!(meta["type"], "feature_requirement");
    assert_eq!(meta["status"], "draft");
    assert_eq!(meta["body_md_path"], "spec/10-auth.md");
}

#[test]
fn write_updates_existing_meta_without_losing_edges() {
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

    let write = run_foundry(
        &root,
        &[
            "spec",
            "write",
            "--path",
            "spec/a.md",
            "--body",
            "# A Updated\n\nnew content",
            "--status",
            "active",
        ],
    );
    assert!(
        write.status.success(),
        "write failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let a_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a"))
            .expect("parse a");
    assert_eq!(a_meta["id"], "SPC-001");
    assert_eq!(a_meta["status"], "active");
    assert_eq!(a_meta["title"], "A Updated");
    let edges = a_meta["edges"].as_array().expect("edges should be array");
    assert_eq!(edges.len(), 1, "existing edge should remain");
    assert_eq!(edges[0]["to"], "SPC-002");
}

#[test]
fn write_updates_node_by_id_without_path() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A\n\nold").expect("write a");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let write = run_foundry(
        &root,
        &[
            "spec",
            "write",
            "--id",
            "SPC-001",
            "--status",
            "doing",
            "--body",
            "# A In Progress\n\nnew body",
        ],
    );
    assert!(
        write.status.success(),
        "write by id failed: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let md = fs::read_to_string(spec_dir.join("a.md")).expect("read markdown");
    assert!(md.contains("A In Progress"));

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("a.meta.json")).expect("read meta"))
            .expect("parse meta");
    assert_eq!(meta["id"], "SPC-001");
    assert_eq!(meta["status"], "doing");
    assert_eq!(meta["title"], "A In Progress");
}

#[test]
fn derive_design_creates_design_node_and_refines_edge() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("10-spec.md"), "# Auth Spec\n\ncontent").expect("write source spec");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let derive = run_foundry(
        &root,
        &[
            "spec",
            "derive",
            "design",
            "--from",
            "SPC-001",
            "--path",
            "spec/40-auth-design.md",
            "--type",
            "component_design",
            "--status",
            "review",
        ],
    );
    assert!(
        derive.status.success(),
        "derive failed: {}",
        String::from_utf8_lossy(&derive.stderr)
    );

    let design_md = root.join("spec/40-auth-design.md");
    let design_meta = root.join("spec/40-auth-design.meta.json");
    assert!(design_md.exists(), "design markdown should exist");
    assert!(design_meta.exists(), "design meta should exist");

    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(design_meta).expect("read design meta"))
            .expect("parse design meta");
    assert_eq!(meta["type"], "component_design");
    assert_eq!(meta["status"], "review");
    let edges = meta["edges"].as_array().expect("edges should be array");
    assert!(edges.iter().any(|e| {
        e["to"] == "SPC-001" && e["type"] == "refines" && e["status"] == "confirmed"
    }));
}

#[test]
fn derive_design_fails_when_source_missing() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    fs::create_dir_all(root.join("spec")).expect("create spec dir");

    let derive = run_foundry(
        &root,
        &[
            "spec",
            "derive",
            "design",
            "--from",
            "SPC-999",
            "--path",
            "spec/40-auth-design.md",
        ],
    );
    assert!(
        !derive.status.success(),
        "derive should fail for unknown source"
    );
}

#[test]
fn derive_tasks_creates_task_node_with_refines_and_depends_on() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");

    fs::write(spec_dir.join("10-design-a.md"), "# Design A\n\ncontent").expect("write design a");
    fs::write(spec_dir.join("11-design-b.md"), "# Design B\n\ncontent").expect("write design b");
    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let task1 = run_foundry(
        &root,
        &[
            "spec",
            "derive",
            "tasks",
            "--from",
            "SPC-001",
            "--path",
            "spec/50-task-a.md",
            "--type",
            "implementation_task",
            "--status",
            "todo",
        ],
    );
    assert!(task1.status.success(), "task1 derive failed");

    let task2 = run_foundry(
        &root,
        &[
            "spec",
            "derive",
            "tasks",
            "--from",
            "SPC-002",
            "--path",
            "spec/51-task-b.md",
            "--type",
            "implementation_task",
            "--status",
            "todo",
            "--depends-on",
            "SPC-003",
        ],
    );
    assert!(
        task2.status.success(),
        "task2 derive failed: {}",
        String::from_utf8_lossy(&task2.stderr)
    );

    let task2_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("51-task-b.meta.json")).expect("read task2 meta"))
            .expect("parse task2 meta");
    assert_eq!(task2_meta["type"], "implementation_task");
    assert_eq!(task2_meta["status"], "todo");
    let edges = task2_meta["edges"].as_array().expect("edges array");
    assert!(edges.iter().any(|e| e["type"] == "refines" && e["to"] == "SPC-002"));
    assert!(edges.iter().any(|e| e["type"] == "depends_on" && e["to"] == "SPC-003"));
}

#[test]
fn derive_tasks_fails_for_unknown_depends_on_target() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("10-design-a.md"), "# Design A\n\ncontent").expect("write design a");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let task = run_foundry(
        &root,
        &[
            "spec",
            "derive",
            "tasks",
            "--from",
            "SPC-001",
            "--path",
            "spec/50-task-a.md",
            "--depends-on",
            "SPC-999",
        ],
    );
    assert!(!task.status.success(), "derive should fail on unknown depends-on");
}

#[test]
fn init_with_agents_generates_command_templates() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("01-example.md"), "# Example\n\ncontent").expect("write markdown");

    let init = run_foundry(
        &root,
        &[
            "spec",
            "init",
            "--sync",
            "--template-source",
            "local",
            "--agent",
            "codex",
            "--agent",
            "claude",
        ],
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let codex = root.join("docs/agents/codex/commands/spec-plan.md");
    let claude = root.join("docs/agents/claude/commands/spec-plan.md");
    let codex_skill = root.join("docs/agents/codex/skills/spec-plan.md");
    let claude_skill = root.join("docs/agents/claude/skills/spec-plan.md");
    let codex_implement = root.join("docs/agents/codex/commands/implement.md");
    let codex_impl_review = root.join("docs/agents/codex/commands/impl-review.md");
    let codex_design_plan = root.join("docs/agents/codex/commands/design-plan.md");
    let codex_task_breakdown = root.join("docs/agents/codex/commands/task-breakdown.md");
    assert!(codex.exists(), "missing codex template");
    assert!(claude.exists(), "missing claude template");
    assert!(codex_skill.exists(), "missing codex skill template");
    assert!(claude_skill.exists(), "missing claude skill template");
    assert!(codex_implement.exists(), "missing codex implement template");
    assert!(codex_impl_review.exists(), "missing codex impl-review template");
    assert!(codex_design_plan.exists(), "missing codex design-plan template");
    assert!(codex_task_breakdown.exists(), "missing codex task-breakdown template");

    let codex_text = fs::read_to_string(codex).expect("read codex template");
    let claude_text = fs::read_to_string(claude).expect("read claude template");
    let codex_skill_text = fs::read_to_string(codex_skill).expect("read codex skill template");
    let claude_skill_text =
        fs::read_to_string(claude_skill).expect("read claude skill template");
    let codex_implement_text =
        fs::read_to_string(codex_implement).expect("read codex implement template");
    let codex_impl_review_text =
        fs::read_to_string(codex_impl_review).expect("read codex impl-review template");
    let codex_design_plan_text =
        fs::read_to_string(codex_design_plan).expect("read codex design-plan template");
    let codex_task_breakdown_text =
        fs::read_to_string(codex_task_breakdown).expect("read codex task-breakdown template");
    assert!(codex_text.contains("# spec-plan"));
    assert!(codex_text.contains("Codex Overlay: spec-plan"));
    assert!(claude_text.contains("Claude Overlay: spec-plan"));
    assert!(codex_skill_text.contains("Codex Skill Overlay: spec-plan"));
    assert!(claude_skill_text.contains("Claude Skill Overlay: spec-plan"));
    assert!(codex_implement_text.contains("spec write --id <TASK-ID> --status doing"));
    assert!(codex_implement_text.contains("spec write --id <TASK-ID> --status done"));
    assert!(codex_impl_review_text.contains("spec write --id <TASK-ID> --status blocked"));
    assert!(codex_design_plan_text.contains("spec derive design --from <SPC-ID>"));
    assert!(!codex_design_plan_text.contains("spec link add --from <DESIGN-ID>"));
    assert!(codex_task_breakdown_text.contains("spec derive tasks --from <DESIGN-ID>"));
    assert!(
        codex_task_breakdown_text.contains("--depends-on <TASK-ID>"),
        "task breakdown should suggest dependency capture at derive stage"
    );
    assert!(!codex_text.contains("{{"), "placeholder should be rendered");
    assert!(!codex_skill_text.contains("{{"), "placeholder should be rendered");
    let project_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .expect("project dir name");
    assert!(codex_text.contains(project_name));
    assert!(codex_text.contains("SPC-001"));
}

#[test]
fn init_with_agents_install_output_writes_agent_auto_read_dirs() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("01-example.md"), "# Example\n\ncontent").expect("write markdown");

    let codex_home = root.join(".test-codex");
    let claude_dir = root.join(".test-claude");
    let codex_home_str = codex_home.to_string_lossy().to_string();
    let claude_dir_str = claude_dir.to_string_lossy().to_string();

    let init = run_foundry(
        &root,
        &[
            "spec",
            "init",
            "--sync",
            "--template-source",
            "local",
            "--agent-output",
            "install",
            "--codex-home",
            &codex_home_str,
            "--claude-dir",
            &claude_dir_str,
            "--agent",
            "codex",
            "--agent",
            "claude",
        ],
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let codex_cmd = codex_home.join("commands/foundry/spec-plan.md");
    let codex_skill = codex_home.join("skills/foundry/spec-plan.md");
    let claude_cmd = claude_dir.join("commands/foundry/spec-plan.md");
    let claude_skill = claude_dir.join("skills/foundry/spec-plan.md");
    assert!(codex_cmd.exists(), "missing codex command template");
    assert!(codex_skill.exists(), "missing codex skill template");
    assert!(claude_cmd.exists(), "missing claude command template");
    assert!(claude_skill.exists(), "missing claude skill template");

    let docs_output = root.join("docs/agents/codex/commands/spec-plan.md");
    assert!(
        !docs_output.exists(),
        "docs output should not be created in install-only mode"
    );
}

#[test]
fn init_agent_without_sync_does_not_overwrite_existing_template() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("01-example.md"), "# Example\n\ncontent").expect("write markdown");

    let first = run_foundry(
        &root,
        &[
            "spec",
            "init",
            "--sync",
            "--template-source",
            "local",
            "--agent",
            "codex",
        ],
    );
    assert!(first.status.success(), "first init failed");

    let target = root.join("docs/agents/codex/commands/spec-plan.md");
    fs::write(&target, "CUSTOM\n").expect("write custom");

    let second = run_foundry(
        &root,
        &[
            "spec",
            "init",
            "--template-source",
            "local",
            "--agent",
            "codex",
        ],
    );
    assert!(second.status.success(), "second init failed");

    let text = fs::read_to_string(&target).expect("read template");
    assert_eq!(text, "CUSTOM\n");
}

#[test]
fn agent_doctor_reports_ok_after_agent_init() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("01-example.md"), "# Example\n\ncontent").expect("write markdown");

    let init = run_foundry(
        &root,
        &[
            "spec",
            "init",
            "--sync",
            "--template-source",
            "local",
            "--agent",
            "codex",
            "--agent",
            "claude",
        ],
    );
    assert!(init.status.success(), "init failed");

    let doctor = run_foundry(
        &root,
        &[
            "spec",
            "agent",
            "doctor",
            "--template-source",
            "local",
            "--format",
            "json",
        ],
    );
    assert!(doctor.status.success(), "agent doctor should succeed");
    let output: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("parse doctor output");
    assert_eq!(output["ok"], true);
    assert_eq!(output["issues"], serde_json::json!([]));
}

#[test]
fn agent_doctor_detects_stale_generated_file() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("01-example.md"), "# Example\n\ncontent").expect("write markdown");

    let init = run_foundry(
        &root,
        &[
            "spec",
            "init",
            "--sync",
            "--template-source",
            "local",
            "--agent",
            "codex",
        ],
    );
    assert!(init.status.success(), "init failed");

    let target = root.join("docs/agents/codex/commands/spec-plan.md");
    fs::write(&target, "BROKEN\n").expect("write broken template");

    let doctor = run_foundry(
        &root,
        &[
            "spec",
            "agent",
            "doctor",
            "--template-source",
            "local",
            "--agent",
            "codex",
            "--format",
            "json",
        ],
    );
    assert_eq!(doctor.status.code(), Some(1), "doctor should fail on stale output");
    let output: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("parse doctor output");
    assert_eq!(output["ok"], false);
    let issues = output["issues"].as_array().expect("issues should be array");
    assert!(!issues.is_empty());
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
    assert_eq!(lint.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(stdout.contains("term key drift detected"), "{stdout}");
}

#[test]
fn lint_json_format_reports_success() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A").expect("write a");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let mut a_meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a"))
            .expect("parse a");
    a_meta["type"] = serde_json::Value::String("product_goal".to_string());
    fs::write(
        spec_dir.join("a.meta.json"),
        serde_json::to_string_pretty(&a_meta).expect("serialize a") + "\n",
    )
    .expect("write a");

    let lint = run_foundry(&root, &["spec", "lint", "--format", "json"]);
    assert!(lint.status.success(), "lint should pass");
    let output: serde_json::Value = serde_json::from_slice(&lint.stdout).expect("parse lint json");
    assert_eq!(output["ok"], true);
    assert_eq!(output["error_count"], 0);
    assert_eq!(output["errors"], serde_json::json!([]));
}

#[test]
fn lint_json_format_reports_errors_with_exit_code_one() {
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

    let lint = run_foundry(&root, &["spec", "lint", "--format", "json"]);
    assert_eq!(lint.status.code(), Some(1));
    let output: serde_json::Value = serde_json::from_slice(&lint.stdout).expect("parse lint json");
    assert_eq!(output["ok"], false);
    assert!(output["error_count"].as_u64().unwrap_or(0) >= 1);
    let errors = output["errors"].as_array().expect("errors should be array");
    assert!(!errors.is_empty());
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
    let obj = output.as_object().expect("ask output object");
    let expected_keys = [
        "question",
        "mode",
        "answer",
        "confidence",
        "citations",
        "evidence",
        "explanations",
        "gaps",
    ];
    for key in expected_keys {
        assert!(obj.contains_key(key), "missing key: {key}");
    }
    assert_eq!(obj.len(), 8, "unexpected top-level keys: {:?}", obj.keys());
    assert!(matches!(output["mode"].as_str(), Some("lexical" | "hybrid")));
    assert!(output["confidence"].as_f64().is_some());
    assert!(output["answer"].is_string());
    assert!(output["citations"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(output["evidence"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(output["explanations"].as_array().is_some_and(|a| a.is_empty()));
    assert!(output["gaps"].as_array().is_some());
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

#[test]
fn ask_explain_returns_reason_entries() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# Auth\n\nAuthentication spec flow.").expect("write a");
    fs::write(spec_dir.join("b.md"), "# Session\n\nSession dependency spec.").expect("write b");

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
            "auth depends on session",
        ],
    );
    assert!(add.status.success(), "link failed");
    let index = run_foundry(&root, &["spec", "search", "index", "--rebuild"]);
    assert!(index.status.success(), "index failed");

    let ask = run_foundry(
        &root,
        &[
            "spec",
            "ask",
            "auth flow",
            "--format",
            "json",
            "--top-k",
            "1",
            "--explain",
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
    let explanations = output["explanations"]
        .as_array()
        .expect("explanations should be array");
    assert!(!explanations.is_empty());
    let first = explanations.first().expect("has explanation");
    assert!(first["id"].is_string());
    assert!(first["reason"].is_string());
    let reason_text = explanations
        .iter()
        .filter_map(|v| v["reason"].as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        reason_text.contains("token match"),
        "expected token match in explanations: {reason_text}"
    );
    assert!(
        reason_text.contains("w=") || reason_text.contains("weighted_score="),
        "expected weighted edge hints in explanations: {reason_text}"
    );
}

#[test]
fn plan_ready_reports_ready_and_blocked_tasks() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("t1.md"), "# Task 1").expect("write t1");
    fs::write(spec_dir.join("t2.md"), "# Task 2").expect("write t2");
    fs::write(spec_dir.join("t3.md"), "# Task 3").expect("write t3");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let mut t1: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("t1.meta.json")).expect("read t1"))
            .expect("parse t1");
    let mut t2: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("t2.meta.json")).expect("read t2"))
            .expect("parse t2");
    let mut t3: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("t3.meta.json")).expect("read t3"))
            .expect("parse t3");
    t1["type"] = serde_json::json!("implementation_task");
    t2["type"] = serde_json::json!("implementation_task");
    t3["type"] = serde_json::json!("implementation_task");
    t1["status"] = serde_json::json!("done");
    t2["status"] = serde_json::json!("todo");
    t3["status"] = serde_json::json!("todo");
    t2["edges"] = serde_json::json!([
      {
        "to": "SPC-001",
        "type": "depends_on",
        "rationale": "needs task1",
        "confidence": 1.0,
        "status": "confirmed"
      }
    ]);
    t3["edges"] = serde_json::json!([
      {
        "to": "SPC-002",
        "type": "depends_on",
        "rationale": "needs task2",
        "confidence": 1.0,
        "status": "confirmed"
      }
    ]);
    fs::write(
        spec_dir.join("t1.meta.json"),
        serde_json::to_string_pretty(&t1).expect("serialize t1") + "\n",
    )
    .expect("write t1");
    fs::write(
        spec_dir.join("t2.meta.json"),
        serde_json::to_string_pretty(&t2).expect("serialize t2") + "\n",
    )
    .expect("write t2");
    fs::write(
        spec_dir.join("t3.meta.json"),
        serde_json::to_string_pretty(&t3).expect("serialize t3") + "\n",
    )
    .expect("write t3");

    let out = run_foundry(&root, &["spec", "plan", "ready", "--format", "json"]);
    assert!(out.status.success(), "plan ready failed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse output");
    let ready = json["ready"].as_array().expect("ready array");
    let blocked = json["blocked"].as_array().expect("blocked array");
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0]["id"], "SPC-002");
    assert_eq!(ready[0]["path"], "spec/t2.md");
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0]["id"], "SPC-003");
    assert_eq!(blocked[0]["path"], "spec/t3.md");
    assert_eq!(blocked[0]["blocked_by"][0], "SPC-002");
}

#[test]
fn plan_batches_groups_parallel_tasks() {
    let root = tempdir().expect("create temp dir");
    let root = root.path();
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).expect("create spec dir");
    fs::write(spec_dir.join("a.md"), "# A").expect("write a");
    fs::write(spec_dir.join("b.md"), "# B").expect("write b");
    fs::write(spec_dir.join("c.md"), "# C").expect("write c");
    fs::write(spec_dir.join("d.md"), "# D").expect("write d");

    let init = run_foundry(&root, &["spec", "init", "--sync"]);
    assert!(init.status.success(), "init failed");

    let mut a: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("a.meta.json")).expect("read a"))
            .expect("parse a");
    let mut b: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("b.meta.json")).expect("read b"))
            .expect("parse b");
    let mut c: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("c.meta.json")).expect("read c"))
            .expect("parse c");
    let mut d: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(spec_dir.join("d.meta.json")).expect("read d"))
            .expect("parse d");
    for meta in [&mut a, &mut b, &mut c, &mut d] {
        meta["type"] = serde_json::json!("implementation_task");
        meta["status"] = serde_json::json!("todo");
    }
    c["edges"] = serde_json::json!([
      {
        "to": "SPC-001",
        "type": "depends_on",
        "rationale": "c needs a",
        "confidence": 1.0,
        "status": "confirmed"
      }
    ]);
    d["edges"] = serde_json::json!([
      {
        "to": "SPC-002",
        "type": "depends_on",
        "rationale": "d needs b",
        "confidence": 1.0,
        "status": "confirmed"
      }
    ]);
    fs::write(
        spec_dir.join("a.meta.json"),
        serde_json::to_string_pretty(&a).expect("serialize a") + "\n",
    )
    .expect("write a");
    fs::write(
        spec_dir.join("b.meta.json"),
        serde_json::to_string_pretty(&b).expect("serialize b") + "\n",
    )
    .expect("write b");
    fs::write(
        spec_dir.join("c.meta.json"),
        serde_json::to_string_pretty(&c).expect("serialize c") + "\n",
    )
    .expect("write c");
    fs::write(
        spec_dir.join("d.meta.json"),
        serde_json::to_string_pretty(&d).expect("serialize d") + "\n",
    )
    .expect("write d");

    let out = run_foundry(&root, &["spec", "plan", "batches", "--format", "json"]);
    assert!(out.status.success(), "plan batches failed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse output");
    let batches = json["batches"].as_array().expect("batches array");
    assert_eq!(batches.len(), 2);
    let b1 = batches[0]["task_ids"].as_array().expect("batch1 ids");
    let b2 = batches[1]["task_ids"].as_array().expect("batch2 ids");
    assert_eq!(b1.len(), 2);
    assert_eq!(b2.len(), 2);
    let b1_tasks = batches[0]["tasks"].as_array().expect("batch1 tasks");
    let b2_tasks = batches[1]["tasks"].as_array().expect("batch2 tasks");
    assert_eq!(b1_tasks.len(), 2);
    assert_eq!(b2_tasks.len(), 2);
    assert!(b1_tasks.iter().all(|t| t["path"].is_string()));
    assert!(b2_tasks.iter().all(|t| t["path"].is_string()));
    assert!(json["blocked_or_cyclic"]
        .as_array()
        .expect("blocked_or_cyclic")
        .is_empty());
    assert!(json["blocked_or_cyclic_tasks"]
        .as_array()
        .expect("blocked_or_cyclic_tasks")
        .is_empty());
}
