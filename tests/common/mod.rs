//! Shared golden-JSON parsing and parity assertions for the test crates.
//!
//! `golden_layout.rs` uses this to pin [`layout()`](crate::layout::layout) alone
//! against the reference goldens; `integration.rs` uses the same helpers to pin the
//! whole `data::commits → layout::layout()` path end to end.
//!
//! The expected [`Segment`] values are re-derived **only** from the golden's own
//! geometry and `lockedFirst`, so the tests cannot simply re-assert what the port did.
//!
//! This module is pulled into each integration-test crate with `mod common;`, so it
//! refers to the sibling `data`/`layout` modules through `crate::` — every test that
//! includes it must also declare those two `#[path]` modules.

#![allow(dead_code)] // not every helper is used by every test crate.

use std::fs;
use std::path::Path;

use crate::data::Commit;
use crate::layout::{parent_rows, GraphLayout, RowIdx, SegKind, Segment, PALETTE};

/// Placeholder hash for a golden `parents: [-1]` entry. `layout()` looks it up,
/// fails, and links the parent to `NULL_VERTEX_ID` exactly like the reference.
pub const NULL_PARENT: &str = "<out-of-range-parent>";

/// Read and parse one golden fixture file.
pub fn read_golden(path: &Path) -> Json {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    parse(&text)
}

/// Parse golden JSON text.
pub fn parse(text: &str) -> Json {
    Parser::new(text).parse()
}

/// The golden fields the parity assertions consume.
pub fn golden_from(json: &Json) -> Golden {
    Golden::from_json(json)
}

