/**
 * Projects the live reference `Graph` instance (after `loadCommits()`) into the G8
 * golden-fixture JSON contract from `p0-plan_v1.0.md` T3.
 *
 * Contract (binding, unchanged):
 *
 *   rows: [{ "row": int, "lane": int, "colour": int, "parents": [int], "lockedFirst": bool }]
 *
 * Field meanings, all taken from the reference's own state after `loadCommits()`:
 *  - `row`         vertex id == commit index (newest commit is row 0) — `Vertex.id`.
 *  - `lane`        graph grid column of the commit node — `Vertex.x`
 *                  (`web/graph.ts:245` `addToBranch(branch, x)`); `-1` when the
 *                  reference draws no node for that row (`onBranch === null`,
 *                  `Vertex.draw()` returns early at `web/graph.ts:301`).
 *  - `colour`      index into the 12-entry palette — `Vertex.getColour()` ->
 *                  `Branch.getColour()`; `-1` when the row has no branch.
 *                  NOTE: this is a palette index, not a column (D8 `§3.4.2`).
 *  - `parents`     parent vertex ids in `git` order; `-1` == `NULL_VERTEX_ID`, i.e.
 *                  the parent is outside the loaded graph (`web/graph.ts:2`).
 *  - `lockedFirst` row projection of the line state: `true` when any branch line
 *                  leaving this row (`line.p1.y === row`) is locked to `p1`
 *                  (`Line.lockedFirst`, `web/graph.ts:15`); `false` when the row has
 *                  no outgoing line (root commit / row without a branch start).
 *
 * Additive extras (documented in `tools/g8-dump/README.md`, do not change `rows`):
 *  - `rowNodes[]`  per-row extras: `isCurrent`, `isCommitted`, `isMerge`, `onBranch`.
 *  - `branches[]`  full branch state: `colour`, `end`, `numUncommitted`, and every
 *                  `line {p1, p2, lockedFirst}` in graph space. This is the exact
 *                  parity data W1 needs to assert its ported `determinePath()`.
 */
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

import { defaultGraphConfig } from './reference-graph.mjs';

/**
 * Build the golden fixture object for one repository.
 *
 * @param {object} params
 * @param {string} params.name fixture name (matches the repo directory name)
 * @param {string} params.repoPathLabel path of the fixture repo, relative to the project root
 *   (relative on purpose: the golden file must be identical on any checkout/machine)
 * @param {object} params.commits `GitCommit[]` (newest first)
 * @param {string|null} params.commitHead HEAD hash or null
 * @param {{[hash: string]: number}} params.commitLookup hash -> index
 * @param {object} params.graph the reference `Graph` instance after `loadCommits()`
 * @param {string} params.referenceRepo path of the read-only reference checkout
 * @param {string} params.referenceSourcePath path of the reference `web/graph.ts` that was executed
 * @returns {object} fixture object, key order is stable for clean diffs
 */
export function buildFixture({
  name,
  repoPathLabel,
  commits,
  commitHead,
  commitLookup,
  graph,
  referenceRepo,
  referenceSourcePath,
}) {
  const config = defaultGraphConfig();

  const rows = graph.vertices.map((vertex, row) => {
    const onBranch = vertex.onBranch !== null;
    const outgoing = onBranch
      ? graph.branches
          .flatMap((branch) => branch.lines)
          .filter((line) => line.p1.y === row)
      : [];
    return {
      row,
      lane: onBranch ? vertex.getPoint().x : -1,
      colour: onBranch ? vertex.getColour() : -1,
      parents: vertex.parents.map((parent) => parent.id),
      lockedFirst: outgoing.some((line) => line.lockedFirst),
    };
  });

  const rowNodes = graph.vertices.map((vertex, row) => ({
    row,
    isCurrent: vertex.isCurrent,
    isCommitted: vertex.isCommitted,
    isMerge: vertex.parents.length > 1,
    onBranch: vertex.onBranch !== null,
  }));

  const branches = graph.branches.map((branch) => ({
    colour: branch.getColour(),
    end: branch.end,
    numUncommitted: branch.numUncommitted,
    lines: branch.lines.map((line) => ({
      p1: { x: line.p1.x, y: line.p1.y },
      p2: { x: line.p2.x, y: line.p2.y },
      lockedFirst: line.lockedFirst,
    })),
  }));

  return {
    fixture: name,
    generatedBy: 'tools/g8-dump (P0 T3 seed)',
    reference: {
      repo: path.basename(referenceRepo),
      file: 'web/graph.ts',
      sha256: crypto.createHash('sha256').update(fs.readFileSync(referenceSourcePath)).digest('hex'),
      method: 'Graph.loadCommits() -> Graph.determinePath()',
    },
    config: {
      grid: config.grid,
      style: 'rounded',
      uncommittedChanges: 'OpenCircleAtTheUncommittedChanges',
      colours: config.colours,
    },
    repo: {
      name,
      path: repoPathLabel,
      commitHead,
      commitCount: commits.length,
      commitHashes: commits.map((commit) => commit.hash),
    },
    rows,
    rowNodes,
    branches,
  };
}
