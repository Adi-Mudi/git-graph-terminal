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

## Status

- [x] Q1: project folder created, git initialized, initial commit
- [ ] Q2: ...
