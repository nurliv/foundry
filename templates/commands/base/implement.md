# implement

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Pick ready tasks.
2. Mark task as `doing` before changes.
3. Implement with spec citations and checks.
4. Mark task as `done` when verification passes.

## Core Commands
- foundry spec plan ready --format json
- foundry spec ask "implementation guidance for <TASK-ID>" --format json
- foundry spec write --id <TASK-ID> --status doing
- foundry spec impact <TASK-ID> --format json
- foundry spec write --id <TASK-ID> --status done
