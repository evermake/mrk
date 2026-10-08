use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, FocusHandle, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, StyledText, TextLayout,
    TextRun, Window, WrappedLineLayout, fill, font, point, px, size, transparent_black,
};

use crate::document::BlockId;
use crate::editor::Editor;
use crate::theme::UI_FONT;

const CARET_WIDTH: Pixels = px(2.);
/// Width used to show that a selection includes a line break or an empty line.
const LINE_BREAK_WIDTH: Pixels = px(5.);
/// What the displayed text has inside each end of a stretch of runs with a background (inline
/// code), so that the background reaches a little past the text and the caret has two places
/// there: inside it and outside it. These are narrow no-break spaces, which the text is not
/// wrapped at; in the interface font the three are about a third of an em wide.
const PADDING: &str = "\u{202F}\u{202F}\u{202F}";

/// One [`PADDING`] in the displayed text.
#[derive(Clone, Copy, Debug)]
struct Pad {
    /// Where it is in the text of the block, which does not have it.
    offset: usize,
    /// Whether it is at the start of what it pads rather than at its end.
    leading: bool,
}

/// Where the displayed text has padding, in the order of the text. It maps between offsets in
/// the text of a block, which is what everything outside this module uses, and offsets in the
/// text as displayed.
#[derive(Clone, Default)]
struct Padding(Rc<[Pad]>);

impl Padding {
    /// The displayed offset of the caret at `offset`. Where there is padding, the caret is on
    /// the side of it that is `inside` what is padded, or on the other one.
    fn displayed(&self, offset: usize, inside: bool) -> usize {
        let passed = self
            .0
            .iter()
            .filter(|pad| pad.offset < offset || (pad.offset == offset && pad.leading == inside))
            .count();
        offset + passed * PADDING.len()
    }

    /// The displayed range of `range` of the text. Padding is part of what it pads, so it is
    /// in the range only together with the text next to it.
    fn displayed_range(&self, range: &Range<usize>) -> Range<usize> {
        self.displayed(range.start, false)..self.displayed(range.end, false)
    }

    /// The offset in the text that the `displayed` one is at and, when it is next to padding,
    /// whether it is on the inside of what is padded.
    fn offset(&self, displayed: usize) -> (usize, Option<bool>) {
        let mut shift = 0;
        for pad in self.0.iter() {
            let start = pad.offset + shift;
            if displayed < start {
                break;
            }
            if displayed <= start + PADDING.len() {
                // Within the padding, it is the nearer end of it.
                let after = (displayed - start) * 2 > PADDING.len();
                return (pad.offset, Some(pad.leading == after));
            }
            shift += PADDING.len();
        }
        (displayed - shift, None)
    }
}

/// Where every block was last painted. The editor reads it to move the caret between visual
/// rows, to place it on a click and to scroll it into view.
pub type LayoutMap = Rc<RefCell<HashMap<BlockId, BlockLayout>>>;

/// The painted geometry of one block: its bounds in window coordinates and, for blocks with
/// text, how that text was shaped and wrapped.
#[derive(Clone)]
pub struct BlockLayout {
    pub bounds: Bounds<Pixels>,
    line_height: Pixels,
    /// One entry per line of the displayed text, as separated by line breaks.
    lines: Vec<Arc<WrappedLineLayout>>,
    padding: Padding,
}

/// One row of text as displayed, after soft wrapping.
#[derive(Clone, Debug)]
pub struct VisualRow {
    /// The bytes of the block text shown on this row.
    pub range: Range<usize>,
    /// Whether the row ends at a soft wrap rather than a line break or the end of the text.
    /// The offset at such a boundary belongs to both this row and the next one.
    pub wraps: bool,
    /// The bytes of the displayed text shown on this row.
    displayed: Range<usize>,
    line: usize,
    /// Where the line starts in the displayed text.
    line_start: usize,
    start_x: Pixels,
}

/// A place for the caret, as found from a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretPlace {
    pub offset: usize,
    /// Whether it is the upstream side of a soft wrap.
    pub upstream: bool,
    /// At an edge of inline code, whether it is the place inside the code.
    pub inside_code: Option<bool>,
}

/// The index of the row that `offset` is on, by the range `range_of` gives for each row.
/// `upstream` picks the upper row when the offset is at a soft wrap.
fn row_containing(
    rows: &[VisualRow],
    offset: usize,
    upstream: bool,
    range_of: impl Fn(&VisualRow) -> &Range<usize>,
) -> usize {
    for (index, row) in rows.iter().enumerate() {
        let range = range_of(row);
        if offset < range.end {
            return index;
        }
        if offset == range.end && (!row.wraps || upstream) {
            return index;
        }
    }
    rows.len().saturating_sub(1)
}

