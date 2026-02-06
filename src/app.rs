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

#[derive(Debug, Serialize)]
struct SearchHit {
    id: String,
    title: String,
    path: String,
    score: f64,
    matched_terms: Vec<String>,
    snippet: String,
}

#[derive(Debug, Serialize)]
struct SearchQueryOutput {
    query: String,
    mode: String,
    hits: Vec<SearchHit>,
}

#[derive(Debug, Serialize)]
struct AskCitation {
    id: String,
    title: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct AskEvidence {
    id: String,
    snippet: String,
    score: f64,
}

#[derive(Debug, Serialize)]
struct AskOutput {
    question: String,
    mode: String,
    answer: String,
    confidence: f64,
    citations: Vec<AskCitation>,
    evidence: Vec<AskEvidence>,
    explanations: Vec<AskExplanation>,
    gaps: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AskExplanation {
    id: String,
    reason: String,
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

#[derive(Default, Debug)]
struct SearchIndexSummary {
    indexed: usize,
    skipped: usize,
    deleted: usize,
}

#[derive(Debug, Clone)]
struct SearchCandidate {
    id: String,
    title: String,
    path: String,
    terms: Vec<String>,
    snippet: String,
    lexical_score: f64,
}

#[derive(Debug, Clone)]
struct SemanticCandidate {
    id: String,
    title: String,
    path: String,
    terms: Vec<String>,
    snippet: String,
    semantic_score: f64,
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
                run_ask(&args)?;
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

fn run_search(search: SearchCommand) -> Result<()> {
    match search.command {
        SearchSubcommand::Index(args) => run_search_index(args.rebuild),
        SearchSubcommand::Query(args) => run_search_query(&args),
        SearchSubcommand::Doctor => run_search_doctor(),
    }
}

fn run_search_index(rebuild: bool) -> Result<()> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        println!("search index: spec/ directory not found");
        return Ok(());
    }
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut conn = open_search_db()?;
    ensure_search_schema(&mut conn)?;
    let vec_available = ensure_sqlite_vec_ready(&conn)?;
    let tx = conn.transaction()?;

    if rebuild {
        tx.execute("DELETE FROM fts_chunks;", [])?;
        tx.execute("DELETE FROM chunks;", [])?;
        tx.execute("DELETE FROM chunk_vectors;", [])?;
        if vec_available {
            tx.execute("DELETE FROM vec_chunks;", [])?;
        }
        tx.execute("DELETE FROM nodes;", [])?;
    }

    let mut summary = SearchIndexSummary::default();
    let mut current_ids = HashSet::new();

    for (meta_path, meta) in metas {
        current_ids.insert(meta.id.clone());
        let existing_hash: Option<String> = tx
            .query_row(
                "SELECT hash FROM nodes WHERE id = ?1",
                params![meta.id],
                |row| row.get(0),
            )
            .optional()?;
        if !rebuild && existing_hash.as_deref() == Some(meta.hash.as_str()) {
            summary.skipped += 1;
            continue;
        }

        let body = fs::read_to_string(&meta.body_md_path)
            .with_context(|| format!("failed reading {}", meta.body_md_path))?;
        let chunks = split_into_chunks(&body, 800);
        let terms_json = serde_json::to_string(&meta.terms)?;
        let md_path = meta.body_md_path.clone();
        let now = unix_ts();

        tx.execute("DELETE FROM fts_chunks WHERE node_id = ?1", params![meta.id])?;
        tx.execute("DELETE FROM chunks WHERE node_id = ?1", params![meta.id])?;
        tx.execute("DELETE FROM chunk_vectors WHERE chunk_id LIKE ?1", params![format!("{}:%", meta.id)])?;
        if vec_available {
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id LIKE ?1", params![format!("{}:%", meta.id)])?;
        }
        tx.execute(
            "INSERT INTO nodes (id, title, md_path, meta_path, hash, terms_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, md_path=excluded.md_path, meta_path=excluded.meta_path, hash=excluded.hash, terms_json=excluded.terms_json, updated_at=excluded.updated_at",
            params![meta.id, meta.title, md_path, meta_path.to_string_lossy().to_string(), meta.hash, terms_json, now],
        )?;

        for (idx, chunk) in chunks.iter().enumerate() {
            let chunk_id = format!("{}:{idx}", meta.id);
            let token_len = tokenize(chunk).len() as i64;
            tx.execute(
                "INSERT INTO chunks (chunk_id, node_id, ord, text, token_len) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![chunk_id, meta.id, idx as i64, chunk, token_len],
            )?;
            tx.execute(
                "INSERT INTO fts_chunks (chunk_id, node_id, text) VALUES (?1, ?2, ?3)",
                params![format!("{}:{idx}", meta.id), meta.id, chunk],
            )?;
            let embedding = semantic_vector(chunk);
            tx.execute(
                "INSERT INTO chunk_vectors (chunk_id, model, dim, embedding) VALUES (?1, ?2, ?3, ?4)",
                params![
                    format!("{}:{idx}", meta.id),
                    "local-hash-ngrams-v1",
                    embedding.len() as i64,
                    vector_to_blob(&embedding)
                ],
            )?;
            if vec_available {
                tx.execute(
                    "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                    params![format!("{}:{idx}", meta.id), vector_to_json(&embedding)],
                )?;
            }
        }
        summary.indexed += 1;
    }

    let mut stale_ids = Vec::new();
    {
        let mut stmt = tx.prepare("SELECT id FROM nodes")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let id = row?;
            if !current_ids.contains(&id) {
                stale_ids.push(id);
            }
        }
    }
    for id in stale_ids {
        tx.execute("DELETE FROM fts_chunks WHERE node_id = ?1", params![id])?;
        tx.execute("DELETE FROM chunks WHERE node_id = ?1", params![id])?;
        tx.execute("DELETE FROM chunk_vectors WHERE chunk_id LIKE ?1", params![format!("{id}:%")])?;
        if vec_available {
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id LIKE ?1", params![format!("{id}:%")])?;
        }
        tx.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
        summary.deleted += 1;
    }

