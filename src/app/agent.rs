use super::*;

const TEMPLATE_PHASES: &[&str] = &[
    "spec-plan",
    "spec-review",
    "design-plan",
    "design-review",
    "task-breakdown",
    "implement",
    "impl-review",
];

#[derive(Clone, Copy)]
struct TemplateArtifact {
    label: &'static str,
    template_subdir: &'static str,
    output_subdir: &'static str,
}

const TEMPLATE_ARTIFACTS: &[TemplateArtifact] = &[
    TemplateArtifact {
        label: "commands",
        template_subdir: "commands",
        output_subdir: "commands",
    },
    TemplateArtifact {
        label: "skills",
        template_subdir: "skills",
        output_subdir: "skills",
    },
];

struct TemplateContext {
    project_name: String,
    main_spec_id: String,
    default_depth: String,
}

#[derive(Default)]
pub(super) struct AgentTemplateSummary {
    pub(super) written: usize,
    pub(super) skipped: usize,
    pub(super) errors: usize,
}

pub(super) struct TemplateConfig {
    pub(super) source: TemplateSource,
    pub(super) repo: String,
    pub(super) git_ref: String,
    pub(super) output: AgentOutput,
    pub(super) codex_home: Option<String>,
    pub(super) claude_dir: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentDoctorIssue {
    agent: String,
    artifact: String,
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
        AgentSubcommand::Doctor(args) => {
            let config = TemplateConfig {
                source: args.template_source,
                repo: args.template_repo.clone(),
                git_ref: args.template_ref.clone(),
                output: args.agent_output,
                codex_home: args.codex_home.clone(),
                claude_dir: args.claude_dir.clone(),
            };
            run_agent_doctor(&args, &config)
        }
    }
}

pub(super) fn generate_agent_templates(
    agents: &[AgentTarget],
    sync: bool,
    config: &TemplateConfig,
) -> AgentTemplateSummary {
    let mut summary = AgentTemplateSummary::default();
    let template_base_root = resolve_template_base_root(config);
    let context = build_template_context();
    let mut uniq = HashSet::new();
    for agent in agents {
        if !uniq.insert(*agent) {
            continue;
        }
        let slug = agent_slug(*agent);
        for artifact in TEMPLATE_ARTIFACTS {
            let template_root = template_base_root.join(artifact.template_subdir);
            for phase in TEMPLATE_PHASES {
                let base_path = template_root.join(format!("base/{phase}.md"));
                let overlay_path = template_root.join(format!("overlays/{slug}/{phase}.md"));
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

                let rendered = render_template(&base, &overlay, &context);
                let out_paths = output_paths(*agent, *artifact, phase, config);
                let mut uniq_paths = HashSet::new();
                for out_path in out_paths {
                    if !uniq_paths.insert(out_path.clone()) {
                        continue;
                    }
                    if out_path.exists() && !sync {
                        summary.skipped += 1;
                        continue;
                    }
                    if let Some(parent) = out_path.parent()
                        && let Err(err) = fs::create_dir_all(parent)
                    {
                        summary.errors += 1;
                        eprintln!("agent template error creating {}: {err}", parent.display());
                        continue;
                    }
                    if let Err(err) = fs::write(&out_path, &rendered) {
                        summary.errors += 1;
                        eprintln!("agent template error writing {}: {err}", out_path.display());
                        continue;
                    }
                    summary.written += 1;
                }
            }
        }
    }
    summary
}