/// Rebuild the commit list the golden repo produced, newest first.
pub fn build_commits(json: &Json) -> Vec<Commit> {
    let repo = json.get("repo");
    let hashes: Vec<String> = repo
        .get("commitHashes")
        .as_array()
        .iter()
        .map(|hash| hash.as_str().to_string())
        .collect();
    let commit_head = match repo.get("commitHead") {
        Json::Str(hash) => Some(hash.clone()),
        _ => None,
    };
    let rows = json.get("rows").as_array();

    rows.iter()
        .enumerate()
        .map(|(row, golden_row)| {
            let parents = golden_row
                .get("parents")
                .as_array()
                .iter()
                .map(|parent| match parent.as_i64() {
                    index if index < 0 => NULL_PARENT.to_string(),
                    index => hashes[index as usize].clone(),
                })
                .collect();
            Commit {
                hash: hashes[row].clone(),
                abbrev: hashes[row].chars().take(7).collect(),
                parents,
                author: "fixture".to_string(),
                date: 0,
                subject: format!("row {row}"),
                refs: if commit_head.as_deref() == Some(hashes[row].as_str()) {
                    vec!["HEAD -> main".to_string()]
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

/// Assert exact parity between `graph` (laid out from `commits`) and `golden`.
///
/// Returns how many `parents: [-1]` (`NULL_VERTEX_ID`) entries the fixture contains,
/// so the caller can prove the reachable sentinel is actually exercised.
pub fn assert_layout_matches(
    name: &str,
    graph: &GraphLayout,
    commits: &[Commit],
    golden: &Golden,
) -> usize {
    let parents = parent_rows(commits);

    assert_eq!(
        graph.rows.len(),
        golden.commit_hashes.len(),
        "{name}: row count ({} rows expected)",
        golden.commit_hashes.len()
    );

    // D4 canary: the frozen `lane: -1` / `colour: -1` sentinel is unreachable for any
    // commit list this harness can feed (proof in `src/layout/mod.rs`; confirmed by
    // running the reference's own `loadCommits()`). If a golden ever carries it, this
    // fails loudly so the proof can be revisited instead of silently diverging.
    for (row, lane) in golden.lane.iter().enumerate() {
        assert!(
            *lane >= 0,
            "{name}: row {row} has lane -1, but the reference never leaves a vertex off-branch"
        );
        assert!(
            golden.colour[row] >= 0,
            "{name}: row {row} has colour -1, but the reference never leaves a vertex off-branch"
        );
    }
    for (row, on_branch) in golden.on_branch.iter().enumerate() {
        assert!(
            *on_branch,
            "{name}: row {row} has onBranch=false, but the reference never leaves a vertex off-branch"
        );
    }

    // Parent links (the contract's GraphLayout cannot carry them, so assert them
    // through the additive `parent_rows` helper).
    let expected_parents: Vec<Vec<Option<RowIdx>>> = golden
        .row_parents
        .iter()
        .map(|row| {
            row.iter()
                .map(|parent| (*parent >= 0).then_some(*parent as RowIdx))
                .collect()
        })
        .collect();
    assert_eq!(parents, expected_parents, "{name}: parents per row");

    for (row, layout_row) in graph.rows.iter().enumerate() {
        let vertex = layout_row
            .vertex
            .unwrap_or_else(|| panic!("{name}: row {row} must have a vertex"));
        assert_eq!(
            vertex.lane as i64, golden.lane[row],
            "{name}: row {row} lane"
        );
        assert_eq!(
            vertex.colour_idx as i64, golden.colour[row],
            "{name}: row {row} colour"
        );
        assert_eq!(
            vertex.is_head, golden.is_current[row],
            "{name}: row {row} isCurrent"
        );
        assert_eq!(
            vertex.on_branch, golden.on_branch[row],
            "{name}: row {row} onBranch"
        );
        assert_eq!(
            layout_row.row, row as RowIdx,
            "{name}: row {row} RowLayout.row"
        );

        let expected = expected_segments(golden, row);
        assert_eq!(
            layout_row.segments, expected,
            "{name}: row {row} segments (golden geometry vs port)"
        );
        // `rows[].lockedFirst` is the golden's row projection of `Line.lockedFirst`
        // (any outgoing line locked to p1). The port has no such field, so the
        // value is covered through `pers` above; here we only check the golden's own
        // schema is self-consistent with the line data we asserted against.
        let outgoing_locked_first = golden
            .branches
            .iter()
            .flat_map(|branch| branch.lines.iter())
            .any(|line| line.y1 == row as i64 && line.locked_first);
        assert_eq!(
            outgoing_locked_first, golden.locked_first[row],
            "{name}: row {row} rows[].lockedFirst vs branches[].lines"
        );
    }

    // `max_lane` = highest column in use, over vertices and both segment ends.
    let mut expected_max_lane = 0i64;
    for (row, layout_row) in graph.rows.iter().enumerate() {
        expected_max_lane = expected_max_lane.max(golden.lane[row]);
        for segment in &layout_row.segments {
            expected_max_lane = expected_max_lane.max(segment.col_from as i64);
            expected_max_lane = expected_max_lane.max(segment.col_to as i64);
            expected_max_lane = expected_max_lane.max(segment.pers as i64);
        }
    }
    assert_eq!(graph.max_lane as i64, expected_max_lane, "{name}: max_lane");
    assert_eq!(graph.palette, PALETTE, "{name}: palette");

    golden
        .row_parents
        .iter()
        .flatten()
        .filter(|parent| **parent < 0)
        .count()
}

/// Segments the golden says must start on `row`, in branch order.
fn expected_segments(golden: &Golden, row: usize) -> Vec<Segment> {
    let mut segments = Vec::new();
    for branch in &golden.branches {
        for line in &branch.lines {
            if line.y1 != row as i64 {
                continue;
            }
            assert_eq!(
                line.y2,
                line.y1 + 1,
                "golden invariant: segments span adjacent rows"
            );
            let pers = if line.locked_first { line.x1 } else { line.x2 };
            segments.push(Segment {
                row_from: line.y1 as RowIdx,
                row_to: line.y2 as RowIdx,
                col_from: line.x1 as u8,
                col_to: line.x2 as u8,
                kind: expected_kind(line.x1, line.x2),
                colour_idx: branch.colour as u8,
                pers: pers as u8,
            });
        }
    }
    segments
}

/// The documented geometry -> [`SegKind`] mapping, derived from the golden `x`s only.
fn expected_kind(x1: i64, x2: i64) -> SegKind {
    if x1 == x2 {
        SegKind::Vertical
    } else if (x1 - x2).abs() == 1 {
        SegKind::RoundedCorner(u8::from(x2 < x1))
    } else {
        SegKind::Horizontal
    }
}

/// The golden fields the parity assertions consume.
pub struct Golden {
    pub lane: Vec<i64>,
    pub colour: Vec<i64>,
    pub row_parents: Vec<Vec<i64>>,
    pub locked_first: Vec<bool>,
    pub is_current: Vec<bool>,
    pub on_branch: Vec<bool>,
    pub commit_hashes: Vec<String>,
    pub branches: Vec<GoldenBranch>,
}

impl Golden {
    fn from_json(json: &Json) -> Self {
        let rows = json.get("rows").as_array();
        let nodes = json.get("rowNodes").as_array();
        let repo = json.get("repo");
        let branches = json
            .get("branches")
            .as_array()
            .iter()
            .map(|branch| GoldenBranch {
                colour: branch.get("colour").as_i64(),
                lines: branch
                    .get("lines")
                    .as_array()
                    .iter()
                    .map(|line| GoldenLine {
                        x1: line.get("p1").get("x").as_i64(),
                        y1: line.get("p1").get("y").as_i64(),
                        x2: line.get("p2").get("x").as_i64(),
                        y2: line.get("p2").get("y").as_i64(),
                        locked_first: line.get("lockedFirst").as_bool(),
                    })
                    .collect(),
            })
            .collect();

        Self {
            lane: rows.iter().map(|row| row.get("lane").as_i64()).collect(),
            colour: rows.iter().map(|row| row.get("colour").as_i64()).collect(),
            row_parents: rows
                .iter()
                .map(|row| {
                    row.get("parents")
                        .as_array()
                        .iter()
                        .map(Json::as_i64)
                        .collect()
                })
                .collect(),
            locked_first: rows
                .iter()
                .map(|row| row.get("lockedFirst").as_bool())
                .collect(),
            is_current: nodes
                .iter()
                .map(|node| node.get("isCurrent").as_bool())
                .collect(),
            on_branch: nodes
                .iter()
                .map(|node| node.get("onBranch").as_bool())
                .collect(),
            commit_hashes: repo
                .get("commitHashes")
                .as_array()
                .iter()
                .map(|hash| hash.as_str().to_string())
                .collect(),
            branches,
        }
    }
}

pub struct GoldenBranch {
    pub colour: i64,
    pub lines: Vec<GoldenLine>,
}

pub struct GoldenLine {
    pub x1: i64,
    pub y1: i64,
    pub x2: i64,
    pub y2: i64,
    pub locked_first: bool,
}

/* ------------------------------------------------------------------------- */
/* Minimal dependency-free JSON parser (the goldens are JSON.stringify output) */
/* ------------------------------------------------------------------------- */

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

const JSON_NULL: Json = Json::Null;

impl Json {
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(entries) => entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| value)
                .unwrap_or(&JSON_NULL),
            _ => &JSON_NULL,
        }
    }

    pub fn as_array(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            other => panic!("expected array, got {other:?}"),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Json::Str(text) => text,
            other => panic!("expected string, got {other:?}"),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Json::Bool(value) => *value,
            other => panic!("expected bool, got {other:?}"),
        }
    }

    pub fn as_i64(&self) -> i64 {
        match self {
            Json::Num(value) => *value as i64,
            other => panic!("expected number, got {other:?}"),
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn parse(&mut self) -> Json {
        self.skip_whitespace();
        let value = self.value();
        self.skip_whitespace();
        assert_eq!(self.pos, self.bytes.len(), "trailing data in JSON");
        value
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> u8 {
        self.bytes.get(self.pos).copied().unwrap_or(0)
    }

    fn value(&mut self) -> Json {
        match self.peek() {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Json::Str(self.string()),
            b't' => {
                self.expect("true");
                Json::Bool(true)
            }
            b'f' => {
                self.expect("false");
                Json::Bool(false)
            }
            b'n' => {
                self.expect("null");
                Json::Null
            }
            _ => self.number(),
        }
    }

    fn expect(&mut self, literal: &str) {
        let end = self.pos + literal.len();
        assert_eq!(
            std::str::from_utf8(&self.bytes[self.pos..end]),
            Ok(literal),
            "expected `{literal}`"
        );
        self.pos = end;
    }

    fn object(&mut self) -> Json {
        self.pos += 1;
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == b'}' {
            self.pos += 1;
            return Json::Obj(entries);
        }
        loop {
            self.skip_whitespace();
            let key = self.string();
            self.skip_whitespace();
            assert_eq!(self.peek(), b':', "expected `:`");
            self.pos += 1;
            self.skip_whitespace();
            entries.push((key, self.value()));
            self.skip_whitespace();
            match self.peek() {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    break;
                }
                other => panic!("unexpected byte {other} in object"),
            }
        }
        Json::Obj(entries)
    }

    fn array(&mut self) -> Json {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == b']' {
            self.pos += 1;
            return Json::Arr(items);
        }
        loop {
            self.skip_whitespace();
            items.push(self.value());
            self.skip_whitespace();
            match self.peek() {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    break;
                }
                other => panic!("unexpected byte {other} in array"),
            }
        }
        Json::Arr(items)
    }

    fn string(&mut self) -> String {
        assert_eq!(self.peek(), b'"', "expected string");
        self.pos += 1;
        let mut out = String::new();
        loop {
            let byte = self.bytes[self.pos];
            self.pos += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escape = self.bytes[self.pos];
                    self.pos += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'u' => {
                            let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
                                .expect("unicode escape");
                            self.pos += 4;
                            let code = u32::from_str_radix(hex, 16).expect("unicode escape");
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => panic!("bad string escape {other}"),
                    }
                }
                other if other.is_ascii() => out.push(other as char),
                other => panic!("non-ASCII byte {other} in JSON string"),
            }
        }
        out
    }

    fn number(&mut self) -> Json {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && matches!(
                self.bytes[self.pos],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
            )
        {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).expect("number utf8");
        Json::Num(text.parse::<f64>().expect("valid JSON number"))
    }
}
