use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, FocusHandle, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, StyledText,
    TextLayout, TextRun, Window, WrappedLineLayout, fill, point, px, size,
};

use crate::document::BlockId;
use crate::editor::Editor;

const CARET_WIDTH: Pixels = px(2.);
/// Width used to show that a selection includes a line break or an empty line.
const LINE_BREAK_WIDTH: Pixels = px(5.);

/// Where every block was last painted. The editor reads it to move the caret between visual
/// rows, to place it on a click and to scroll it into view.
pub type LayoutMap = Rc<RefCell<HashMap<BlockId, BlockLayout>>>;

/// The painted geometry of one block: its bounds in window coordinates and, for blocks with
/// text, how that text was shaped and wrapped.
#[derive(Clone)]
pub struct BlockLayout {
    pub bounds: Bounds<Pixels>,
    line_height: Pixels,
    /// One entry per line of the text, as separated by line breaks.
    lines: Vec<Arc<WrappedLineLayout>>,
}

/// One row of text as displayed, after soft wrapping.
#[derive(Clone, Debug)]
pub struct VisualRow {
    /// The bytes of the block text shown on this row.
    pub range: Range<usize>,
    /// Whether the row ends at a soft wrap rather than a line break or the end of the text.
    /// The offset at such a boundary belongs to both this row and the next one.
    pub wraps: bool,
    line: usize,
    line_start: usize,
    start_x: Pixels,
}

impl BlockLayout {
    fn new(layout: &TextLayout) -> Self {
        Self {
            bounds: layout.bounds(),
            line_height: layout.line_height(),
            lines: layout.line_layouts().into_vec(),
        }
    }

    pub fn without_text(bounds: Bounds<Pixels>) -> Self {
        Self {
            bounds,
            line_height: bounds.size.height,
            lines: Vec::new(),
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
                rows.push(VisualRow {
                    range: line_start + row_start..line_start + glyph.index,
                    wraps: true,
                    line: line_index,
                    line_start,
                    start_x: layout.x_for_index(row_start),
                });
                row_start = glyph.index;
            }
            rows.push(VisualRow {
                range: line_start + row_start..line_start + line.len(),
                wraps: false,
                line: line_index,
                line_start,
                start_x: layout.x_for_index(row_start),
            });
            line_start += line.len() + 1;
        }
        rows
    }

    /// The row a caret at `offset` is displayed on. `upstream` picks the upper row when the
    /// offset is at a soft wrap.
    pub fn row_index(&self, rows: &[VisualRow], offset: usize, upstream: bool) -> usize {
        for (index, row) in rows.iter().enumerate() {
            if offset < row.range.end {
                return index;
            }
            if offset == row.range.end && (!row.wraps || upstream) {
                return index;
            }
        }
        rows.len().saturating_sub(1)
    }

    /// The horizontal distance from the left edge of the block to `offset` on `row`.
    pub fn x_in_row(&self, row: &VisualRow, offset: usize) -> Pixels {
        let Some(line) = self.lines.get(row.line) else {
            return Pixels::ZERO;
        };
        let offset = offset.clamp(row.range.start, row.range.end);
        line.unwrapped_layout.x_for_index(offset - row.line_start) - row.start_x
    }

    /// The offset on `row` closest to `x`, and whether it is the upstream side of a soft wrap.
    pub fn offset_in_row(&self, row: &VisualRow, x: Pixels) -> (usize, bool) {
        let Some(line) = self.lines.get(row.line) else {
            return (row.range.start, false);
        };
        let index = line
            .unwrapped_layout
            .closest_index_for_x(x.max(Pixels::ZERO) + row.start_x);
        let offset = (row.line_start + index).clamp(row.range.start, row.range.end);
        (offset, row.wraps && offset == row.range.end)
    }

    pub fn offset_for_point(&self, position: Point<Pixels>) -> (usize, bool) {
        let rows = self.rows();
        if rows.is_empty() {
            return (0, false);
        }
        let row = ((position.y - self.bounds.top()) / self.line_height).floor();
        let row = (row.max(0.) as usize).min(rows.len() - 1);
        self.offset_in_row(&rows[row], position.x - self.bounds.left())
    }

    pub fn caret_bounds(&self, offset: usize, upstream: bool) -> Option<Bounds<Pixels>> {
        let rows = self.rows();
        let index = self.row_index(&rows, offset, upstream);
        let row = rows.get(index)?;
        let origin =
            self.bounds.origin + point(self.x_in_row(row, offset), self.line_height * index as f32);
        Some(Bounds::new(origin, size(CARET_WIDTH, self.line_height)))
    }

    /// The parts of the rows that `range` covers, as a selection is shown: a line break in the
    /// range takes up some width of its own.
    pub fn range_bounds(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.row_bounds(range, LINE_BREAK_WIDTH)
    }

    fn row_bounds(&self, range: Range<usize>, line_break_width: Pixels) -> Vec<Bounds<Pixels>> {
        let mut bounds = Vec::new();
        for (index, row) in self.rows().iter().enumerate() {
            if range.start > row.range.end || range.end < row.range.start {
                continue;
            }
            let start = range.start.max(row.range.start);
            let end = range.end.min(row.range.end);
            let left = self.x_in_row(row, start);
            let mut right = self.x_in_row(row, end);
            if !row.wraps && range.end > row.range.end {
                right += line_break_width;
            }
            if right > left {
                bounds.push(Bounds::new(
                    self.bounds.origin + point(left, self.line_height * index as f32),
                    size(right - left, self.line_height),
                ));
            }
        }
        bounds
    }
}

/// The text of one block. Wrapping and painting are delegated to [`StyledText`]; this adds
/// the backgrounds of the runs, the selection, the caret, text input and the record of where
/// the text ended up.
pub struct BlockText {
    block: BlockId,
    text: StyledText,
    layouts: LayoutMap,
    backgrounds: Vec<(Range<usize>, Hsla)>,
    selection: Option<(Range<usize>, Hsla)>,
    caret: Option<Caret>,
    input: Option<(FocusHandle, Entity<Editor>)>,
}

pub struct Caret {
    pub offset: usize,
    pub upstream: bool,
    pub color: Hsla,
}

impl BlockText {
    pub fn new(
        block: BlockId,
        text: SharedString,
        mut runs: Vec<TextRun>,
        layouts: LayoutMap,
    ) -> Self {
        // `StyledText` paints the background of a run together with its glyphs, which would
        // put it over the selection, so the backgrounds are taken out to be painted under it.
        let mut backgrounds: Vec<(Range<usize>, Hsla)> = Vec::new();
        let mut offset = 0;
        for run in &mut runs {
            let range = offset..offset + run.len;
            offset = range.end;
            let Some(color) = run.background_color.take() else {
                continue;
            };
            match backgrounds.last_mut() {
                Some((last, last_color)) if last.end == range.start && *last_color == color => {
                    last.end = range.end;
                }
                _ => backgrounds.push((range, color)),
            }
        }
        Self {
            block,
            text: StyledText::new(text).with_runs(runs),
            layouts,
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
        let layout = BlockLayout::new(self.text.layout());
        for (range, color) in &self.backgrounds {
            for background in layout.row_bounds(range.clone(), Pixels::ZERO) {
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
            && let Some(caret_bounds) = layout.caret_bounds(caret.offset, caret.upstream)
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
