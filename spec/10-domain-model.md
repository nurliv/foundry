# Domain Model

## Source of Truth

- Narrative content: `spec/**/*.md`
- Graph/state metadata: `spec/**/*.meta.json`

Markdown remains the human-facing source. Metadata tracks machine-usable structure and lifecycle.

## Node

`SpecNode`:

- `id`: stable node identifier (`SPC-001` format)
- `type`: node type (see list below)
- `status`: lifecycle status
- `title`: short title
- `body_md_path`: relative path to markdown file
- `terms`: glossary keys used in this node
- `hash`: content hash of markdown body

### Recommended Node Types (MVP fixed set)

- `product_goal`
- `feature_requirement`
- `non_functional_requirement`
- `constraint`
- `domain_concept`
- `decision` (ADR-like)
- `workflow`
- `api_contract`
- `data_contract`
- `test_spec`
- `architecture`
- `component_design`
- `api_design`
- `data_design`
- `adr`
- `implementation_task`
- `test_task`
- `migration_task`

### Node Status

- `draft`
- `review`
- `active`
- `deprecated`
- `archived`
- `todo`
- `doing`
- `done`
- `blocked`

## Edge

`SpecEdge`:

- `from`: source node id
- `to`: destination node id
- `type`: relation type
- `rationale`: why relation exists
- `confidence`: `0.0` to `1.0`
- `status`: `confirmed` or `proposed`

### Edge Types

MVP required:

- `depends_on`: source needs destination
- `refines`: source elaborates destination
- `conflicts_with`: source is incompatible with destination
- `tests`: source validates destination

MVP optional:

- `impacts`: source change likely affects destination

## Granularity Rules

Start coarse: one markdown file is one node.

Split a node when one or more applies:

- It contains multiple independent claims (many `and` cases).
- Acceptance criteria branch into different behavior lines.
- One-way relation count exceeds 10.
- Frequent edits touch unrelated sections repeatedly.

Merge candidates:

- Tiny declaration-only nodes with no standalone review value.
- Near-linear chains that always move together.

## ID Policy

- New node always gets a new id. Do not suffix split ids as `A/B`.
- On split, old node moves to `deprecated`.
- Link old -> new with `refines`.
- On merge, keep one surviving id, deprecate merged nodes.
