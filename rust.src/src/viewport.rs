//! Focus viewport and vertical centering (RFC-0004).
//!
//! Given the buffer size, the active line, the available text height and the
//! desired context (`lines_before`/`lines_after`), [`layout`] computes which
//! buffer lines are visible and how each visual row should be styled. The
//! active line is centered vertically; only a limited context window around it
//! is shown, with everything else rendered as blank padding. Rows beyond the
//! file edges are padding too (clamp without reading out of bounds).
//!
//! [`layout`] is a pure function, so the caller simply recomputes it on resize.

/// How a visual row should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// The active (cursor) line, rendered brightest.
    Active,
    /// A context line around the active line, rendered dimmed.
    Context,
    /// Empty filler used for centering and at file edges.
    Padding,
}

/// A single visual row of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// The buffer line shown here, or `None` for padding rows.
    pub line: Option<usize>,
}

/// Computed viewport: one entry per visual text row plus the active row index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub rows: Vec<Row>,
    /// Index of the active line within `rows` (0 when there are no rows).
    pub active_row: usize,
}

impl Layout {
    /// Returns the inclusive range of visible buffer lines, if any are shown.
    pub fn visible_range(&self) -> Option<(usize, usize)> {
        let mut min = None;
        let mut max = None;
        for row in &self.rows {
            if let Some(line) = row.line {
                min = Some(min.map_or(line, |m: usize| m.min(line)));
                max = Some(max.map_or(line, |m: usize| m.max(line)));
            }
        }
        match (min, max) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    }
}

/// Computes the focus viewport.
///
/// * `line_count` — number of lines in the buffer (>= 1).
/// * `active_line` — zero-based index of the cursor line.
/// * `content_height` — number of text rows available (excludes the status line).
/// * `lines_before` / `lines_after` — desired context window around the active line.
///
/// The active line is placed at the vertical center. Visual rows within the
/// context window that map to real buffer lines are [`RowKind::Context`] (or
/// [`RowKind::Active`] for the cursor line); all other rows are
/// [`RowKind::Padding`]. When the terminal is too short to fit the full
/// context, centering naturally keeps the active line visible with as much
/// surrounding context as fits.
pub fn layout(
    line_count: usize,
    active_line: usize,
    content_height: usize,
    lines_before: usize,
    lines_after: usize,
) -> Layout {
    if content_height == 0 {
        return Layout {
            rows: Vec::new(),
            active_row: 0,
        };
    }

    let active_row = (content_height - 1) / 2;
    let mut rows = Vec::with_capacity(content_height);

    for r in 0..content_height {
        let offset = r as isize - active_row as isize;

        // Outside the configured context window -> blank padding.
        if offset < -(lines_before as isize) || offset > lines_after as isize {
            rows.push(Row {
                kind: RowKind::Padding,
                line: None,
            });
            continue;
        }

        let buffer_line = active_line as isize + offset;
        if buffer_line < 0 || buffer_line >= line_count as isize {
            rows.push(Row {
                kind: RowKind::Padding,
                line: None,
            });
            continue;
        }

        let kind = if offset == 0 {
            RowKind::Active
        } else {
            RowKind::Context
        };
        rows.push(Row {
            kind,
            line: Some(buffer_line as usize),
        });
    }

    Layout { rows, active_row }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collects the buffer-line index for each row (`None` for padding).
    fn lines(layout: &Layout) -> Vec<Option<usize>> {
        layout.rows.iter().map(|r| r.line).collect()
    }

    #[test]
    fn empty_height_yields_no_rows() {
        let l = layout(10, 5, 0, 2, 2);
        assert!(l.rows.is_empty());
        assert_eq!(l.active_row, 0);
        assert_eq!(l.visible_range(), None);
    }

    #[test]
    fn active_is_centered_with_context_window() {
        // height 7 -> active_row = 3; context window 2/2.
        let l = layout(100, 50, 7, 2, 2);
        assert_eq!(l.active_row, 3);
        assert_eq!(lines(&l), vec![None, Some(48), Some(49), Some(50), Some(51), Some(52), None]);
        assert_eq!(l.rows[3].kind, RowKind::Active);
        assert_eq!(l.rows[2].kind, RowKind::Context);
        assert_eq!(l.rows[0].kind, RowKind::Padding);
        assert_eq!(l.visible_range(), Some((48, 52)));
    }

    #[test]
    fn top_of_file_pads_above() {
        let l = layout(100, 0, 7, 2, 2);
        assert_eq!(l.active_row, 3);
        // Rows above the first line are padding; active stays centered.
        assert_eq!(lines(&l), vec![None, None, None, Some(0), Some(1), Some(2), None]);
        assert_eq!(l.rows[3].kind, RowKind::Active);
    }

    #[test]
    fn bottom_of_file_pads_below() {
        let l = layout(3, 2, 7, 2, 2);
        assert_eq!(lines(&l), vec![None, Some(0), Some(1), Some(2), None, None, None]);
        assert_eq!(l.rows[3].kind, RowKind::Active);
    }

    #[test]
    fn window_smaller_than_height_pads_outside_context() {
        // Tall terminal, narrow context: only the window is shown, rest padding.
        let l = layout(100, 50, 11, 1, 1);
        assert_eq!(l.active_row, 5);
        let expected = vec![
            None, None, None, None,
            Some(49), Some(50), Some(51),
            None, None, None, None,
        ];
        assert_eq!(lines(&l), expected);
    }

    #[test]
    fn height_one_shows_only_active() {
        let l = layout(100, 50, 1, 2, 2);
        assert_eq!(l.active_row, 0);
        assert_eq!(lines(&l), vec![Some(50)]);
        assert_eq!(l.rows[0].kind, RowKind::Active);
    }

    #[test]
    fn short_terminal_prioritizes_active_with_partial_context() {
        // height 3, large desired context -> clamped to what fits.
        let l = layout(100, 50, 3, 5, 5);
        assert_eq!(l.active_row, 1);
        assert_eq!(lines(&l), vec![Some(49), Some(50), Some(51)]);
        assert_eq!(l.rows[1].kind, RowKind::Active);
    }

    #[test]
    fn zero_context_shows_only_active_line() {
        let l = layout(100, 50, 5, 0, 0);
        assert_eq!(lines(&l), vec![None, None, Some(50), None, None]);
        assert_eq!(l.rows[2].kind, RowKind::Active);
    }

    #[test]
    fn single_line_buffer() {
        let l = layout(1, 0, 5, 2, 2);
        assert_eq!(lines(&l), vec![None, None, Some(0), None, None]);
        assert_eq!(l.visible_range(), Some((0, 0)));
    }
}