impl BlockLayout {
    fn new(layout: &TextLayout, padding: Padding) -> Self {
        Self {
            bounds: layout.bounds(),
            line_height: layout.line_height(),
            lines: layout.line_layouts().into_vec(),
            padding,
        }
    }

    pub fn without_text(bounds: Bounds<Pixels>) -> Self {
        Self {
            bounds,
            line_height: bounds.size.height,
            lines: Vec::new(),
            padding: Padding::default(),
        }
    }

    pub fn rows(&self) -> Vec<VisualRow> {
        let mut rows = Vec::new();
        let mut line_start = 0;
        for (line_index, line) in self.lines.iter().enumerate() {
            let layout = &line.unwrapped_layout;
            let mut row_start = 0;
            for boundary in line.wrap_boundaries() {
                // A wrap boundary names the first glyph of the row that follows it.
                let Some(glyph) = layout
                    .runs
                    .get(boundary.run_ix)
                    .and_then(|run| run.glyphs.get(boundary.glyph_ix))
                else {
                    continue;
                };
                rows.push(self.row(
                    line_start + row_start..line_start + glyph.index,
                    true,
                    line_index,
                    line_start,
                    layout.x_for_index(row_start),
                ));
                row_start = glyph.index;
            }
            rows.push(self.row(
                line_start + row_start..line_start + line.len(),
                false,
                line_index,
                line_start,
                layout.x_for_index(row_start),
            ));
            line_start += line.len() + 1;
        }
        rows
    }

    fn row(
        &self,
        displayed: Range<usize>,
        wraps: bool,
        line: usize,
        line_start: usize,
        start_x: Pixels,
    ) -> VisualRow {
        let (start, _) = self.padding.offset(displayed.start);
        let (end, _) = self.padding.offset(displayed.end);
        VisualRow {
            range: start..end,
            wraps,
            displayed,
            line,
            line_start,
            start_x,
        }
    }

    /// The row a caret at `offset` is displayed on. `upstream` picks the upper row when the
    /// offset is at a soft wrap.
    pub fn row_index(&self, rows: &[VisualRow], offset: usize, upstream: bool) -> usize {
        row_containing(rows, offset, upstream, |row| &row.range)
    }

    /// The horizontal distance from the left edge of the block to `offset` on `row`.
    pub fn x_in_row(&self, row: &VisualRow, offset: usize) -> Pixels {
        self.displayed_x(row, self.padding.displayed(offset, false))
    }

    /// The horizontal distance from the left edge of the block to the `displayed` offset on
    /// `row`.
    fn displayed_x(&self, row: &VisualRow, displayed: usize) -> Pixels {
        let Some(line) = self.lines.get(row.line) else {
            return Pixels::ZERO;
        };
        let displayed = displayed.clamp(row.displayed.start, row.displayed.end);
        line.unwrapped_layout
            .x_for_index(displayed - row.line_start)
            - row.start_x
    }

    /// The place for the caret on `row` closest to `x`.
    pub fn offset_in_row(&self, row: &VisualRow, x: Pixels) -> CaretPlace {
        let Some(line) = self.lines.get(row.line) else {
            return CaretPlace {
                offset: row.range.start,
                upstream: false,
                inside_code: None,
            };
        };
        let index = line
            .unwrapped_layout
            .closest_index_for_x(x.max(Pixels::ZERO) + row.start_x);
        let displayed = (row.line_start + index).clamp(row.displayed.start, row.displayed.end);
        let (offset, inside_code) = self.padding.offset(displayed);
        CaretPlace {
            offset,
            upstream: row.wraps && displayed == row.displayed.end,
            inside_code,
        }
    }

    pub fn offset_for_point(&self, position: Point<Pixels>) -> CaretPlace {
        let rows = self.rows();
        if rows.is_empty() {
            return CaretPlace {
                offset: 0,
                upstream: false,
                inside_code: None,
            };
        }
        let row = ((position.y - self.bounds.top()) / self.line_height).floor();
        let row = (row.max(0.) as usize).min(rows.len() - 1);
        self.offset_in_row(&rows[row], position.x - self.bounds.left())
    }

