use super::*;

pub(super) fn run_derive(args: DeriveCommand) -> Result<()> {
    match args.command {
        DeriveSubcommand::Design(design) => run_derive_design(&design),
        DeriveSubcommand::Tasks(tasks) => run_derive_tasks(&tasks),
    }
}

#[derive(Debug, Serialize)]
struct DeriveEdgeOutput {
    from: String,
    to: String,
    #[serde(rename = "type")]
    edge_type: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct DerivedNodeOutput {
    id: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct DeriveDesignOutput {
    mode: &'static str,
    source: String,
    derived: DerivedNodeOutput,
    edges: Vec<DeriveEdgeOutput>,
}

#[derive(Debug, Serialize)]
struct DeriveTasksOutput {
    mode: &'static str,
    source: String,
    derived: Vec<DerivedNodeOutput>,
    edges: Vec<DeriveEdgeOutput>,
    chain: bool,
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
    let design_id = super::write::run_write_silent(&write_args)?;
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
    let design_md_path = design_meta.body_md_path.clone();
    write_meta_json(path, design_meta)?;
    let output = DeriveDesignOutput {
        mode: "design",
        source: args.from.clone(),
        derived: DerivedNodeOutput {
            id: design_id.clone(),
            path: design_md_path,
        },
        edges: vec![DeriveEdgeOutput {
            from: design_id.clone(),
            to: args.from.clone(),
            edge_type: "refines".to_string(),
            status: "confirmed".to_string(),
        }],
    };
    print_design_output(&output, args.format)?;
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
    if !args.items.is_empty()
        && (args.path.is_some() || args.title.is_some() || args.body.is_some() || args.body_file.is_some())
    {
        anyhow::bail!("--item mode cannot be combined with --path/--title/--body/--body-file");
    }

    let task_ids = if args.items.is_empty() {
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
        let task_id = write_task_node(args, path, title, body)?;
        vec![task_id]
    } else {
        let mut ids = Vec::new();
        for (i, item) in args.items.iter().enumerate() {
            let title = item.trim();
            if title.is_empty() {
                anyhow::bail!("--item cannot be empty");
            }
            let path = default_task_item_path(&args.from, i + 1, title);
            let body = default_task_body(from_meta, title);
            let task_id = write_task_node(args, path, title.to_string(), body)?;
            ids.push(task_id);
        }
        ids
    };

    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut by_id = to_meta_map(metas);
    for dep_id in &args.depends_on {
        if !by_id.contains_key(dep_id) {
            anyhow::bail!("depends-on target not found: {dep_id}");
        }
    }

    let mut edge_outputs = Vec::<DeriveEdgeOutput>::new();
    let mut derived_outputs = Vec::<DerivedNodeOutput>::new();
    for (idx, task_id) in task_ids.iter().enumerate() {
        if *task_id == args.from {
            anyhow::bail!("derived node id is identical to source id: {}", args.from);
        }
        let (path, task_meta) = by_id
            .get_mut(task_id)
            .with_context(|| format!("derived node not found after write: {task_id}"))?;
        upsert_refines_edge(task_meta, &args.from, &args.rationale);
        edge_outputs.push(DeriveEdgeOutput {
            from: task_id.clone(),
            to: args.from.clone(),
            edge_type: "refines".to_string(),
            status: "confirmed".to_string(),
        });
        for dep_id in &args.depends_on {
            upsert_edge(
                task_meta,
                dep_id,
                "depends_on",
                "task dependency",
                1.0,
                "confirmed",
            );
            edge_outputs.push(DeriveEdgeOutput {
                from: task_id.clone(),
                to: dep_id.clone(),
                edge_type: "depends_on".to_string(),
                status: "confirmed".to_string(),
            });
        }
        if args.chain && idx > 0 {
            upsert_edge(
                task_meta,
                &task_ids[idx - 1],
                "depends_on",
                "auto chain dependency",
                1.0,
                "confirmed",
            );
            edge_outputs.push(DeriveEdgeOutput {
                from: task_id.clone(),
                to: task_ids[idx - 1].clone(),
                edge_type: "depends_on".to_string(),
                status: "confirmed".to_string(),
            });
        }
        derived_outputs.push(DerivedNodeOutput {
            id: task_id.clone(),
            path: task_meta.body_md_path.clone(),
        });
        write_meta_json(path, task_meta)?;
    }
    let output = DeriveTasksOutput {
        mode: "tasks",
        source: args.from.clone(),
        derived: derived_outputs,
        edges: edge_outputs,
        chain: args.chain,
    };
    print_tasks_output(&output, args.format)?;
    Ok(())
}

fn write_task_node(args: &DeriveTasksArgs, path: String, title: String, body: String) -> Result<String> {
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
    super::write::run_write_silent(&write_args)
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
    format!("tasks/{}/task.md", from_id.to_ascii_lowercase())
}

fn default_task_item_path(from_id: &str, index: usize, title: &str) -> String {
    let slug = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "task".to_string() } else { slug };
    format!(
        "tasks/{}/{:02}-{}.md",
        from_id.to_ascii_lowercase(),
        index,
        slug
    )
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

fn print_design_output(output: &DeriveDesignOutput, format: DeriveFormat) -> Result<()> {
    match format {
        DeriveFormat::Json => println!("{}", serde_json::to_string_pretty(output)?),
        DeriveFormat::Table => println!(
            "spec derive design: source={} derived={} edge=refines",
            output.source, output.derived.id
        ),
    }
    Ok(())
}

fn print_tasks_output(output: &DeriveTasksOutput, format: DeriveFormat) -> Result<()> {
    match format {
        DeriveFormat::Json => println!("{}", serde_json::to_string_pretty(output)?),
        DeriveFormat::Table => println!(
            "spec derive tasks: source={} derived={} edges={} chain={}",
            output.source,
            output.derived.len(),
            output.edges.len(),
            output.chain
        ),
    }
    Ok(())
}
