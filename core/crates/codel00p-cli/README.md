# codel00p-cli

Terminal interface for codel00p.

The first implemented surface is the project-memory review workflow. Commands
open a SQLite memory store and operate on a project-scoped memory repository.

## Memory Review

```bash
codel00p \
  --memory-db .codel00p/memory.sqlite \
  --organization-id org-1 \
  --project-id project-1 \
  --project-name codel00p \
  memory list --status candidate

codel00p ... memory show mem-1
codel00p ... memory approve mem-1 --actor alice
codel00p ... memory reject mem-1 --actor alice --reason "too vague"
codel00p ... memory archive mem-1 --actor alice --reason "obsolete"
codel00p ... memory audit mem-1
```

Output is intentionally stable and scriptable:

- `memory list` prints `id`, `status`, `kind`, and `content` as tab-separated
  fields.
- review commands print `id` and the resulting status.
- `memory audit` prints `sequence`, `action`, `actor`, and `reason`.
