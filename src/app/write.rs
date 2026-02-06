use super::*;

pub(super) fn run_write(args: &WriteArgs) -> Result<String> {
    if args.body.is_some() && args.body_file.is_some() {
        anyhow::bail!("--body and --body-file cannot be used together");
    }

    let md_path = PathBuf::from(&args.path);
    validate_markdown_path(&md_path)?;

    if let Some(parent) = md_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating directory: {}", parent.display()))?;
    }

    if let Some(body_file) = &args.body_file {
        let body = fs::read_to_string(body_file)
            .with_context(|| format!("failed reading --body-file: {body_file}"))?;
        fs::write(&md_path, body)
            .with_context(|| format!("failed writing markdown: {}", md_path.display()))?;
    } else if let Some(body) = &args.body {
        fs::write(&md_path, body)
            .with_context(|| format!("failed writing markdown: {}", md_path.display()))?;
    }

    if !md_path.exists() {
        anyhow::bail!(
            "markdown file not found: {} (use --body or --body-file to create it)",
            md_path.display()
        );
    }

    let body = fs::read_to_string(&md_path)
        .with_context(|| format!("failed reading markdown: {}", md_path.display()))?;
    let body_hash = sha256_hex(body.as_bytes());
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| extract_title(&body, &md_path));
    let body_md_path = normalize_path(&md_path).to_string_lossy().to_string();
    let meta_path = md_to_meta_path(&md_path)?;

    let mut existing_ids = load_existing_ids(Path::new("spec"))?;
    let next_id = next_available_id(&existing_ids);

    let mut created = false;
    let mut meta = if meta_path.exists() {
        let raw = fs::read_to_string(&meta_path)
            .with_context(|| format!("failed reading {}", meta_path.display()))?;
        serde_json::from_str::<SpecNodeMeta>(&raw)
            .with_context(|| format!("invalid .meta.json: {}", meta_path.display()))?
    } else {
        created = true;
        SpecNodeMeta {
            id: String::new(),
            node_type: "feature_requirement".to_string(),
            status: "draft".to_string(),
            title: String::new(),
            body_md_path: String::new(),
            terms: Vec::new(),
            hash: String::new(),
            edges: Vec::new(),
        }
    };

    if let Some(id) = &args.id {
        validate_node_id(id)?;
        if existing_ids.contains(id) && meta.id != *id {
            anyhow::bail!("id already exists: {id}");
        }
        meta.id = id.clone();
    } else if meta.id.trim().is_empty() {
        meta.id = format!("SPC-{next_id:03}");
    }
    existing_ids.insert(meta.id.clone());

    if let Some(node_type) = &args.node_type {
        if !NODE_TYPES.contains(&node_type.as_str()) {
            anyhow::bail!("invalid node type: {node_type}");
        }
        meta.node_type = node_type.clone();
    } else if meta.node_type.trim().is_empty() {
        meta.node_type = "feature_requirement".to_string();
    }

    if let Some(status) = &args.status {
        if !NODE_STATUSES.contains(&status.as_str()) {
            anyhow::bail!("invalid node status: {status}");
        }
        meta.status = status.clone();
    } else if meta.status.trim().is_empty() {
        meta.status = "draft".to_string();
    }

    meta.title = title;
    meta.body_md_path = body_md_path;
    meta.hash = body_hash;
    if !args.terms.is_empty() {
        meta.terms = args.terms.clone();
    }

    write_meta_json(&meta_path, &meta)?;
    let action = if created { "created" } else { "updated" };
    println!(
        "spec write: {} id={} md={} meta={}",
        action,
        meta.id,
        md_path.display(),
        meta_path.display()
    );
    Ok(meta.id)
}

fn validate_markdown_path(md_path: &Path) -> Result<()> {
    if md_path
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        != Some("spec")
    {
        anyhow::bail!("--path must be under spec/: {}", md_path.display());
    }
    if md_path.extension().and_then(|e| e.to_str()) != Some("md") {
        anyhow::bail!("--path must end with .md: {}", md_path.display());
    }
    Ok(())
}

fn validate_node_id(id: &str) -> Result<()> {
    if !is_valid_node_id(id) {
        anyhow::bail!("invalid id format: {id}");
    }
    Ok(())
}
