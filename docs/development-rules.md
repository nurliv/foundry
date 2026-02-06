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
