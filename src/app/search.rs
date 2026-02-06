use super::*;

#[derive(Debug, Serialize)]
pub(super) struct SearchHit {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) path: String,
    pub(super) score: f64,
    pub(super) matched_terms: Vec<String>,
    pub(super) snippet: String,
}

#[derive(Debug, Serialize)]
struct SearchQueryOutput {
    query: String,
    mode: String,
    hits: Vec<SearchHit>,
}

#[derive(Default, Debug)]
struct SearchIndexSummary {
    indexed: usize,
    skipped: usize,
    deleted: usize,
}

#[derive(Debug, Clone)]
struct SearchCandidate {
    id: String,
    title: String,
    path: String,
    terms: Vec<String>,
    snippet: String,
    lexical_score: f64,
}

#[derive(Debug, Clone)]
struct SemanticCandidate {
    id: String,
    title: String,
    path: String,
    terms: Vec<String>,
    snippet: String,
    semantic_score: f64,
}

pub(super) fn run_search(search: SearchCommand) -> Result<()> {
    match search.command {
        SearchSubcommand::Index(args) => run_search_index(args.rebuild),
        SearchSubcommand::Query(args) => run_search_query(&args),
        SearchSubcommand::Doctor => run_search_doctor(),
    }
}

pub(super) fn run_search_index(rebuild: bool) -> Result<()> {
    let spec_root = Path::new("spec");
    if !spec_root.exists() {
        println!("search index: spec/ directory not found");
        return Ok(());
    }
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut conn = open_search_db()?;
    ensure_search_schema(&mut conn)?;
    let vec_available = ensure_sqlite_vec_ready(&conn)?;
    let tx = conn.transaction()?;

    if rebuild {
        tx.execute("DELETE FROM fts_chunks;", [])?;
        tx.execute("DELETE FROM chunks;", [])?;
        tx.execute("DELETE FROM chunk_vectors;", [])?;
        if vec_available {
            tx.execute("DELETE FROM vec_chunks;", [])?;
        }
        tx.execute("DELETE FROM nodes;", [])?;
    }

    let mut summary = SearchIndexSummary::default();
    let mut current_ids = HashSet::new();

    for (meta_path, meta) in metas {
        current_ids.insert(meta.id.clone());
        let existing_hash: Option<String> = tx
            .query_row(
                "SELECT hash FROM nodes WHERE id = ?1",
                params![meta.id],
                |row| row.get(0),
            )
            .optional()?;
        if !rebuild && existing_hash.as_deref() == Some(meta.hash.as_str()) {
            summary.skipped += 1;
            continue;
        }

        let body = fs::read_to_string(&meta.body_md_path)
            .with_context(|| format!("failed reading {}", meta.body_md_path))?;
        let chunks = split_into_chunks(&body, 800);
        let terms_json = serde_json::to_string(&meta.terms)?;
        let md_path = meta.body_md_path.clone();
        let now = unix_ts();

        tx.execute("DELETE FROM fts_chunks WHERE node_id = ?1", params![meta.id])?;
        tx.execute("DELETE FROM chunks WHERE node_id = ?1", params![meta.id])?;
        tx.execute("DELETE FROM chunk_vectors WHERE chunk_id LIKE ?1", params![format!("{}:%", meta.id)])?;
        if vec_available {
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id LIKE ?1", params![format!("{}:%", meta.id)])?;
        }
        tx.execute(
            "INSERT INTO nodes (id, title, md_path, meta_path, hash, terms_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, md_path=excluded.md_path, meta_path=excluded.meta_path, hash=excluded.hash, terms_json=excluded.terms_json, updated_at=excluded.updated_at",
            params![meta.id, meta.title, md_path, meta_path.to_string_lossy().to_string(), meta.hash, terms_json, now],
        )?;

        for (idx, chunk) in chunks.iter().enumerate() {
            let chunk_id = format!("{}:{idx}", meta.id);
            let token_len = tokenize(chunk).len() as i64;
            tx.execute(
                "INSERT INTO chunks (chunk_id, node_id, ord, text, token_len) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![chunk_id, meta.id, idx as i64, chunk, token_len],
            )?;
            tx.execute(
                "INSERT INTO fts_chunks (chunk_id, node_id, text) VALUES (?1, ?2, ?3)",
                params![format!("{}:{idx}", meta.id), meta.id, chunk],
            )?;
            let embedding = semantic_vector(chunk);
            tx.execute(
                "INSERT INTO chunk_vectors (chunk_id, model, dim, embedding) VALUES (?1, ?2, ?3, ?4)",
                params![
                    format!("{}:{idx}", meta.id),
                    "local-hash-ngrams-v1",
                    embedding.len() as i64,
                    vector_to_blob(&embedding)
                ],
            )?;
            if vec_available {
                tx.execute(
                    "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                    params![format!("{}:{idx}", meta.id), vector_to_json(&embedding)],
                )?;
            }
        }
        summary.indexed += 1;
    }

    let mut stale_ids = Vec::new();
    {
        let mut stmt = tx.prepare("SELECT id FROM nodes")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let id = row?;
            if !current_ids.contains(&id) {
                stale_ids.push(id);
            }
        }
    }
    for id in stale_ids {
        tx.execute("DELETE FROM fts_chunks WHERE node_id = ?1", params![id])?;
        tx.execute("DELETE FROM chunks WHERE node_id = ?1", params![id])?;
        tx.execute("DELETE FROM chunk_vectors WHERE chunk_id LIKE ?1", params![format!("{id}:%")])?;
        if vec_available {
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id LIKE ?1", params![format!("{id}:%")])?;
        }
        tx.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
        summary.deleted += 1;
    }

    tx.commit()?;
    println!(
        "search index summary: indexed={} skipped={} deleted={}",
        summary.indexed, summary.skipped, summary.deleted
    );
    Ok(())
}

