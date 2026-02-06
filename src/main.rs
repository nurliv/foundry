use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "foundry")]
#[command(about = "Spec graph CLI for AI-driven development support")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Spec(SpecCommand),
}

#[derive(Args, Debug)]
struct SpecCommand {
    #[command(subcommand)]
    command: SpecSubcommand,
}

#[derive(Subcommand, Debug)]
enum SpecSubcommand {
    Init(InitArgs),
    Lint,
    Link(LinkCommand),
    Impact(ImpactArgs),
}

#[derive(Args, Debug)]
struct InitArgs {
    #[arg(long)]
    sync: bool,
}

#[derive(Args, Debug)]
struct ImpactArgs {
    node_id: String,
    #[arg(long, default_value_t = 2)]
    depth: usize,
    #[arg(long, value_enum, default_value_t = ImpactFormat::Table)]
    format: ImpactFormat,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum ImpactFormat {
    Table,
    Json,
}

#[derive(Args, Debug)]
struct LinkCommand {
    #[command(subcommand)]
    command: LinkSubcommand,
}

#[derive(Subcommand, Debug)]
enum LinkSubcommand {
    Add(LinkAddArgs),
    Remove(LinkRemoveArgs),
    List(LinkListArgs),
    Propose(LinkProposeArgs),
}

#[derive(Args, Debug)]
struct LinkAddArgs {
    #[arg(long)]
    from: String,
    #[arg(long)]
    to: String,
    #[arg(long)]
    r#type: String,
    #[arg(long)]
    rationale: String,
    #[arg(long, default_value_t = 1.0)]
    confidence: f64,
}

#[derive(Args, Debug)]
struct LinkRemoveArgs {
    #[arg(long)]
    from: String,
    #[arg(long)]
    to: String,
    #[arg(long)]
    r#type: String,
}

#[derive(Args, Debug)]
struct LinkListArgs {
    #[arg(long)]
    node: String,
}

#[derive(Args, Debug)]
struct LinkProposeArgs {
    #[arg(long)]
    node: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecNodeMeta {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    status: String,
    title: String,
    body_md_path: String,
    terms: Vec<String>,
    hash: String,
    edges: Vec<SpecEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecEdge {
    to: String,
    #[serde(rename = "type")]
    edge_type: String,
    rationale: String,
    confidence: f64,
    status: String,
}

#[derive(Default)]
struct InitSummary {
    created: usize,
    updated: usize,
    skipped: usize,
    errors: usize,
}

#[derive(Default)]
struct LintState {
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DirectDependency {
    to: String,
    edge_type: String,
    status: String,
    confidence: f64,
    rationale: String,
}

#[derive(Debug, Serialize)]
struct ImpactOutput {
    node_id: String,
    depth: usize,
    direct_dependencies: Vec<DirectDependency>,
    reverse_dependents: Vec<String>,
    test_coverage_chain: Vec<String>,
    conflict_risks: Vec<String>,
    recommended_review_order: Vec<String>,
}

fn main() {
    match run() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(err) => {
            eprintln!("error: {err:#}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        Command::Spec(spec) => match spec.command {
            SpecSubcommand::Init(args) => {
                run_init(args.sync)?;
                Ok(0)
            }
            SpecSubcommand::Lint => Ok(run_lint()?),
            SpecSubcommand::Link(link) => {
                run_link(link)?;
                Ok(0)
            }
            SpecSubcommand::Impact(args) => {
                run_impact(&args)?;
                Ok(0)
            }
        },
    }
}

fn run_init(sync: bool) -> Result<()> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        println!("spec/ directory not found. nothing to initialize.");
        return Ok(());
    }

    let md_files = find_markdown_files(spec_root)?;
    let mut used_ids = load_existing_ids(spec_root)?;
    let mut next_id = next_available_id(&used_ids);
    let mut summary = InitSummary::default();

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
    Ok(())
}

fn run_lint() -> Result<i32> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        println!("lint: spec/ directory not found");
        return Ok(0);
    }

    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut id_to_meta = HashMap::<String, SpecNodeMeta>::new();
    let mut duplicate_ids = HashSet::<String>::new();
    let mut incoming_counts = HashMap::<String, usize>::new();
    let mut outgoing_counts = HashMap::<String, usize>::new();

