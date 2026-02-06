# task-breakdown

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Create task nodes from design outcomes.
2. Add depends_on edges among tasks.
3. Validate execution order and parallelism.

## Core Commands
- foundry spec plan ready --format json
- foundry spec plan batches --format json
- foundry spec link add --from <TASK-ID> --to <TASK-ID> --type depends_on --rationale "..."
