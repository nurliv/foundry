# CLI Contract (MVP)

## Command Set

- `foundry spec init`
- `foundry spec write`
- `foundry spec derive`
- `foundry spec lint`
- `foundry spec link`
- `foundry spec impact`
- `foundry spec plan`
- `foundry spec agent`
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
- Optional: `--agent codex|claude` can be specified multiple times to generate agent command templates.
- `--agent-sync` overwrites existing generated agent template files.
- `--agent-output docs|install|both` controls output destination (default `docs`)
- `--codex-home <path>` overrides codex install root (default `$CODEX_HOME`, then `$HOME/.codex`)
- `--claude-dir <path>` overrides claude install root (default `.claude`)
- Template source can be selected with:
- `--template-source local|github` (default `github`)
- `--template-repo <git_url>` (default `https://github.com/nurliv/foundry.git`)
- `--template-ref <git_ref>` (default `main`)
- `github` mode downloads `<repo>/archive/<ref>.tar.gz`, extracts to `.foundry/template-sources/`, then reuses local cache
- downloaded archive file is deleted after extraction
- If github fetch fails, generation falls back to local `templates/`.

Output:

- summary counts (`created`, `updated`, `skipped`, `error`)
- agent template summary (`written`, `skipped`, `errors`) when `--agent` is used

## `foundry spec write`

Purpose:

- create or update a single node markdown and matching `.meta.json` in one step
- enable AI agents to persist phase outputs through Foundry CLI

Usage:

- `foundry spec write --path spec/10-auth.md --body "# Auth\n\n..." --type feature_requirement --status draft`
- `foundry spec write --path spec/40-design-auth.md --body-file /tmp/design.md --type component_design --status review`
- `foundry spec write --id SPC-010 --status doing`

Rules:

- either `--path` or `--id` is required
- if `--path` is provided, it must be under `spec/` and end with `.md`
- if only `--id` is provided, markdown path is resolved from existing meta
- `--body` and `--body-file` are mutually exclusive
- when meta exists, unspecified fields are preserved (including existing `edges`)
- when meta is missing, defaults are used (`type=feature_requirement`, `status=draft`, auto `id`)
- title is resolved from `--title` or markdown first heading (`# ...`)
- hash is always updated from markdown content

Flags:

- `--path <spec/*.md>` optional (required for create flow)
- `--id <SPC-xxx>` optional explicit id
- `--type <node_type>` optional
- `--status <node_status>` optional
- `--title <text>` optional
- `--body <markdown>` optional
- `--body-file <path>` optional
- `--term <text>` repeatable; if provided, replaces `terms[]`

## `foundry spec derive`

Subcommands:

- `design`: derive or update one design node from a source node
- `tasks`: derive or update one task node from a source design node

Examples:

- `foundry spec derive design --from SPC-001 --path spec/40-auth-design.md --type component_design --status review`
- `foundry spec derive design --from SPC-001 --body-file /tmp/design.md`
- `foundry spec derive design --from SPC-001 --format json`

Rules (`design`):

- `--from` source node id is required and must exist
- derived node is written through `spec write`
- when `--path` is omitted, default path is `spec/design-<from-id-lower>.md`
- generated/updated design node gets a confirmed `refines` edge to source node
- if `--body` and `--body-file` are omitted, a default design skeleton body is generated

Examples (`tasks`):

- `foundry spec derive tasks --from SPC-010 --path spec/60-auth-task.md --type implementation_task --status todo`
- `foundry spec derive tasks --from SPC-010 --depends-on SPC-020 --depends-on SPC-021`
- `foundry spec derive tasks --from SPC-010 --item "API" --item "DB Migration" --item "Tests" --chain`
- `foundry spec derive tasks --from SPC-010 --item "API" --item "Tests" --format json`

Rules (`tasks`):

- `--from` source node id is required and must exist
- derived node is written through `spec write`
- when `--path` is omitted, default path is `spec/task-<from-id-lower>.md`
- generated/updated task node gets a confirmed `refines` edge to source node
- `--depends-on` adds confirmed `depends_on` edges from derived task to given node ids
- `--item` can be repeated to generate multiple task nodes in one command
- `--chain` adds auto `depends_on` edges from each generated task to the previous generated task
- if `--body` and `--body-file` are omitted, a default task skeleton body is generated
- `--format table|json` default `table`

Output fields (`derive design --format json`):

- `mode` (`design`)
- `source`
- `derived` (`id`, `path`)
- `edges[]` (`from`, `to`, `type`, `status`)

Output fields (`derive tasks --format json`):

- `mode` (`tasks`)
- `source`
- `derived[]` (`id`, `path`)
- `edges[]` (`from`, `to`, `type`, `status`)
- `chain`

Generated paths (`--agent`):

- `docs/agents/<agent>/commands/*.md` from
  `templates/commands/base/*.md` + `templates/commands/overlays/<agent>/*.md`
- `docs/agents/<agent>/skills/*.md` from
  `templates/skills/base/*.md` + `templates/skills/overlays/<agent>/*.md`
- agent templates also reference document skeletons under:
- `templates/node-docs/spec.md`
- `templates/node-docs/design.md`
- `templates/node-docs/task.md`
- install output (`--agent-output install|both`):
- codex: `<codex_home>/{commands|skills}/foundry/*.md`
- claude: `<claude_dir>/{commands|skills}/foundry/*.md`

Supported template placeholders:

- `{{project_name}}`: current working directory name
- `{{main_spec_id}}`: first `product_goal` id if present, otherwise smallest node id, fallback `SPC-001`
- `{{default_depth}}`: default impact traversal depth (`2`)
- implement / impl-review command templates should include `spec write --id <TASK-ID> --status ...` examples for task state transitions
- design / task-breakdown command templates should include `spec derive design` / `spec derive tasks` as primary generation path

## `foundry spec agent`

Subcommands:

- `doctor`: validate generated agent command templates against current source templates

Examples:

- `foundry spec agent doctor`
- `foundry spec agent doctor --agent codex --format json`
- `foundry spec agent doctor --template-source local --format json`
- `foundry spec agent doctor --agent-output install --codex-home /tmp/.codex --claude-dir /tmp/.claude`

Rules:

- default agents: `codex`, `claude` when `--agent` is omitted
- template source options are the same as `spec init`
- `--agent-output docs|install|both` is supported for destination validation (default `docs`)
- compares generated files under:
- `docs/agents/<agent>/commands/*.md`
- `docs/agents/<agent>/skills/*.md`
- install mode compares:
- codex: `<codex_home>/{commands|skills}/foundry/*.md`
- claude: `<claude_dir>/{commands|skills}/foundry/*.md`
- with rendered content from matching `templates/{commands|skills}/...` files

Exit codes:

- `0`: no issue
- `1`: missing/stale template output found

Output fields (`doctor --format json`):

- `ok`
- `checked`
- `issues[]` (`agent`, `artifact`, `phase`, `kind`, `detail`)

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
