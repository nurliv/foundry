# design-review

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Check design-to-spec traceability.
2. Verify risk propagation and conflict points.
3. Finalize design links and status.

## Core Commands
- foundry spec lint --format json
- foundry spec impact <DESIGN-ID> --format json --depth 3
- foundry spec ask "review design risks" --format json --explain
