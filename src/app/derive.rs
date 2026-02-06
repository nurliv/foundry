use super::*;

pub(super) fn run_derive(args: DeriveCommand) -> Result<()> {
    match args.command {
        DeriveSubcommand::Design(design) => run_derive_design(&design),
        DeriveSubcommand::Tasks(tasks) => run_derive_tasks(&tasks),
    }
}

fn run_derive_design(args: &DeriveDesignArgs) -> Result<()> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        anyhow::bail!("spec/ directory not found");
    }
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let by_id: HashMap<String, SpecNodeMeta> = metas.into_iter().map(|(_, m)| (m.id.clone(), m)).collect();
    let from_meta = by_id
        .get(&args.from)
        .with_context(|| format!("source node not found: {}", args.from))?;

    if args.body.is_some() && args.body_file.is_some() {
        anyhow::bail!("--body and --body-file cannot be used together");
    }

    let path = args
        .path
        .clone()
        .unwrap_or_else(|| default_design_path(&args.from));
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| format!("Design for {}", from_meta.title));
    let body = match (&args.body, &args.body_file) {
        (Some(body), _) => body.clone(),
        (None, Some(body_file)) => fs::read_to_string(body_file)
            .with_context(|| format!("failed reading --body-file: {body_file}"))?,
        (None, None) => default_design_body(from_meta, &title),
    };

    let write_args = WriteArgs {
        path: Some(path),
        id: None,
        node_type: Some(args.node_type.clone()),
        status: Some(args.status.clone()),
        title: Some(title),
        body: Some(body),
        body_file: None,
        terms: args.terms.clone(),
    };
    let design_id = super::write::run_write(&write_args)?;
    if design_id == args.from {
        anyhow::bail!("derived node id is identical to source id: {}", args.from);
    }

    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut by_id = to_meta_map(metas);
    let (path, design_meta) = by_id
        .get_mut(&design_id)
        .with_context(|| format!("derived node not found after write: {design_id}"))?;
    upsert_refines_edge(design_meta, &args.from, &args.rationale);
    write_meta_json(path, design_meta)?;
    println!(
        "spec derive design: source={} derived={} edge=refines",
        args.from, design_id
    );
    Ok(())
}

fn run_derive_tasks(args: &DeriveTasksArgs) -> Result<()> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        anyhow::bail!("spec/ directory not found");
    }
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let by_id: HashMap<String, SpecNodeMeta> =
        metas.into_iter().map(|(_, m)| (m.id.clone(), m)).collect();
    let from_meta = by_id
        .get(&args.from)
        .with_context(|| format!("source node not found: {}", args.from))?;

    if args.body.is_some() && args.body_file.is_some() {
        anyhow::bail!("--body and --body-file cannot be used together");
    }

    let path = args
        .path
        .clone()
        .unwrap_or_else(|| default_task_path(&args.from));
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| format!("Task for {}", from_meta.title));
    let body = match (&args.body, &args.body_file) {
        (Some(body), _) => body.clone(),
        (None, Some(body_file)) => fs::read_to_string(body_file)
            .with_context(|| format!("failed reading --body-file: {body_file}"))?,
        (None, None) => default_task_body(from_meta, &title),
    };

    let write_args = WriteArgs {
        path: Some(path),
        id: None,
        node_type: Some(args.node_type.clone()),
        status: Some(args.status.clone()),
        title: Some(title),
        body: Some(body),
        body_file: None,
        terms: args.terms.clone(),
    };
    let task_id = super::write::run_write(&write_args)?;
    if task_id == args.from {
        anyhow::bail!("derived node id is identical to source id: {}", args.from);
    }

    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut by_id = to_meta_map(metas);
    for dep_id in &args.depends_on {
        if !by_id.contains_key(dep_id) {
            anyhow::bail!("depends-on target not found: {dep_id}");
        }
    }
    let (path, task_meta) = by_id
        .get_mut(&task_id)
        .with_context(|| format!("derived node not found after write: {task_id}"))?;
    upsert_refines_edge(task_meta, &args.from, &args.rationale);
    for dep_id in &args.depends_on {
        upsert_edge(
            task_meta,
            dep_id,
            "depends_on",
            "task dependency",
            1.0,
            "confirmed",
        );
    }
    write_meta_json(path, task_meta)?;
    println!(
        "spec derive tasks: source={} derived={} edge=refines deps={}",
        args.from,
        task_id,
        args.depends_on.len()
    );
    Ok(())
}

fn upsert_refines_edge(meta: &mut SpecNodeMeta, to: &str, rationale: &str) {
    upsert_edge(meta, to, "refines", rationale, 1.0, "confirmed");
}

fn upsert_edge(
    meta: &mut SpecNodeMeta,
    to: &str,
    edge_type: &str,
    rationale: &str,
    confidence: f64,
    status: &str,
) {
    if let Some(edge) = meta
        .edges
        .iter_mut()
        .find(|e| e.to == to && e.edge_type == edge_type)
    {
        edge.rationale = rationale.to_string();
        edge.confidence = confidence;
        edge.status = status.to_string();
        return;
    }
    meta.edges.push(SpecEdge {
        to: to.to_string(),
        edge_type: edge_type.to_string(),
        rationale: rationale.to_string(),
        confidence,
        status: status.to_string(),
    });
}

fn default_design_path(from_id: &str) -> String {
    format!("spec/design-{}.md", from_id.to_ascii_lowercase())
}

fn default_design_body(source: &SpecNodeMeta, title: &str) -> String {
    format!(
        "# {title}\n\n## Objective\n- Derived from: {}\n\n## Architecture\n- Components:\n- Data flow:\n- Integration points:\n\n## Decisions\n- Decision:\n- Rationale:\n- Trade-offs:\n\n## Validation\n- Test strategy:\n- Observability:\n",
        source.id
    )
}

fn default_task_path(from_id: &str) -> String {
    format!("spec/task-{}.md", from_id.to_ascii_lowercase())
}

fn default_task_body(source: &SpecNodeMeta, title: &str) -> String {
    format!(
        "# {title}\n\n## Goal\n- Derived from: {}\n\n## Implementation Steps\n1.\n2.\n3.\n\n## Definition of Done\n- Code complete:\n- Test complete:\n- Docs updated:\n\n## Verification\n- Commands:\n- Expected results:\n",
        source.id
    )
}

fn to_meta_map(metas: Vec<(PathBuf, SpecNodeMeta)>) -> HashMap<String, (PathBuf, SpecNodeMeta)> {
    let mut map = HashMap::<String, (PathBuf, SpecNodeMeta)>::new();
    for (path, meta) in metas {
        map.insert(meta.id.clone(), (path, meta));
    }
    map
}
