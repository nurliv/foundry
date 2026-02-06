# Foundry

Graph-native, spec-driven development toolkit for AI-assisted workflows.

Foundry keeps human-readable specs in Markdown while tracking machine-usable graph metadata (dependencies, impacts, conflicts, tests, and task planning).

## Core Idea

- Human source of truth: `spec/**/*.md` and `tasks/**/*.md`
- Tool-managed graph/state: `spec/**/*.meta.json` and `tasks/**/*.meta.json`
- AI suggests, humans confirm

## Features

- `spec init`: generate/sync meta files from Markdown
- `spec write`: create/update one spec node markdown + meta in one command
- `spec derive design`: generate/update a design node from a source spec node and auto-link with `refines`
- `spec derive tasks`: generate/update a task node from a design node and auto-link with `refines`
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

# 4) Write/update one node
foundry spec write --path spec/10-auth.md --body "# Auth\n\n..." --type feature_requirement --status draft
foundry spec write --id SPC-001 --status doing

# 5) Derive design from spec node
foundry spec derive design --from SPC-001 --path spec/40-auth-design.md --type component_design --status review
foundry spec derive design --from SPC-001 --format json

# 6) Derive implementation task from design node
foundry spec derive tasks --from SPC-010 --path tasks/spc-010/60-auth-task.md --type implementation_task --status todo
foundry spec derive tasks --from SPC-010 --item "API" --item "DB Migration" --item "Tests" --chain
foundry spec derive tasks --from SPC-010 --item "API" --item "Tests" --chain --format json

# 7) Build search index
foundry spec search index

# 8) Ask and inspect impact
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

Use local templates explicitly (recommended for offline/CI):

```bash
foundry spec init --agent codex --template-source local
```

Install into agent auto-read directories (instead of `docs/agents/...`):

```bash
foundry spec init --agent codex --agent claude --agent-output install
```

Generate both docs output and install output:

```bash
foundry spec init --agent codex --agent claude --agent-output both
```

Template source options:

- `--template-source local|github` (default `github`)
- `--template-repo <git_url>` (default `https://github.com/nurliv/foundry.git`)
- `--template-ref <git_ref>` (default `main`)
- `--agent-output docs|install|both` (default `docs`)
- `--codex-home <path>` override codex install root (default: `$CODEX_HOME`, then `$HOME/.codex`)
- `--claude-dir <path>` override claude install root (default: `.claude`)
- github mode downloads `<repo>/archive/<ref>.tar.gz`, extracts under `.foundry/template-sources/`, and reuses the extracted cache on later runs
- temporary archive files are deleted after extraction
- github fetch failure falls back to local `templates/`

Validate generated outputs against templates:

```bash
foundry spec agent doctor --format json
```

Template sources:

- `templates/commands/base`
- `templates/commands/overlays/{codex|claude}`
- `templates/skills/base`
- `templates/skills/overlays/{codex|claude}`
- `templates/node-docs/{spec|design|task}.md` (document skeletons used by agent instructions)

Generated outputs:

- `docs/agents/<agent>/commands/*.md`
- `docs/agents/<agent>/skills/*.md`
- implement/impl-review templates include task status transition commands via `spec write --id <TASK-ID> --status ...`
- design/task planning templates prioritize `spec derive design` / `spec derive tasks` over manual link wiring
- install mode:
- codex: `<codex_home>/{commands|skills}/foundry/*.md`
- claude: `<claude_dir>/{commands|skills}/foundry/*.md`

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