pub(super) fn run_search_query(args: &SearchQueryArgs) -> Result<()> {
    let conn = open_search_db()?;
    ensure_search_schema_readonly(&conn)?;
    let hits = build_search_hits(&conn, &args.query, args.top_k, args.mode)?;

    let mode = match args.mode {
        SearchMode::Lexical => "lexical",
        SearchMode::Hybrid => "hybrid",
    }
    .to_string();
    let output = SearchQueryOutput {
        query: args.query.clone(),
        mode,
        hits,
    };
    match args.format {
        SearchFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        SearchFormat::Table => print_search_table(&output),
    }
    Ok(())
}

pub(super) fn build_search_hits(
    conn: &Connection,
    query: &str,
    top_k: usize,
    mode: SearchMode,
) -> Result<Vec<SearchHit>> {
    let normalized = normalize_query_for_fts(query);
    if normalized.trim().is_empty() {
        anyhow::bail!("query is empty after normalization");
    }

    let lexical = collect_lexical_candidates(conn, query, top_k.max(1) * 8)?;
    let hits = match mode {
        SearchMode::Lexical => lexical
            .into_iter()
            .take(top_k)
            .map(|c| SearchHit {
                id: c.id,
                title: c.title,
                path: c.path,
                score: c.lexical_score,
                matched_terms: matched_terms(query, &c.terms),
                snippet: c.snippet,
            })
            .collect::<Vec<_>>(),
        SearchMode::Hybrid => {
            let semantic = collect_semantic_candidates(conn, query)?;
            merge_hybrid_results(query, lexical, semantic, top_k)
        }
    };
    Ok(hits)
}