    tx.commit()?;
    println!(
        "search index summary: indexed={} skipped={} deleted={}",
        summary.indexed, summary.skipped, summary.deleted
    );
    Ok(())
}

fn run_search_query(args: &SearchQueryArgs) -> Result<()> {
    let conn = open_search_db()?;
    ensure_search_schema_readonly(&conn)?;
    let hits = build_search_hits(&conn, &args.query, args.top_k, args.mode)?;

    let mode = match args.mode {
        SearchMode::Lexical => "lexical",
        SearchMode::Hybrid => "hybrid",
    }
    .to_string();
    let output = SearchQueryOutput {
        query: args.query.clone(),
        mode,
        hits,
    };
    match args.format {
        SearchFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        SearchFormat::Table => print_search_table(&output),
    }
    Ok(())
}

fn run_ask(args: &AskArgs) -> Result<()> {
    let config = load_runtime_config();
    let conn = open_search_db()?;
    ensure_search_schema_readonly(&conn)?;
    let hits = build_search_hits(&conn, &args.question, args.top_k, args.mode)?;
    let spec_root = Path::new("spec");
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut meta_by_id = HashMap::<String, SpecNodeMeta>::new();
    for (_, meta) in metas {
        meta_by_id.insert(meta.id.clone(), meta);
    }
    let mode = match args.mode {
        SearchMode::Lexical => "lexical",
        SearchMode::Hybrid => "hybrid",
    }
    .to_string();
    let output = synthesize_ask_output(args, mode, hits, &meta_by_id, &config.ask);
    match args.format {
        AskFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        AskFormat::Table => print_ask_table(&output),
    }
    Ok(())
}

fn build_search_hits(
    conn: &Connection,
    query: &str,
    top_k: usize,
    mode: SearchMode,
) -> Result<Vec<SearchHit>> {
    let normalized = normalize_query_for_fts(query);
    if normalized.trim().is_empty() {
        anyhow::bail!("query is empty after normalization");
    }

    let lexical = collect_lexical_candidates(conn, query, top_k.max(1) * 8)?;
    let hits = match mode {
        SearchMode::Lexical => lexical
            .into_iter()
            .take(top_k)
            .map(|c| SearchHit {
                id: c.id,
                title: c.title,
                path: c.path,
                score: c.lexical_score,
                matched_terms: matched_terms(query, &c.terms),
                snippet: c.snippet,
            })
            .collect::<Vec<_>>(),
        SearchMode::Hybrid => {
            let semantic = collect_semantic_candidates(conn, query)?;
            merge_hybrid_results(query, lexical, semantic, top_k)
        }
    };
    Ok(hits)
}

fn collect_lexical_candidates(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchCandidate>> {
    let normalized = normalize_query_for_fts(query);
    let sql = "
        SELECT
            n.id,
            n.title,
            n.md_path,
            bm25(fts_chunks) AS bm25_score,
            SUBSTR(c.text, 1, 220) AS snippet,
            n.terms_json
        FROM fts_chunks
        JOIN chunks c ON c.chunk_id = fts_chunks.chunk_id
        JOIN nodes n ON n.id = fts_chunks.node_id
        WHERE fts_chunks MATCH ?1
        ORDER BY bm25_score ASC
        LIMIT ?2
    ";
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params![normalized, limit as i64])?;
    let mut by_node = HashMap::<String, SearchCandidate>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let path: String = row.get(2)?;
        let bm25_score: f64 = row.get(3)?;
        let snippet: String = row.get(4)?;
        let terms_json: String = row.get(5)?;
        let terms: Vec<String> = serde_json::from_str(&terms_json).unwrap_or_default();

        let lexical_base = -bm25_score;
        let boost = ranking_boost(query, &title, &terms);
        let score = lexical_base + boost;
        let candidate = SearchCandidate {
            id: id.clone(),
            title,
            path,
            terms,
            snippet: snippet.replace('\n', " "),
            lexical_score: score,
        };
        match by_node.get(&id) {
            Some(existing) if existing.lexical_score >= candidate.lexical_score => {}
            _ => {
                by_node.insert(id, candidate);
            }
        }
    }
    let mut out = by_node.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.lexical_score
            .total_cmp(&a.lexical_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

fn collect_semantic_candidates(conn: &Connection, query: &str) -> Result<Vec<SemanticCandidate>> {
    if sqlite_vec_available(conn) {
        if let Ok(from_vec) = collect_semantic_candidates_with_sqlite_vec(conn, query) {
            if !from_vec.is_empty() {
                return Ok(from_vec);
            }
        }
    }
    collect_semantic_candidates_from_local_store(conn, query)
}

fn collect_semantic_candidates_with_sqlite_vec(
    conn: &Connection,
    query: &str,
) -> Result<Vec<SemanticCandidate>> {
    let query_vec_json = vector_to_json(&semantic_vector(query));
    let mut stmt = conn.prepare(
        "
        SELECT
            n.id,
            n.title,
            n.md_path,
            n.terms_json,
            SUBSTR(c.text, 1, 220) AS snippet,
            vc.distance
        FROM vec_chunks vc
        JOIN chunks c ON c.chunk_id = vc.chunk_id
        JOIN nodes n ON n.id = c.node_id
        WHERE embedding MATCH ?1 AND k = ?2
        ",
    )?;
    let mut rows = stmt.query(params![query_vec_json, 60_i64])?;
    let mut by_node = HashMap::<String, SemanticCandidate>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let path: String = row.get(2)?;
        let terms_json: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let distance: f64 = row.get(5)?;
        let score = 1.0 / (1.0 + distance.max(0.0));
        if score < 0.2 {
            continue;
        }
        let terms: Vec<String> = serde_json::from_str(&terms_json).unwrap_or_default();
        let candidate = SemanticCandidate {
            id: id.clone(),
            title,
            path,
            terms,
            snippet: snippet.replace('\n', " "),
            semantic_score: score,
        };
        match by_node.get(&id) {
            Some(existing) if existing.semantic_score >= candidate.semantic_score => {}
            _ => {
                by_node.insert(id, candidate);
            }
        }
    }
    let mut out = by_node.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.semantic_score
            .total_cmp(&a.semantic_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

fn collect_semantic_candidates_from_local_store(
    conn: &Connection,
    query: &str,
) -> Result<Vec<SemanticCandidate>> {
    let query_vec = semantic_vector(query);
    let mut stmt = conn.prepare(
        "
        SELECT
            n.id,
            n.title,
            n.md_path,
            n.terms_json,
            SUBSTR(c.text, 1, 220) AS snippet,
            cv.embedding
        FROM chunk_vectors cv
        JOIN chunks c ON c.chunk_id = cv.chunk_id
        JOIN nodes n ON n.id = c.node_id
        WHERE cv.model = 'local-hash-ngrams-v1'
        ",
    )?;
    let mut rows = stmt.query([])?;
    let mut by_node = HashMap::<String, SemanticCandidate>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let path: String = row.get(2)?;
        let terms_json: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let embedding_blob: Vec<u8> = row.get(5)?;
        let chunk_vec = blob_to_vector(&embedding_blob)?;
        if chunk_vec.is_empty() {
            continue;
        }
        let score = cosine_similarity(&query_vec, &chunk_vec);
        if score < 0.2 {
            continue;
        }
        let terms: Vec<String> = serde_json::from_str(&terms_json).unwrap_or_default();
        let candidate = SemanticCandidate {
            id: id.clone(),
            title,
            path,
            terms,
            snippet: snippet.replace('\n', " "),
            semantic_score: score,
        };
        match by_node.get(&id) {
            Some(existing) if existing.semantic_score >= candidate.semantic_score => {}
            _ => {
                by_node.insert(id, candidate);
            }
        }
    }
    let mut out = by_node.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.semantic_score
            .total_cmp(&a.semantic_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

fn merge_hybrid_results(
    query: &str,
    lexical: Vec<SearchCandidate>,
    semantic: Vec<SemanticCandidate>,
    top_k: usize,
) -> Vec<SearchHit> {
    let mut lexical_rank = HashMap::<String, usize>::new();
    for (idx, c) in lexical.iter().enumerate() {
        lexical_rank.insert(c.id.clone(), idx + 1);
    }
    let mut semantic_rank = HashMap::<String, usize>::new();
    for (idx, c) in semantic.iter().enumerate() {
        semantic_rank.insert(c.id.clone(), idx + 1);
    }

    let mut merged = HashMap::<String, SearchHit>::new();
    for c in lexical {
        merged.entry(c.id.clone()).or_insert(SearchHit {
            id: c.id.clone(),
            title: c.title,
            path: c.path,
            score: 0.0,
            matched_terms: matched_terms(query, &c.terms),
            snippet: c.snippet,
        });
    }
    for c in semantic {
        merged.entry(c.id.clone()).or_insert(SearchHit {
            id: c.id.clone(),
            title: c.title,
            path: c.path,
            score: 0.0,
            matched_terms: matched_terms(query, &c.terms),
            snippet: c.snippet,
        });
    }

    for hit in merged.values_mut() {
        let l_rank = lexical_rank.get(&hit.id).copied().unwrap_or(10_000);
        let s_rank = semantic_rank.get(&hit.id).copied().unwrap_or(10_000);
        hit.score = reciprocal_rank_fusion(l_rank) + reciprocal_rank_fusion(s_rank);
    }

    let mut hits = merged.into_values().collect::<Vec<_>>();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    hits.truncate(top_k);
    hits
}

fn reciprocal_rank_fusion(rank: usize) -> f64 {
    if rank >= 10_000 {
        0.0
    } else {
        1.0 / (60.0 + rank as f64)
    }
}

fn run_search_doctor() -> Result<()> {
    let spec_root = Path::new("spec");
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut expected = HashMap::new();
    for (_, meta) in metas {
        expected.insert(meta.id, meta.hash);
    }

    let conn = open_search_db()?;
    ensure_search_schema_readonly(&conn)?;

    let mut issues = Vec::new();
    let mut stmt = conn.prepare("SELECT id, hash FROM nodes")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut indexed_ids = HashSet::new();
    for row in rows {
        let (id, hash) = row?;
        indexed_ids.insert(id.clone());
        match expected.get(&id) {
            Some(expected_hash) if expected_hash == &hash => {}
            Some(expected_hash) => issues.push(format!(
                "hash mismatch in index for {id}: indexed={hash} expected={expected_hash}"
            )),
            None => issues.push(format!("stale indexed node: {id}")),
        }
    }
    for id in expected.keys() {
        if !indexed_ids.contains(id) {
            issues.push(format!("missing indexed node: {id}"));
        }
    }

    let orphan_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks c LEFT JOIN nodes n ON n.id = c.node_id WHERE n.id IS NULL",
        [],
        |row| row.get(0),
    )?;
    if orphan_chunks > 0 {
        issues.push(format!("orphan chunks: {orphan_chunks}"));
    }

    if issues.is_empty() {
        println!("search doctor: ok");
    } else {
        for issue in &issues {
            println!("search doctor: issue: {issue}");
        }
        println!("search doctor summary: {} issue(s)", issues.len());
    }
    Ok(())
}

fn ensure_search_schema(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            md_path TEXT NOT NULL,
            meta_path TEXT NOT NULL,
            hash TEXT NOT NULL,
            terms_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL,
            ord INTEGER NOT NULL,
            text TEXT NOT NULL,
            token_len INTEGER NOT NULL,
            FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
            chunk_id UNINDEXED,
            node_id UNINDEXED,
            text,
            tokenize = 'unicode61'
        );
        CREATE TABLE IF NOT EXISTS chunk_vectors (
            chunk_id TEXT PRIMARY KEY,
            model TEXT NOT NULL,
            dim INTEGER NOT NULL,
            embedding BLOB
        );
        ",
    )?;
    Ok(())
}

fn ensure_sqlite_vec_ready(conn: &Connection) -> Result<bool> {
    if !sqlite_vec_available(conn) {
        let _ = try_load_sqlite_vec_extension(conn);
    }
    if !sqlite_vec_available(conn) {
        return Ok(false);
    }
    conn.execute_batch(
        &format!(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
                chunk_id TEXT,
                embedding FLOAT[{EMBEDDING_DIM}]
            );
            "
        ),
    )?;
    Ok(true)
}

