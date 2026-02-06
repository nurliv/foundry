use super::*;

#[derive(Debug, Serialize)]
struct TaskSummary {
    id: String,
    title: String,
    path: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct BlockedTask {
    id: String,
    title: String,
    path: String,
    status: String,
    blocked_by: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PlanReadyOutput {
    ready: Vec<TaskSummary>,
    blocked: Vec<BlockedTask>,
}

#[derive(Debug, Serialize)]
struct PlanBatch {
    batch: usize,
    task_ids: Vec<String>,
    tasks: Vec<TaskSummary>,
}

#[derive(Debug, Serialize)]
struct PlanBatchesOutput {
    batches: Vec<PlanBatch>,
    blocked_or_cyclic: Vec<String>,
    blocked_or_cyclic_tasks: Vec<TaskSummary>,
}

pub(super) fn run_plan(plan: PlanCommand) -> Result<()> {
    match plan.command {
        PlanSubcommand::Ready(args) => run_plan_ready(args.format),
        PlanSubcommand::Batches(args) => run_plan_batches(args.format),
    }
}

fn run_plan_ready(format: PlanFormat) -> Result<()> {
    let by_id = load_meta_by_id()?;
    let mut ready = Vec::new();
    let mut blocked = Vec::new();

    for meta in by_id.values() {
        if !is_task_node(meta) || is_done_status(&meta.status) {
            continue;
        }
        let blockers = unresolved_task_dependencies(meta, &by_id);
        if blockers.is_empty() {
            ready.push(TaskSummary {
                id: meta.id.clone(),
                title: meta.title.clone(),
                path: meta.body_md_path.clone(),
                status: meta.status.clone(),
            });
        } else {
            blocked.push(BlockedTask {
                id: meta.id.clone(),
                title: meta.title.clone(),
                path: meta.body_md_path.clone(),
                status: meta.status.clone(),
                blocked_by: blockers,
            });
        }
    }

    ready.sort_by(|a, b| a.id.cmp(&b.id));
    blocked.sort_by(|a, b| a.id.cmp(&b.id));
    let output = PlanReadyOutput { ready, blocked };
    match format {
        PlanFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        PlanFormat::Table => print_plan_ready_table(&output),
    }
    Ok(())
}

fn run_plan_batches(format: PlanFormat) -> Result<()> {
    let by_id = load_meta_by_id()?;
    let pending_ids = by_id
        .values()
        .filter(|m| is_task_node(m) && !is_done_status(&m.status))
        .map(|m| m.id.clone())
        .collect::<HashSet<_>>();

    let mut indegree = HashMap::<String, usize>::new();
    let mut dependents = HashMap::<String, Vec<String>>::new();
    for id in &pending_ids {
        indegree.insert(id.clone(), 0);
    }

    for id in &pending_ids {
        let meta = by_id.get(id).expect("pending id exists");
        for edge in &meta.edges {
            if edge.edge_type != "depends_on" {
                continue;
            }
            if !pending_ids.contains(&edge.to) {
                continue;
            }
            *indegree.entry(id.clone()).or_default() += 1;
            dependents.entry(edge.to.clone()).or_default().push(id.clone());
        }
    }

    let mut batches = Vec::<PlanBatch>::new();
    let mut processed = HashSet::<String>::new();
    let mut batch_no = 1usize;
    loop {
        let mut current = indegree
            .iter()
            .filter(|(id, degree)| **degree == 0 && !processed.contains(*id))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        if current.is_empty() {
            break;
        }
        current.sort();
        for id in &current {
            processed.insert(id.clone());
            if let Some(ds) = dependents.get(id) {
                for dep in ds {
                    if let Some(v) = indegree.get_mut(dep) {
                        *v = v.saturating_sub(1);
                    }
                }
            }
        }
        batches.push(PlanBatch {
            batch: batch_no,
            task_ids: current.clone(),
            tasks: current
                .iter()
                .filter_map(|id| by_id.get(id))
                .map(|meta| TaskSummary {
                    id: meta.id.clone(),
                    title: meta.title.clone(),
                    path: meta.body_md_path.clone(),
                    status: meta.status.clone(),
                })
                .collect(),
        });
        batch_no += 1;
    }

    let mut blocked_or_cyclic = pending_ids
        .into_iter()
        .filter(|id| !processed.contains(id))
        .collect::<Vec<_>>();
    blocked_or_cyclic.sort();

    let blocked_or_cyclic_tasks = blocked_or_cyclic
        .iter()
        .filter_map(|id| by_id.get(id))
        .map(|meta| TaskSummary {
            id: meta.id.clone(),
            title: meta.title.clone(),
            path: meta.body_md_path.clone(),
            status: meta.status.clone(),
        })
        .collect::<Vec<_>>();

    let output = PlanBatchesOutput {
        batches,
        blocked_or_cyclic,
        blocked_or_cyclic_tasks,
    };
    match format {
        PlanFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        PlanFormat::Table => print_plan_batches_table(&output),
    }
    Ok(())
}

fn load_meta_by_id() -> Result<HashMap<String, SpecNodeMeta>> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        return Ok(HashMap::new());
    }
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut by_id = HashMap::<String, SpecNodeMeta>::new();
    for (_, meta) in metas {
        by_id.insert(meta.id.clone(), meta);
    }
    Ok(by_id)
}

fn unresolved_task_dependencies(
    meta: &SpecNodeMeta,
    by_id: &HashMap<String, SpecNodeMeta>,
) -> Vec<String> {
    let mut blocked_by = Vec::new();
    for edge in &meta.edges {
        if edge.edge_type != "depends_on" {
            continue;
        }
        let Some(dep) = by_id.get(&edge.to) else {
            continue;
        };
        if !is_task_node(dep) {
            continue;
        }
        if !is_done_status(&dep.status) {
            blocked_by.push(dep.id.clone());
        }
    }
    blocked_by.sort();
    blocked_by
}

fn is_task_node(meta: &SpecNodeMeta) -> bool {
    matches!(
        meta.node_type.as_str(),
        "implementation_task" | "test_task" | "migration_task"
    )
}

fn is_done_status(status: &str) -> bool {
    matches!(status, "done" | "archived" | "deprecated")
}

fn print_plan_ready_table(output: &PlanReadyOutput) {
    println!("ready_tasks:");
    if output.ready.is_empty() {
        println!("  (none)");
    } else {
        for task in &output.ready {
            println!(
                "  - {} [{}] {} ({})",
                task.id, task.status, task.title, task.path
            );
        }
    }
    println!("blocked_tasks:");
    if output.blocked.is_empty() {
        println!("  (none)");
    } else {
        for task in &output.blocked {
            println!(
                "  - {} [{}] blocked_by={} {} ({})",
                task.id,
                task.status,
                task.blocked_by.join(","),
                task.title,
                task.path
            );
        }
    }
}

fn print_plan_batches_table(output: &PlanBatchesOutput) {
    println!("parallel_batches:");
    if output.batches.is_empty() {
        println!("  (none)");
    } else {
        for batch in &output.batches {
            println!("  - batch {}: {}", batch.batch, batch.task_ids.join(", "));
        }
    }
    println!("blocked_or_cyclic:");
    if output.blocked_or_cyclic.is_empty() {
        println!("  (none)");
    } else {
        for task in &output.blocked_or_cyclic_tasks {
            println!("  - {} [{}] {} ({})", task.id, task.status, task.title, task.path);
        }
    }
}
