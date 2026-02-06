use super::*;

const COMMAND_PHASES: &[&str] = &[
    "spec-plan",
    "spec-review",
    "design-plan",
    "design-review",
    "task-breakdown",
    "implement",
    "impl-review",
];

#[derive(Default)]
struct AgentTemplateSummary {
    written: usize,
    skipped: usize,
    errors: usize,
}

pub(super) fn run_init(sync: bool, agents: &[AgentTarget], agent_sync: bool) -> Result<()> {
    let spec_root = Path::new("spec");
    let mut summary = InitSummary::default();

    if spec_root.exists() {
        let md_files = find_markdown_files(spec_root)?;
        let mut used_ids = load_existing_ids(spec_root)?;
        let mut next_id = next_available_id(&used_ids);

        for md_path in md_files {
            let md_rel = normalize_path(&md_path);
            let meta_path = md_to_meta_path(&md_path)?;
            let body = match fs::read_to_string(&md_path) {
                Ok(v) => v,
                Err(err) => {
                    summary.errors += 1;
                    eprintln!("error reading {}: {err}", md_rel.display());
                    continue;
                }
            };
            let title = extract_title(&body, &md_path);
            let hash = sha256_hex(body.as_bytes());

            if meta_path.exists() {
                let existing = fs::read_to_string(&meta_path)
                    .with_context(|| format!("failed reading {}", meta_path.display()));
                let mut meta: SpecNodeMeta = match existing
                    .and_then(|s| serde_json::from_str(&s).context("invalid .meta.json"))
                {
                    Ok(m) => m,
                    Err(err) => {
                        summary.errors += 1;
                        eprintln!("error parsing {}: {err:#}", meta_path.display());
                        continue;
                    }
                };

                let mut changed = false;
                if meta.id.trim().is_empty() {
                    meta.id = format!("SPC-{next_id:03}");
                    used_ids.insert(meta.id.clone());
                    next_id += 1;
                    changed = true;
                } else {
                    used_ids.insert(meta.id.clone());
                }
                if meta.node_type.trim().is_empty() {
                    meta.node_type = "feature_requirement".to_string();
                    changed = true;
                }
                if meta.status.trim().is_empty() {
                    meta.status = "draft".to_string();
                    changed = true;
                }
                if meta.title.trim().is_empty() || sync {
                    if meta.title != title {
                        meta.title = title.clone();
                        changed = true;
                    }
                }
                if meta.body_md_path.trim().is_empty() || sync {
                    let rel = md_rel.to_string_lossy().to_string();
                    if meta.body_md_path != rel {
                        meta.body_md_path = rel;
                        changed = true;
                    }
                }
                if meta.hash != hash {
                    meta.hash = hash.clone();
                    changed = true;
                }

                if changed {
                    write_meta_json(&meta_path, &meta)?;
                    summary.updated += 1;
                } else {
                    summary.skipped += 1;
                }
            } else {
                let id = loop {
                    let candidate = format!("SPC-{next_id:03}");
                    next_id += 1;
                    if !used_ids.contains(&candidate) {
                        used_ids.insert(candidate.clone());
                        break candidate;
                    }
                };
                let meta = SpecNodeMeta {
                    id,
                    node_type: "feature_requirement".to_string(),
                    status: "draft".to_string(),
                    title,
                    body_md_path: md_rel.to_string_lossy().to_string(),
                    terms: Vec::new(),
                    hash,
                    edges: Vec::new(),
                };
                write_meta_json(&meta_path, &meta)?;
                summary.created += 1;
            }
        }

        println!(
            "init summary: created={} updated={} skipped={} errors={}",
            summary.created, summary.updated, summary.skipped, summary.errors
        );
    } else {
        println!("spec/ directory not found. skipping spec metadata initialization.");
    }

    if !agents.is_empty() {
        let agent_summary = init_agent_templates(agents, agent_sync);
        println!(
            "agent template summary: written={} skipped={} errors={}",
            agent_summary.written, agent_summary.skipped, agent_summary.errors
        );
    }

    Ok(())
}

fn init_agent_templates(agents: &[AgentTarget], sync: bool) -> AgentTemplateSummary {
    let mut summary = AgentTemplateSummary::default();
    let template_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/commands");
    let mut uniq = HashSet::new();
    for agent in agents {
        if !uniq.insert(*agent) {
            continue;
        }
        let slug = agent_slug(*agent);
        for phase in COMMAND_PHASES {
            let base_path = template_root.join(format!("base/{phase}.md"));
            let overlay_path = template_root.join(format!("overlays/{slug}/{phase}.md"));
            let out_path = PathBuf::from(format!("docs/agents/{slug}/commands/{phase}.md"));
            if out_path.exists() && !sync {
                summary.skipped += 1;
                continue;
            }
            let base = match fs::read_to_string(&base_path) {
                Ok(v) => v,
                Err(err) => {
                    summary.errors += 1;
                    eprintln!("agent template error reading {}: {err}", base_path.display());
                    continue;
                }
            };
            let overlay = match fs::read_to_string(&overlay_path) {
                Ok(v) => v,
                Err(err) => {
                    summary.errors += 1;
                    eprintln!(
                        "agent template error reading {}: {err}",
                        overlay_path.display()
                    );
                    continue;
                }
            };

            let rendered = render_command_template(&base, &overlay);
            if let Some(parent) = out_path.parent()
                && let Err(err) = fs::create_dir_all(parent)
            {
                summary.errors += 1;
                eprintln!(
                    "agent template error creating {}: {err}",
                    parent.display()
                );
                continue;
            }
            if let Err(err) = fs::write(&out_path, rendered) {
                summary.errors += 1;
                eprintln!(
                    "agent template error writing {}: {err}",
                    out_path.display()
                );
                continue;
            }
            summary.written += 1;
        }
    }
    summary
}

fn render_command_template(base: &str, overlay: &str) -> String {
    let mut out = String::new();
    out.push_str(base.trim_end());
    out.push_str("\n\n---\n\n");
    out.push_str(overlay.trim_end());
    out.push('\n');
    out
}

fn agent_slug(agent: AgentTarget) -> &'static str {
    match agent {
        AgentTarget::Codex => "codex",
        AgentTarget::Claude => "claude",
    }
}
