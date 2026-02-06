# spec-review

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Validate consistency and unresolved conflicts.
2. Inspect propagation risks for changed nodes.
3. Resolve or confirm links before approval.

## Core Commands
- foundry spec lint --format json
- foundry spec impact <SPC-ID> --format json --depth 2
- foundry spec ask "what should be reviewed?" --format json --explain
