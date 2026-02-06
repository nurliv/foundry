use super::*;

pub(super) struct AskRetrieved {
    pub(super) mode: String,
    pub(super) hits: Vec<SearchHit>,
    pub(super) meta_by_id: HashMap<String, SpecNodeMeta>,
}

pub(super) fn retrieve_ask_inputs(args: &AskArgs) -> Result<AskRetrieved> {
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

    Ok(AskRetrieved {
        mode,
        hits,
        meta_by_id,
    })
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
