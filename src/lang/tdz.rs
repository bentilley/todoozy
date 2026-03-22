// TODO #62 (D) 2026-03-22 Implement .tdz file format for standalone TODOs +parser +tdz
//
// A plain text format for writing TODOs outside of source code. Useful when:
// - Writing TODOs for code that doesn't exist yet (e.g., `thing.py.tdz`)
// - Batching TODOs in one place (e.g., `next-sprint.tdz`)
//
// Format:
//   # TODO (A) Title here +project `@context` key:value
//
//   Description spans multiple lines and paragraphs until the next
//   `# TODO` or end of file.
//
//   # TODO (B) Another todo
//
//   Its description here.
//
// Rules:
// - `# TODO ...` starts a new TODO (markdown H1 style)
// - Full TODO syntax supported: priority, dates, projects, contexts, metadata
// - Description = everything until next `# TODO` or EOF
// - Trailing whitespace trimmed (extra blank lines between TODOs ignored)
// - `## TODO` / `### TODO` reserved for future sub-task support (not parsed yet)
//
// Location: Use filename as-is (e.g., file: "thing.py.tdz")
