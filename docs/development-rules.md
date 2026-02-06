# Development Rules

This file stores incremental engineering rules and decisions for this repository.

## Testing Strategy

- Use both unit tests and integration tests.
- Unit tests live close to logic in source files with `#[cfg(test)]`.
- Integration tests live under `tests/` and validate CLI behavior end-to-end.
- For this project:
- unit tests should cover pure logic (`hash`, `title extraction`, graph traversal helpers).
- integration tests should cover commands (`spec init`, `spec lint`, `spec link`, `spec impact`).

## Process Rule

- When a new coding rule is agreed during development, append it here in the relevant section.

## Agent Template Rule

- Agent-oriented generated files should support writing directly to each agent's auto-read directory layout, not only repository docs output.

## Node Layout Rule

- Task node markdown/meta should default to `tasks/<source-node-id>/...` rather than `spec/...` for better operational visibility.

## Codex Output Rule

- In install mode, Codex command templates must be generated under `<codex_home>/prompts/`.
