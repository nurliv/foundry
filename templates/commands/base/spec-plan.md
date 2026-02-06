# spec-plan

## Objective
Drive this phase for {{project_name}} with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Discover relevant specs and constraints.
2. Produce candidate scope and acceptance criteria.
3. Link proposals as proposed edges.

## Core Commands
- foundry spec search query "<topic>" --format json --mode hybrid
- foundry spec ask "<goal and constraints>" --format json --explain
- foundry spec link propose --node {{main_spec_id}}
- foundry spec impact {{main_spec_id}} --format json --depth {{default_depth}}
