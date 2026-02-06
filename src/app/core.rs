use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn load_existing_ids(spec_root: &Path) -> Result<HashSet<String>> {
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

pub(super) fn load_all_meta(
    spec_root: &Path,
    lint: &mut LintState,
) -> Result<Vec<(PathBuf, SpecNodeMeta)>> {
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

pub(super) fn find_markdown_files(spec_root: &Path) -> Result<Vec<PathBuf>> {
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

pub(super) fn validate_meta_semantics(path: &Path, meta: &SpecNodeMeta, lint: &mut LintState) {
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

pub(super) fn is_valid_node_id(id: &str) -> bool {
    if let Some(num) = id.strip_prefix("SPC-") {
        return !num.is_empty() && num.chars().all(|c| c.is_ascii_digit());
    }
    false
}

pub(super) fn is_valid_sha256(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

pub(super) fn normalize_term_key(term: &str) -> String {
    term.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

pub(super) fn score_to_confidence(score: usize) -> f64 {
    match score {
        0 => 0.0,
        1 => 0.5,
        2 => 0.6,
        3 => 0.7,
        4 => 0.8,
        _ => 0.9,
    }
}

pub(super) fn is_meta_json(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.ends_with(".meta.json"))
}

pub(super) fn md_to_meta_path(md_path: &Path) -> Result<PathBuf> {
    let file_name = md_path
        .file_name()
        .and_then(|s| s.to_str())
        .with_context(|| format!("invalid markdown filename: {}", md_path.display()))?;
    let base = file_name
        .strip_suffix(".md")
        .with_context(|| format!("markdown file must end with .md: {}", md_path.display()))?;
    Ok(md_path.with_file_name(format!("{base}.meta.json")))
}

pub(super) fn write_meta_json(path: &Path, meta: &SpecNodeMeta) -> Result<()> {
    let text = serde_json::to_string_pretty(meta)?;
    fs::write(path, text + "\n")
        .with_context(|| format!("failed writing meta file: {}", path.display()))?;
    Ok(())
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

pub(super) fn extract_title(body: &str, path: &Path) -> String {
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

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

pub(super) fn next_available_id(existing: &HashSet<String>) -> usize {
    existing
        .iter()
        .filter_map(|id| id.strip_prefix("SPC-"))
        .filter_map(|v| v.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

pub(super) fn unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
