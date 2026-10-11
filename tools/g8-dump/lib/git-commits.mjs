/**
 * Builds the reference extension's `GitCommit[]` model for a fixture repository by
 * running exactly the git commands the extension runs, then parsing them the same way
 * (`vscode-git-graph/src/dataSource.ts` private `getLog()` `:1489-1560` and `getRefs()`
 * `:1562-1595`).
 *
 * Mirrored details (no invented flags):
 *  - `-c log.showSignature=false`                     (dataSource.ts:1500)
 *  - `log --max-count=N+1 --format=<...> --date-order`(dataSource.ts:1500)
 *  - `--branches --tags HEAD --`                      ("show all", dataSource.ts:1511-1526)
 *  - field separator `GIT_LOG_SEPARATOR`              (dataSource.ts:18)
 *  - stashes are not modelled in P0 (`stash: null`), uncommitted-changes row omitted
 *
 * Only `hash`, `parents` and `stash` are consumed by `Graph.loadCommits()`
 * (`web/graph.ts:393-437`); the label fields are filled in for fidelity and later waves.
 */
import { execFileSync } from 'node:child_process';

/** `GIT_LOG_SEPARATOR` — `vscode-git-graph/src/dataSource.ts:18`. */
const GIT_LOG_SEPARATOR = 'XX7Nal-YARtTpjCikii9nJxER19D6diSyk-AWkPb';

/** Maximum commits requested (`maxCommits` used by the extension for the graph view). */
const MAX_COMMITS = 1000;

/**
 * Run `git` in `repo` and return stdout.
 *
 * @param {string} repo repository path
 * @param {string[]} args git arguments
 * @returns {string} stdout
 */
function git(repo, args) {
  return execFileSync('git', ['-C', repo, ...args], {
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
    env: { ...process.env, GIT_CONFIG_NOSYSTEM: '1' },
  });
}

/**
 * Raw commit records, newest first — the port of `getLog()`.
 *
 * `maxCommits` mirrors the extension's `getCommits()` window: git is asked for
 * `maxCommits + 1` commits and the extra one is dropped (`src/dataSource.ts:167-172`),
 * exactly as the graph view does when it pages commits. Dumping a repository through a
 * window smaller than its history therefore leaves the commit(s) at the window boundary
 * with a parent outside the loaded graph — `parents: [-1]` (`NULL_VERTEX_ID`).
 *
 * @param {string} repo repository path
 * @param {number} [maxCommits] commit window (default {@link MAX_COMMITS})
 * @returns {Array<{hash: string, parents: string[], author: string, email: string, date: number, message: string}>}
 */
export function getLog(repo, maxCommits = MAX_COMMITS) {
  // `dateType` default is the author date ("%at") — src/config.ts `get dateType`.
  const format = ['%H', '%P', '%an', '%ae', '%at', '%s'].join(GIT_LOG_SEPARATOR);
  const out = git(repo, [
    '-c',
    'log.showSignature=false',
    'log',
    `--max-count=${maxCommits + 1}`,
    `--format=${format}`,
    '--date-order',
    '--branches',
    '--tags',
    'HEAD',
    '--',
  ]);

  const commits = [];
  for (const line of out.split(/\r?\n/)) {
    if (line === '') continue;
    const fields = line.split(GIT_LOG_SEPARATOR);
    if (fields.length !== 6) continue;
    commits.push({
      hash: fields[0],
      parents: fields[1] !== '' ? fields[1].split(' ') : [],
      author: fields[2],
      email: fields[3],
      date: Number.parseInt(fields[4], 10),
      message: fields[5],
    });
  }
  return commits.slice(0, maxCommits);
}

/**
 * Refs of the repository — the port of `getRefs()` (dataSource.ts:1562-1595).
 *
 * @param {string} repo repository path
 * @returns {{head: string|null, heads: {hash: string, name: string}[], tags: {hash: string, name: string, annotated: boolean}[], remotes: {hash: string, name: string}[]}}
 */
export function getRefs(repo) {
  const out = git(repo, ['show-ref', '-d', '--head']);
  const refData = { head: null, heads: [], tags: [], remotes: [] };

  for (const line of out.split(/\r?\n/)) {
    if (line === '') continue;
    const parts = line.split(' ');
    if (parts.length < 2) continue;
    const hash = parts.shift();
    const ref = parts.join(' ');

    if (ref.startsWith('refs/heads/')) {
      refData.heads.push({ hash, name: ref.substring(11) });
    } else if (ref.startsWith('refs/tags/')) {
      const annotated = ref.endsWith('^{}');
      refData.tags.push({
        hash,
        name: annotated ? ref.substring(10, ref.length - 3) : ref.substring(10),
        annotated,
      });
    } else if (ref.startsWith('refs/remotes/')) {
      if (!ref.endsWith('/HEAD')) {
        refData.remotes.push({ hash, name: ref.substring(13) });
      }
    } else if (ref === 'HEAD') {
      refData.head = hash;
    }
  }
  return refData;
}

/**
 * `GitCommit[]` + lookup table for a repository, ready for `Graph.loadCommits()`.
 *
 * @param {string} repo repository path
 * @param {number} [maxCommits] commit window (default {@link MAX_COMMITS})
 * @returns {{commits: Array<object>, commitHead: string|null, commitLookup: {[hash: string]: number}, refs: object}}
 */
export function readRepositoryCommits(repo, maxCommits = MAX_COMMITS) {
  const records = getLog(repo, maxCommits);
  const refs = getRefs(repo);

  /** @type {{[hash: string]: number}} */
  const commitLookup = {};
  const commits = records.map((record, index) => {
    commitLookup[record.hash] = index;
    return {
      ...record,
      heads: [],
      tags: [],
      remotes: [],
      stash: null,
    };
  });

  for (const head of refs.heads) {
    if (typeof commitLookup[head.hash] === 'number') {
      commits[commitLookup[head.hash]].heads.push(head.name);
    }
  }
  for (const tag of refs.tags) {
    if (typeof commitLookup[tag.hash] === 'number') {
      commits[commitLookup[tag.hash]].tags.push({
        name: tag.name,
        annotated: tag.annotated,
      });
    }
  }
  for (const remote of refs.remotes) {
    if (typeof commitLookup[remote.hash] === 'number') {
      commits[commitLookup[remote.hash]].remotes.push({
        name: remote.name,
        remote: null,
      });
    }
  }

  return { commits, commitHead: refs.head, commitLookup, refs };
}
