# Implementation Plan (MVP)

## Milestone 1: File Scanner and Meta Sync

- discover `spec/**/*.md`
- create corresponding `.meta.json` when missing
- compute `sha256` hash from markdown content
- sync generated fields (`title`, `body_md_path`, `hash`)

Done definition:

- `foundry spec init` can initialize a clean `spec/` directory.

## Milestone 2: Lint Engine

- load all meta files and validate with schema
- detect hash mismatch and missing references
- detect duplicate ids and orphan nodes
- emit machine-readable JSON and human-readable table

Done definition:

- `foundry spec lint` returns stable exit codes and actionable messages.
- `foundry spec lint --format json` returns machine-readable diagnostics.

## Milestone 3: Edge CRUD

- implement `spec link add/remove/list`
- enforce edge type enum and confidence range
- preserve manual edits for rationale/status

Done definition:

- edges are safely editable through CLI with no schema violations.

## Milestone 4: Impact Report

- graph traversal from a seed node
- include forward, reverse, test, and conflict sections
- recommend review order by shortest path distance

Done definition:

- `foundry spec impact SPC-xxx` helps reviewers identify likely blast radius.

## Milestone 5: AI Proposal Hook (Optional in MVP+)

- implement `spec link propose`
- mark all proposed edges as `status=proposed`
- do not auto-merge proposals

Done definition:

- suggestions are reviewable and do not alter confirmed graph semantics.

## Milestone 6: Ask Contract Stabilization (MVP+)

- define a stable JSON contract for `spec ask --format json`
- publish JSON schema for agent/tool integration
- add integration tests for required keys and explain-mode behavior

Done definition:

- ask JSON output matches the documented schema structure for required fields.

## Milestone 7: Task Planning from Graph (MVP+)

- implement `spec plan ready` to list immediately executable task nodes
- implement `spec plan batches` to output parallel execution layers
- treat task dependencies as `depends_on` edges between task node types

Done definition:

- planning commands produce stable table/json outputs that can be consumed by agents.

## Milestone 8: Agent Template Bootstrap (MVP+)

- add `spec init --agent <codex|claude>` command-template generation
- compose templates from `templates/commands/base` + agent overlays
- support safe regeneration with `--agent-sync`

Done definition:

- init can generate agent command templates into `docs/agents/<agent>/commands`.
