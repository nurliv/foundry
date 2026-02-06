# spec-plan

## Objective
Drive this phase for {{project_name}} with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Discover relevant specs and constraints.
2. Draft or update spec markdown from `templates/node-docs/spec.md`.
3. Produce candidate scope and acceptance criteria.
4. Link proposals as proposed edges.

## Document Template
- Markdown template: `templates/node-docs/spec.md`
- Suggested path: `spec/NN-topic.md`
- After writing markdown, run `foundry spec init --sync` to sync `.meta.json`

## Core Commands
- foundry spec search query "<topic>" --format json --mode hybrid
- foundry spec ask "<goal and constraints>" --format json --explain
- foundry spec init --sync
- foundry spec link propose --node {{main_spec_id}}
- foundry spec impact {{main_spec_id}} --format json --depth {{default_depth}}
