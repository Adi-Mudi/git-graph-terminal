#!/usr/bin/env node
/**
 * G8 golden-fixture dumper — P0 T3 seed of the D2 parity gate.
 *
 * Executes the **reference's own** layout implementation on fixture repositories:
 *   fixture repo -> reference `GitCommit[]` model (git shell-out, `src/dataSource.ts` parity)
 *                 -> reference `Graph.loadCommits()` in jsdom (web/graph.ts, unmodified)
 *                 -> `tests/fixtures/golden/<fixture>.json`
 *
 * The reference checkout is only ever read. No file is written there.
 *
 * Usage (from `tools/g8-dump/`):
 *   node g8-dump.mjs --fixture linear
 *   node g8-dump.mjs --fixture branch-merge-rebase --stdout
 *   node g8-dump.mjs --all
 *
 * Options:
 *   --fixture <name>   fixture under tests/fixtures/repos/<name> (repeatable, or a
 *                      truncated-window fixture name — see TRUNCATED_WINDOWS)
 *   --all              every directory in tests/fixtures/repos + every truncated fixture
 *   --stdout           print the JSON instead of writing it
 *   --out <dir>        output directory (default <repo>/tests/fixtures/golden)
 *   --repos <dir>      fixture repository directory (default <repo>/tests/fixtures/repos)
 *   --check            do not write; fail if the on-disk golden file differs
 *   --reference <dir>  reference checkout (default: env G8_REFERENCE_REPO or ../../../vscode-git-graph)
 *   --help             print this usage text
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { loadReferenceGraph, runReferenceLayout } from './lib/reference-graph.mjs';
import { readRepositoryCommits } from './lib/git-commits.mjs';
import { buildFixture } from './lib/extract-layout.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
/** Worktree root (`tools/g8-dump/` -> up two levels). */
const PROJECT_ROOT = path.resolve(HERE, '..', '..');

/**
 * Fixtures that are an existing repository loaded through a **smaller commit window**.
 *
 * The reference's `getCommits()` asks git for `maxCommits + 1` commits and drops the
 * extra one (`src/dataSource.ts:167-172`), which is how the graph view pages history. A
 * repository dumped with a window smaller than its history therefore leaves the commit
 * at the boundary with a parent outside the loaded graph — `parents: [-1]`
 * (`NULL_VERTEX_ID`, `web/graph.ts:2`).
 *
 * That boundary row is the **only reachable** form of the frozen `lane: -1` /
 * `on_branch: false` sentinel: `loadCommits()` calls `determinePath()` for every vertex
 * that has a next parent or is not yet on a branch (`web/graph.ts:427-434`), and
 * `determinePath()`'s normal path always calls `addToBranch()` (`web/graph.ts:737`), so
 * every vertex ends up on a branch. `Vertex.draw()`'s `onBranch === null` early return
 * (`web/graph.ts:301`) is dead code for any commit list this harness can feed.
 *
 * See `src/layout/mod.rs` (reachability proof) and `tests/fixtures/golden/README.md`.
 * `branch-merge-rebase` has 9 commits; a window of 6 puts rows 1 and 5 (whose parents
 * are commit 8) at the boundary.
 */
const TRUNCATED_WINDOWS = {
  'branch-merge-rebase-truncated': { repo: 'branch-merge-rebase', maxCommits: 6 },
};

/**
 * Minimal flag parser.
 *
 * @param {string[]} argv process arguments after node + script
 * @returns {{fixtures: string[], all: boolean, stdout: boolean, check: boolean, out: string|null, repos: string|null, reference: string|null}}
 */
