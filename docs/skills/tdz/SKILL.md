---
name: tdz
description: Manage TODO comments in a codebase with the `tdz` CLI (todoozy). Use when listing, querying, adding, editing, tracing, or removing TODO comments embedded in source files, or when writing TODO comments in the todoozy format (IDs, priorities, tags, metadata).
---

# tdz — TODO comments as project management

`tdz` (todoozy) is a CLI that scans a codebase for specially formatted TODO
comments and treats them as trackable tasks: it assigns IDs, filters, sorts,
links into git history, and can edit/remove the comments in place. It supports
18+ languages (Rust, Go, Python, TypeScript, shell, CSS/SCSS/LESS, Terraform,
YAML, Markdown, Makefile, Dockerfile, Java, Protobuf, …) and respects
`.gitignore`. The contract is to add todo comments to VCS to create them, and
delete them to mark them as done.

The binary is `tdz`. Verify availability with `tdz --help`.

## TODO comment format

The token is `TODO` by default (configurable via `todo_token` in
`todoozy.json`). General shape:

```
TODO [#ID] [(PRIORITY)] TITLE [+tag ...] [@tag ...] [key:value ...]

[DESCRIPTION]
```

| Component  | Syntax                     | Notes                                             |
|------------|----------------------------|---------------------------------------------------|
| Primary ID | `#42`                      | Tracking ID, assigned by `tdz todo add`           |
| Reference  | `&42`                      | Comment that references/links to todo `#42`       |
| Priority   | `(A)`–`(Z)`                | `(A)` = highest; must directly follow the ID      |
| Tag        | `+project`                 | Tags; `tdz tag list` aggregates them              |
| Metadata   | `key:value`                | e.g. `assignee:john`; keys starting `_` reserved  |
| Body       | following comment lines    | Continuation comment lines become the description |

Examples:

```rust
// TODO Fix the login bug
// TODO #15 (B) Refactor database layer +backend db:postgresql
//
// The current implementation needs abstraction.
```

```go
/* TODO #23 (C) Add caching +performance

   Cache API responses to reduce latency.
*/
```

```python
# TODO &15 Also update the DAO tests +backend
```

When you write TODO comments: include meaningful `+tags`, use `key:value`
metadata sparingly, keep the title on the first line, put longer explanation in
following comment lines. Leave out the `#ID`, do not ivent them yourself — if
the user asks run `tdz todo add` to assign IDs.

## CLI usage

The `tdz` CLI commands all support --help to explain usage / syntax, etc.

Specifically, see `tdz todo --help` for details of filter / sort syntax and
options.

### Notes

- `tdz todo add` edits source files in place, inserting the next available `#ID`.
- `tdz todo remove` deletes the comment from the source file permanently — prefer
  asking the user before removing, unless removal was requested.
- To "complete" a todo, remove it.
- `tdz lint` exists to check todos comform to the expected format.

## Configuration

`todoozy.json` at the repository root (all fields optional):

```json
{
  "exclude": ["vendor", "node_modules"],
  "filter": "priority>=z",
  "sorter": "priority:asc > creation_date:desc",
  "todo_token": null
}
```

- `exclude` — paths skipped when scanning (also `-E "a,b"` on the CLI).
- `filter` / `sorter` — defaults applied to listings (override with `-f`/`-s`;
  pass an empty string to unset).
- `todo_token` — comment keyword to scan for (default `TODO`).
