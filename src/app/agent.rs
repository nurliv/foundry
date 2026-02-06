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
pub(super) struct AgentTemplateSummary {
    pub(super) written: usize,
    pub(super) skipped: usize,
    pub(super) errors: usize,
}

#[derive(Debug, Serialize)]
struct AgentDoctorIssue {
    agent: String,
    phase: String,
    kind: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct AgentDoctorOutput {
    ok: bool,
    checked: usize,
    issues: Vec<AgentDoctorIssue>,
}

pub(super) fn run_agent(agent: AgentCommand) -> Result<i32> {
    match agent.command {
        AgentSubcommand::Doctor(args) => run_agent_doctor(&args),
    }
}

pub(super) fn generate_agent_templates(agents: &[AgentTarget], sync: bool) -> AgentTemplateSummary {
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
                eprintln!("agent template error creating {}: {err}", parent.display());
                continue;
            }
            if let Err(err) = fs::write(&out_path, rendered) {
                summary.errors += 1;
                eprintln!("agent template error writing {}: {err}", out_path.display());
                continue;
            }
            summary.written += 1;
        }
    }
    summary
}

fn run_agent_doctor(args: &AgentDoctorArgs) -> Result<i32> {
    let agents = if args.agent.is_empty() {
        vec![AgentTarget::Codex, AgentTarget::Claude]
    } else {
        args.agent.clone()
    };
    let mut uniq = HashSet::new();
    let agents = agents
        .into_iter()
        .filter(|a| uniq.insert(*a))
        .collect::<Vec<_>>();

    let template_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/commands");
    let mut issues = Vec::<AgentDoctorIssue>::new();
    let mut checked = 0usize;

    for agent in agents {
        let slug = agent_slug(agent).to_string();
        for phase in COMMAND_PHASES {
            checked += 1;
            let base_path = template_root.join(format!("base/{phase}.md"));
            let overlay_path = template_root.join(format!("overlays/{slug}/{phase}.md"));
            let out_path = PathBuf::from(format!("docs/agents/{slug}/commands/{phase}.md"));

            let base = match fs::read_to_string(&base_path) {
                Ok(v) => v,
                Err(err) => {
                    issues.push(AgentDoctorIssue {
                        agent: slug.clone(),
                        phase: phase.to_string(),
                        kind: "template_missing".to_string(),
                        detail: format!("missing base template {}: {err}", base_path.display()),
                    });
                    continue;
                }
            };
            let overlay = match fs::read_to_string(&overlay_path) {
                Ok(v) => v,
                Err(err) => {
                    issues.push(AgentDoctorIssue {
                        agent: slug.clone(),
                        phase: phase.to_string(),
                        kind: "template_missing".to_string(),
                        detail: format!(
                            "missing overlay template {}: {err}",
                            overlay_path.display()
                        ),
                    });
                    continue;
                }
            };
            let expected = render_command_template(&base, &overlay);
            let actual = match fs::read_to_string(&out_path) {
                Ok(v) => v,
                Err(err) => {
                    issues.push(AgentDoctorIssue {
                        agent: slug.clone(),
                        phase: phase.to_string(),
                        kind: "generated_missing".to_string(),
                        detail: format!("missing generated file {}: {err}", out_path.display()),
                    });
                    continue;
                }
            };
            if actual != expected {
                issues.push(AgentDoctorIssue {
                    agent: slug.clone(),
                    phase: phase.to_string(),
                    kind: "generated_stale".to_string(),
                    detail: format!("generated file differs from template {}", out_path.display()),
                });
            }
        }
    }

    let output = AgentDoctorOutput {
        ok: issues.is_empty(),
        checked,
        issues,
    };
    match args.format {
        AgentFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        AgentFormat::Table => print_agent_doctor_table(&output),
    }
    if output.ok {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn print_agent_doctor_table(output: &AgentDoctorOutput) {
    if output.ok {
        println!("agent doctor: ok (checked={})", output.checked);
        return;
    }
    for issue in &output.issues {
        println!(
            "agent doctor: issue: agent={} phase={} kind={} detail={}",
            issue.agent, issue.phase, issue.kind, issue.detail
        );
    }
    println!(
        "agent doctor summary: checked={} issues={}",
        output.checked,
        output.issues.len()
    );
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
