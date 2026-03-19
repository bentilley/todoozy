# todoozy

> para todos ;)

CLI for managing todo comment based project management.

> [doozy](https://dictionary.cambridge.org/dictionary/english/doozy)
>
> - something special or unusual, especially something unusually bad

## Features

- **Multi-language support** - Parse TODO comments from 14+ languages including
  Rust, Go, Python, TypeScript, Bash, Terraform, and more
- **Rich TODO metadata** - IDs, priorities (A-Z), dates, projects (`+project`),
  contexts (`@context`), and custom key:value pairs
- **Interactive TUI** - Full-screen terminal interface for browsing, filtering,
  and managing TODOs
- **Filtering & sorting** - Flexible query expressions to find exactly what you need
- **Auto-import** - Automatically assign tracking IDs to untracked TODOs
- **Git integration** - Automatically finds repository root and respects .gitignore

## Installation

Build from source using Cargo:

```bash
cargo install --path .
```

The binary `tdz` will be available in your cargo `bin` directory.

## Usage

### Basic Commands

```bash
# Launch interactive TUI (default)
tdz

# List all projects found in TODOs
tdz --list-projects

# List all contexts found in TODOs
tdz --list-contexts

# Import all untracked TODOs (assigns IDs)
tdz --import-all

# Filter TODOs
tdz -f "priority=A"

# Sort TODOs
tdz -s "priority:asc > creation_date:desc"

# Exclude paths
tdz -E "vendor,node_modules"
```

### TUI Keyboard Shortcuts

| Key          | Action               |
|--------------|----------------------|
| `q` / `Esc`  | Exit / deselect      |
| `j` / `↓`    | Next TODO            |
| `k` / `↑`    | Previous TODO        |
| `g` / `Home` | First TODO           |
| `G` / `End`  | Last TODO            |
| `l` / `→`    | Toggle completion    |
| `Enter`      | View TODO details    |
| `e`          | Edit TODO in editor  |
| `f`          | Filter TODOs         |
| `F`          | Reset filter         |
| `s`          | Sort TODOs           |
| `S`          | Reset sort           |
| `i`          | Import selected TODO |
| `I`          | Import all TODOs     |
| `R`          | Refresh from disk    |

## TODO Comment Format

Todoozy parses TODO comments with a flexible syntax:

```todoozy
TODO [#ID] [(PRIORITY)] [DATES] TITLE [@context] [+project] [key:value]

[DESCRIPTION]
```

### Components

| Component       | Format         | Example                 |
|-----------------|----------------|-------------------------|
| ID              | `#` + number   | `#42`                   |
| Priority        | `(A)` to `(Z)` | `(A)` = highest         |
| Creation date   | `YYYY-MM-DD`   | `2024-08-05`            |
| Completion date | `YYYY-MM-DD`   | `2024-08-10 2024-08-05` |
| Project         | `+` + name     | `+backend`              |
| Context         | `@` + name     | `@work`                 |
| Metadata        | `key:value`    | `assignee:john`         |

### Examples

Simple TODO:

```rust
// TODO Fix the login bug
```

With priority and project:

```rust
// TODO (A) Implement authentication +backend @security
```

Full metadata with description:

```rust
// TODO #15 (B) 2024-08-14 Refactor database layer +backend db:postgresql
//
// The current implementation needs abstraction.
// Create a proper DAO layer with traits.
```

Multiline in block comment:

```go
/* TODO #23 (C) 2024-03-01 Add caching +performance

   We need to cache API responses to reduce latency.
   Consider using Redis or in-memory cache.
*/
```

## Supported Languages

| Language   | Extensions                  |
|------------|-----------------------------|
| Rust       | `.rs`                       |
| Go         | `.go`                       |
| Python     | `.py`                       |
| TypeScript | `.ts`, `.tsx`               |
| Bash       | `.bash`                     |
| Shell      | `.sh`                       |
| Zsh        | `.zsh`                      |
| Ksh        | `.ksh`                      |
| Protobuf   | `.proto`                    |
| Terraform  | `.tf`                       |
| Markdown   | `.md`                       |
| YAML       | `.yaml`, `.yml`             |
| Makefile   | `Makefile`, `.mk`           |
| Dockerfile | `Dockerfile`, `.dockerfile` |

## Configuration

Todoozy uses a `todoozy.json` file in your repository root:

```json
{
  "_num_todos": 51,
  "exclude": ["vendor", "node_modules"],
  "filter": "priority>Z",
  "sorter": "priority:asc > creation_date:desc"
}
```

| Field        | Description                    |
|--------------|--------------------------------|
| `_num_todos` | Counter for auto-assigned IDs  |
| `exclude`    | Paths to exclude from scanning |
| `filter`     | Default filter expression      |
| `sorter`     | Default sort expression        |

### Filter Syntax

```bash
# By priority
priority=A
priority>B

# By project or context
project=backend
context=work

# By date
creation_date>=2024-03-01

# By file
file=/path/to/file.rs

# Logical operators
priority=A or priority=B
(priority=A or priority=B) and project=backend
not context=personal
```

### Sort Syntax

Sort by multiple fields with direction:

```bash
# Single field
priority:asc

# Multiple fields (left to right precedence)
priority:asc > creation_date:desc
file:asc > line_number:asc
```

Sortable fields: `title`, `file`, `line_number`, `priority`, `creation_date`, `completion_date`

## Objectives

As a user I want to...

- ...view TODO comments in my codebase;
- ...interact with my codebase via TODO comments (e.g. jump to them, delete
  them if done, etc.);
- ...view previous TODO comments stored in version control history;
- ...run queries over current and history todos;
- ...sync these todos with external project management tool (e.g. Github
  Issues);
