//! Hand-built [`GraphLayout`] fixture — the renderer's own data source.
//!
//! Lane b1 must be testable **without** lane a1's `layout()` (W1 lanes run in
//! parallel). This module builds a small, deterministic graph by hand that
//! exercises every renderer feature:
//!
//! ```text
//! row 0   ┄        uncommitted stub (drawn dashed + grey)
//! row 1   ◯        HEAD open circle
//! row 2   ●─╮      merge: rounded corner down to lane 1
//! row 3   │ │      feature branch
//! row 4   │ ◎      stash (nested circle)
//! row 5   │ │
//! row 6   ● ┘      diagonal to the second root (sharp corner)
//! row 7     ●      second root
//! ```
//!
//! It is used by the widget/table unit tests and by the app's `--mock` mode (the
//! pty smoke capture). W2 replaces `--mock` with a1's `layout::layout()`.

use crate::data::Commit;
use crate::layout::types::{GraphLayout, Lane, RowIdx, RowLayout, SegKind, Segment, VertexRef};

/// The frozen 12-entry palette (reference `src/config.ts:275`, see D8/D10).
pub const fn frozen_palette() -> [(u8, u8, u8); 12] {
    [
        (0x00, 0x85, 0xd9),
        (0xd9, 0x00, 0x8f),
        (0x00, 0xd9, 0x0a),
        (0xd9, 0x85, 0x00),
        (0xa3, 0x00, 0xd9),
        (0xff, 0x00, 0x00),
        (0x00, 0xd9, 0xcc),
        (0xe1, 0x38, 0xe8),
        (0x85, 0xd9, 0x00),
        (0xdc, 0x5b, 0x23),
        (0x6f, 0x24, 0xd6),
        (0xff, 0xcc, 0x00),
    ]
}

fn vertex(row: RowIdx, lane: Lane, colour: u8, is_head: bool, is_stash: bool) -> VertexRef {
    VertexRef {
        row,
        lane,
        colour_idx: colour,
        is_head,
        is_stash,
        on_branch: true,
    }
}

fn segment(from: RowIdx, col_from: Lane, col_to: Lane, kind: SegKind, colour: u8) -> Segment {
    Segment {
        row_from: from,
        row_to: from + 1,
        col_from,
        col_to,
        kind,
        colour_idx: colour,
        pers: 0,
    }
}

/// The hand-built demo layout (see the module docstring for the picture).
pub fn demo_layout() -> GraphLayout {
    let vertical =
        |from: RowIdx, lane: Lane, colour: u8| segment(from, lane, lane, SegKind::Vertical, colour);

    let rows = vec![
        RowLayout {
            row: 0,
            vertex: Some(vertex(0, 0, 0, false, false)),
            segments: vec![vertical(0, 0, 0)],
        },
        RowLayout {
            row: 1,
            vertex: Some(vertex(1, 0, 0, true, false)),
            segments: vec![vertical(1, 0, 0)],
        },
        RowLayout {
            row: 2,
            vertex: Some(vertex(2, 0, 0, false, false)),
            segments: vec![
                vertical(2, 0, 0),
                segment(2, 0, 1, SegKind::RoundedCorner(0), 0),
            ],
        },
        RowLayout {
            row: 3,
            vertex: Some(vertex(3, 1, 3, false, false)),
            segments: vec![vertical(3, 0, 0), vertical(3, 1, 3)],
        },
        RowLayout {
            row: 4,
            vertex: Some(vertex(4, 1, 3, false, true)),
            segments: vec![vertical(4, 0, 0), vertical(4, 1, 3)],
        },
        RowLayout {
            row: 5,
            vertex: Some(vertex(5, 0, 0, false, false)),
            segments: vec![vertical(5, 0, 0)],
        },
        RowLayout {
            row: 6,
            vertex: Some(vertex(6, 0, 0, false, false)),
            segments: vec![segment(6, 0, 1, SegKind::Diagonal, 1)],
        },
        RowLayout {
            row: 7,
            vertex: Some(vertex(7, 1, 1, false, false)),
            segments: Vec::new(),
        },
    ];

    GraphLayout {
        rows,
        max_lane: 1,
        palette: frozen_palette(),
    }
}

/// Commits matching [`demo_layout`] row for row, so the `--mock` screenshot
/// shows a coherent graph **and** matching text columns (no real repo needed).
pub fn demo_commits() -> Vec<Commit> {
    let commit = |abbrev: &str, date: i64, subject: &str, refs: &[&str]| Commit {
        hash: abbrev.to_string(),
        abbrev: abbrev.to_string(),
        parents: Vec::new(),
        author: "Ada Lovelace".to_string(),
        date,
        subject: subject.to_string(),
        refs: refs.iter().map(|r| r.to_string()).collect(),
    };

    vec![
        commit("-------", 1_700_000_000, "Uncommitted changes", &[]),
        commit(
            "a1b2c3d",
            1_699_999_000,
            "Add graph renderer widget",
            &["HEAD -> main"],
        ),
        commit(
            "b2c3d4e",
            1_699_998_000,
            "Merge branch 'feature/graph'",
            &[],
        ),
        commit("c3d4e5f", 1_699_997_000, "Draw rounded corners", &[]),
        commit("d4e5f6a", 1_699_996_000, "WIP stash demo", &["stash@{0}"]),
        commit("e5f6a7b", 1_699_995_000, "Base commit", &["tag: v0.1"]),
        commit("f6a7b8c", 1_699_994_000, "Second root", &[]),
        commit("a7b8c9d", 1_699_993_000, "Orphan root", &[]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_layout_is_well_formed() {
        let layout = demo_layout();
        assert_eq!(layout.rows.len(), 8);
        assert_eq!(layout.max_lane, 1);
        assert_eq!(layout.palette, frozen_palette());
        for (i, row) in layout.rows.iter().enumerate() {
            assert_eq!(row.row, i as RowIdx, "rows are dense and ordered");
            for seg in &row.segments {
                assert_eq!(seg.row_from, row.row);
                assert_eq!(seg.row_to, seg.row_from + 1, "adjacent rows only");
                assert!(seg.col_from <= layout.max_lane);
                assert!(seg.col_to <= layout.max_lane);
            }
        }
    }

    #[test]
    fn demo_covers_every_special_vertex() {
        let layout = demo_layout();
        let head = layout
            .rows
            .iter()
            .filter_map(|r| r.vertex)
            .any(|v| v.is_head);
        let stash = layout
            .rows
            .iter()
            .filter_map(|r| r.vertex)
            .any(|v| v.is_stash);
        assert!(head, "demo must show a HEAD vertex");
        assert!(stash, "demo must show a stash vertex");
    }

    #[test]
    fn palette_is_the_frozen_twelve() {
        let palette = frozen_palette();
        assert_eq!(palette.len(), 12);
        assert_eq!(palette[0], (0x00, 0x85, 0xd9));
        assert_eq!(palette[11], (0xff, 0xcc, 0x00));
    }

    #[test]
    fn demo_commits_line_up_with_the_layout_rows() {
        let layout = demo_layout();
        let commits = demo_commits();
        assert_eq!(
            commits.len(),
            layout.rows.len(),
            "one text row per graph row"
        );
        assert_eq!(commits[1].refs, vec!["HEAD -> main".to_string()]);
        assert!(commits[4].refs.iter().any(|r| r.starts_with("stash@")));
    }
}