    for (_, meta) in &metas {
        if id_to_meta.insert(meta.id.clone(), meta.clone()).is_some() {
            duplicate_ids.insert(meta.id.clone());
        }
    }
    for id in duplicate_ids {
        lint.errors.push(format!("duplicate node id: {id}"));
    }

    for (meta_path, meta) in &metas {
        if !Path::new(&meta.body_md_path).exists() {
            lint.errors.push(format!(
                "{} points to missing markdown file: {}",
                meta_path.display(),
                meta.body_md_path
            ));
            continue;
        }

        let body = fs::read_to_string(&meta.body_md_path).with_context(|| {
            format!("failed reading markdown for lint: {}", meta.body_md_path)
        })?;
        let actual_hash = sha256_hex(body.as_bytes());
        if meta.hash != actual_hash {
            lint.errors.push(format!(
                "hash mismatch for {} (id={}): expected {} actual {}",
                meta.body_md_path, meta.id, meta.hash, actual_hash
            ));
        }

        for edge in &meta.edges {
            *outgoing_counts.entry(meta.id.clone()).or_default() += 1;
            *incoming_counts.entry(edge.to.clone()).or_default() += 1;

            if !id_to_meta.contains_key(&edge.to) {
                lint.errors.push(format!(
                    "unknown edge target from {} to {}",
                    meta.id, edge.to
                ));
            }
            if edge.confidence < 0.0 || edge.confidence > 1.0 {
                lint.errors.push(format!(
                    "invalid edge confidence from {} to {}: {}",
                    meta.id, edge.to, edge.confidence
                ));
            }
            if edge.edge_type == "conflicts_with" && edge.status == "confirmed" {
                if let Some(target) = id_to_meta.get(&edge.to) {
                    if meta.status == "active" && target.status == "active" {
                        lint.errors.push(format!(
                            "unresolved conflict: {} conflicts_with {}",
                            meta.id, target.id
                        ));
                    }
                }
            }
        }
    }

    for (_, meta) in &metas {
        let in_count = incoming_counts.get(&meta.id).copied().unwrap_or(0);
        let out_count = outgoing_counts.get(&meta.id).copied().unwrap_or(0);
        if meta.node_type != "product_goal" && in_count == 0 && out_count == 0 {
            lint.errors.push(format!("orphan node: {}", meta.id));
        }
    }

    if lint.errors.is_empty() {
        println!("lint: ok");
        return Ok(0);
    }

    for err in &lint.errors {
        println!("lint: error: {err}");
    }
    println!("lint summary: {} error(s)", lint.errors.len());
    Ok(1)
}