fn run_agent_doctor(args: &AgentDoctorArgs, config: &TemplateConfig) -> Result<i32> {
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
    let context = build_template_context();
    let template_base_root = resolve_template_base_root(config);

    let mut issues = Vec::<AgentDoctorIssue>::new();
    let mut checked = 0usize;

    for agent in agents {
        let slug = agent_slug(agent).to_string();
        for artifact in TEMPLATE_ARTIFACTS {
            let template_root = template_base_root.join(artifact.template_subdir);
            for phase in TEMPLATE_PHASES {
                checked += 1;
                let base_path = template_root.join(format!("base/{phase}.md"));
                let overlay_path = template_root.join(format!("overlays/{slug}/{phase}.md"));
                let out_paths = output_paths(agent, *artifact, phase, config);

                let base = match fs::read_to_string(&base_path) {
                    Ok(v) => v,
                    Err(err) => {
                        issues.push(AgentDoctorIssue {
                            agent: slug.clone(),
                            artifact: artifact.label.to_string(),
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
                            artifact: artifact.label.to_string(),
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
                let expected = render_template(&base, &overlay, &context);
                let mut uniq_paths = HashSet::new();
                for out_path in out_paths {
                    if !uniq_paths.insert(out_path.clone()) {
                        continue;
                    }
                    let actual = match fs::read_to_string(&out_path) {
                        Ok(v) => v,
                        Err(err) => {
                            issues.push(AgentDoctorIssue {
                                agent: slug.clone(),
                                artifact: artifact.label.to_string(),
                                phase: phase.to_string(),
                                kind: "generated_missing".to_string(),
                                detail: format!(
                                    "missing generated file {}: {err}",
                                    out_path.display()
                                ),
                            });
                            continue;
                        }
                    };
                    if actual != expected {
                        issues.push(AgentDoctorIssue {
                            agent: slug.clone(),
                            artifact: artifact.label.to_string(),
                            phase: phase.to_string(),
                            kind: "generated_stale".to_string(),
                            detail: format!(
                                "generated file differs from template {}",
                                out_path.display()
                            ),
                        });
                    }
                }
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

fn resolve_template_base_root(config: &TemplateConfig) -> PathBuf {
    let local_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    match config.source {
        TemplateSource::Local => local_root,
        TemplateSource::Github => match sync_templates_from_github(config) {
            Ok(path) => path,
            Err(err) => {
                eprintln!(
                    "agent template warning: failed to fetch templates from github ({err}). fallback to local templates."
                );
                local_root
            }
        },
    }
}

fn sync_templates_from_github(config: &TemplateConfig) -> Result<PathBuf> {
    let cache_root = PathBuf::from(".foundry/template-sources");
    fs::create_dir_all(&cache_root)?;
    let key = template_cache_key(config);
    let repo_dir = cache_root.join(&key);

    if repo_dir.exists() {
        if repo_dir.join("commands").is_dir() && repo_dir.join("skills").is_dir() {
            return Ok(repo_dir);
        }
        if repo_dir.join("templates").is_dir() {
            return Ok(repo_dir.join("templates"));
        }
        let _ = fs::remove_dir_all(&repo_dir);
    }

    let archive_url = github_archive_url(&config.repo, &config.git_ref)?;
    let archive_path = cache_root.join(format!("{key}.tar.gz"));
    let extract_tmp = cache_root.join(format!("{key}.extract"));

    if extract_tmp.exists() {
        let _ = fs::remove_dir_all(&extract_tmp);
    }
    fs::create_dir_all(&extract_tmp)?;

    let response = reqwest::blocking::get(&archive_url)
        .with_context(|| format!("failed to download template archive: {archive_url}"))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "template archive download failed: {} ({})",
            archive_url,
            response.status()
        );
    }

    let bytes = response
        .bytes()
        .with_context(|| format!("failed to read archive body: {archive_url}"))?;
    fs::write(&archive_path, &bytes)
        .with_context(|| format!("failed writing archive file: {}", archive_path.display()))?;

    let tar_gz = fs::File::open(&archive_path)
        .with_context(|| format!("failed opening archive: {}", archive_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(&extract_tmp)
        .with_context(|| format!("failed unpacking archive to {}", extract_tmp.display()))?;

    let extracted_repo_root = detect_extracted_repo_root(&extract_tmp)
        .with_context(|| format!("failed to locate extracted repo root in {}", extract_tmp.display()))?;
    let templates_src = extracted_repo_root.join("templates");
    if !templates_src.is_dir() {
        anyhow::bail!(
            "templates directory missing in extracted archive: {}",
            templates_src.display()
        );
    }
    fs::rename(&templates_src, &repo_dir).with_context(|| {
        format!(
            "failed moving extracted templates from {} to {}",
            templates_src.display(),
            repo_dir.display()
        )
    })?;

    let _ = fs::remove_file(&archive_path);
    let _ = fs::remove_dir_all(&extract_tmp);
    Ok(repo_dir)
}

fn template_cache_key(config: &TemplateConfig) -> String {
    let mut hasher = Sha256::new();
    hasher.update(config.repo.as_bytes());
    hasher.update(b"\n");
    hasher.update(config.git_ref.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn github_archive_url(repo: &str, git_ref: &str) -> Result<String> {
    let repo = repo.trim().trim_end_matches(".git");
    let normalized = if let Some(path) = repo.strip_prefix("git@github.com:") {
        format!("https://github.com/{path}")
    } else {
        repo.to_string()
    };
    if !normalized.starts_with("https://github.com/") {
        anyhow::bail!("unsupported template repo url for github archive: {repo}");
    }
    Ok(format!("{normalized}/archive/{git_ref}.tar.gz"))
}

fn detect_extracted_repo_root(extract_tmp: &Path) -> Result<PathBuf> {
    let mut dirs = fs::read_dir(extract_tmp)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    for dir in dirs {
        if dir.join("templates").is_dir() {
            return Ok(dir);
        }
    }
    anyhow::bail!("no extracted directory with templates/ found")
}

fn print_agent_doctor_table(output: &AgentDoctorOutput) {
    if output.ok {
        println!("agent doctor: ok (checked={})", output.checked);
        return;
    }
    for issue in &output.issues {
        println!(
            "agent doctor: issue: agent={} artifact={} phase={} kind={} detail={}",
            issue.agent, issue.artifact, issue.phase, issue.kind, issue.detail
        );
    }
    println!(
        "agent doctor summary: checked={} issues={}",
        output.checked,
        output.issues.len()
    );
}

fn render_template(base: &str, overlay: &str, context: &TemplateContext) -> String {
    let mut out = String::new();
    out.push_str(base.trim_end());
    out.push_str("\n\n---\n\n");
    out.push_str(overlay.trim_end());
    out.push('\n');
    apply_placeholders(&out, context)
}

fn agent_slug(agent: AgentTarget) -> &'static str {
    match agent {
        AgentTarget::Codex => "codex",
        AgentTarget::Claude => "claude",
    }
}

fn build_template_context() -> TemplateContext {
    let project_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|v| v.to_string_lossy().to_string()))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "project".to_string());
    let main_spec_id = detect_main_spec_id();
    let default_depth = "2".to_string();
    TemplateContext {
        project_name,
        main_spec_id,
        default_depth,
    }
}

fn detect_main_spec_id() -> String {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        return "SPC-001".to_string();
    }
    let mut lint = LintState::default();
    let metas = match load_all_meta(spec_root, &mut lint) {
        Ok(v) => v,
        Err(_) => return "SPC-001".to_string(),
    };
    let mut product_goals = metas
        .iter()
        .map(|(_, m)| m)
        .filter(|m| m.node_type == "product_goal")
        .map(|m| m.id.clone())
        .collect::<Vec<_>>();
    product_goals.sort();
    if let Some(first) = product_goals.first() {
        return first.clone();
    }
    let mut ids = metas.into_iter().map(|(_, m)| m.id).collect::<Vec<_>>();
    ids.sort();
    ids.first().cloned().unwrap_or_else(|| "SPC-001".to_string())
}

fn apply_placeholders(text: &str, context: &TemplateContext) -> String {
    text.replace("{{project_name}}", &context.project_name)
        .replace("{{main_spec_id}}", &context.main_spec_id)
        .replace("{{default_depth}}", &context.default_depth)
}

fn output_paths(
    agent: AgentTarget,
    artifact: TemplateArtifact,
    phase: &str,
    config: &TemplateConfig,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if matches!(config.output, AgentOutput::Docs | AgentOutput::Both) {
        let slug = agent_slug(agent);
        paths.push(PathBuf::from(format!(
            "docs/agents/{slug}/{}/{phase}.md",
            artifact.output_subdir
        )));
    }
    if matches!(config.output, AgentOutput::Install | AgentOutput::Both) {
        paths.push(install_path(agent, artifact, phase, config));
    }
    paths
}

fn install_path(
    agent: AgentTarget,
    artifact: TemplateArtifact,
    phase: &str,
    config: &TemplateConfig,
) -> PathBuf {
    match agent {
        AgentTarget::Codex => {
            if artifact.output_subdir == "commands" {
                resolve_codex_home(config)
                    .join("prompts")
                    .join(format!("{phase}.md"))
            } else {
                resolve_codex_home(config)
                    .join(artifact.output_subdir)
                    .join("foundry")
                    .join(format!("{phase}.md"))
            }
        }
        AgentTarget::Claude => resolve_claude_dir(config)
            .join(artifact.output_subdir)
            .join("foundry")
            .join(format!("{phase}.md")),
    }
}

fn resolve_codex_home(config: &TemplateConfig) -> PathBuf {
    if let Some(path) = config.codex_home.as_ref().filter(|s| !s.trim().is_empty()) {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("CODEX_HOME") && !path.trim().is_empty() {
        return PathBuf::from(path);
    }
    if let Ok(home) = std::env::var("HOME") && !home.trim().is_empty() {
        return PathBuf::from(home).join(".codex");
    }
    PathBuf::from(".codex")
}

fn resolve_claude_dir(config: &TemplateConfig) -> PathBuf {
    if let Some(path) = config.claude_dir.as_ref().filter(|s| !s.trim().is_empty()) {
        return PathBuf::from(path);
    }
    PathBuf::from(".claude")
}
