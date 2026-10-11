/**
 * Loads the reference Git Graph webview layout (`web/graph.ts` of
 * `mhutchie/vscode-git-graph`) into Node so its own `Graph` class can be executed
 * headlessly — no browser, no VS Code, no compiled/shipped JS.
 *
 * How it works:
 *  1. `web/graph.ts` is a global script (`module: none` in its tsconfig), so it is
 *     transpiled in memory with `ts.transpileModule` and then given an export shim
 *     (`module.exports = { Graph, Vertex, Branch }`) inside a `new Function` scope.
 *  2. The reference `Graph` constructor touches the DOM (`document.getElementById`,
 *     `createElementNS`, `appendChild`, `setAttribute`), so a minimal jsdom document
 *     is provided. No rendering method (`render()`, `draw()`) is ever called; only
 *     `loadCommits()` runs, which is pure layout logic.
 *  3. The few runtime globals the reference file expects are injected:
 *     `document`, `GG.GraphStyle`, `GG.GraphUncommittedChangesStyle`, `UNCOMMITTED`
 *     and `SVG_NAMESPACE`. Their values are copied from the reference sources
 *     (see the constants below) — nothing is invented.
 *
 * Nothing in the reference repository is written to; its files are only read.
 */
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import { JSDOM } from 'jsdom';

/** Default 12-entry palette — `vscode-git-graph/src/config.ts:275-277`. */
export const DEFAULT_COLOURS = [
  '#0085d9',
  '#d9008f',
  '#00d90a',
  '#d98500',
  '#a300d9',
  '#ff0000',
  '#00d9cc',
  '#e138e8',
  '#85d900',
  '#dc5b23',
  '#6f24d6',
  '#ffcc00',
];

/** `GG.GraphStyle` — `vscode-git-graph/src/types.ts:517-520` (order defines the values). */
export const GRAPH_STYLE = { Rounded: 0, Angular: 1 };

/** `GG.GraphUncommittedChangesStyle` — `src/types.ts:522-525`. */
export const GRAPH_UNCOMMITTED_CHANGES_STYLE = {
  OpenCircleAtTheUncommittedChanges: 0,
  OpenCircleAtTheCheckedOutCommit: 1,
};

/** `SVG_NAMESPACE` — `web/utils.ts:65`. */
const SVG_NAMESPACE = 'http://www.w3.org/2000/svg';

/** `UNCOMMITTED` — `web/utils.ts:66`. */
const UNCOMMITTED = '*';

/**
 * The graph config the extension uses by default — `src/config.ts:283`
 * (`grid`), `:275-277` (colours), `:278-280` (style), `:281-282` (uncommitted).
 */
export function defaultGraphConfig() {
  return {
    colours: DEFAULT_COLOURS,
    style: GRAPH_STYLE.Rounded,
    grid: { x: 16, y: 24, offsetX: 16, offsetY: 12, expandY: 250 },
    uncommittedChanges:
      GRAPH_UNCOMMITTED_CHANGES_STYLE.OpenCircleAtTheUncommittedChanges,
  };
}

/**
 * Transpile `web/graph.ts` and instantiate its `Graph` class against a jsdom document.
 *
 * @param {string} referenceRepo path to the read-only `vscode-git-graph` checkout
 * @returns {{ Graph: Function, Vertex: Function, Branch: Function, document: object, sourcePath: string }}
 */
export function loadReferenceGraph(referenceRepo) {
  const sourcePath = path.join(referenceRepo, 'web', 'graph.ts');
  const source = fs.readFileSync(sourcePath, 'utf8');

  const transpiled = ts.transpileModule(source, {
    fileName: 'graph.ts',
    compilerOptions: {
      target: ts.ScriptTarget.ES2019,
      module: ts.ModuleKind.CommonJS,
      removeComments: true,
    },
  }).outputText;

  const dom = new JSDOM(
    '<!doctype html><html><body><div id="graphContent"><div id="graph"></div></div></body></html>',
  );
  const document = dom.window.document;

  const GG = {
    GraphStyle: GRAPH_STYLE,
    GraphUncommittedChangesStyle: GRAPH_UNCOMMITTED_CHANGES_STYLE,
  };

  const moduleShim = { exports: {} };
  const factory = new Function(
    'module',
    'exports',
    'require',
    'document',
    'GG',
    'UNCOMMITTED',
    'SVG_NAMESPACE',
    `${transpiled}\nmodule.exports = { Graph, Vertex, Branch, NULL_VERTEX_ID };`,
  );
  factory(
    moduleShim,
    moduleShim.exports,
    (name) => {
      throw new Error(`unexpected require("${name}") in reference graph.ts`);
    },
    document,
    GG,
    UNCOMMITTED,
    SVG_NAMESPACE,
  );

  return { ...moduleShim.exports, document, sourcePath };
}

/**
 * Run the reference layout on a commit list.
 *
 * @param {object} reference result of {@link loadReferenceGraph}
 * @param {Array<object>} commits `GitCommit`-shaped objects, newest first
 * @param {string|null} commitHead hash of HEAD, or null
 * @param {{[hash: string]: number}} commitLookup hash -> index
 * @param {object} [config] graph config (defaults to {@link defaultGraphConfig})
 * @returns {object} the live reference `Graph` instance (its private fields are
 *   readable at runtime — TypeScript `private` is compile-time only)
 */
export function runReferenceLayout(
  reference,
  commits,
  commitHead,
  commitLookup,
  config = defaultGraphConfig(),
) {
  const graph = new reference.Graph('graph', reference.document.body, config, {});
  graph.loadCommits(commits, commitHead, commitLookup, false);
  return graph;
}
