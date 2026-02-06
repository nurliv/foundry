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