fn collect_lexical_candidates(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchCandidate>> {
    let normalized = normalize_query_for_fts(query);
    let sql = "
        SELECT
            n.id,
            n.title,
            n.md_path,
            bm25(fts_chunks) AS bm25_score,
            SUBSTR(c.text, 1, 220) AS snippet,
            n.terms_json
        FROM fts_chunks
        JOIN chunks c ON c.chunk_id = fts_chunks.chunk_id
        JOIN nodes n ON n.id = fts_chunks.node_id
        WHERE fts_chunks MATCH ?1
        ORDER BY bm25_score ASC
        LIMIT ?2
    ";
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params![normalized, limit as i64])?;
    let mut by_node = HashMap::<String, SearchCandidate>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let path: String = row.get(2)?;
        let bm25_score: f64 = row.get(3)?;
        let snippet: String = row.get(4)?;
        let terms_json: String = row.get(5)?;
        let terms: Vec<String> = serde_json::from_str(&terms_json).unwrap_or_default();

        let lexical_base = -bm25_score;
        let boost = ranking_boost(query, &title, &terms);
        let score = lexical_base + boost;
        let candidate = SearchCandidate {
            id: id.clone(),
            title,
            path,
            terms,
            snippet: snippet.replace('\n', " "),
            lexical_score: score,
        };
        match by_node.get(&id) {
            Some(existing) if existing.lexical_score >= candidate.lexical_score => {}
            _ => {
                by_node.insert(id, candidate);
            }
        }
    }
    let mut out = by_node.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.lexical_score
            .total_cmp(&a.lexical_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

fn collect_semantic_candidates(conn: &Connection, query: &str) -> Result<Vec<SemanticCandidate>> {
    if sqlite_vec_available(conn) {
        if let Ok(from_vec) = collect_semantic_candidates_with_sqlite_vec(conn, query) {
            if !from_vec.is_empty() {
                return Ok(from_vec);
            }
        }
    }
    collect_semantic_candidates_from_local_store(conn, query)
}

fn collect_semantic_candidates_with_sqlite_vec(
    conn: &Connection,
    query: &str,
) -> Result<Vec<SemanticCandidate>> {
    let query_vec_json = vector_to_json(&semantic_vector(query));
    let mut stmt = conn.prepare(
        "
        SELECT
            n.id,
            n.title,
            n.md_path,
            n.terms_json,
            SUBSTR(c.text, 1, 220) AS snippet,
            vc.distance
        FROM vec_chunks vc
        JOIN chunks c ON c.chunk_id = vc.chunk_id
        JOIN nodes n ON n.id = c.node_id
        WHERE embedding MATCH ?1 AND k = ?2
        ",
    )?;
    let mut rows = stmt.query(params![query_vec_json, 60_i64])?;
    let mut by_node = HashMap::<String, SemanticCandidate>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let path: String = row.get(2)?;
        let terms_json: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let distance: f64 = row.get(5)?;
        let score = 1.0 / (1.0 + distance.max(0.0));
        if score < 0.2 {
            continue;
        }
        let terms: Vec<String> = serde_json::from_str(&terms_json).unwrap_or_default();
        let candidate = SemanticCandidate {
            id: id.clone(),
            title,
            path,
            terms,
            snippet: snippet.replace('\n', " "),
            semantic_score: score,
        };
        match by_node.get(&id) {
            Some(existing) if existing.semantic_score >= candidate.semantic_score => {}
            _ => {
                by_node.insert(id, candidate);
            }
        }
    }
    let mut out = by_node.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.semantic_score
            .total_cmp(&a.semantic_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

fn collect_semantic_candidates_from_local_store(
    conn: &Connection,
    query: &str,
) -> Result<Vec<SemanticCandidate>> {
    let query_vec = semantic_vector(query);
    let mut stmt = conn.prepare(
        "
        SELECT
            n.id,
            n.title,
            n.md_path,
            n.terms_json,
            SUBSTR(c.text, 1, 220) AS snippet,
            cv.embedding
        FROM chunk_vectors cv
        JOIN chunks c ON c.chunk_id = cv.chunk_id
        JOIN nodes n ON n.id = c.node_id
        WHERE cv.model = 'local-hash-ngrams-v1'
        ",
    )?;
    let mut rows = stmt.query([])?;
    let mut by_node = HashMap::<String, SemanticCandidate>::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let path: String = row.get(2)?;
        let terms_json: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let embedding_blob: Vec<u8> = row.get(5)?;
        let chunk_vec = blob_to_vector(&embedding_blob)?;
        if chunk_vec.is_empty() {
            continue;
        }
        let score = cosine_similarity(&query_vec, &chunk_vec);
        if score < 0.2 {
            continue;
        }
        let terms: Vec<String> = serde_json::from_str(&terms_json).unwrap_or_default();
        let candidate = SemanticCandidate {
            id: id.clone(),
            title,
            path,
            terms,
            snippet: snippet.replace('\n', " "),
            semantic_score: score,
        };
        match by_node.get(&id) {
            Some(existing) if existing.semantic_score >= candidate.semantic_score => {}
            _ => {
                by_node.insert(id, candidate);
            }
        }
    }
    let mut out = by_node.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.semantic_score
            .total_cmp(&a.semantic_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

fn merge_hybrid_results(
    query: &str,
    lexical: Vec<SearchCandidate>,
    semantic: Vec<SemanticCandidate>,
    top_k: usize,
) -> Vec<SearchHit> {
    let mut lexical_rank = HashMap::<String, usize>::new();
    for (idx, c) in lexical.iter().enumerate() {
        lexical_rank.insert(c.id.clone(), idx + 1);
    }
    let mut semantic_rank = HashMap::<String, usize>::new();
    for (idx, c) in semantic.iter().enumerate() {
        semantic_rank.insert(c.id.clone(), idx + 1);
    }

    let mut merged = HashMap::<String, SearchHit>::new();
    for c in lexical {
        merged.entry(c.id.clone()).or_insert(SearchHit {
            id: c.id.clone(),
            title: c.title,
            path: c.path,
            score: 0.0,
            matched_terms: matched_terms(query, &c.terms),
            snippet: c.snippet,
        });
    }
    for c in semantic {
        merged.entry(c.id.clone()).or_insert(SearchHit {
            id: c.id.clone(),
            title: c.title,
            path: c.path,
            score: 0.0,
            matched_terms: matched_terms(query, &c.terms),
            snippet: c.snippet,
        });
    }

    for hit in merged.values_mut() {
        let l_rank = lexical_rank.get(&hit.id).copied().unwrap_or(10_000);
        let s_rank = semantic_rank.get(&hit.id).copied().unwrap_or(10_000);
        hit.score = reciprocal_rank_fusion(l_rank) + reciprocal_rank_fusion(s_rank);
    }

    let mut hits = merged.into_values().collect::<Vec<_>>();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    hits.truncate(top_k);
    hits
}

pub(super) fn reciprocal_rank_fusion(rank: usize) -> f64 {
    if rank >= 10_000 {
        0.0
    } else {
        1.0 / (60.0 + rank as f64)
    }
}

pub(super) fn run_search_doctor() -> Result<()> {
    let spec_root = Path::new("spec");
    let mut lint = LintState::default();
    let metas = load_all_meta(spec_root, &mut lint)?;
    let mut expected = HashMap::new();
    for (_, meta) in metas {
        expected.insert(meta.id, meta.hash);
    }

    let conn = open_search_db()?;
    ensure_search_schema_readonly(&conn)?;

    let mut issues = Vec::new();
    let mut stmt = conn.prepare("SELECT id, hash FROM nodes")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut indexed_ids = HashSet::new();
    for row in rows {
        let (id, hash) = row?;
        indexed_ids.insert(id.clone());
        match expected.get(&id) {
            Some(expected_hash) if expected_hash == &hash => {}
            Some(expected_hash) => issues.push(format!(
                "hash mismatch in index for {id}: indexed={hash} expected={expected_hash}"
            )),
            None => issues.push(format!("stale indexed node: {id}")),
        }
    }
    for id in expected.keys() {
        if !indexed_ids.contains(id) {
            issues.push(format!("missing indexed node: {id}"));
        }
    }

    let orphan_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks c LEFT JOIN nodes n ON n.id = c.node_id WHERE n.id IS NULL",
        [],
        |row| row.get(0),
    )?;
    if orphan_chunks > 0 {
        issues.push(format!("orphan chunks: {orphan_chunks}"));
    }

    if issues.is_empty() {
        println!("search doctor: ok");
    } else {
        for issue in &issues {
            println!("search doctor: issue: {issue}");
        }
        println!("search doctor summary: {} issue(s)", issues.len());
    }
    Ok(())
}

pub(super) fn ensure_search_schema(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            md_path TEXT NOT NULL,
            meta_path TEXT NOT NULL,
            hash TEXT NOT NULL,
            terms_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL,
            ord INTEGER NOT NULL,
            text TEXT NOT NULL,
            token_len INTEGER NOT NULL,
            FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
            chunk_id UNINDEXED,
            node_id UNINDEXED,
            text,
            tokenize = 'unicode61'
        );
        CREATE TABLE IF NOT EXISTS chunk_vectors (
            chunk_id TEXT PRIMARY KEY,
            model TEXT NOT NULL,
            dim INTEGER NOT NULL,
            embedding BLOB
        );
        ",
    )?;
    Ok(())
}

