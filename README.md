# git-graph-terminal

Reverse engineer the VS Code "Git Graph" extension (mhutchie/vscode-git-graph)
so its commit-graph experience works in the terminal.

## Goal

Recreate the Git Graph view — branch/merge topology, commit nodes, labels,
HEAD marker, context actions — as a terminal-first tool.

## Source of reference

- Cloned source: `../vscode-git-graph` (github.com/mhutchie/vscode-git-graph)
  - `web/graph.ts` — graph generation and rendering (Vertex, Branch, Graph classes)
  - `src/dataSource.ts` — executes git commands and parses output
  - `src/gitGraphView.ts` — webview ↔ backend message passing

## Decisions

- Q1 (language/stack): Rust + ratatui/crossterm. Chosen for the better final
  artifact (single static binary, tiny footprint, best-in-class TUI libs),
  accepting the cost of porting ~6,900 lines of reference TypeScript logic
  (web/graph.ts, src/dataSource.ts, web/main.ts) instead of copying it.
- Q1 (tests): fast unit/golden tests run locally; ALL visual/PTY end-to-end
  tests run remotely only — GitHub Actions CI (cargo test + snapshot tests
  against terminal emulators on GitHub runners). Local hardware not used for E2E.

## Status

- [x] Q1: project folder created, git initialized, initial commit
- [x] Q1: stack decided — Rust, tests split local-unit / remote-E2E (GitHub Actions)
- [ ] Q2: ...
