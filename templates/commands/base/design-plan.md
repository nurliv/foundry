# design-plan

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Translate accepted spec into design nodes.
2. Draft design markdown from `templates/node-docs/design.md`.
3. Connect design nodes via refines and depends_on.
4. Capture trade-offs with explicit rationale.

## Document Template
- Markdown template: `templates/node-docs/design.md`
- Suggested path: `spec/NN-design-<topic>.md`
- After writing markdown, run `foundry spec init --sync` to sync `.meta.json`

## Core Commands
- foundry spec ask "propose design for <SPC-ID>" --format json
- foundry spec write --path spec/<NN-design-topic>.md --body-file <design.md> --type component_design --status review
- foundry spec derive design --from <SPC-ID> --path spec/<NN-design-topic>.md --type component_design --status review
- foundry spec init --sync
- foundry spec link add --from <DESIGN-ID> --to <SPC-ID> --type refines --rationale "..."
- foundry spec impact <DESIGN-ID> --format json
