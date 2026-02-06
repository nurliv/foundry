# impl-review

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Re-run consistency checks after implementation.
2. Review downstream impact and remaining blockers.
3. Close or reopen tasks with evidence.

## Core Commands
- foundry spec lint --format json
- foundry spec impact <TASK-ID> --format json --depth 2
- foundry spec plan batches --format json
