<meta>
description: implementation review with optional user directive
argument-hint: <追加指示>
arguments:
   user-directive: $ARGUMENTS
</meta>

# Codex Overlay: impl-review

- Prefer direct CLI execution and JSON parsing in the agent loop.
- If context is stale, run in order: foundry spec init --sync, foundry spec search index, foundry spec lint --format json.
- Keep output concise and always reference node IDs in conclusions.
