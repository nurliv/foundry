use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
mod impact;
mod init;
mod link;
mod search;
use impact::*;
use init::*;
use link::*;
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

#[cfg(test)]
#[path = "app/tests.rs"]
mod tests;
