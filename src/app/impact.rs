use super::*;
use std::collections::VecDeque;

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

pub(super) fn run_impact(args: &ImpactArgs) -> Result<()> {
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

pub(super) fn bfs_review_order(
    seed: &str,
    max_depth: usize,
    by_id: &HashMap<String, SpecNodeMeta>,
) -> Vec<String> {
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