fn sqlite_vec_available(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM pragma_module_list WHERE name = 'vec0'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

fn try_load_sqlite_vec_extension(conn: &Connection) -> Result<()> {
    let path = match std::env::var("FOUNDRY_SQLITE_VEC_PATH") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(()),
    };
    unsafe {
        conn.load_extension_enable()?;
        let load_result = conn.load_extension(Path::new(&path), None);
        conn.load_extension_disable()?;
        load_result?;
    }
    Ok(())
}

fn ensure_search_schema_readonly(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            md_path TEXT NOT NULL,
            meta_path TEXT NOT NULL,
            hash TEXT NOT NULL,
            terms_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL,
            ord INTEGER NOT NULL,
            text TEXT NOT NULL,
            token_len INTEGER NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
            chunk_id UNINDEXED,
            node_id UNINDEXED,
            text,
            tokenize = 'unicode61'
        );
        CREATE TABLE IF NOT EXISTS chunk_vectors (
            chunk_id TEXT PRIMARY KEY,
            model TEXT NOT NULL,
            dim INTEGER NOT NULL,
            embedding BLOB
        );
        ",
    )?;
    let _ = ensure_sqlite_vec_ready(conn);
    Ok(())
}

fn open_search_db() -> Result<Connection> {
    let db_path = search_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    Ok(conn)
}

