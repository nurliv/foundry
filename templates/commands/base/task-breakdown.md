# task-breakdown

## Objective
Drive this phase with graph-traceable decisions and reproducible CLI evidence.

## Recommended Flow
1. Create task nodes from design outcomes.
2. Draft task markdown from `templates/node-docs/task.md`.
3. Derive tasks from design with `spec derive tasks`.
4. Validate execution order and parallelism.

## Document Template
- Markdown template: `templates/node-docs/task.md`
- Suggested path: `spec/NN-task-<topic>.md`
- After writing markdown, run `foundry spec init --sync` to sync `.meta.json`

## Core Commands
- foundry spec write --path spec/<NN-task-topic>.md --body-file <task.md> --type implementation_task --status todo
- foundry spec derive tasks --from <DESIGN-ID> --path spec/<NN-task-topic>.md --type implementation_task --status todo --depends-on <TASK-ID>
- foundry spec plan ready --format json
- foundry spec plan batches --format json
- foundry spec write --id <TASK-ID> --status review
