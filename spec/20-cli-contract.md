# CLI Contract (MVP)

## Command Set

- `foundry spec init`
- `foundry spec lint`
- `foundry spec link`
- `foundry spec impact`
- `foundry spec search`

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

## `foundry spec search`

Subcommands:

- `index`: build or update search index from `spec/**/*.md` and `spec/**/*.meta.json`
- `query`: run lexical search (hybrid flag is accepted for forward compatibility)
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
- `query --mode lexical|hybrid` default `lexical` (`hybrid` currently falls back to lexical ranking)