    /// Where the caret at `offset` is painted. At an edge of inline code it is `inside_code`,
    /// within the padding of the code, or outside it.
    pub fn caret_bounds(
        &self,
        offset: usize,
        upstream: bool,
        inside_code: bool,
    ) -> Option<Bounds<Pixels>> {
        let rows = self.rows();
        let displayed = self.padding.displayed(offset, inside_code);
        let index = row_containing(&rows, displayed, upstream, |row| &row.displayed);
        let row = rows.get(index)?;
        let origin = self.bounds.origin
            + point(
                self.displayed_x(row, displayed),
                self.line_height * index as f32,
            );
        Some(Bounds::new(origin, size(CARET_WIDTH, self.line_height)))
    }

    /// The parts of the rows that `range` covers, as a selection is shown: a line break in the
    /// range takes up some width of its own.
    pub fn range_bounds(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let displayed = self.padding.displayed_range(&range);
        self.rows()
            .iter()
            .enumerate()
            .filter_map(|(index, row)| self.bounds_in_row(index, row, &displayed, LINE_BREAK_WIDTH))
            .collect()
    }

    /// The parts of the rows that the background of the `displayed` range fills. The range
    /// starts and ends with padding, and a row that has nothing of it but that, because the
    /// text was wrapped right next to the padding, is left bare.
    fn background_bounds(&self, displayed: &Range<usize>) -> Vec<Bounds<Pixels>> {
        let text = displayed.start + PADDING.len()..displayed.end.saturating_sub(PADDING.len());
        self.rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| row.displayed.start < text.end && row.displayed.end > text.start)
            .filter_map(|(index, row)| self.bounds_in_row(index, row, displayed, Pixels::ZERO))
            .collect()
    }

    /// The part of the row at `index` that the `displayed` range covers.
    fn bounds_in_row(
        &self,
        index: usize,
        row: &VisualRow,
        displayed: &Range<usize>,
        line_break_width: Pixels,
    ) -> Option<Bounds<Pixels>> {
        if displayed.start > row.displayed.end || displayed.end < row.displayed.start {
            return None;
        }
        let left = self.displayed_x(row, displayed.start);
        let mut right = self.displayed_x(row, displayed.end);
        if !row.wraps && displayed.end > row.displayed.end {
            right += line_break_width;
        }
        (right > left).then(|| {
            Bounds::new(
                self.bounds.origin + point(left, self.line_height * index as f32),
                size(right - left, self.line_height),
            )
        })
    }
}

/// The text of one block. Wrapping and painting are delegated to [`StyledText`]; this adds
/// the backgrounds of the runs and the padding inside them, the selection, the caret, text
/// input and the record of where the text ended up.
pub struct BlockText {
    block: BlockId,
    text: StyledText,
    layouts: LayoutMap,
    padding: Padding,
    /// The displayed ranges that have a background, padding included.
    backgrounds: Vec<(Range<usize>, Hsla)>,
    selection: Option<(Range<usize>, Hsla)>,
    caret: Option<Caret>,
    input: Option<(FocusHandle, Entity<Editor>)>,
}

pub struct Caret {
    pub offset: usize,
    pub upstream: bool,
    /// At an edge of inline code, whether the caret is on the inside of it.
    pub inside_code: bool,
    pub color: Hsla,
}

