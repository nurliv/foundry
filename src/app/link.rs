use super::*;

pub(super) fn run_link(link: LinkCommand) -> Result<()> {
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

    let source_terms: HashSet<String> = source
        .terms
        .iter()
        .map(|t| normalize_term_key(t))
        .collect();
    let source_title_tokens = tokenize(&source.title);

    let mut candidates: Vec<(String, usize)> = by_id
        .iter()
        .filter(|(id, _)| id.as_str() != node_id)
        .map(|(id, (_, meta))| {
            let target_terms: HashSet<String> =
                meta.terms.iter().map(|t| normalize_term_key(t)).collect();
            let term_overlap = source_terms.intersection(&target_terms).count();
            let title_overlap = source_title_tokens.intersection(&tokenize(&meta.title)).count();
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
