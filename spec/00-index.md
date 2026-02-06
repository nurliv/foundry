# Foundry Spec Graph

Foundry is an AI-assisted development tool for maintaining spec consistency with a graph model.

## Map

- [Domain Model](./10-domain-model.md)
- [CLI Contract](./20-cli-contract.md)

## Principles

- Human-readable source of truth is `spec/**/*.md`.
- Tool-managed relation/state is `spec/**/*.meta.json`.
- AI only proposes links and conflicts. Humans confirm.

## MVP Scope

- 1 file = 1 node.
- Meta generation and update from markdown hash.
- Edge CRUD via CLI.
- Impact report (depends/tests/impacts traversal).
- Lint for consistency and unresolved risks.

## Non-Goals (MVP)

- Auto-confirming AI-proposed edges.
- Fully automatic document rewrites.
- Rich graph UI.
