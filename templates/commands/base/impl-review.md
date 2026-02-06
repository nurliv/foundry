# impl-review

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Re-run consistency checks after implementation.
2. Review downstream impact and remaining blockers.
3. Confirm final task state with evidence (`done` or `blocked`).

## Core Commands
- foundry spec lint --format json
- foundry spec impact <TASK-ID> --format json --depth 2
- foundry spec plan batches --format json
- foundry spec write --id <TASK-ID> --status done
- foundry spec write --id <TASK-ID> --status blocked