pub(super) fn ensure_sqlite_vec_ready(conn: &Connection) -> Result<bool> {
    if !sqlite_vec_available(conn) {
        let _ = try_load_sqlite_vec_extension(conn);
    }
    if !sqlite_vec_available(conn) {
        return Ok(false);
    }
    conn.execute_batch(
        &format!(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
                chunk_id TEXT,
                embedding FLOAT[{EMBEDDING_DIM}]
            );
            "
        ),
    )?;
    Ok(true)
}

pub(super) fn sqlite_vec_available(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM pragma_module_list WHERE name = 'vec0'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

pub(super) fn try_load_sqlite_vec_extension(conn: &Connection) -> Result<()> {
    let path = match std::env::var("FOUNDRY_SQLITE_VEC_PATH") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(()),
    };
    unsafe {
        conn.load_extension_enable()?;
        let load_result = conn.load_extension(Path::new(&path), None);
        conn.load_extension_disable()?;
        load_result?;
    }
    Ok(())
}

pub(super) fn ensure_search_schema_readonly(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            md_path TEXT NOT NULL,
            meta_path TEXT NOT NULL,
            hash TEXT NOT NULL,
            terms_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL,
            ord INTEGER NOT NULL,
            text TEXT NOT NULL,
            token_len INTEGER NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
            chunk_id UNINDEXED,
            node_id UNINDEXED,
            text,
            tokenize = 'unicode61'
        );
        CREATE TABLE IF NOT EXISTS chunk_vectors (
            chunk_id TEXT PRIMARY KEY,
            model TEXT NOT NULL,
            dim INTEGER NOT NULL,
            embedding BLOB
        );
        ",
    )?;
    let _ = ensure_sqlite_vec_ready(conn);
    Ok(())
}