impl BlockText {
    pub fn new(block: BlockId, text: &str, runs: Vec<TextRun>, layouts: LayoutMap) -> Self {
        // `StyledText` paints the background of a run together with its glyphs, which would
        // put it over the selection, so the backgrounds are taken out to be painted under it.
        let mut backgrounds: Vec<(Range<usize>, Hsla)> = Vec::new();
        let mut pads = Vec::new();
        let mut displayed = String::with_capacity(text.len());
        let mut displayed_runs = Vec::with_capacity(runs.len());
        let mut offset = 0;
        let mut filled = false;
        // The `None` after the last run ends a background that reaches the end of the text.
        for mut run in runs.into_iter().map(Some).chain([None]) {
            let color = run.as_mut().and_then(|run| run.background_color.take());
            if color.is_some() != filled {
                filled = color.is_some();
                pads.push(Pad {
                    offset,
                    leading: filled,
                });
                let start = displayed.len();
                displayed.push_str(PADDING);
                displayed_runs.push(TextRun {
                    len: PADDING.len(),
                    font: font(UI_FONT),
                    color: transparent_black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
                match (color, backgrounds.last_mut()) {
                    (Some(color), _) => backgrounds.push((start..displayed.len(), color)),
                    (None, Some((last, _))) => last.end = displayed.len(),
                    (None, None) => {}
                }
            }
            let Some(run) = run else {
                break;
            };
            let start = displayed.len();
            displayed.push_str(&text[offset..offset + run.len]);
            offset += run.len;
            if let Some(color) = color {
                match backgrounds.last_mut() {
                    Some((last, last_color)) if last.end == start && *last_color == color => {
                        last.end = displayed.len();
                    }
                    _ => backgrounds.push((start..displayed.len(), color)),
                }
            }
            displayed_runs.push(run);
        }
        Self {
            block,
            text: StyledText::new(displayed).with_runs(displayed_runs),
            layouts,
            padding: Padding(pads.into()),
            backgrounds,
            selection: None,
            caret: None,
            input: None,
        }
    }

    pub fn selection(mut self, range: Range<usize>, color: Hsla) -> Self {
        self.selection = Some((range, color));
        self
    }

    pub fn caret(mut self, caret: Caret) -> Self {
        self.caret = Some(caret);
        self
    }

    /// Routes platform text input (typing, IME composition) to `editor` while `focus_handle`
    /// is focused.
    pub fn input(mut self, focus_handle: FocusHandle, editor: Entity<Editor>) -> Self {
        self.input = Some((focus_handle, editor));
        self
    }
}

impl IntoElement for BlockText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BlockText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(None, inspector_id, bounds, request_layout, window, cx)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = BlockLayout::new(self.text.layout(), self.padding.clone());
        for (range, color) in &self.backgrounds {
            for background in layout.background_bounds(range) {
                window.paint_quad(fill(background, *color));
            }
        }
        if let Some((range, color)) = &self.selection {
            for selected in layout.range_bounds(range.clone()) {
                window.paint_quad(fill(selected, *color));
            }
        }
        self.text.paint(
            None,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );
        if let Some(caret) = &self.caret
            && let Some(caret_bounds) =
                layout.caret_bounds(caret.offset, caret.upstream, caret.inside_code)
        {
            window.paint_quad(fill(caret_bounds, caret.color));
        }
        if let Some((focus_handle, editor)) = &self.input {
            window.handle_input(
                focus_handle,
                ElementInputHandler::new(bounds, editor.clone()),
                cx,
            );
        }
        self.layouts.borrow_mut().insert(self.block, layout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The padding of `a CODE b`, where the code is bytes 2 to 6 of the text.
    fn padding() -> Padding {
        let pads = [
            Pad {
                offset: 2,
                leading: true,
            },
            Pad {
                offset: 6,
                leading: false,
            },
        ];
        Padding(pads.into())
    }

    #[test]
    fn the_caret_has_a_place_on_each_side_of_padding() {
        let padding = padding();
        let pad = PADDING.len();
        // Outside the code the caret is before the padding at its start and after the one at
        // its end.
        assert_eq!(padding.displayed(2, false), 2);
        assert_eq!(padding.displayed(2, true), 2 + pad);
        assert_eq!(padding.displayed(6, true), 6 + pad);
        assert_eq!(padding.displayed(6, false), 6 + 2 * pad);
        // Everywhere else the side makes no difference.
        for inside in [false, true] {
            assert_eq!(padding.displayed(0, inside), 0);
            assert_eq!(padding.displayed(4, inside), 4 + pad);
            assert_eq!(padding.displayed(8, inside), 8 + 2 * pad);
        }
    }

    #[test]
    fn displayed_offsets_map_back_to_the_text() {
        let padding = padding();
        for offset in 0..=8 {
            for inside in [false, true] {
                let at_edge = offset == 2 || offset == 6;
                assert_eq!(
                    padding.offset(padding.displayed(offset, inside)),
                    (offset, at_edge.then_some(inside)),
                    "{offset} {inside}"
                );
            }
        }
        // An offset within the padding is at the nearer end of it.
        let pad = PADDING.len();
        assert_eq!(padding.offset(2 + 1), (2, Some(false)));
        assert_eq!(padding.offset(2 + pad - 1), (2, Some(true)));
        assert_eq!(padding.offset(6 + pad + 1), (6, Some(true)));
        assert_eq!(padding.offset(6 + 2 * pad - 1), (6, Some(false)));
    }

    #[test]
    fn padding_is_selected_together_with_what_it_pads() {
        let padding = padding();
        let pad = PADDING.len();
        assert_eq!(padding.displayed_range(&(0..2)), 0..2);
        assert_eq!(padding.displayed_range(&(2..6)), 2..6 + 2 * pad);
        assert_eq!(padding.displayed_range(&(3..5)), 3 + pad..5 + pad);
        assert_eq!(padding.displayed_range(&(6..8)), 6 + 2 * pad..8 + 2 * pad);
        assert_eq!(padding.displayed_range(&(0..8)), 0..8 + 2 * pad);
    }
}