fn search_db_path() -> PathBuf {
    PathBuf::from(".foundry/search/index.db")
}

fn split_into_chunks(text: &str, target_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let overlap = (target_len / 6).clamp(80, 180);

    for part in text.split("\n\n") {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if p.len() > target_len {
            if !current.is_empty() {
                out.push(current.trim().to_string());
                current.clear();
            }
            out.extend(split_long_text_with_overlap(p, target_len, overlap));
            continue;
        }
        if !current.is_empty() && current.len() + p.len() + 2 > target_len {
            out.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(p);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    if out.is_empty() {
        let fallback = text.trim();
        if fallback.is_empty() {
            vec![String::new()]
        } else {
            vec![fallback.to_string()]
        }
    } else {
        out
    }
}

fn normalize_query_for_fts(query: &str) -> String {
    query_terms_for_fts(query).join(" ")
}

fn query_terms_for_fts(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for part in query.split(|c: char| !c.is_alphanumeric()) {
        let token = part.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        if seen.insert(token.clone()) {
            out.push(token);
        }
    }
    out
}

fn split_long_text_with_overlap(text: &str, target_len: usize, overlap: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let sentences = split_sentences(text);
    let mut current = String::new();

    for sentence in sentences {
        if sentence.len() > target_len {
            if !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
                current.clear();
            }
            chunks.extend(split_by_char_window(&sentence, target_len, overlap));
            continue;
        }
        if !current.is_empty() && current.len() + sentence.len() + 1 > target_len {
            let finalized = current.trim().to_string();
            if !finalized.is_empty() {
                chunks.push(finalized.clone());
            }
            let carry = tail_overlap(&finalized, overlap);
            current.clear();
            if !carry.is_empty() {
                current.push_str(&carry);
                current.push(' ');
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&sentence);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    if chunks.is_empty() {
        vec![text.trim().to_string()]
    } else {
        chunks
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
            let s = current.trim();
            if !s.is_empty() {
                out.push(s.to_string());
            }
            current.clear();
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn split_by_char_window(text: &str, target_len: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= target_len {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    let step = target_len.saturating_sub(overlap).max(1);
    while start < chars.len() {
        let end = (start + target_len).min(chars.len());
        let slice = chars[start..end].iter().collect::<String>();
        let s = slice.trim();
        if !s.is_empty() {
            out.push(s.to_string());
        }
        if end == chars.len() {
            break;
        }
        start += step;
    }
    out
}

fn tail_overlap(text: &str, overlap: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= overlap {
        text.to_string()
    } else {
        chars[chars.len() - overlap..].iter().collect::<String>()
    }
}

fn matched_terms(query: &str, terms: &[String]) -> Vec<String> {
    let query_tokens = tokenize(query);
    terms
        .iter()
        .filter(|t| query_tokens.contains(&normalize_term_key(t)))
        .cloned()
        .collect()
}

fn ranking_boost(query: &str, title: &str, terms: &[String]) -> f64 {
    let q_tokens = tokenize(query);
    let title_tokens = tokenize(title);
    let q_norm_tokens = query
        .split_whitespace()
        .map(normalize_term_key)
        .filter(|s| !s.is_empty())
        .collect::<HashSet<_>>();
    let title_overlap = q_tokens.intersection(&title_tokens).count() as f64;
    let exact_phrase = title
        .to_ascii_lowercase()
        .contains(&query.to_ascii_lowercase()) as u8 as f64;
    let term_overlap = terms
        .iter()
        .filter(|t| {
            let n = normalize_term_key(t);
            q_tokens.contains(&n) || q_norm_tokens.contains(&n)
        })
        .count() as f64;
    (title_overlap * 3.0) + (term_overlap * 2.5) + (exact_phrase * 4.0)
}

fn semantic_vector(text: &str) -> Vec<f64> {
    let mut vec = vec![0.0_f64; EMBEDDING_DIM];
    let normalized = text.to_ascii_lowercase();

    for token in tokenize(&normalized) {
        let idx = stable_hash(token.as_bytes()) % EMBEDDING_DIM;
        vec[idx] += 2.0;
    }
    let compact: String = normalized
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
        .collect();
    let chars: Vec<char> = compact.chars().collect();
    for window in chars.windows(3) {
        let gram = window.iter().collect::<String>();
        let idx = stable_hash(gram.as_bytes()) % EMBEDDING_DIM;
        vec[idx] += 1.0;
    }

    let norm = vec.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}

fn vector_to_blob(vec: &[f64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vec.len() * std::mem::size_of::<f64>());
    for value in vec {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn vector_to_json(vec: &[f64]) -> String {
    let mut out = String::with_capacity(vec.len() * 8 + 2);
    out.push('[');
    for (idx, value) in vec.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&format!("{value:.8}"));
    }
    out.push(']');
    out
}

fn blob_to_vector(blob: &[u8]) -> Result<Vec<f64>> {
    if !blob.len().is_multiple_of(std::mem::size_of::<f64>()) {
        anyhow::bail!("invalid embedding blob length: {}", blob.len());
    }
    let mut out = Vec::with_capacity(blob.len() / std::mem::size_of::<f64>());
    for chunk in blob.chunks_exact(std::mem::size_of::<f64>()) {
        let mut buf = [0_u8; std::mem::size_of::<f64>()];
        buf.copy_from_slice(chunk);
        out.push(f64::from_le_bytes(buf));
    }
    Ok(out)
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    let mut dot = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
    }
    dot
}

fn stable_hash(bytes: &[u8]) -> usize {
    let mut hash = 1469598103934665603_u64;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    hash as usize
}

fn print_search_table(output: &SearchQueryOutput) {
    println!("query: {}", output.query);
    println!("mode: {}", output.mode);
    if output.hits.is_empty() {
        println!("hits: (none)");
        return;
    }
    println!("hits:");
    for hit in &output.hits {
        let terms = if hit.matched_terms.is_empty() {
            "-".to_string()
        } else {
            hit.matched_terms.join(",")
        };
        println!(
            "  - {} | {} | score={:.4} | terms={} | {}",
            hit.id, hit.path, hit.score, terms, hit.snippet
        );
    }
}

fn synthesize_ask_output(
    args: &AskArgs,
    mode: String,
    hits: Vec<SearchHit>,
    meta_by_id: &HashMap<String, SpecNodeMeta>,
    config: &AskRuntimeConfig,
) -> AskOutput {
    if hits.is_empty() {
        return AskOutput {
            question: args.question.clone(),
            mode,
            answer: "No relevant spec nodes were found for this question.".to_string(),
            confidence: 0.0,
            citations: Vec::new(),
            evidence: Vec::new(),
            explanations: Vec::new(),
            gaps: vec![
                "No matching spec nodes. Try a broader query or run `foundry spec search index --rebuild`."
                    .to_string(),
            ],
        };
    }

    let (related_ids, conflict_risks) = expand_ask_context(
        &hits,
        meta_by_id,
        config.neighbor_limit,
        &config.edge_weight,
    );
    let primary_ids = hits.iter().map(|h| h.id.clone()).collect::<HashSet<_>>();
    let mut citations = hits
        .iter()
        .map(|hit| AskCitation {
            id: hit.id.clone(),
            title: hit.title.clone(),
            path: hit.path.clone(),
        })
        .collect::<Vec<_>>();
    for related_id in &related_ids {
        if primary_ids.contains(related_id) {
            continue;
        }
        if let Some(meta) = meta_by_id.get(related_id) {
            citations.push(AskCitation {
                id: meta.id.clone(),
                title: meta.title.clone(),
                path: meta.body_md_path.clone(),
            });
        }
    }

    let evidence = hits
        .iter()
        .map(|hit| AskEvidence {
            id: hit.id.clone(),
            snippet: hit.snippet.clone(),
            score: hit.score,
        })
        .chain(related_ids.iter().filter_map(|id| {
            if primary_ids.contains(id) {
                return None;
            }
            meta_by_id.get(id).map(|meta| AskEvidence {
                id: meta.id.clone(),
                snippet: markdown_head_snippet(&meta.body_md_path, 220),
                score: 0.0,
            })
        }))
        .collect::<Vec<_>>();

    let focus_titles = citations
        .iter()
        .take(3)
        .map(|h| format!("{} ({})", h.title, h.id))
        .collect::<Vec<_>>()
        .join(", ");
    let top_score = hits.first().map(|h| h.score).unwrap_or(0.0);
    let confidence = confidence_from_hits(top_score, hits.len(), conflict_risks.is_empty());

    let related_summary = if related_ids.is_empty() {
        "No adjacent dependency/test nodes were found.".to_string()
    } else {
        format!(
            "Related context nodes: {}.",
            related_ids
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let risk_summary = if conflict_risks.is_empty() {
        "No direct conflict edges were detected in the 1-hop context.".to_string()
    } else {
        format!("Conflict risks to review: {}.", conflict_risks.join(", "))
    };
    let snippet_summary = evidence
        .iter()
        .take(config.snippet_count_in_answer.max(1))
        .map(|e| {
            let short = e.snippet.chars().take(100).collect::<String>();
            format!("[{}] {}", e.id, short)
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let answer = format!(
        "Primary relevant specs: {focus_titles}. {related_summary} {risk_summary} Evidence highlights: {snippet_summary}. Use `spec impact <ID>` on the first cited node for deeper propagation checks."
    );

    let mut gaps = Vec::new();
    if hits.len() < 2 {
        gaps.push("Low evidence count: fewer than 2 strong retrieval hits.".to_string());
    }
    if citations.len() <= 1 {
        gaps.push("Limited cross-spec context: consider adding more explicit links.".to_string());
    }

    let explanations = if args.explain {
        build_ask_explanations(
            &args.question,
            &hits,
            &related_ids,
            meta_by_id,
            &config.edge_weight,
        )
    } else {
        Vec::new()
    };

    AskOutput {
        question: args.question.clone(),
        mode,
        answer,
        confidence,
        citations,
        evidence,
        explanations,
        gaps,
    }
}

fn confidence_from_hits(top_score: f64, hit_count: usize, no_conflict_risk: bool) -> f64 {
    if hit_count == 0 {
        return 0.0;
    }
    let score_signal = if top_score <= 1.0 {
        (top_score.abs() * 30.0).min(1.0)
    } else {
        top_score.abs().min(1.0)
    };
    let coverage_signal = (hit_count as f64 / 5.0).min(1.0);
    let risk_signal = if no_conflict_risk { 1.0 } else { 0.6 };
    ((score_signal * 0.5) + (coverage_signal * 0.35) + (risk_signal * 0.15)).min(1.0)
}

fn print_ask_table(output: &AskOutput) {
    println!("question: {}", output.question);
    println!("mode: {}", output.mode);
    println!("confidence: {:.2}", output.confidence);
    println!("answer: {}", output.answer);
    println!("citations:");
    if output.citations.is_empty() {
        println!("  (none)");
    } else {
        for c in &output.citations {
            println!("  - {} | {} | {}", c.id, c.title, c.path);
        }
    }
    println!("evidence:");
    if output.evidence.is_empty() {
        println!("  (none)");
    } else {
        for e in &output.evidence {
            println!("  - {} | score={:.4} | {}", e.id, e.score, e.snippet);
        }
    }
    if !output.gaps.is_empty() {
        println!("gaps:");
        for gap in &output.gaps {
            println!("  - {gap}");
        }
    }
    if !output.explanations.is_empty() {
        println!("explanations:");
        for exp in &output.explanations {
            println!("  - {} | {}", exp.id, exp.reason);
        }
    }
}

fn build_ask_explanations(
    question: &str,
    hits: &[SearchHit],
    related_ids: &[String],
    meta_by_id: &HashMap<String, SpecNodeMeta>,
    weights: &AskEdgeWeightConfig,
) -> Vec<AskExplanation> {
    let mut out = Vec::new();
    let primary_ids = hits.iter().map(|h| h.id.clone()).collect::<HashSet<_>>();
    let query_tokens = query_terms_for_fts(question)
        .into_iter()
        .collect::<HashSet<_>>();

    for (idx, hit) in hits.iter().enumerate() {
        let mut parts = vec![format!("retrieval rank #{} (score={:.4})", idx + 1, hit.score)];
        if !hit.matched_terms.is_empty() {
            parts.push(format!("matched terms: {}", hit.matched_terms.join(",")));
        }
        let title_matches = token_matches_in_text(&query_tokens, &hit.title);
        let snippet_matches = token_matches_in_text(&query_tokens, &hit.snippet);
        if !title_matches.is_empty() {
            parts.push(format!("title token match: {}", title_matches.join(",")));
        }
        if !snippet_matches.is_empty() {
            parts.push(format!("snippet token match: {}", snippet_matches.join(",")));
        }
        out.push(AskExplanation {
            id: hit.id.clone(),
            reason: parts.join("; "),
        });
    }

    for related in related_ids {
        if primary_ids.contains(related) {
            continue;
        }
        let edge_reasons = edge_reasons_to_primary(related, &primary_ids, meta_by_id, weights);
        if edge_reasons.is_empty() {
            continue;
        }
        let weighted_total = edge_reasons
            .iter()
            .map(|e| e.weighted_contribution)
            .sum::<f64>();
        out.push(AskExplanation {
            id: related.clone(),
            reason: format!(
                "graph neighbor via {} (weighted_score={:.2})",
                edge_reasons
                    .iter()
                    .map(|e| e.label.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                weighted_total
            ),
        });
    }
    out
}

fn token_matches_in_text(query_tokens: &HashSet<String>, text: &str) -> Vec<String> {
    let text_tokens = query_terms_for_fts(text).into_iter().collect::<HashSet<_>>();
    let mut matches = query_tokens
        .intersection(&text_tokens)
        .cloned()
        .collect::<Vec<_>>();
    matches.sort();
    matches
}

fn edge_reasons_to_primary(
    candidate_id: &str,
    primary_ids: &HashSet<String>,
    meta_by_id: &HashMap<String, SpecNodeMeta>,
    weights: &AskEdgeWeightConfig,
) -> Vec<WeightedEdgeReason> {
    let mut reasons = HashMap::<String, WeightedEdgeReason>::new();
    if let Some(meta) = meta_by_id.get(candidate_id) {
        for edge in &meta.edges {
            if primary_ids.contains(&edge.to) {
                let weight = edge_weight(edge.edge_type.as_str(), weights);
                let label = format!(
                    "{} -> {} ({},w={:.2})",
                    candidate_id, edge.to, edge.edge_type, weight
                );
                reasons.entry(label.clone()).or_insert(WeightedEdgeReason {
                    label,
                    weighted_contribution: weight,
                });
            }
        }
    }
    for primary_id in primary_ids {
        if let Some(primary) = meta_by_id.get(primary_id) {
            for edge in &primary.edges {
                if edge.to == candidate_id {
                    let weight = edge_weight(edge.edge_type.as_str(), weights) * 0.9;
                    let label = format!(
                        "{} -> {} ({},w={:.2})",
                        primary_id, candidate_id, edge.edge_type, weight
                    );
                    reasons.entry(label.clone()).or_insert(WeightedEdgeReason {
                        label,
                        weighted_contribution: weight,
                    });
                }
            }
        }
    }
    let mut out = reasons.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.weighted_contribution
            .total_cmp(&a.weighted_contribution)
            .then(a.label.cmp(&b.label))
    });
    out
}

#[derive(Debug, Clone)]
struct WeightedEdgeReason {
    label: String,
    weighted_contribution: f64,
}

fn expand_ask_context(
    hits: &[SearchHit],
    meta_by_id: &HashMap<String, SpecNodeMeta>,
    limit: usize,
    weights: &AskEdgeWeightConfig,
) -> (Vec<String>, Vec<String>) {
    let seed_ids = hits.iter().map(|h| h.id.clone()).collect::<HashSet<_>>();
    let mut related_score = HashMap::<String, f64>::new();
    let mut conflicts = BTreeSet::new();

    for seed_id in &seed_ids {
        if let Some(meta) = meta_by_id.get(seed_id) {
            for edge in &meta.edges {
                match edge.edge_type.as_str() {
                    "depends_on" | "tests" | "refines" | "impacts" => {
                        *related_score.entry(edge.to.clone()).or_insert(0.0) +=
                            edge_weight(edge.edge_type.as_str(), weights);
                    }
                    "conflicts_with" => {
                        *related_score.entry(edge.to.clone()).or_insert(0.0) +=
                            edge_weight(edge.edge_type.as_str(), weights);
                        conflicts.insert(edge.to.clone());
                    }
                    _ => {}
                }
            }
        }
        for (id, candidate) in meta_by_id {
            for edge in &candidate.edges {
                if edge.to != *seed_id {
                    continue;
                }
                match edge.edge_type.as_str() {
                    "depends_on" | "tests" | "refines" | "impacts" => {
                        *related_score.entry(id.clone()).or_insert(0.0) +=
                            edge_weight(edge.edge_type.as_str(), weights) * 0.9;
                    }
                    "conflicts_with" => {
                        *related_score.entry(id.clone()).or_insert(0.0) +=
                            edge_weight(edge.edge_type.as_str(), weights) * 0.9;
                        conflicts.insert(id.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    for seed in &seed_ids {
        related_score.remove(seed);
        conflicts.remove(seed);
    }
    let mut related_ranked = related_score.into_iter().collect::<Vec<_>>();
    related_ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let related_vec = related_ranked
        .into_iter()
        .map(|(id, _)| id)
        .take(limit)
        .collect::<Vec<_>>();
    let conflict_vec = conflicts.into_iter().take(limit).collect::<Vec<_>>();
    (related_vec, conflict_vec)
}

fn edge_weight(edge_type: &str, w: &AskEdgeWeightConfig) -> f64 {
    match edge_type {
        "depends_on" => w.depends_on,
        "tests" => w.tests,
        "refines" => w.refines,
        "impacts" => w.impacts,
        "conflicts_with" => w.conflicts_with,
        _ => 0.0,
    }
}

fn markdown_head_snippet(path: &str, max_len: usize) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text
            .chars()
            .take(max_len)
            .collect::<String>()
            .replace('\n', " "),
        Err(_) => "(snippet unavailable)".to_string(),
    }
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

    #[test]
    fn normalize_term_key_collapses_style_variants() {
        assert_eq!(normalize_term_key("User_ID"), "userid");
        assert_eq!(normalize_term_key("user-id"), "userid");
        assert_eq!(normalize_term_key("User Id"), "userid");
    }

    #[test]
    fn validate_meta_semantics_rejects_invalid_fields() {
        let meta = SpecNodeMeta {
            id: "BAD-001".to_string(),
            node_type: "unknown_type".to_string(),
            status: "unknown_status".to_string(),
            title: "".to_string(),
            body_md_path: "docs/a.txt".to_string(),
            terms: vec![],
            hash: "not-a-hash".to_string(),
            edges: vec![],
        };
        let mut lint = LintState::default();
        validate_meta_semantics(Path::new("spec/a.meta.json"), &meta, &mut lint);
        assert!(lint.errors.iter().any(|e| e.contains("invalid node id format")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid node type")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid node status")));
        assert!(lint.errors.iter().any(|e| e.contains("empty title")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid body_md_path format")));
        assert!(lint.errors.iter().any(|e| e.contains("invalid hash format")));
    }

    #[test]
    fn score_to_confidence_is_bounded() {
        assert_eq!(score_to_confidence(0), 0.0);
        assert_eq!(score_to_confidence(2), 0.6);
        assert_eq!(score_to_confidence(20), 0.9);
    }

    #[test]
    fn split_into_chunks_splits_long_text() {
        let text = "Sentence one. Sentence two is long enough to force splitting. Sentence three keeps going with more words. Sentence four concludes.";
        let chunks = split_into_chunks(text, 40);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| !c.trim().is_empty()));
    }

    #[test]
    fn semantic_similarity_prefers_related_text() {
        let q = semantic_vector("authorization policy");
        let related = semantic_vector("authorization rules and policy for access");
        let unrelated = semantic_vector("invoice tax and payment details");
        assert!(cosine_similarity(&q, &related) > cosine_similarity(&q, &unrelated));
    }

    #[test]
    fn ranking_boost_favors_title_phrase_match() {
        let boost = ranking_boost("checkout flow", "Checkout Flow", &[]);
        let low = ranking_boost("checkout flow", "Payment module", &[]);
        assert!(boost > low);
    }

    #[test]
    fn vector_blob_roundtrip() {
        let vec = vec![0.1, -0.5, 1.25, 3.0];
        let blob = vector_to_blob(&vec);
        let decoded = blob_to_vector(&blob).expect("decode vector");
        assert_eq!(vec.len(), decoded.len());
        for (a, b) in vec.iter().zip(decoded.iter()) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn vector_json_shape() {
        let json = vector_to_json(&[0.1, 0.2, -0.3]);
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
        assert!(json.contains(','));
    }

    #[test]
    fn normalize_query_for_fts_removes_punctuation() {
        let normalized = normalize_query_for_fts("How does auth-flow work?");
        assert_eq!(normalized, "how does auth flow work");
    }

    #[test]
    fn expand_ask_context_collects_neighbors_and_conflicts() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            SpecNodeMeta {
                id: "SPC-001".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "A".to_string(),
                body_md_path: "spec/a.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![
                    SpecEdge {
                        to: "SPC-002".to_string(),
                        edge_type: "depends_on".to_string(),
                        rationale: "dep".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                    SpecEdge {
                        to: "SPC-003".to_string(),
                        edge_type: "conflicts_with".to_string(),
                        rationale: "risk".to_string(),
                        confidence: 1.0,
                        status: "confirmed".to_string(),
                    },
                ],
            },
        );
        map.insert(
            "SPC-004".to_string(),
            SpecNodeMeta {
                id: "SPC-004".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "B".to_string(),
                body_md_path: "spec/b.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![SpecEdge {
                    to: "SPC-001".to_string(),
                    edge_type: "tests".to_string(),
                    rationale: "test".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            },
        );
        let hits = vec![SearchHit {
            id: "SPC-001".to_string(),
            title: "A".to_string(),
            path: "spec/a.md".to_string(),
            score: 0.5,
            matched_terms: vec![],
            snippet: "x".to_string(),
        }];
        let (related, conflicts) =
            expand_ask_context(&hits, &map, 10, &AskEdgeWeightConfig::default());
        assert!(related.contains(&"SPC-002".to_string()));
        assert!(related.contains(&"SPC-003".to_string()));
        assert!(related.contains(&"SPC-004".to_string()));
        assert!(conflicts.contains(&"SPC-003".to_string()));
    }

    #[test]
    fn load_runtime_config_defaults_when_missing() {
        let cfg = load_runtime_config();
        assert!(cfg.ask.neighbor_limit >= 1);
        assert!(cfg.ask.snippet_count_in_answer >= 1);
    }

    #[test]
    fn build_ask_explanations_contains_graph_neighbor_reason() {
        let mut map = HashMap::new();
        map.insert(
            "SPC-001".to_string(),
            SpecNodeMeta {
                id: "SPC-001".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "Root".to_string(),
                body_md_path: "spec/root.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![SpecEdge {
                    to: "SPC-002".to_string(),
                    edge_type: "depends_on".to_string(),
                    rationale: "dep".to_string(),
                    confidence: 1.0,
                    status: "confirmed".to_string(),
                }],
            },
        );
        map.insert(
            "SPC-002".to_string(),
            SpecNodeMeta {
                id: "SPC-002".to_string(),
                node_type: "feature_requirement".to_string(),
                status: "active".to_string(),
                title: "Dep".to_string(),
                body_md_path: "spec/dep.md".to_string(),
                terms: vec![],
                hash: "0".repeat(64),
                edges: vec![],
            },
        );
        let hits = vec![SearchHit {
            id: "SPC-001".to_string(),
            title: "Root".to_string(),
            path: "spec/root.md".to_string(),
            score: 0.5,
            matched_terms: vec![],
            snippet: "root".to_string(),
        }];
        let exps = build_ask_explanations(
            "root dependency",
            &hits,
            &["SPC-002".to_string()],
            &map,
            &AskEdgeWeightConfig::default(),
        );
        assert!(exps.iter().any(|e| e.id == "SPC-002" && e.reason.contains("graph neighbor")));
        assert!(exps.iter().any(|e| e.id == "SPC-002" && e.reason.contains("w=")));
    }
}
