<meta>
description: design planning with optional user directive
argument-hint: <追加指示>
arguments:
   user-directive: $ARGUMENTS
</meta>

# Codex Overlay: design-plan

- Prefer direct CLI execution and JSON parsing in the agent loop.
- If context is stale, run in order: foundry spec init --sync, foundry spec search index, foundry spec lint --format json.
- Keep output concise and always reference node IDs in conclusions.