fn run_link(link: LinkCommand) -> Result<()> {
    let spec_root = Path::new("spec");
    let metas = load_all_meta(spec_root, &mut LintState::default())?;
    let mut by_id = HashMap::<String, (PathBuf, SpecNodeMeta)>::new();
    for (path, meta) in metas {
        by_id.insert(meta.id.clone(), (path, meta));
    }

    match link.command {
        LinkSubcommand::Add(args) => {
            let valid_types = [
                "depends_on",
                "refines",
                "conflicts_with",
                "tests",
                "impacts",
            ];
            if !valid_types.contains(&args.r#type.as_str()) {
                anyhow::bail!("invalid edge type: {}", args.r#type);
            }
            if !(0.0..=1.0).contains(&args.confidence) {
                anyhow::bail!("confidence must be between 0.0 and 1.0");
            }
            if !by_id.contains_key(&args.to) {
                anyhow::bail!("target node not found: {}", args.to);
            }
            let (path, from_meta) = by_id
                .get_mut(&args.from)
                .with_context(|| format!("source node not found: {}", args.from))?;

            if let Some(edge) = from_meta
                .edges
                .iter_mut()
                .find(|e| e.to == args.to && e.edge_type == args.r#type)
            {
                edge.rationale = args.rationale;
                edge.confidence = args.confidence;
                edge.status = "confirmed".to_string();
                println!("link updated: {} -> {} ({})", args.from, args.to, args.r#type);
            } else {
                from_meta.edges.push(SpecEdge {
                    to: args.to.clone(),
                    edge_type: args.r#type.clone(),
                    rationale: args.rationale,
                    confidence: args.confidence,
                    status: "confirmed".to_string(),
                });
                println!("link added: {} -> {} ({})", args.from, args.to, args.r#type);
            }
            write_meta_json(path, from_meta)?;
        }
        LinkSubcommand::Remove(args) => {
            let (path, from_meta) = by_id
                .get_mut(&args.from)
                .with_context(|| format!("source node not found: {}", args.from))?;
            let before = from_meta.edges.len();
            from_meta
                .edges
                .retain(|e| !(e.to == args.to && e.edge_type == args.r#type));
            if from_meta.edges.len() == before {
                println!("link not found: {} -> {} ({})", args.from, args.to, args.r#type);
            } else {
                write_meta_json(path, from_meta)?;
                println!("link removed: {} -> {} ({})", args.from, args.to, args.r#type);
            }
        }
        LinkSubcommand::List(args) => {
            let (_, meta) = by_id
                .get(&args.node)
                .with_context(|| format!("node not found: {}", args.node))?;
            println!("outgoing edges for {}:", args.node);
            if meta.edges.is_empty() {
                println!("  (none)");
            }
            for e in &meta.edges {
                println!(
                    "  -> {} [{}] status={} confidence={} rationale={}",
                    e.to, e.edge_type, e.status, e.confidence, e.rationale
                );
            }

            println!("incoming edges for {}:", args.node);
            let mut found_incoming = false;
            for (from_id, (_, from_meta)) in &by_id {
                for e in &from_meta.edges {
                    if e.to == args.node {
                        found_incoming = true;
                        println!(
                            "  <- {} [{}] status={} confidence={} rationale={}",
                            from_id, e.edge_type, e.status, e.confidence, e.rationale
                        );
                    }
                }
            }
            if !found_incoming {
                println!("  (none)");
            }
        }
        LinkSubcommand::Propose(args) => {
            println!(
                "link propose is not implemented yet. requested node={:?}",
                args.node
            );
        }
    }
    Ok(())
}

fn run_impact(args: &ImpactArgs) -> Result<()> {
    let node_id = args.node_id.as_str();
    let spec_root = Path::new("spec");
    let metas = load_all_meta(spec_root, &mut LintState::default())?;
    let mut by_id = HashMap::<String, SpecNodeMeta>::new();
    for (_, meta) in metas {
        by_id.insert(meta.id.clone(), meta);
    }
    if !by_id.contains_key(node_id) {
        anyhow::bail!("node not found: {node_id}");
    }

    let node = by_id.get(node_id).expect("node existence checked");
    let mut direct_dependencies: Vec<DirectDependency> = node
        .edges
        .iter()
        .filter(|e| e.edge_type == "depends_on" || e.edge_type == "impacts")
        .map(|e| DirectDependency {
            to: e.to.clone(),
            edge_type: e.edge_type.clone(),
            status: e.status.clone(),
            confidence: e.confidence,
            rationale: e.rationale.clone(),
        })
        .collect();
    direct_dependencies.sort_by(|a, b| a.to.cmp(&b.to).then(a.edge_type.cmp(&b.edge_type)));

    let reverse_dependents = reverse_dependents(node_id, args.depth, &by_id);

    let test_coverage_chain = test_coverage_chain(node_id, args.depth, &by_id);

    let mut conflicts = BTreeSet::<String>::new();
    for e in &node.edges {
        if e.edge_type == "conflicts_with" {
            conflicts.insert(e.to.clone());
        }
    }
    for (id, m) in &by_id {
        if m.edges
            .iter()
            .any(|e| e.to == node_id && e.edge_type == "conflicts_with")
        {
            conflicts.insert(id.clone());
        }
    }

    let review_order = bfs_review_order(node_id, args.depth, &by_id);
    let output = ImpactOutput {
        node_id: node_id.to_string(),
        depth: args.depth,
        direct_dependencies,
        reverse_dependents,
        test_coverage_chain,
        conflict_risks: conflicts.into_iter().collect(),
        recommended_review_order: review_order,
    };

    if args.format == ImpactFormat::Json {
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("direct_dependencies:");
    print_direct_dependencies(&output.direct_dependencies);
    println!("reverse_dependents:");
    print_string_list(&output.reverse_dependents);
    println!("test_coverage_chain:");
    print_string_list(&output.test_coverage_chain);
    println!("conflict_risks:");
    print_string_list(&output.conflict_risks);
    println!("recommended_review_order:");
    print_string_list(&output.recommended_review_order);
    Ok(())
}

fn load_existing_ids(spec_root: &Path) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    for entry in WalkDir::new(spec_root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if !is_meta_json(path) {
            continue;
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read meta file: {}", path.display()))?;
        let meta: SpecNodeMeta = serde_json::from_str(&raw)
            .with_context(|| format!("invalid meta file: {}", path.display()))?;
        ids.insert(meta.id);
    }
    Ok(ids)
}

fn load_all_meta(spec_root: &Path, lint: &mut LintState) -> Result<Vec<(PathBuf, SpecNodeMeta)>> {
    let mut metas = Vec::new();
    for entry in WalkDir::new(spec_root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if !is_meta_json(path) {
            continue;
        }
        let raw = match fs::read_to_string(path) {
            Ok(v) => v,
            Err(err) => {
                lint.errors
                    .push(format!("cannot read {}: {err}", path.display()));
                continue;
            }
        };
        match serde_json::from_str::<SpecNodeMeta>(&raw) {
            Ok(meta) => metas.push((path.to_path_buf(), meta)),
            Err(err) => lint
                .errors
                .push(format!("invalid json {}: {err}", path.display())),
        }
    }
    Ok(metas)
}

fn find_markdown_files(spec_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(spec_root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.file_type().is_file() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && !path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .ends_with(".meta.md")
            {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_meta_json(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.ends_with(".meta.json"))
}

fn md_to_meta_path(md_path: &Path) -> Result<PathBuf> {
    let file_name = md_path
        .file_name()
        .and_then(|s| s.to_str())
        .with_context(|| format!("invalid markdown filename: {}", md_path.display()))?;
    let base = file_name
        .strip_suffix(".md")
        .with_context(|| format!("markdown file must end with .md: {}", md_path.display()))?;
    Ok(md_path.with_file_name(format!("{base}.meta.json")))
}

fn write_meta_json(path: &Path, meta: &SpecNodeMeta) -> Result<()> {
    let text = serde_json::to_string_pretty(meta)?;
    fs::write(path, text + "\n")
        .with_context(|| format!("failed writing meta file: {}", path.display()))?;
    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

fn extract_title(body: &str, path: &Path) -> String {
    for line in body.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("# ") {
            let v = value.trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn next_available_id(existing: &HashSet<String>) -> usize {
    existing
        .iter()
        .filter_map(|id| id.strip_prefix("SPC-"))
        .filter_map(|v| v.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn reverse_dependents(seed: &str, max_depth: usize, by_id: &HashMap<String, SpecNodeMeta>) -> Vec<String> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut out = BTreeSet::new();

    queue.push_back((seed.to_string(), 0usize));
    visited.insert(seed.to_string());

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for (id, m) in by_id {
            let connected = m
                .edges
                .iter()
                .any(|e| e.to == current && e.edge_type == "depends_on");
            if connected && visited.insert(id.clone()) {
                out.insert(id.clone());
                queue.push_back((id.clone(), depth + 1));
            }
        }
    }

    out.into_iter().collect()
}

fn test_coverage_chain(seed: &str, max_depth: usize, by_id: &HashMap<String, SpecNodeMeta>) -> Vec<String> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut out = BTreeSet::new();

    queue.push_back((seed.to_string(), 0usize));
    visited.insert(seed.to_string());

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(meta) = by_id.get(&current) {
            for edge in &meta.edges {
                if edge.edge_type == "tests" && visited.insert(edge.to.clone()) {
                    out.insert(edge.to.clone());
                    queue.push_back((edge.to.clone(), depth + 1));
                }
            }
        }
        for (id, m) in by_id {
            let connected = m.edges.iter().any(|e| e.to == current && e.edge_type == "tests");
            if connected && visited.insert(id.clone()) {
                out.insert(id.clone());
                queue.push_back((id.clone(), depth + 1));
            }
        }
    }

    out.into_iter().collect()
}

fn bfs_review_order(seed: &str, max_depth: usize, by_id: &HashMap<String, SpecNodeMeta>) -> Vec<String> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut out = Vec::new();

    queue.push_back((seed.to_string(), 0usize));
    visited.insert(seed.to_string());

    while let Some((current, depth)) = queue.pop_front() {
        out.push(current.clone());
        if depth >= max_depth {
            continue;
        }

        if let Some(meta) = by_id.get(&current) {
            for edge in &meta.edges {
                if (edge.edge_type == "depends_on"
                    || edge.edge_type == "impacts"
                    || edge.edge_type == "tests")
                    && visited.insert(edge.to.clone())
                {
                    queue.push_back((edge.to.clone(), depth + 1));
                }
            }
        }
        for (id, m) in by_id {
            let connected = m.edges.iter().any(|e| {
                e.to == current
                    && (e.edge_type == "depends_on"
                        || e.edge_type == "impacts"
                        || e.edge_type == "tests")
            });
            if connected && visited.insert(id.clone()) {
                queue.push_back((id.clone(), depth + 1));
            }
        }
    }

    out
}

fn print_direct_dependencies(edges: &[DirectDependency]) {
    if edges.is_empty() {
        println!("  (none)");
        return;
    }
    for edge in edges {
        println!(
            "  - {} [{}] status={} confidence={} rationale={}",
            edge.to, edge.edge_type, edge.status, edge.confidence, edge.rationale
        );
    }
}

fn print_string_list(values: &[String]) {
    if values.is_empty() {
        println!("  (none)");
        return;
    }
    for value in values {
        println!("  - {value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, edges: Vec<SpecEdge>) -> SpecNodeMeta {
        SpecNodeMeta {
            id: id.to_string(),
            node_type: "feature_requirement".to_string(),
            status: "draft".to_string(),
            title: id.to_string(),
            body_md_path: format!("spec/{id}.md"),
            terms: Vec::new(),
            hash: "0".repeat(64),
            edges,
        }
    }

    #[test]
    fn extract_title_uses_heading() {
        let title = extract_title("# Hello\n\ntext", Path::new("spec/a.md"));
        assert_eq!(title, "Hello");
    }

    #[test]
    fn extract_title_falls_back_to_filename() {
        let title = extract_title("no heading", Path::new("spec/fallback-name.md"));
        assert_eq!(title, "fallback-name");
    }

    #[test]
    fn md_to_meta_path_converts_suffix() {
        let path = md_to_meta_path(Path::new("spec/10-domain-model.md")).unwrap();
        assert_eq!(path, PathBuf::from("spec/10-domain-model.meta.json"));
    }

    #[test]
    fn sha256_hex_returns_64_chars() {
        let hash = sha256_hex(b"hello");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn next_available_id_skips_max() {
        let ids = HashSet::from([
            "SPC-001".to_string(),
            "SPC-003".to_string(),
            "SPC-010".to_string(),
        ]);
        assert_eq!(next_available_id(&ids), 11);
    }

    #[test]
    fn bfs_review_order_follows_link_types() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            node(
                "SPC-001",
                vec![
                    SpecEdge {
                        to: "SPC-002".to_string(),
                        edge_type: "depends_on".to_string(),
                        rationale: "dep".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                    SpecEdge {
                        to: "SPC-004".to_string(),
                        edge_type: "conflicts_with".to_string(),
                        rationale: "conflict".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                ],
            ),
        );
        map.insert("SPC-002".to_string(), node("SPC-002", Vec::new()));
        map.insert(
            "SPC-003".to_string(),
            node(
                "SPC-003",
                vec![SpecEdge {
                    to: "SPC-001".to_string(),
                    edge_type: "tests".to_string(),
                    rationale: "test".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            ),
        );
        map.insert("SPC-004".to_string(), node("SPC-004", Vec::new()));

        let order = bfs_review_order("SPC-001", 3, &map);

        assert!(order.contains(&"SPC-001".to_string()));
        assert!(order.contains(&"SPC-002".to_string()));
        assert!(order.contains(&"SPC-003".to_string()));
        assert!(!order.contains(&"SPC-004".to_string()));
    }

    #[test]
    fn bfs_review_order_respects_depth() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            node(
                "SPC-001",
                vec![SpecEdge {
                    to: "SPC-002".to_string(),
                    edge_type: "depends_on".to_string(),
                    rationale: "dep".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            ),
        );
        map.insert(
            "SPC-002".to_string(),
            node(
                "SPC-002",
                vec![SpecEdge {
                    to: "SPC-003".to_string(),
                    edge_type: "depends_on".to_string(),
                    rationale: "dep".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            ),
        );
        map.insert("SPC-003".to_string(), node("SPC-003", Vec::new()));

        let depth1 = bfs_review_order("SPC-001", 1, &map);
        assert!(depth1.contains(&"SPC-001".to_string()));
        assert!(depth1.contains(&"SPC-002".to_string()));
        assert!(!depth1.contains(&"SPC-003".to_string()));
    }
}
