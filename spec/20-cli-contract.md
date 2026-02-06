# CLI Contract (MVP)

## Command Set

- `foundry spec init`
- `foundry spec lint`
- `foundry spec link`
- `foundry spec impact`
- `foundry spec plan`
- `foundry spec search`
- `foundry spec ask`

## `foundry spec init`

Purpose:

- Scan `spec/**/*.md`
- Create missing `.meta.json`
- Refresh `hash` for existing meta files

Behavior:

- By default, only missing fields are filled.
- Use `--sync` to rewrite generated fields (`title`, `hash`, path).

Output:

- summary counts (`created`, `updated`, `skipped`, `error`)

## `foundry spec lint`

Checks:

- markdown/meta hash mismatch
- missing required fields
- duplicate node ids
- orphan nodes (no in/out edges) except `product_goal`
- unresolved `conflicts_with` (`confirmed` + both nodes `active`)
- term key drift (same term written with multiple keys)
- edge points to unknown node

Exit codes:

- `0`: no error
- `1`: lint errors found
- `2`: runtime/system error

Flags:

- `--format table|json` default `table`

Output fields (`--format json`):

- `ok`
- `error_count`
- `errors[]`

## `foundry spec link`

Subcommands:

- `add`: create edge
- `remove`: delete edge
- `list`: list edges for a node
- `propose`: AI-assisted suggestions (human confirmation required)

Examples:

- `foundry spec link add --from SPC-014 --to SPC-021 --type depends_on --rationale "auth flow prerequisite"`
- `foundry spec link list --node SPC-014`

Rules:

- `from`, `to`, `type` required for `add`
- `confidence` default is `1.0` for manual links
- `propose` creates edges with `status=proposed`
- `propose --from --to --type` creates/updates one manual proposal
- `propose --node <ID>` auto-generates ranked proposals for that node (MVP heuristic mode)

## `foundry spec impact`

Usage:

- `foundry spec impact <NODE_ID>`

Traversal (MVP):

- forward: `depends_on`, `impacts`
- reverse: nodes that `depends_on` source
- verification chain: `tests` connected nodes
- include `conflicts_with` as risk list

Output sections:

- `direct_dependencies`
- `reverse_dependents`
- `test_coverage_chain`
- `conflict_risks`
- `recommended_review_order`

Flags:

- `--depth <n>` default `2`
- `--format table|json` default `table`

Notes:

- `--depth` limits traversal distance for `reverse_dependents`, `test_coverage_chain`, and `recommended_review_order`.

## `foundry spec plan`

Purpose:

- derive executable task queues from task dependency edges
- identify tasks ready for immediate execution
- group independent tasks into parallel batches

Subcommands:

- `ready`: list task nodes without unresolved task dependencies
- `batches`: compute layered parallel execution batches

Rules:

- task node types: `implementation_task`, `test_task`, `migration_task`
- dependency edge considered for planning: `depends_on` to another task node
- done statuses: `done`, `archived`, `deprecated`

Examples:

- `foundry spec plan ready --format table`
- `foundry spec plan ready --format json`
- `foundry spec plan batches --format json`

Output fields (`ready --format json`):

- `ready[]` (`id`, `title`, `path`, `status`)
- `blocked[]` (`id`, `title`, `path`, `status`, `blocked_by[]`)

Output fields (`batches --format json`):

- `batches[]` (`batch`, `task_ids[]`, `tasks[]`)
- `tasks[]` item fields: (`id`, `title`, `path`, `status`)
- `blocked_or_cyclic[]`
- `blocked_or_cyclic_tasks[]` (`id`, `title`, `path`, `status`)

## `foundry spec search`

Subcommands:

- `index`: build or update search index from `spec/**/*.md` and `spec/**/*.meta.json`
- `query`: run lexical or hybrid search
- `doctor`: verify index consistency against current node hashes

Examples:

- `foundry spec search index`
- `foundry spec search query "auth flow" --top-k 10 --format table`
- `foundry spec search query "auth flow" --format json --mode lexical`
- `foundry spec search doctor`

Flags:

- `index --rebuild`: full rebuild
- `query --top-k <n>` default `10`
- `query --format table|json` default `table`
- `query --mode lexical|hybrid` default `lexical`
- `hybrid` combines lexical ranking and semantic similarity (RRF merge)
- semantic similarity in `hybrid` is computed from pre-indexed chunk vectors (`spec search index`)
- if `FOUNDRY_SQLITE_VEC_PATH` is set, the tool loads `sqlite-vec` and uses `vec0` search; otherwise it falls back to local cosine ranking

## `foundry spec ask`

Purpose:

- provide retrieval-augmented, citation-first answers for AI agents and reviewers
- return machine-readable evidence that can be interpreted by external agents
- expand 1-hop graph neighbors from top hits (depends/tests/refines/impacts/conflicts) for context and risk surfacing

Usage:

- `foundry spec ask "<question>" --format json`

Flags:

- `--top-k <n>` default `5`
- `--mode lexical|hybrid` default `hybrid`
- `--format table|json` default `table`
- `--explain` include per-citation selection reasons

Output fields (`--format json`):

- `answer`
- `confidence`
- `citations[]` (`id`, `title`, `path`)
- `evidence[]` (`id`, `snippet`, `score`)
- `explanations[]` (`id`, `reason`) when `--explain` is enabled
- reason includes retrieval rank/score and token-level match hints (title/snippet) when available
- graph-neighbor reasons include edge-weight contribution hints (for configured `ask.edge_weight.*`)
- `gaps[]` (empty if enough evidence exists)
- contract schema: `docs/schemas/spec-ask-output.schema.json`

Runtime tuning:

- optional file: `.foundry/config.json`
- supported keys:
- `ask.neighbor_limit`
- `ask.snippet_count_in_answer`
- `ask.edge_weight.depends_on|tests|refines|impacts|conflicts_with`
