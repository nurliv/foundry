# Foundry

Graph-native, spec-driven development toolkit for AI-assisted workflows.

Foundry keeps human-readable specs in Markdown while tracking machine-usable graph metadata (dependencies, impacts, conflicts, tests, and task planning).

## Core Idea

- Human source of truth: `spec/**/*.md`
- Tool-managed graph/state: `spec/**/*.meta.json`
- AI suggests, humans confirm

## Features

- `spec init`: generate/sync meta files from Markdown
- `spec lint`: consistency checks with table/json output
- `spec link`: edge CRUD and proposal support
- `spec impact`: blast-radius and review-order analysis
- `spec search`: lexical/hybrid retrieval + index doctor
- `spec ask`: citation-first RAG-style answers
- `spec plan`: ready task extraction + parallel batches
- `spec agent`: generated template drift checks

## Quick Start

```bash
# 1) Build
cargo build

# 2) Initialize metadata
foundry spec init --sync

# 3) Validate graph consistency
foundry spec lint --format json

# 4) Build search index
foundry spec search index

# 5) Ask and inspect impact
foundry spec ask "what should I implement first?" --format json --explain
foundry spec impact SPC-001 --format json
```

## Task Planning

Use task node types (`implementation_task`, `test_task`, `migration_task`) and `depends_on` edges.

```bash
foundry spec plan ready --format json
foundry spec plan batches --format json
```

## Agent Templates (Codex / Claude)

Generate commands + skills documents from base templates and agent overlays:

```bash
foundry spec init --agent codex --agent claude
```

Force regeneration:

```bash
foundry spec init --agent codex --agent claude --agent-sync
```

Validate generated outputs against templates:

```bash
foundry spec agent doctor --format json
```

Template sources:

- `templates/commands/base`
- `templates/commands/overlays/{codex|claude}`
- `templates/skills/base`
- `templates/skills/overlays/{codex|claude}`

Generated outputs:

- `docs/agents/<agent>/commands/*.md`
- `docs/agents/<agent>/skills/*.md`

Supported placeholders:

- `{{project_name}}`
- `{{main_spec_id}}`
- `{{default_depth}}`

## Docs

- CLI contract: `spec/20-cli-contract.md`
- Domain model: `spec/10-domain-model.md`
- Implementation plan: `docs/implementation-plan.md`
- Ask JSON schema: `docs/schemas/spec-ask-output.schema.json`

## Development

```bash
cargo test
```

