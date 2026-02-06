use super::*;

#[derive(Debug, Serialize)]
struct LintOutput {
    ok: bool,
    error_count: usize,
    errors: Vec<String>,
}

pub(super) fn run_lint(args: &LintArgs) -> Result<i32> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        if args.format == LintFormat::Json {
            let output = LintOutput {
                ok: true,
                error_count: 0,
                errors: Vec::new(),
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("lint: spec/ directory not found");
        }
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

        let body = fs::read_to_string(&meta.body_md_path)
            .with_context(|| format!("failed reading markdown for lint: {}", meta.body_md_path))?;
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
                lint.errors
                    .push(format!("unknown edge target from {} to {}", meta.id, edge.to));
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
            if edge.edge_type == "conflicts_with"
                && edge.status == "confirmed"
                && let Some(target) = id_to_meta.get(&edge.to)
                && meta.status == "active"
                && target.status == "active"
            {
                lint.errors.push(format!(
                    "unresolved conflict: {} conflicts_with {}",
                    meta.id, target.id
                ));
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
        if args.format == LintFormat::Json {
            let output = LintOutput {
                ok: true,
                error_count: 0,
                errors: Vec::new(),
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("lint: ok");
        }
        return Ok(0);
    }

    if args.format == LintFormat::Json {
        let output = LintOutput {
            ok: false,
            error_count: lint.errors.len(),
            errors: lint.errors,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        for err in &lint.errors {
            println!("lint: error: {err}");
        }
        println!("lint summary: {} error(s)", lint.errors.len());
    }
    Ok(1)
}