function parseArgs(argv) {
  const options = {
    fixtures: [],
    all: false,
    stdout: false,
    check: false,
    out: null,
    repos: null,
    reference: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--all') options.all = true;
    else if (arg === '--stdout') options.stdout = true;
    else if (arg === '--check') options.check = true;
    else if (arg === '--fixture') options.fixtures.push(argv[++i]);
    else if (arg === '--out') options.out = argv[++i];
    else if (arg === '--repos') options.repos = argv[++i];
    else if (arg === '--reference') options.reference = argv[++i];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return options;
}

/**
 * Resolve the fixture repos to dump.
 *
 * @param {string} reposDir directory holding the fixture repositories
 * @param {object} options parsed CLI options
 * @returns {string[]} fixture names
 */
function resolveFixtureNames(reposDir, options) {
  if (options.all || options.fixtures.length === 0) {
    if (!fs.existsSync(reposDir)) {
      throw new Error(
        `no fixture repositories at ${reposDir} — run \`npm run fixtures\` first`,
      );
    }
    const repos = fs
      .readdirSync(reposDir, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
    if (options.fixtures.length === 0 && repos.length === 0) {
      throw new Error(`no fixture repositories at ${reposDir} — run \`npm run fixtures\` first`);
    }
    return [...repos, ...Object.keys(TRUNCATED_WINDOWS)].sort();
  }
  return options.fixtures;
}

/**
 * Dump one fixture.
 *
 * @param {string} name fixture name (a repository directory, or a key of
 *   {@link TRUNCATED_WINDOWS})
 * @param {object} context resolved paths and the loaded reference module
 * @returns {object} the fixture object
 */
function dumpFixture(name, context) {
  const variant = TRUNCATED_WINDOWS[name];
  const repoName = variant !== undefined ? variant.repo : name;
  const maxCommits = variant !== undefined ? variant.maxCommits : undefined;
  const repoPath = path.join(context.reposDir, repoName);
  if (!fs.existsSync(path.join(repoPath, '.git'))) {
    throw new Error(`missing fixture repository: ${repoPath} — run \`npm run fixtures\``);
  }

  const { commits, commitHead, commitLookup } = readRepositoryCommits(repoPath, maxCommits);
  if (commits.length === 0) {
    throw new Error(`fixture repository has no commits: ${repoPath}`);
  }
  const graph = runReferenceLayout(context.reference, commits, commitHead, commitLookup);

  return buildFixture({
    name,
    // Fixed, checkout-independent label: the golden file must not embed machine paths.
    repoPathLabel: `tests/fixtures/repos/${repoName}`,
    commits,
    commitHead,
    commitLookup,
    graph,
    referenceRepo: context.referenceRepo,
    referenceSourcePath: context.reference.sourcePath,
  });
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8').split('*/')[0].replace(/^#!.*\n/, ''));
    return;
  }
  const referenceRepo = path.resolve(
    options.reference ?? process.env.G8_REFERENCE_REPO ?? path.join(PROJECT_ROOT, '..', 'vscode-git-graph'),
  );
  const reference = loadReferenceGraph(referenceRepo);
  const context = {
    reference,
    referenceRepo,
    reposDir: options.repos
      ? path.resolve(options.repos)
      : path.join(PROJECT_ROOT, 'tests', 'fixtures', 'repos'),
  };
  const outDir = options.out
    ? path.resolve(options.out)
    : path.join(PROJECT_ROOT, 'tests', 'fixtures', 'golden');

  const names = resolveFixtureNames(context.reposDir, options);
  if (!options.stdout) fs.mkdirSync(outDir, { recursive: true });

  let failures = 0;
  for (const name of names) {
    const fixture = dumpFixture(name, context);
    const json = `${JSON.stringify(fixture, null, 2)}\n`;
    const target = path.join(outDir, `${name}.json`);

    if (options.stdout) {
      process.stdout.write(json);
      continue;
    }
    if (options.check) {
      const existing = fs.existsSync(target) ? fs.readFileSync(target, 'utf8') : null;
      if (existing !== json) {
        failures++;
        console.error(`FAIL ${name}: ${target} differs from a fresh dump`);
      } else {
        console.log(`ok   ${name}: matches ${path.relative(PROJECT_ROOT, target)}`);
      }
      continue;
    }
    fs.writeFileSync(target, json, 'utf8');
    console.log(
      `wrote ${path.relative(PROJECT_ROOT, target)} (${fixture.rows.length} rows, ${fixture.branches.length} branches)`,
    );
  }

  if (options.check && failures > 0) process.exitCode = 1;
}

main();