pub(super) fn open_search_db() -> Result<Connection> {
    let db_path = search_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    Ok(conn)
}

pub(super) fn search_db_path() -> PathBuf {
    PathBuf::from(".foundry/search/index.db")
}

pub(super) fn split_into_chunks(text: &str, target_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let overlap = (target_len / 6).clamp(80, 180);

    for part in text.split("\n\n") {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if p.len() > target_len {
            if !current.is_empty() {
                out.push(current.trim().to_string());
                current.clear();
            }
            out.extend(split_long_text_with_overlap(p, target_len, overlap));
            continue;
        }
        if !current.is_empty() && current.len() + p.len() + 2 > target_len {
            out.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(p);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    if out.is_empty() {
        let fallback = text.trim();
        if fallback.is_empty() {
            vec![String::new()]
        } else {
            vec![fallback.to_string()]
        }
    } else {
        out
    }
}

pub(super) fn normalize_query_for_fts(query: &str) -> String {
    query_terms_for_fts(query).join(" ")
}

pub(super) fn query_terms_for_fts(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for part in query.split(|c: char| !c.is_alphanumeric()) {
        let token = part.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        if seen.insert(token.clone()) {
            out.push(token);
        }
    }
    out
}

pub(super) fn split_long_text_with_overlap(text: &str, target_len: usize, overlap: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let sentences = split_sentences(text);
    let mut current = String::new();

    for sentence in sentences {
        if sentence.len() > target_len {
            if !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
                current.clear();
            }
            chunks.extend(split_by_char_window(&sentence, target_len, overlap));
            continue;
        }
        if !current.is_empty() && current.len() + sentence.len() + 1 > target_len {
            let finalized = current.trim().to_string();
            if !finalized.is_empty() {
                chunks.push(finalized.clone());
            }
            let carry = tail_overlap(&finalized, overlap);
            current.clear();
            if !carry.is_empty() {
                current.push_str(&carry);
                current.push(' ');
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&sentence);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    if chunks.is_empty() {
        vec![text.trim().to_string()]
    } else {
        chunks
    }
}

pub(super) fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
            let s = current.trim();
            if !s.is_empty() {
                out.push(s.to_string());
            }
            current.clear();
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

pub(super) fn split_by_char_window(text: &str, target_len: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= target_len {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    let step = target_len.saturating_sub(overlap).max(1);
    while start < chars.len() {
        let end = (start + target_len).min(chars.len());
        let slice = chars[start..end].iter().collect::<String>();
        let s = slice.trim();
        if !s.is_empty() {
            out.push(s.to_string());
        }
        if end == chars.len() {
            break;
        }
        start += step;
    }
    out
}

pub(super) fn tail_overlap(text: &str, overlap: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= overlap {
        text.to_string()
    } else {
        chars[chars.len() - overlap..].iter().collect::<String>()
    }
}

pub(super) fn matched_terms(query: &str, terms: &[String]) -> Vec<String> {
    let query_tokens = tokenize(query);
    terms
        .iter()
        .filter(|t| query_tokens.contains(&normalize_term_key(t)))
        .cloned()
        .collect()
}

pub(super) fn ranking_boost(query: &str, title: &str, terms: &[String]) -> f64 {
    let q_tokens = tokenize(query);
    let title_tokens = tokenize(title);
    let q_norm_tokens = query
        .split_whitespace()
        .map(normalize_term_key)
        .filter(|s| !s.is_empty())
        .collect::<HashSet<_>>();
    let title_overlap = q_tokens.intersection(&title_tokens).count() as f64;
    let exact_phrase = title
        .to_ascii_lowercase()
        .contains(&query.to_ascii_lowercase()) as u8 as f64;
    let term_overlap = terms
        .iter()
        .filter(|t| {
            let n = normalize_term_key(t);
            q_tokens.contains(&n) || q_norm_tokens.contains(&n)
        })
        .count() as f64;
    (title_overlap * 3.0) + (term_overlap * 2.5) + (exact_phrase * 4.0)
}

pub(super) fn semantic_vector(text: &str) -> Vec<f64> {
    let mut vec = vec![0.0_f64; EMBEDDING_DIM];
    let normalized = text.to_ascii_lowercase();

    for token in tokenize(&normalized) {
        let idx = stable_hash(token.as_bytes()) % EMBEDDING_DIM;
        vec[idx] += 2.0;
    }
    let compact: String = normalized
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
        .collect();
    let chars: Vec<char> = compact.chars().collect();
    for window in chars.windows(3) {
        let gram = window.iter().collect::<String>();
        let idx = stable_hash(gram.as_bytes()) % EMBEDDING_DIM;
        vec[idx] += 1.0;
    }

    let norm = vec.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}

pub(super) fn vector_to_blob(vec: &[f64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vec.len() * std::mem::size_of::<f64>());
    for value in vec {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub(super) fn vector_to_json(vec: &[f64]) -> String {
    let mut out = String::with_capacity(vec.len() * 8 + 2);
    out.push('[');
    for (idx, value) in vec.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&format!("{value:.8}"));
    }
    out.push(']');
    out
}

pub(super) fn blob_to_vector(blob: &[u8]) -> Result<Vec<f64>> {
    if !blob.len().is_multiple_of(std::mem::size_of::<f64>()) {
        anyhow::bail!("invalid embedding blob length: {}", blob.len());
    }
    let mut out = Vec::with_capacity(blob.len() / std::mem::size_of::<f64>());
    for chunk in blob.chunks_exact(std::mem::size_of::<f64>()) {
        let mut buf = [0_u8; std::mem::size_of::<f64>()];
        buf.copy_from_slice(chunk);
        out.push(f64::from_le_bytes(buf));
    }
    Ok(out)
}

pub(super) fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    let mut dot = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
    }
    dot
}

pub(super) fn stable_hash(bytes: &[u8]) -> usize {
    let mut hash = 1469598103934665603_u64;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    hash as usize
}

fn print_search_table(output: &SearchQueryOutput) {
    println!("query: {}", output.query);
    println!("mode: {}", output.mode);
    if output.hits.is_empty() {
        println!("hits: (none)");
        return;
    }
    println!("hits:");
    for hit in &output.hits {
        let terms = if hit.matched_terms.is_empty() {
            "-".to_string()
        } else {
            hit.matched_terms.join(",")
        };
        println!(
            "  - {} | {} | score={:.4} | terms={} | {}",
            hit.id, hit.path, hit.score, terms, hit.snippet
        );
    }
}
