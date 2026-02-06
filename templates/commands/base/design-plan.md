# design-plan

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Translate accepted spec into design nodes.
2. Connect design nodes via refines and depends_on.
3. Capture trade-offs with explicit rationale.

## Core Commands
- foundry spec ask "propose design for <SPC-ID>" --format json
- foundry spec link add --from <DESIGN-ID> --to <SPC-ID> --type refines --rationale "..."
- foundry spec impact <DESIGN-ID> --format json
