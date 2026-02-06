use super::*;

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
pub(super) struct AskExplanation {
    pub(super) id: String,
    pub(super) reason: String,
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

pub(super) fn run_ask(args: &AskArgs) -> Result<()> {
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

pub(super) fn build_ask_explanations(
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

pub(super) fn expand_ask_context(
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
