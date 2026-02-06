use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use crate::cli::*;
mod ask;
mod search;
use search::*;

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

const NODE_TYPES: &[&str] = &[
    "product_goal",
    "feature_requirement",
    "non_functional_requirement",
    "constraint",
    "domain_concept",
    "decision",
    "workflow",
    "api_contract",
    "data_contract",
    "test_spec",
];

const NODE_STATUSES: &[&str] = &["draft", "review", "active", "deprecated", "archived"];
const EDGE_TYPES: &[&str] = &["depends_on", "refines", "conflicts_with", "tests", "impacts"];
const EDGE_STATUSES: &[&str] = &["confirmed", "proposed"];
const EMBEDDING_DIM: usize = 256;

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


#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RuntimeConfig {
    ask: AskRuntimeConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            ask: AskRuntimeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct AskRuntimeConfig {
    neighbor_limit: usize,
    snippet_count_in_answer: usize,
    edge_weight: AskEdgeWeightConfig,
}

impl Default for AskRuntimeConfig {
    fn default() -> Self {
        Self {
            neighbor_limit: 5,
            snippet_count_in_answer: 2,
            edge_weight: AskEdgeWeightConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct AskEdgeWeightConfig {
    depends_on: f64,
    tests: f64,
    refines: f64,
    impacts: f64,
    conflicts_with: f64,
}

impl Default for AskEdgeWeightConfig {
    fn default() -> Self {
        Self {
            depends_on: 1.0,
            tests: 0.8,
            refines: 0.7,
            impacts: 0.6,
            conflicts_with: 1.2,
        }
    }
}


pub fn run_main() {
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
            SpecSubcommand::Search(search) => {
                run_search(search)?;
                Ok(0)
            }
            SpecSubcommand::Ask(args) => {
                ask::run_ask(&args)?;
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
    let mut normalized_term_variants = BTreeMap::<String, BTreeSet<String>>::new();

    for (_, meta) in &metas {
        if id_to_meta.insert(meta.id.clone(), meta.clone()).is_some() {
            duplicate_ids.insert(meta.id.clone());
        }
    }
    for id in duplicate_ids {
        lint.errors.push(format!("duplicate node id: {id}"));
    }

    for (meta_path, meta) in &metas {
        validate_meta_semantics(meta_path, meta, &mut lint);

        for term in &meta.terms {
            let normalized = normalize_term_key(term);
            if normalized.is_empty() {
                lint.errors.push(format!(
                    "empty or non-normalizable term in {} (id={})",
                    meta_path.display(),
                    meta.id
                ));
                continue;
            }
            normalized_term_variants
                .entry(normalized)
                .or_default()
                .insert(term.clone());
        }

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
            if !EDGE_TYPES.contains(&edge.edge_type.as_str()) {
                lint.errors.push(format!(
                    "invalid edge type from {} to {}: {}",
                    meta.id, edge.to, edge.edge_type
                ));
            }
            if !EDGE_STATUSES.contains(&edge.status.as_str()) {
                lint.errors.push(format!(
                    "invalid edge status from {} to {}: {}",
                    meta.id, edge.to, edge.status
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

    for (normalized, variants) in normalized_term_variants {
        if variants.len() > 1 {
            let joined = variants.into_iter().collect::<Vec<_>>().join(", ");
            lint.errors.push(format!(
                "term key drift detected for normalized key '{normalized}': {joined}"
            ));
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
            upsert_edge(
                &mut by_id,
                UpsertEdge {
                    from: &args.from,
                    to: &args.to,
                    edge_type: &args.r#type,
                    rationale: &args.rationale,
                    confidence: args.confidence,
                    status: "confirmed",
                    created_label: "link added",
                    updated_label: "link updated",
                },
            )?;
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
            if let (Some(from), Some(to)) = (&args.from, &args.to) {
                let rationale = args
                    .rationale
                    .as_deref()
                    .unwrap_or("manual proposed link")
                    .to_string();
                upsert_edge(
                    &mut by_id,
                    UpsertEdge {
                        from,
                        to,
                        edge_type: &args.r#type,
                        rationale: &rationale,
                        confidence: args.confidence,
                        status: "proposed",
                        created_label: "proposal added",
                        updated_label: "proposal updated",
                    },
                )?;
            } else if let Some(node_id) = args.node.as_deref() {
                propose_links_for_node(&mut by_id, node_id, args.limit)?;
            } else {
                anyhow::bail!(
                    "propose requires either --node <ID> or both --from <ID> and --to <ID>"
                );
            }
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

fn load_runtime_config() -> RuntimeConfig {
    let path = Path::new(".foundry/config.json");
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return RuntimeConfig::default(),
    };
    serde_json::from_str::<RuntimeConfig>(&raw).unwrap_or_default()
}

fn unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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

fn validate_meta_semantics(path: &Path, meta: &SpecNodeMeta, lint: &mut LintState) {
    if !is_valid_node_id(&meta.id) {
        lint.errors.push(format!(
            "invalid node id format in {}: {}",
            path.display(),
            meta.id
        ));
    }
    if meta.title.trim().is_empty() {
        lint.errors
            .push(format!("empty title in {} (id={})", path.display(), meta.id));
    }
    if meta.body_md_path.trim().is_empty() {
        lint.errors.push(format!(
            "empty body_md_path in {} (id={})",
            path.display(),
            meta.id
        ));
    } else if !(meta.body_md_path.starts_with("spec/") && meta.body_md_path.ends_with(".md")) {
        lint.errors.push(format!(
            "invalid body_md_path format in {} (id={}): {}",
            path.display(),
            meta.id,
            meta.body_md_path
        ));
    }
    if !NODE_TYPES.contains(&meta.node_type.as_str()) {
        lint.errors.push(format!(
            "invalid node type in {} (id={}): {}",
            path.display(),
            meta.id,
            meta.node_type
        ));
    }
    if !NODE_STATUSES.contains(&meta.status.as_str()) {
        lint.errors.push(format!(
            "invalid node status in {} (id={}): {}",
            path.display(),
            meta.id,
            meta.status
        ));
    }
    if !is_valid_sha256(&meta.hash) {
        lint.errors.push(format!(
            "invalid hash format in {} (id={}): {}",
            path.display(),
            meta.id,
            meta.hash
        ));
    }
}

fn is_valid_node_id(id: &str) -> bool {
    if let Some(num) = id.strip_prefix("SPC-") {
        return !num.is_empty() && num.chars().all(|c| c.is_ascii_digit());
    }
    false
}

fn is_valid_sha256(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn normalize_term_key(term: &str) -> String {
    term.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

struct UpsertEdge<'a> {
    from: &'a str,
    to: &'a str,
    edge_type: &'a str,
    rationale: &'a str,
    confidence: f64,
    status: &'a str,
    created_label: &'a str,
    updated_label: &'a str,
}

fn upsert_edge(
    by_id: &mut HashMap<String, (PathBuf, SpecNodeMeta)>,
    args: UpsertEdge<'_>,
) -> Result<()> {
    if !EDGE_TYPES.contains(&args.edge_type) {
        anyhow::bail!("invalid edge type: {}", args.edge_type);
    }
    if !(0.0..=1.0).contains(&args.confidence) {
        anyhow::bail!("confidence must be between 0.0 and 1.0");
    }
    if !EDGE_STATUSES.contains(&args.status) {
        anyhow::bail!("invalid edge status: {}", args.status);
    }
    if !by_id.contains_key(args.to) {
        anyhow::bail!("target node not found: {}", args.to);
    }
    let (path, from_meta) = by_id
        .get_mut(args.from)
        .with_context(|| format!("source node not found: {}", args.from))?;

    if let Some(edge) = from_meta
        .edges
        .iter_mut()
        .find(|e| e.to == args.to && e.edge_type == args.edge_type)
    {
        edge.rationale = args.rationale.to_string();
        edge.confidence = args.confidence;
        edge.status = args.status.to_string();
        println!(
            "{}: {} -> {} ({})",
            args.updated_label, args.from, args.to, args.edge_type
        );
    } else {
        from_meta.edges.push(SpecEdge {
            to: args.to.to_string(),
            edge_type: args.edge_type.to_string(),
            rationale: args.rationale.to_string(),
            confidence: args.confidence,
            status: args.status.to_string(),
        });
        println!(
            "{}: {} -> {} ({})",
            args.created_label, args.from, args.to, args.edge_type
        );
    }
    write_meta_json(path, from_meta)?;
    Ok(())
}

fn propose_links_for_node(
    by_id: &mut HashMap<String, (PathBuf, SpecNodeMeta)>,
    node_id: &str,
    limit: usize,
) -> Result<()> {
    if !by_id.contains_key(node_id) {
        anyhow::bail!("node not found: {node_id}");
    }
    let source = by_id
        .get(node_id)
        .map(|(_, meta)| meta.clone())
        .expect("checked above");

    let source_terms: HashSet<String> = source.terms.iter().map(|t| normalize_term_key(t)).collect();
    let source_title_tokens = tokenize(&source.title);

    let mut candidates: Vec<(String, usize)> = by_id
        .iter()
        .filter(|(id, _)| id.as_str() != node_id)
        .map(|(id, (_, meta))| {
            let target_terms: HashSet<String> =
                meta.terms.iter().map(|t| normalize_term_key(t)).collect();
            let term_overlap = source_terms.intersection(&target_terms).count();
            let title_overlap = source_title_tokens
                .intersection(&tokenize(&meta.title))
                .count();
            (id.clone(), term_overlap * 2 + title_overlap)
        })
        .filter(|(_, score)| *score > 0)
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut proposed = 0usize;
    for (target_id, score) in candidates.into_iter().take(limit) {
        let rationale = format!("auto proposal based on term/title overlap score={score}");
        upsert_edge(
            by_id,
            UpsertEdge {
                from: node_id,
                to: &target_id,
                edge_type: "impacts",
                rationale: &rationale,
                confidence: score_to_confidence(score),
                status: "proposed",
                created_label: "proposal added",
                updated_label: "proposal updated",
            },
        )?;
        proposed += 1;
    }
    println!("propose summary: node={node_id} proposed={proposed}");
    Ok(())
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn score_to_confidence(score: usize) -> f64 {
    match score {
        0 => 0.0,
        1 => 0.5,
        2 => 0.6,
        3 => 0.7,
        4 => 0.8,
        _ => 0.9,
    }
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
#[path = "app/tests.rs"]
mod tests;
