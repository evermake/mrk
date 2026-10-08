use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, Entity, EntityInputHandler, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, KeyBinding, KeyContext, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Rems, ScrollHandle, SharedString,
    StrikethroughStyle, Task, TextRun, UTF16Selection, UnderlineStyle, Window, canvas, div, fill,
    point, prelude::*, px,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::block_text::{BlockLayout, BlockText, Caret, LayoutMap};
use crate::document::{Block, BlockId, BlockKind, Document, Row, TextPosition};
use crate::markdown;
use crate::rich_text::{InlineStyle, Mark, RichText};
use crate::scrollbar::Scrollbar;
use crate::theme::{MONO_FONT, Theme, UI_FONT};
use crate::zoom::zoomed;

#[cfg(test)]
mod tests;

/// The actions live in their own module because one of them is named `Copy`, which would
/// otherwise shadow the trait of that name throughout this file.
pub mod actions {
    gpui::actions!(
        editor,
        [
            MoveUp,
            MoveDown,
            MoveLeft,
            MoveRight,
            SelectUp,
            SelectDown,
            SelectLeft,
            SelectRight,
            MoveWordLeft,
            MoveWordRight,
            SelectWordLeft,
            SelectWordRight,
            MoveToLineStart,
            MoveToLineEnd,
            SelectToLineStart,
            SelectToLineEnd,
            Enter,
            LineBreak,
            NewBlockBelow,
            OpenBelow,
            OpenAbove,
            WriteAtStart,
            WriteAtEnd,
            Escape,
            Backspace,
            Delete,
            DeleteWordBackward,
            DeleteToLineStart,
            Indent,
            Outdent,
            MoveBlocksUp,
            MoveBlocksDown,
            SelectAll,
            Copy,
            Cut,
            Paste,
            Undo,
            Redo,
            ToggleBold,
            ToggleItalic,
            ToggleCode,
            ToggleStrikethrough,
            ToggleTodos,
        ]
    );
}

use actions::{
    Backspace, Cut, Delete, DeleteToLineStart, DeleteWordBackward, Enter, Escape, Indent,
    LineBreak, MoveBlocksDown, MoveBlocksUp, MoveDown, MoveLeft, MoveRight, MoveToLineEnd,
    MoveToLineStart, MoveUp, MoveWordLeft, MoveWordRight, NewBlockBelow, OpenAbove, OpenBelow,
    Outdent, Paste, Redo, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd,
    SelectToLineStart, SelectUp, SelectWordLeft, SelectWordRight, ToggleBold, ToggleCode,
    ToggleItalic, ToggleStrikethrough, ToggleTodos, Undo, WriteAtEnd, WriteAtStart,
};

const KEY_CONTEXT: &str = "Editor";
/// The Vim-style keys must stay out of writing mode, where they are text.
const NOT_WRITING: &str = "Editor && mode != writing";

pub fn bind_keys(cx: &mut App) {
    let editor = Some(KEY_CONTEXT);
    let not_writing = Some(NOT_WRITING);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, editor),
        KeyBinding::new("down", MoveDown, editor),
        KeyBinding::new("left", MoveLeft, editor),
        KeyBinding::new("right", MoveRight, editor),
        KeyBinding::new("shift-up", SelectUp, editor),
        KeyBinding::new("shift-down", SelectDown, editor),
        KeyBinding::new("shift-left", SelectLeft, editor),
        KeyBinding::new("shift-right", SelectRight, editor),
        KeyBinding::new("alt-up", MoveBlocksUp, editor),
        KeyBinding::new("alt-down", MoveBlocksDown, editor),
        KeyBinding::new("k", MoveUp, not_writing),
        KeyBinding::new("j", MoveDown, not_writing),
        KeyBinding::new("h", MoveLeft, not_writing),
        KeyBinding::new("l", MoveRight, not_writing),
        KeyBinding::new("shift-k", SelectUp, not_writing),
        KeyBinding::new("shift-j", SelectDown, not_writing),
        KeyBinding::new("shift-h", SelectLeft, not_writing),
        KeyBinding::new("shift-l", SelectRight, not_writing),
        KeyBinding::new("alt-k", MoveBlocksUp, not_writing),
        KeyBinding::new("alt-j", MoveBlocksDown, not_writing),
        KeyBinding::new("d", Backspace, not_writing),
        KeyBinding::new("o", OpenBelow, not_writing),
        KeyBinding::new("shift-o", OpenAbove, not_writing),
        KeyBinding::new("i", WriteAtStart, not_writing),
        KeyBinding::new("shift-i", WriteAtStart, not_writing),
        KeyBinding::new("a", WriteAtEnd, not_writing),
        KeyBinding::new("shift-a", WriteAtEnd, not_writing),
        KeyBinding::new("space", ToggleTodos, not_writing),
        KeyBinding::new("alt-left", MoveWordLeft, editor),
        KeyBinding::new("alt-right", MoveWordRight, editor),
        KeyBinding::new("alt-shift-left", SelectWordLeft, editor),
        KeyBinding::new("alt-shift-right", SelectWordRight, editor),
        KeyBinding::new("cmd-left", MoveToLineStart, editor),
        KeyBinding::new("cmd-right", MoveToLineEnd, editor),
        KeyBinding::new("home", MoveToLineStart, editor),
        KeyBinding::new("end", MoveToLineEnd, editor),
        KeyBinding::new("ctrl-a", MoveToLineStart, editor),
        KeyBinding::new("ctrl-e", MoveToLineEnd, editor),
        KeyBinding::new("cmd-shift-left", SelectToLineStart, editor),
        KeyBinding::new("cmd-shift-right", SelectToLineEnd, editor),
        KeyBinding::new("shift-home", SelectToLineStart, editor),
        KeyBinding::new("shift-end", SelectToLineEnd, editor),
        KeyBinding::new("enter", Enter, editor),
        KeyBinding::new("shift-enter", LineBreak, editor),
        KeyBinding::new("cmd-enter", NewBlockBelow, editor),
        KeyBinding::new("escape", Escape, editor),
        KeyBinding::new("backspace", Backspace, editor),
        KeyBinding::new("shift-backspace", Backspace, editor),
        KeyBinding::new("delete", Delete, editor),
        KeyBinding::new("alt-backspace", DeleteWordBackward, editor),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, editor),
        KeyBinding::new("tab", Indent, editor),
        KeyBinding::new("shift-tab", Outdent, editor),
        KeyBinding::new("cmd-a", SelectAll, editor),
        KeyBinding::new("cmd-c", actions::Copy, editor),
        KeyBinding::new("cmd-x", Cut, editor),
        KeyBinding::new("cmd-v", Paste, editor),
        KeyBinding::new("cmd-z", Undo, editor),
        KeyBinding::new("cmd-shift-z", Redo, editor),
        KeyBinding::new("cmd-b", ToggleBold, editor),
        KeyBinding::new("cmd-i", ToggleItalic, editor),
        KeyBinding::new("cmd-e", ToggleCode, editor),
        KeyBinding::new("cmd-shift-x", ToggleStrikethrough, editor),
    ]);
}

const INDENT: Rems = zoomed(24.);
const CONTENT_WIDTH: Rems = zoomed(760.);
const SCROLL_MARGIN: Pixels = px(24.);
const CODE_INDENT: &str = "    ";
/// What turns a paragraph into a divider as soon as it is typed at its start.
const DIVIDER_SHORTCUT: &str = "---";
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
/// Consecutive typing within this interval is undone as one step.
const UNDO_GROUP_INTERVAL: Duration = Duration::from_secs(1);
const MAX_UNDO_STEPS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Nothing is selected, as when a document was just opened.
    Idle,
    Navigation,
    Writing,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    None,
    /// The siblings between `anchor` and `head`, each including its whole subtree. The two
    /// always share a parent.
    Blocks {
        anchor: BlockId,
        head: BlockId,
    },
    Text(TextSelection),
    /// Text selected across blocks, from `anchor` to `head`, which are in different blocks;
    /// either may come first in the document. A selection within one block is a
    /// [`Selection::Text`].
    Span {
        anchor: TextPosition,
        head: TextPosition,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextSelection {
    pub block: BlockId,
    pub range: Range<usize>,
    /// Whether the caret is at the start of the range rather than its end.
    pub reversed: bool,
}

impl TextSelection {
    fn caret(block: BlockId, offset: usize) -> Self {
        Self {
            block,
            range: offset..offset,
            reversed: false,
        }
    }

    fn head(&self) -> usize {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    fn tail(&self) -> usize {
        if self.reversed {
            self.range.end
        } else {
            self.range.start
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Character,
    Word,
    Line,
}

/// What moving the mouse selects while its button is held down, as the click that pressed
/// it decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    /// Text from the click up to the pointer.
    Characters,
    /// Whole words, from the word that was double-clicked (`start` to `end`) up to the word
    /// under the pointer.
    Words {
        start: TextPosition,
        end: TextPosition,
    },
}

struct Snapshot {
    document: Document,
    selection: Selection,
    revision: u64,
}

#[derive(Default)]
struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The kind, block and time of the last edit, to group runs of typing into one step.
    last_edit: Option<(EditKind, BlockId, Instant)>,
}

pub struct Editor {
    document: Document,
    selection: Selection,
    /// The text being composed by an input method, within the block being written in.
    marked_range: Option<Range<usize>>,
    /// The style for the next typed text, set by toggling a mark with nothing selected.
    pending_style: Option<InlineStyle>,
    /// The window x coordinate the caret keeps to while moving vertically.
    goal_x: Option<Pixels>,
    /// At a soft wrap, whether the caret is at the end of the upper row.
    caret_upstream: bool,
    history: History,
    revision: u64,
    saved_revision: u64,
    last_revision: u64,
    layouts: LayoutMap,
    scroll_handle: ScrollHandle,
    scrollbar: Entity<Scrollbar>,
    focus_handle: FocusHandle,
    needs_autoscroll: bool,
    drag: Option<Drag>,
    caret_visible: bool,
    blink_task: Option<Task<()>>,
}

impl Focusable for Editor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Editor {
    pub fn new(document: Document, cx: &mut Context<Self>) -> Self {
        let scroll_handle = ScrollHandle::new();
        Self {
            document,
            selection: Selection::None,
            marked_range: None,
            pending_style: None,
            goal_x: None,
            caret_upstream: false,
            history: History::default(),
            revision: 0,
            saved_revision: 0,
            last_revision: 0,
            layouts: Rc::new(RefCell::new(HashMap::new())),
            scrollbar: cx.new(|_| Scrollbar::new(scroll_handle.clone())),
            scroll_handle,
            focus_handle: cx.focus_handle(),
            needs_autoscroll: false,
            drag: None,
            caret_visible: true,
            blink_task: None,
        }
    }

    /// Keeps the same part of the note in view after everything was scaled by `ratio`.
    pub fn rescale_scroll(&mut self, ratio: f32, cx: &mut Context<Self>) {
        let mut offset = self.scroll_handle.offset();
        offset.y *= ratio;
        self.scroll_handle.set_offset(offset);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn scroll_offset(&self) -> Point<Pixels> {
        self.scroll_handle.offset()
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Identifies the current content, to tell [`Self::mark_saved`] what was written.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Records that the content as of `revision` is what is on disk. Edits made while the
    /// file was being written keep the document dirty.
    pub fn mark_saved(&mut self, revision: u64, cx: &mut Context<Self>) {
        self.saved_revision = revision;
        cx.notify();
    }

    pub fn mode(&self) -> Mode {
        match self.selection {
            Selection::None => Mode::Idle,
            Selection::Blocks { .. } => Mode::Navigation,
            Selection::Text(_) | Selection::Span { .. } => Mode::Writing,
        }
    }

    /// The selected text when it is within one block.
    fn text_selection(&self) -> Option<&TextSelection> {
        match &self.selection {
            Selection::Text(selection) => Some(selection),
            _ => None,
        }
    }

    /// The anchor and the head of the selected text, within a block or across blocks.
    fn text_endpoints(&self) -> Option<(TextPosition, TextPosition)> {
        match &self.selection {
            Selection::Text(selection) => Some((
                TextPosition {
                    block: selection.block,
                    offset: selection.tail(),
                },
                TextPosition {
                    block: selection.block,
                    offset: selection.head(),
                },
            )),
            Selection::Span { anchor, head } => Some((*anchor, *head)),
            Selection::None | Selection::Blocks { .. } => None,
        }
    }

    /// The selected text across blocks, as its start and end in document order.
    fn span(&self) -> Option<(TextPosition, TextPosition)> {
        match &self.selection {
            Selection::Span { anchor, head } => self.document.in_order(*anchor, *head),
            _ => None,
        }
    }

    /// The siblings the current selection acts on as blocks: the selected blocks, the block
    /// being written in, or the blocks that text selected across blocks reaches into.
    fn block_range(&self) -> Option<(BlockId, BlockId)> {
        match &self.selection {
            Selection::None => None,
            Selection::Blocks { anchor, head } => Some((*anchor, *head)),
            Selection::Text(selection) => Some((selection.block, selection.block)),
            Selection::Span { anchor, head } => {
                self.document.covering_siblings(anchor.block, head.block)
            }
        }
    }

    /// The selection that platform text input (an input method, dictation) sees: the
    /// selected text of the block being written in. Of a selection across blocks it sees the
    /// part in the block that has the caret.
    fn input_selection(&self) -> Option<TextSelection> {
        match &self.selection {
            Selection::Text(selection) => Some(selection.clone()),
            Selection::Span { anchor, head } => {
                let length = self.document.block(head.block)?.text.len();
                let offset = head.offset.min(length);
                let reversed = self.document.order(head.block, anchor.block)? == Ordering::Less;
                Some(TextSelection {
                    block: head.block,
                    range: if reversed { offset..length } else { 0..offset },
                    reversed,
                })
            }
            Selection::None | Selection::Blocks { .. } => None,
        }
    }

    fn active_text(&self) -> Option<&RichText> {
        let selection = self.input_selection()?;
        self.document
            .block(selection.block)
            .map(|block| &block.text)
    }

    fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.selection = selection;
        self.marked_range = None;
        self.pending_style = None;
        self.goal_x = None;
        self.caret_upstream = false;
        self.history.last_edit = None;
        self.selection_changed(cx);
    }

    fn selection_changed(&mut self, cx: &mut Context<Self>) {
        self.needs_autoscroll = true;
        self.restart_blink(cx);
        cx.notify();
    }

    fn select_block(&mut self, block: BlockId, cx: &mut Context<Self>) {
        self.set_selection(
            Selection::Blocks {
                anchor: block,
                head: block,
            },
            cx,
        );
    }

    fn set_caret(&mut self, block: BlockId, offset: usize, cx: &mut Context<Self>) {
        self.set_selection(Selection::Text(TextSelection::caret(block, offset)), cx);
    }

    /// Selects the text from `anchor` to `head`, which may be in different blocks.
    fn select_text(&mut self, anchor: TextPosition, head: TextPosition, cx: &mut Context<Self>) {
        self.set_selection(text_selection_between(anchor, head), cx);
    }

    /// Moves the head of the text selection to `head`, or the caret when not `extend`ing.
    fn move_head(
        &mut self,
        anchor: TextPosition,
        head: TextPosition,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if extend {
            self.select_text(anchor, head, cx);
        } else {
            self.set_caret(head.block, head.offset, cx);
        }
    }

    /// Deletes the text selected across blocks, leaving the caret where it started. That is
    /// one undo step.
    fn delete_span(&mut self, cx: &mut Context<Self>) -> Option<TextSelection> {
        let (start, end) = self.span()?;
        let mut caret = None;
        self.transact(EditKind::Other, cx, |this| {
            let Some(position) = this.document.delete_span(start, end) else {
                return false;
            };
            let selection = TextSelection::caret(position.block, position.offset);
            this.selection = Selection::Text(selection.clone());
            caret = Some(selection);
            true
        });
        caret
    }

    /// Replaces the text selected across blocks by what `then` does at the caret that is left
    /// when it is deleted. The deletion and the edit that follows are one undo step.
    fn replace_span(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, TextSelection, &mut Context<Self>),
    ) {
        let steps = self.history.undo.len();
        let Some(caret) = self.delete_span(cx) else {
            return;
        };
        then(self, caret, cx);
        if self.history.undo.len() == steps + 2 {
            // Drop the snapshot of the state in between, so that undo goes back to before both.
            self.history.undo.remove(steps + 1);
        }
    }

    /// Enters writing mode at the start (`Backward`) or the end (`Forward`) of `block`. A
    /// divider has no text to write in, so a paragraph is added before or after it instead.
    fn start_writing(&mut self, block: BlockId, side: Direction, cx: &mut Context<Self>) {
        let Some(target) = self.document.block(block) else {
            return;
        };
        if target.kind.has_text() {
            let offset = match side {
                Direction::Backward => 0,
                Direction::Forward => target.text.len(),
            };
            self.set_caret(block, offset, cx);
        } else {
            self.write_in_new_block(block, side, BlockKind::Paragraph, cx);
        }
    }

    /// Adds an empty block of `kind` right before or after `neighbor`, at its nesting level,
    /// and starts writing in it.
    fn write_in_new_block(
        &mut self,
        neighbor: BlockId,
        direction: Direction,
        kind: BlockKind,
        cx: &mut Context<Self>,
    ) {
        self.transact(EditKind::Other, cx, |this| {
            let block = Block::new(kind, RichText::new());
            let inserted = match direction {
                Direction::Backward => this.document.insert_before(neighbor, vec![block]),
                Direction::Forward => this.document.insert_after(neighbor, vec![block]),
            };
            match inserted.first() {
                Some(&block) => {
                    this.selection = Selection::Text(TextSelection::caret(block, 0));
                    true
                }
                None => false,
            }
        });
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            document: self.document.clone(),
            selection: self.selection.clone(),
            revision: self.revision,
        }
    }

    /// Runs an edit as one undoable step. `edit` returns whether it changed anything; when it
    /// did not, nothing is recorded.
    fn transact(
        &mut self,
        kind: EditKind,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        let before = self.snapshot();
        let block = self.text_selection().map(|selection| selection.block);
        if !edit(self) {
            return false;
        }
        let now = Instant::now();
        let continues_last_edit = kind != EditKind::Other
            && self
                .history
                .last_edit
                .is_some_and(|(last_kind, last_block, time)| {
                    last_kind == kind
                        && Some(last_block) == block
                        && now.duration_since(time) < UNDO_GROUP_INTERVAL
                });
        if !continues_last_edit {
            self.history.undo.push(before);
            if self.history.undo.len() > MAX_UNDO_STEPS {
                self.history.undo.remove(0);
            }
        }
        self.history.redo.clear();
        self.history.last_edit = match (kind, block) {
            (EditKind::Other, _) | (_, None) => None,
            (kind, Some(block)) => Some((kind, block, now)),
        };
        self.last_revision += 1;
        self.revision = self.last_revision;
        self.goal_x = None;
        self.caret_upstream = false;
        self.selection_changed(cx);
        true
    }

    fn restore(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.document = snapshot.document;
        self.revision = snapshot.revision;
        self.set_selection(snapshot.selection, cx);
    }

    fn undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.history.undo.pop() {
            let current = self.snapshot();
            self.history.redo.push(current);
            self.restore(snapshot, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.history.redo.pop() {
            let current = self.snapshot();
            self.history.undo.push(current);
            self.restore(snapshot, cx);
        }
    }

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        if self.mode() != Mode::Writing {
            self.blink_task = None;
            return;
        }
        self.blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;
                let updated = this.update(cx, |this, cx| {
                    this.caret_visible = !this.caret_visible;
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
            }
        }));
    }

    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => self.select_block(self.document.last(), cx),
            Selection::Blocks { head, .. } => {
                let target = self.document.previous(head).unwrap_or(head);
                self.select_block(target, cx);
            }
            Selection::Text(_) | Selection::Span { .. } => {
                self.move_caret_vertically(Direction::Backward, false, cx)
            }
        }
    }

    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => self.select_block(self.document.first(), cx),
            Selection::Blocks { head, .. } => {
                let target = self.document.next(head).unwrap_or(head);
                self.select_block(target, cx);
            }
            Selection::Text(_) | Selection::Span { .. } => {
                self.move_caret_vertically(Direction::Forward, false, cx)
            }
        }
    }

    fn move_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => {}
            Selection::Blocks { head, .. } => {
                if let Some(parent) = self.document.parent(head) {
                    self.select_block(parent, cx);
                }
            }
            Selection::Text(_) | Selection::Span { .. } => {
                self.move_caret(Direction::Backward, Unit::Character, false, cx)
            }
        }
    }

    fn move_right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => {}
            Selection::Blocks { head, .. } => {
                if let Some(child) = self.document.first_child(head) {
                    self.select_block(child, cx);
                }
            }
            Selection::Text(_) | Selection::Span { .. } => {
                self.move_caret(Direction::Forward, Unit::Character, false, cx)
            }
        }
    }

    fn select_up(&mut self, _: &SelectUp, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => self.select_block(self.document.last(), cx),
            Selection::Blocks { anchor, head } => {
                if let Some(head) = self.document.previous_sibling(head) {
                    self.set_selection(Selection::Blocks { anchor, head }, cx);
                }
            }
            Selection::Text(_) | Selection::Span { .. } => {
                self.move_caret_vertically(Direction::Backward, true, cx)
            }
        }
    }

    fn select_down(&mut self, _: &SelectDown, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => self.select_block(self.document.first(), cx),
            Selection::Blocks { anchor, head } => {
                if let Some(head) = self.document.next_sibling(head) {
                    self.set_selection(Selection::Blocks { anchor, head }, cx);
                }
            }
            Selection::Text(_) | Selection::Span { .. } => {
                self.move_caret_vertically(Direction::Forward, true, cx)
            }
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(Direction::Backward, Unit::Character, true, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(Direction::Forward, Unit::Character, true, cx);
    }

    fn move_word_left(&mut self, _: &MoveWordLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(Direction::Backward, Unit::Word, false, cx);
    }

    fn move_word_right(&mut self, _: &MoveWordRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(Direction::Forward, Unit::Word, false, cx);
    }

    fn select_word_left(
        &mut self,
        _: &SelectWordLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(Direction::Backward, Unit::Word, true, cx);
    }

    fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(Direction::Forward, Unit::Word, true, cx);
    }

    fn move_to_line_start(
        &mut self,
        _: &MoveToLineStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(Direction::Backward, Unit::Line, false, cx);
    }

    fn move_to_line_end(
        &mut self,
        _: &MoveToLineEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(Direction::Forward, Unit::Line, false, cx);
    }

    fn select_to_line_start(
        &mut self,
        _: &SelectToLineStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(Direction::Backward, Unit::Line, true, cx);
    }

    fn select_to_line_end(
        &mut self,
        _: &SelectToLineEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(Direction::Forward, Unit::Line, true, cx);
    }

    /// The offset reached from `position` by moving one `unit` within the block's text, and
    /// whether the caret then sits on the upstream side of a soft wrap.
    fn offset_after_move(
        &self,
        position: TextPosition,
        direction: Direction,
        unit: Unit,
    ) -> (usize, bool) {
        let Some(text) = self
            .document
            .block(position.block)
            .map(|block| block.text.text())
        else {
            return (position.offset, false);
        };
        let offset = position.offset;
        match (unit, direction) {
            (Unit::Character, Direction::Backward) => (previous_grapheme(text, offset), false),
            (Unit::Character, Direction::Forward) => (next_grapheme(text, offset), false),
            (Unit::Word, Direction::Backward) => (previous_word_start(text, offset), false),
            (Unit::Word, Direction::Forward) => (next_word_end(text, offset), false),
            (Unit::Line, _) => {
                let layouts = self.layouts.borrow();
                let Some(layout) = layouts.get(&position.block) else {
                    return match direction {
                        Direction::Backward => (0, false),
                        Direction::Forward => (text.len(), false),
                    };
                };
                let rows = layout.rows();
                let Some(row) = rows.get(layout.row_index(&rows, offset, self.caret_upstream))
                else {
                    return (offset, false);
                };
                match direction {
                    Direction::Backward => (row.range.start, false),
                    Direction::Forward => (row.range.end, row.wraps),
                }
            }
        }
    }

    /// Where the caret at `head` goes when it moves one `unit`, and whether it then sits on
    /// the upstream side of a soft wrap. At the edge of the block it continues into the
    /// neighboring one. `None` when there is nowhere to go.
    fn position_after_move(
        &self,
        head: TextPosition,
        direction: Direction,
        unit: Unit,
    ) -> Option<(TextPosition, bool)> {
        let (offset, upstream) = self.offset_after_move(head, direction, unit);
        if offset != head.offset || unit == Unit::Line {
            let position = TextPosition {
                block: head.block,
                offset,
            };
            return Some((position, upstream));
        }
        let block = self.adjacent_text_block(head.block, direction)?;
        let offset = match direction {
            Direction::Backward => self.document.block(block)?.text.len(),
            Direction::Forward => 0,
        };
        Some((TextPosition { block, offset }, false))
    }

    fn move_caret(
        &mut self,
        direction: Direction,
        unit: Unit,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((anchor, head)) = self.text_endpoints() else {
            return;
        };
        if extend {
            let (head, upstream) = self
                .position_after_move(head, direction, unit)
                .unwrap_or((head, false));
            self.select_text(anchor, head, cx);
            self.caret_upstream = upstream;
            return;
        }
        if unit == Unit::Character && anchor != head {
            // Moving by a character collapses a selection to the side it is moved towards.
            let Some((start, end)) = self.document.in_order(anchor, head) else {
                return;
            };
            let position = match direction {
                Direction::Backward => start,
                Direction::Forward => end,
            };
            self.set_caret(position.block, position.offset, cx);
            return;
        }
        if let Some((position, upstream)) = self.position_after_move(head, direction, unit) {
            self.set_caret(position.block, position.offset, cx);
            self.caret_upstream = upstream;
        }
    }

    /// The nearest block with text above or below `block` in document order.
    fn adjacent_text_block(&self, block: BlockId, direction: Direction) -> Option<BlockId> {
        let mut current = block;
        loop {
            current = match direction {
                Direction::Backward => self.document.previous(current)?,
                Direction::Forward => self.document.next(current)?,
            };
            if self.document.block(current)?.kind.has_text() {
                return Some(current);
            }
        }
    }

    fn move_caret_vertically(
        &mut self,
        direction: Direction,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((anchor, head)) = self.text_endpoints() else {
            return;
        };
        let text_length = self
            .document
            .block(head.block)
            .map_or(0, |block| block.text.len());
        let block_edge = TextPosition {
            block: head.block,
            offset: match direction {
                Direction::Backward => 0,
                Direction::Forward => text_length,
            },
        };

        let layouts = self.layouts.borrow().clone();
        let Some(layout) = layouts.get(&head.block) else {
            // Without a layout (nothing was painted yet) rows are unknown, so the caret moves
            // by whole blocks.
            let target = self.adjacent_text_block(head.block, direction);
            let position = match target {
                Some(block) => TextPosition {
                    block,
                    offset: match direction {
                        Direction::Backward => self
                            .document
                            .block(block)
                            .map_or(0, |block| block.text.len()),
                        Direction::Forward => 0,
                    },
                },
                None => block_edge,
            };
            self.move_head(anchor, position, extend, cx);
            return;
        };

        let rows = layout.rows();
        let row_index = layout.row_index(&rows, head.offset, self.caret_upstream);
        let goal_x = self.goal_x.unwrap_or_else(|| {
            rows.get(row_index).map_or(layout.bounds.left(), |row| {
                layout.bounds.left() + layout.x_in_row(row, head.offset)
            })
        });
        let target_row = match direction {
            Direction::Backward => row_index.checked_sub(1),
            Direction::Forward => Some(row_index + 1).filter(|index| *index < rows.len()),
        };

        if let Some(row) = target_row.and_then(|index| rows.get(index)) {
            let (offset, upstream) = layout.offset_in_row(row, goal_x - layout.bounds.left());
            let position = TextPosition {
                block: head.block,
                offset,
            };
            self.move_head(anchor, position, extend, cx);
            self.caret_upstream = upstream;
            self.goal_x = Some(goal_x);
            return;
        }

        // Past the first or last row the caret continues into the neighboring block, and
        // below the last one there is nowhere to go but the edge of the text.
        let Some(neighbor) = self.adjacent_text_block(head.block, direction) else {
            self.move_head(anchor, block_edge, extend, cx);
            return;
        };
        let (offset, upstream) = match layouts.get(&neighbor) {
            Some(neighbor_layout) => {
                let rows = neighbor_layout.rows();
                let row = match direction {
                    Direction::Backward => rows.last(),
                    Direction::Forward => rows.first(),
                };
                row.map_or((0, false), |row| {
                    neighbor_layout.offset_in_row(row, goal_x - neighbor_layout.bounds.left())
                })
            }
            None => (0, false),
        };
        let position = TextPosition {
            block: neighbor,
            offset,
        };
        self.move_head(anchor, position, extend, cx);
        self.caret_upstream = upstream;
        self.goal_x = Some(goal_x);
    }

    fn escape(&mut self, _: &Escape, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => {}
            Selection::Blocks { .. } => self.set_selection(Selection::None, cx),
            Selection::Text(selection) => self.select_block(selection.block, cx),
            Selection::Span { .. } => {
                // The blocks the text reaches into become the selected blocks.
                if let Some((anchor, head)) = self.block_range() {
                    self.set_selection(Selection::Blocks { anchor, head }, cx);
                }
            }
        }
    }

    fn enter(&mut self, _: &Enter, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::None => self.start_writing(self.document.last(), Direction::Forward, cx),
            Selection::Blocks { head, .. } => self.start_writing(head, Direction::Forward, cx),
            Selection::Text(selection) => self.newline(selection, cx),
            Selection::Span { .. } => {
                self.replace_span(cx, |this, caret, cx| this.newline(caret, cx));
            }
        }
    }

    fn write_at_start(&mut self, _: &WriteAtStart, _window: &mut Window, cx: &mut Context<Self>) {
        self.write_in_selected_block(Direction::Backward, cx);
    }

    fn write_at_end(&mut self, _: &WriteAtEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.write_in_selected_block(Direction::Forward, cx);
    }

    /// Starts writing at the start or the end of the block the selection ends at, as `Enter`
    /// does. With nothing selected there is no block to write in.
    fn write_in_selected_block(&mut self, side: Direction, cx: &mut Context<Self>) {
        if let Selection::Blocks { head, .. } = self.selection {
            self.start_writing(head, side, cx);
        }
    }

    fn newline(&mut self, selection: TextSelection, cx: &mut Context<Self>) {
        let Some(block) = self.document.block(selection.block) else {
            return;
        };
        let id = selection.block;
        let kind = block.kind.clone();
        let text = block.text.text().to_string();

        if kind.is_verbatim() {
            self.insert_text(selection.range, "\n", cx);
            return;
        }
        if kind == BlockKind::Paragraph
            && let Some(language) = text.strip_prefix("```")
            && !language.contains('`')
        {
            let language = language.trim().to_string();
            self.transact(EditKind::Other, cx, |this| {
                if let Some(block) = this.document.block_mut(id) {
                    block.text = RichText::new();
                }
                this.selection = Selection::Text(TextSelection::caret(id, 0));
                this.document.set_kind(id, BlockKind::Code { language })
            });
            return;
        }
        if kind == BlockKind::Paragraph && matches!(text.as_str(), "---" | "***" | "___") {
            self.transact(EditKind::Other, cx, |this| {
                if let Some(block) = this.document.block_mut(id) {
                    block.text = RichText::new();
                }
                this.convert_to_divider(id)
            });
            return;
        }
        if text.is_empty() && kind != BlockKind::Paragraph {
            // Enter on an empty item ends the list: the item moves out one level, and at the
            // top level it becomes a plain paragraph.
            self.transact(EditKind::Other, cx, |this| {
                if kind.is_list_item() && this.document.parent(id).is_some() {
                    this.document.outdent(id, id)
                } else {
                    this.document.set_kind(id, BlockKind::Paragraph)
                }
            });
            return;
        }
        self.transact(EditKind::Other, cx, |this| {
            if let Some(block) = this.document.block_mut(id) {
                block.text.delete(selection.range.clone());
            }
            match this.document.split(id, selection.range.start) {
                Some(caret_block) => {
                    this.selection = Selection::Text(TextSelection::caret(caret_block, 0));
                    true
                }
                None => false,
            }
        });
    }

    /// Turns `block` into a divider and continues writing in a new paragraph below it, which
    /// takes over the text of the block.
    fn convert_to_divider(&mut self, block: BlockId) -> bool {
        let Some(text) = self
            .document
            .block_mut(block)
            .map(|block| std::mem::take(&mut block.text))
        else {
            return false;
        };
        if !self.document.set_kind(block, BlockKind::Divider) {
            return false;
        }
        let paragraph = Block::paragraph(text);
        match self.document.insert_after(block, vec![paragraph]).first() {
            Some(&paragraph) => {
                self.selection = Selection::Text(TextSelection::caret(paragraph, 0));
                true
            }
            None => false,
        }
    }

    fn line_break(&mut self, _: &LineBreak, _window: &mut Window, cx: &mut Context<Self>) {
        let (block, range) = match (self.text_selection(), self.span()) {
            (Some(selection), _) => (selection.block, selection.range.clone()),
            // The line break lands where the selection starts.
            (None, Some((start, _))) => (start.block, 0..0),
            (None, None) => return,
        };
        let is_heading = self
            .document
            .block(block)
            .is_some_and(|block| matches!(block.kind, BlockKind::Heading(_)));
        // A Markdown heading is a single line.
        if !is_heading {
            self.insert_text(range, "\n", cx);
        }
    }

    fn new_block_below(&mut self, _: &NewBlockBelow, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((first, second)) = self.block_range() else {
            return;
        };
        let Some(&last) = self.document.siblings_between(first, second).last() else {
            return;
        };
        self.write_in_new_block(last, Direction::Forward, BlockKind::Paragraph, cx);
    }

    fn open_below(&mut self, _: &OpenBelow, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_block(Direction::Forward, cx);
    }

    fn open_above(&mut self, _: &OpenAbove, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_block(Direction::Backward, cx);
    }

    /// Adds an empty block below or above the selected blocks, at their nesting level, and
    /// starts writing in it. The block carries on from the one it is added next to, so next
    /// to a list item it is another item. With nothing selected it goes to the end or the
    /// start of the document.
    fn open_block(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let neighbor = match self.block_range() {
            Some((first, second)) => {
                let siblings = self.document.siblings_between(first, second);
                match direction {
                    Direction::Backward => siblings.first().copied(),
                    Direction::Forward => siblings.last().copied(),
                }
            }
            None => Some(match direction {
                Direction::Backward => self.document.first(),
                Direction::Forward => self.document.last_top_level(),
            }),
        };
        let Some(neighbor) = neighbor else {
            return;
        };
        let Some(kind) = self
            .document
            .block(neighbor)
            .map(|block| block.kind.continuation())
        else {
            return;
        };
        self.write_in_new_block(neighbor, direction, kind, cx);
    }

    fn backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        self.delete_text(Direction::Backward, Unit::Character, cx);
    }

    fn delete(&mut self, _: &Delete, _window: &mut Window, cx: &mut Context<Self>) {
        self.delete_text(Direction::Forward, Unit::Character, cx);
    }

    fn delete_word_backward(
        &mut self,
        _: &DeleteWordBackward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_text(Direction::Backward, Unit::Word, cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_text(Direction::Backward, Unit::Line, cx);
    }

    fn delete_text(&mut self, direction: Direction, unit: Unit, cx: &mut Context<Self>) {
        let selection = match self.selection.clone() {
            Selection::None => return,
            Selection::Blocks { anchor, head } => {
                self.transact(EditKind::Other, cx, |this| {
                    match this.document.delete(anchor, head) {
                        Some(block) => {
                            this.selection = Selection::Blocks {
                                anchor: block,
                                head: block,
                            };
                            true
                        }
                        None => false,
                    }
                });
                return;
            }
            Selection::Span { .. } => {
                self.delete_span(cx);
                return;
            }
            Selection::Text(selection) => selection,
        };
        let id = selection.block;
        let range = if selection.range.is_empty() {
            let head = TextPosition {
                block: id,
                offset: selection.head(),
            };
            let (target, _) = self.offset_after_move(head, direction, unit);
            target.min(selection.head())..target.max(selection.head())
        } else {
            selection.range.clone()
        };
        if !range.is_empty() {
            self.transact(EditKind::Deleting, cx, |this| {
                let Some(block) = this.document.block_mut(id) else {
                    return false;
                };
                block.text.delete(range.clone());
                this.selection = Selection::Text(TextSelection::caret(id, range.start));
                true
            });
            return;
        }

        // Nothing left to delete inside the block, so the block itself gives way.
        match direction {
            Direction::Backward => {
                let is_paragraph = self
                    .document
                    .block(id)
                    .is_some_and(|block| block.kind == BlockKind::Paragraph);
                self.transact(EditKind::Other, cx, |this| {
                    if !is_paragraph {
                        return this.document.set_kind(id, BlockKind::Paragraph);
                    }
                    match this.document.merge_into_previous(id) {
                        Some((block, offset)) => {
                            this.selection = Selection::Text(TextSelection::caret(block, offset));
                            true
                        }
                        None => false,
                    }
                });
            }
            Direction::Forward => {
                self.transact(EditKind::Other, cx, |this| this.document.merge_next(id));
            }
        }
    }

    fn indent(&mut self, _: &Indent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.text_selection().cloned()
            && self.is_verbatim(selection.block)
        {
            self.insert_text(selection.range, CODE_INDENT, cx);
            return;
        }
        if let Some((first, second)) = self.block_range() {
            self.transact(EditKind::Other, cx, |this| {
                this.document.indent(first, second)
            });
        }
    }

    fn outdent(&mut self, _: &Outdent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((first, second)) = self.block_range() {
            self.transact(EditKind::Other, cx, |this| {
                this.document.outdent(first, second)
            });
        }
    }

    fn move_blocks_up(&mut self, _: &MoveBlocksUp, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((first, second)) = self.block_range() {
            self.transact(EditKind::Other, cx, |this| {
                this.document.move_up(first, second)
            });
        }
    }

    fn move_blocks_down(
        &mut self,
        _: &MoveBlocksDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((first, second)) = self.block_range() {
            self.transact(EditKind::Other, cx, |this| {
                this.document.move_down(first, second)
            });
        }
    }

    /// Selects the text of the block being written in; when that is selected already, or
    /// text is selected across blocks, the text of the whole document.
    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Selection::Text(selection) => {
                let length = self
                    .document
                    .block(selection.block)
                    .map_or(0, |block| block.text.len());
                if selection.range == (0..length) {
                    self.select_all_text(cx);
                    return;
                }
                self.set_selection(
                    Selection::Text(TextSelection {
                        block: selection.block,
                        range: 0..length,
                        reversed: false,
                    }),
                    cx,
                );
            }
            Selection::Span { .. } => self.select_all_text(cx),
            Selection::None | Selection::Blocks { .. } => {
                let blocks = self.document.blocks();
                if let (Some(first), Some(last)) = (blocks.first(), blocks.last()) {
                    self.set_selection(
                        Selection::Blocks {
                            anchor: first.id,
                            head: last.id,
                        },
                        cx,
                    );
                }
            }
        }
    }

    /// Selects the text from the start of the first block that has some to the end of the
    /// last one.
    fn select_all_text(&mut self, cx: &mut Context<Self>) {
        let rows = self.document.rows();
        let mut text_blocks = rows.iter().filter(|row| row.block.kind.has_text());
        let Some(first) = text_blocks.next() else {
            return;
        };
        let last = text_blocks.next_back().unwrap_or(first);
        let anchor = TextPosition {
            block: first.block.id,
            offset: 0,
        };
        let head = TextPosition {
            block: last.block.id,
            offset: last.block.text.len(),
        };
        self.select_text(anchor, head, cx);
    }

    fn is_verbatim(&self, block: BlockId) -> bool {
        self.document
            .block(block)
            .is_some_and(|block| block.kind.is_verbatim())
    }

    /// Replaces `range` of the block being written in with `text`, as typing does. Text
    /// selected across blocks is what gets replaced, whatever `range` is.
    fn insert_text(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        if self.span().is_some() {
            self.replace_span(cx, |this, caret, cx| {
                this.insert_text(caret.range, text, cx)
            });
            return;
        }
        let Some(selection) = self.text_selection().cloned() else {
            return;
        };
        let id = selection.block;
        let pending_style = self.pending_style.take();
        let kind = if text.is_empty() {
            EditKind::Deleting
        } else {
            EditKind::Typing
        };
        let changed = self.transact(kind, cx, |this| {
            let Some(block) = this.document.block_mut(id) else {
                return false;
            };
            if range.is_empty() && text.is_empty() {
                return false;
            }
            let style = if block.kind.is_verbatim() {
                InlineStyle::default()
            } else if let Some(style) = pending_style {
                style
            } else if range.is_empty() {
                block.text.typing_style(range.start)
            } else {
                // Typing over a selection continues the style the selection started with.
                block
                    .text
                    .style_at(range.start)
                    .cloned()
                    .unwrap_or_default()
            };
            let range = block.text.clamp(range.start)..block.text.clamp(range.end);
            block.text.replace(range.clone(), text, style);
            let caret = range.start + text.len();
            this.selection = Selection::Text(TextSelection::caret(id, caret));
            true
        });
        // A space ends a marker, and a dash may be the last one of a divider.
        if changed && matches!(text, " " | "-") {
            self.apply_typing_shortcut(id, cx);
        }
    }

    /// Converts the block when the text typed at its start is a Markdown block marker, e.g.
    /// `# ` for a heading, or the dashes of a divider, which need no space after them. It is
    /// its own undo step, so undoing brings the typed marker back.
    fn apply_typing_shortcut(&mut self, id: BlockId, cx: &mut Context<Self>) {
        let Some(selection) = self.text_selection().cloned() else {
            return;
        };
        let Some(block) = self.document.block(id) else {
            return;
        };
        let caret = selection.head();
        let typed = &block.text.text()[..caret];
        let kind = if typed == DIVIDER_SHORTCUT && block.kind == BlockKind::Paragraph {
            Some(BlockKind::Divider)
        } else {
            typed
                .strip_suffix(' ')
                .and_then(|marker| shortcut_kind(marker, &block.kind))
        };
        let Some(kind) = kind else {
            return;
        };
        self.transact(EditKind::Other, cx, |this| {
            if let Some(block) = this.document.block_mut(id) {
                block.text.delete(0..caret);
            }
            if kind == BlockKind::Divider {
                return this.convert_to_divider(id);
            }
            this.selection = Selection::Text(TextSelection::caret(id, 0));
            this.document.set_kind(id, kind)
        });
    }

    fn toggle_bold(&mut self, _: &ToggleBold, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(Mark::Bold, cx);
    }

    fn toggle_italic(&mut self, _: &ToggleItalic, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(Mark::Italic, cx);
    }

    fn toggle_code(&mut self, _: &ToggleCode, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(Mark::Code, cx);
    }

    fn toggle_strikethrough(
        &mut self,
        _: &ToggleStrikethrough,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_mark(Mark::Strikethrough, cx);
    }

    /// The parts of the text selected across blocks that can carry marks and links: the ones
    /// that are not empty and not literal text.
    fn styleable_span_ranges(&self) -> Vec<(BlockId, Range<usize>)> {
        let Some((start, end)) = self.span() else {
            return Vec::new();
        };
        self.document
            .span_ranges(start, end)
            .into_iter()
            .filter(|(block, range)| !range.is_empty() && !block.kind.is_verbatim())
            .map(|(block, range)| (block.id, range))
            .collect()
    }

    /// Sets the mark on all of the selected text, unless it has the mark everywhere already,
    /// when it is removed.
    fn toggle_span_mark(&mut self, mark: Mark, cx: &mut Context<Self>) {
        let ranges = self.styleable_span_ranges();
        let marked_everywhere = ranges.iter().all(|(id, range)| {
            self.document
                .block(*id)
                .is_some_and(|block| block.text.has_mark(range.clone(), mark))
        });
        self.transact(EditKind::Other, cx, |this| {
            for (id, range) in &ranges {
                if let Some(block) = this.document.block_mut(*id) {
                    block.text.set_mark(range.clone(), mark, !marked_everywhere);
                }
            }
            !ranges.is_empty()
        });
    }

    fn toggle_mark(&mut self, mark: Mark, cx: &mut Context<Self>) {
        if self.span().is_some() {
            self.toggle_span_mark(mark, cx);
            return;
        }
        let Some(selection) = self.text_selection().cloned() else {
            return;
        };
        let Some(block) = self.document.block(selection.block) else {
            return;
        };
        if block.kind.is_verbatim() {
            return;
        }
        if selection.range.is_empty() {
            let mut style = self
                .pending_style
                .take()
                .unwrap_or_else(|| block.text.typing_style(selection.head()));
            style.set(mark, !style.has(mark));
            self.pending_style = Some(style);
            cx.notify();
            return;
        }
        self.transact(EditKind::Other, cx, |this| {
            let Some(block) = this.document.block_mut(selection.block) else {
                return false;
            };
            block.text.toggle_mark(selection.range.clone(), mark);
            true
        });
    }

    fn toggle_selected_todos(
        &mut self,
        _: &ToggleTodos,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Selection::Blocks { anchor, head } = self.selection {
            let blocks = self.document.siblings_between(anchor, head);
            self.toggle_todos(&blocks, cx);
        }
    }

    /// Checks the to-dos among `blocks`, unless all of them are checked already, when they
    /// are unchecked. The to-dos nested in them keep their state.
    fn toggle_todos(&mut self, blocks: &[BlockId], cx: &mut Context<Self>) {
        let checked = blocks.iter().any(|id| {
            self.document
                .block(*id)
                .is_some_and(|block| block.kind == BlockKind::Todo { checked: false })
        });
        self.transact(EditKind::Other, cx, |this| {
            let mut changed = false;
            for id in blocks {
                if let Some(Block {
                    kind: BlockKind::Todo { checked: state },
                    ..
                }) = this.document.block_mut(*id)
                {
                    *state = checked;
                    changed = true;
                }
            }
            changed
        });
    }

    fn copy(&mut self, _: &actions::Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_content() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_content() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.delete_text(Direction::Backward, Unit::Character, cx);
        }
    }

    /// What copying the selection puts on the clipboard: the plain selected text within a
    /// block, and the selected blocks as Markdown otherwise. Text selected across blocks
    /// is the blocks it reaches into, with the first and the last one cut to the selection.
    fn selected_content(&self) -> Option<String> {
        match &self.selection {
            Selection::None => None,
            Selection::Blocks { anchor, head } => {
                let blocks = self.document.blocks_between(*anchor, *head);
                Some(markdown::serialize_blocks(&blocks))
            }
            Selection::Text(selection) => {
                let text = self.document.block(selection.block)?.text.text();
                (!selection.range.is_empty()).then(|| text[selection.range.clone()].to_string())
            }
            Selection::Span { .. } => {
                let (start, end) = self.span()?;
                Some(markdown::serialize_blocks(
                    &self.document.copy_span(start, end),
                ))
            }
        }
    }

    fn paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        match self.selection.clone() {
            Selection::None => {}
            Selection::Blocks { anchor, head } => self.paste_blocks(anchor, head, &text, cx),
            Selection::Text(selection) => self.paste_text(selection, &text, cx),
            Selection::Span { .. } => match pasted_url(&text) {
                Some(url) => self.link_span(url, cx),
                None => self.replace_span(cx, |this, caret, cx| this.paste_text(caret, &text, cx)),
            },
        }
    }

    /// Makes the text selected across blocks a link, as pasting a URL over it does.
    fn link_span(&mut self, url: Arc<str>, cx: &mut Context<Self>) {
        let ranges = self.styleable_span_ranges();
        self.transact(EditKind::Other, cx, |this| {
            for (id, range) in &ranges {
                if let Some(block) = this.document.block_mut(*id) {
                    block.text.set_link(range.clone(), Some(url.clone()));
                }
            }
            !ranges.is_empty()
        });
    }

    fn paste_blocks(&mut self, anchor: BlockId, head: BlockId, text: &str, cx: &mut Context<Self>) {
        let blocks = markdown::parse_blocks(text);
        let Some(&last) = self.document.siblings_between(anchor, head).last() else {
            return;
        };
        self.transact(EditKind::Other, cx, |this| {
            let inserted = this.document.insert_after(last, blocks);
            match (inserted.first(), inserted.last()) {
                (Some(&first), Some(&last)) => {
                    this.selection = Selection::Blocks {
                        anchor: first,
                        head: last,
                    };
                    true
                }
                _ => false,
            }
        });
    }

    fn paste_text(&mut self, selection: TextSelection, text: &str, cx: &mut Context<Self>) {
        let id = selection.block;
        if self.is_verbatim(id) {
            self.insert_text(selection.range, text, cx);
            return;
        }
        if !selection.range.is_empty()
            && let Some(url) = pasted_url(text)
        {
            self.transact(EditKind::Other, cx, |this| {
                let Some(block) = this.document.block_mut(id) else {
                    return false;
                };
                block.text.set_link(selection.range.clone(), Some(url));
                true
            });
            return;
        }
        if !text.trim_end_matches('\n').contains('\n') {
            self.insert_text(selection.range, text.trim_end_matches('\n'), cx);
            return;
        }

        let mut blocks = markdown::parse_blocks(text);
        self.transact(EditKind::Other, cx, |this| {
            let Some(block) = this.document.block_mut(id) else {
                return false;
            };
            block.text.delete(selection.range.clone());
            if let [single] = blocks.as_slice()
                && single.kind == BlockKind::Paragraph
            {
                let inserted = blocks.remove(0).text;
                let caret = selection.range.start + inserted.len();
                block.text.insert_rich(selection.range.start, inserted);
                this.selection = Selection::Text(TextSelection::caret(id, caret));
                return true;
            }
            let replaces_empty_paragraph =
                block.kind == BlockKind::Paragraph && block.text.is_empty();
            let inserted = this.document.insert_after(id, blocks);
            let Some(&last) = inserted.last() else {
                return false;
            };
            if replaces_empty_paragraph {
                this.document.remove(id);
            }
            this.selection = match this.document.block(last) {
                Some(block) if block.kind.has_text() => {
                    Selection::Text(TextSelection::caret(last, block.text.len()))
                }
                _ => Selection::Blocks {
                    anchor: last,
                    head: last,
                },
            };
            true
        });
    }

    /// The block whose painted text is at, or vertically closest to, `position`.
    fn block_at(&self, position: Point<Pixels>) -> Option<(BlockId, BlockLayout)> {
        let layouts = self.layouts.borrow();
        layouts
            .iter()
            .min_by_key(|(_, layout)| {
                if position.y < layout.bounds.top() {
                    layout.bounds.top() - position.y
                } else if position.y > layout.bounds.bottom() {
                    position.y - layout.bounds.bottom()
                } else {
                    Pixels::ZERO
                }
            })
            .map(|(id, layout)| (*id, layout.clone()))
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        let Some((id, layout)) = self.block_at(event.position) else {
            return;
        };
        let Some(block) = self.document.block(id) else {
            return;
        };
        if !block.kind.has_text() {
            self.select_block(id, cx);
            return;
        }
        let (offset, upstream) = layout.offset_for_point(event.position);
        if event.modifiers.platform
            && let Some(link) = block.text.link_at(offset)
        {
            cx.open_url(link);
            return;
        }
        let text = block.text.text();
        self.drag = None;
        let selection = match event.click_count {
            1 => {
                self.drag = Some(Drag::Characters);
                match self.text_endpoints() {
                    // Shift extends the selection from where it started, into any block.
                    Some((anchor, _)) if event.modifiers.shift => {
                        text_selection_between(anchor, TextPosition { block: id, offset })
                    }
                    _ => Selection::Text(TextSelection::caret(id, offset)),
                }
            }
            2 => {
                let range = word_range(text, offset);
                self.drag = Some(Drag::Words {
                    start: TextPosition {
                        block: id,
                        offset: range.start,
                    },
                    end: TextPosition {
                        block: id,
                        offset: range.end,
                    },
                });
                Selection::Text(TextSelection {
                    block: id,
                    range,
                    reversed: false,
                })
            }
            _ => Selection::Text(TextSelection {
                block: id,
                range: 0..text.len(),
                reversed: false,
            }),
        };
        self.set_selection(selection, cx);
        self.caret_upstream = upstream;
    }

    /// The text position that dragging the mouse to `point` selects up to, when the
    /// selection started at `anchor`. A block without text (a divider) cannot be selected up
    /// to, so the selection stops just short of it.
    fn position_for_drag(
        &self,
        point: Point<Pixels>,
        anchor: TextPosition,
    ) -> Option<TextPosition> {
        let (id, layout) = self.block_at(point)?;
        if self.document.block(id)?.kind.has_text() {
            let (offset, _) = layout.offset_for_point(point);
            return Some(TextPosition { block: id, offset });
        }
        if self.document.order(id, anchor.block)? == Ordering::Greater {
            let block = self.adjacent_text_block(id, Direction::Backward)?;
            let offset = self.document.block(block)?.text.len();
            Some(TextPosition { block, offset })
        } else {
            let block = self.adjacent_text_block(id, Direction::Forward)?;
            Some(TextPosition { block, offset: 0 })
        }
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = self.drag else {
            return;
        };
        let Some((current_anchor, current_head)) = self.text_endpoints() else {
            return;
        };
        let (anchor, head) = match drag {
            Drag::Characters => {
                let Some(position) = self.position_for_drag(event.position, current_anchor) else {
                    return;
                };
                (current_anchor, position)
            }
            Drag::Words { start, end } => {
                let Some(position) = self.position_for_drag(event.position, start) else {
                    return;
                };
                let Some(block) = self.document.block(position.block) else {
                    return;
                };
                let word = word_range(block.text.text(), position.offset);
                let word_start = TextPosition {
                    block: position.block,
                    offset: word.start,
                };
                let word_end = TextPosition {
                    block: position.block,
                    offset: word.end,
                };
                // The selection always holds the word that was double-clicked and the one
                // under the pointer, whichever side of it that is.
                if self.document.compare(word_start, start) == Some(Ordering::Less) {
                    (end, word_start)
                } else {
                    (start, word_end)
                }
            }
        };
        if (anchor, head) != (current_anchor, current_head) {
            self.select_text(anchor, head, cx);
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.drag = None;
    }

    /// Scrolls so that the caret, or the selected block, is inside the viewport. It runs
    /// after a frame was painted, because only then is the position of the selection known.
    fn scroll_selection_into_view(&mut self, cx: &mut Context<Self>) {
        self.needs_autoscroll = false;
        let target = {
            let layouts = self.layouts.borrow();
            match &self.selection {
                Selection::None => None,
                Selection::Blocks { head, .. } => layouts.get(head).map(|layout| layout.bounds),
                Selection::Text(selection) => layouts
                    .get(&selection.block)
                    .and_then(|layout| layout.caret_bounds(selection.head(), self.caret_upstream)),
                Selection::Span { head, .. } => layouts
                    .get(&head.block)
                    .and_then(|layout| layout.caret_bounds(head.offset, self.caret_upstream)),
            }
        };
        let Some(target) = target else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        let top = viewport.top() + SCROLL_MARGIN;
        let bottom = viewport.bottom() - SCROLL_MARGIN;
        let mut offset = self.scroll_handle.offset();
        if target.top() < top || target.size.height > bottom - top {
            offset.y += top - target.top();
        } else if target.bottom() > bottom {
            offset.y -= target.bottom() - bottom;
        } else {
            return;
        }
        let max_offset = self.scroll_handle.max_offset();
        offset.y = offset.y.clamp(-max_offset.y, Pixels::ZERO);
        if offset != self.scroll_handle.offset() {
            self.scroll_handle.set_offset(offset);
            cx.notify();
        }
    }

    fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add(KEY_CONTEXT);
        context.set(
            "mode",
            match self.mode() {
                Mode::Idle => "idle",
                Mode::Navigation => "navigation",
                Mode::Writing => "writing",
            },
        );
        context
    }

    fn text_runs(&self, block: &Block, theme: &Theme) -> Vec<TextRun> {
        let is_heading = matches!(block.kind, BlockKind::Heading(_));
        let is_done = block.kind == BlockKind::Todo { checked: true };
        let marked_range = self
            .text_selection()
            .filter(|selection| selection.block == block.id)
            .and(self.marked_range.clone());

        let mut runs = Vec::new();
        for (range, style) in block.text.styled_ranges() {
            let monospace = block.kind.is_verbatim() || style.code || style.raw;
            let font = Font {
                family: if monospace { MONO_FONT } else { UI_FONT }.into(),
                features: Default::default(),
                fallbacks: None,
                weight: if style.bold || is_heading {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                },
                style: if style.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
            };
            let color = if style.link.is_some() {
                theme.link
            } else if style.raw || is_done || block.kind == BlockKind::Raw {
                theme.muted
            } else {
                theme.text
            };
            let run = TextRun {
                len: 0,
                font,
                color,
                background_color: (style.code && !block.kind.is_verbatim())
                    .then_some(theme.code_background),
                underline: style.link.is_some().then_some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(theme.link),
                    wavy: false,
                }),
                strikethrough: (style.strikethrough || is_done).then_some(StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(color),
                }),
            };

            // Text being composed by an input method is underlined, which can split a run.
            let mut pieces = vec![(range.clone(), false)];
            if let Some(marked) = &marked_range {
                let start = marked.start.clamp(range.start, range.end);
                let end = marked.end.clamp(range.start, range.end);
                pieces = vec![
                    (range.start..start, false),
                    (start..end, true),
                    (end..range.end, false),
                ];
            }
            for (piece, is_marked) in pieces {
                if piece.is_empty() {
                    continue;
                }
                let mut run = run.clone();
                run.len = piece.len();
                if is_marked {
                    run.underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(color),
                        wavy: false,
                    });
                }
                runs.push(run);
            }
        }
        runs
    }

    fn render_block_text(
        &self,
        block: &Block,
        theme: &Theme,
        is_focused: bool,
        span_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) -> BlockText {
        let text = SharedString::from(block.text.text().to_string());
        let runs = self.text_runs(block, theme);
        let mut element = BlockText::new(block.id, text, runs, self.layouts.clone());
        if let Some(selection) = self.text_selection()
            && selection.block == block.id
        {
            element = element.input(self.focus_handle.clone(), cx.entity());
            if !selection.range.is_empty() {
                element = element.selection(selection.range.clone(), theme.text_selection);
            } else if is_focused && self.caret_visible {
                element = element.caret(Caret {
                    offset: selection.head(),
                    upstream: self.caret_upstream,
                    color: theme.caret,
                });
            }
        }
        if let Some(range) = span_range {
            element = element.selection(range, theme.text_selection);
            // Text input goes to the block that has the caret.
            if matches!(&self.selection, Selection::Span { head, .. } if head.block == block.id) {
                element = element.input(self.focus_handle.clone(), cx.entity());
            }
        }
        element
    }

    fn render_row(
        &self,
        row: &Row,
        highlight: RowHighlight,
        is_focused: bool,
        span_range: Option<Range<usize>>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let block = row.block;
        let metrics = BlockMetrics::for_kind(&block.kind);
        let id = block.id;
        // Rows touch each other, so the bars of consecutive rows read as one line running
        // down the whole quote.
        let quote_bar = || {
            div()
                .w(INDENT)
                .flex_none()
                .flex()
                .child(div().w(zoomed(3.)).bg(theme.faint))
        };

        let marker = match &block.kind {
            BlockKind::Bullet => Some(
                div()
                    .w(INDENT)
                    .flex_none()
                    .h(metrics.line_height)
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .ml(zoomed(5.))
                            .size(zoomed(5.))
                            .rounded_full()
                            .bg(theme.muted),
                    )
                    .into_any_element(),
            ),
            BlockKind::Numbered => Some(
                div()
                    .min_w(INDENT)
                    .flex_none()
                    .pr(zoomed(6.))
                    .text_color(theme.muted)
                    .child(format!("{}.", row.ordinal))
                    .into_any_element(),
            ),
            BlockKind::Todo { checked } => {
                let checked = *checked;
                Some(
                    div()
                        .w(INDENT)
                        .flex_none()
                        .h(metrics.line_height)
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .id(("todo", id as usize))
                                .size(zoomed(15.))
                                .rounded(zoomed(3.))
                                .border_1()
                                .border_color(if checked { theme.accent } else { theme.muted })
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(zoomed(11.))
                                .line_height(zoomed(13.))
                                .text_color(theme.background)
                                .when(checked, |checkbox| checkbox.bg(theme.accent).child("✓"))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.toggle_todos(&[id], cx);
                                    }),
                                ),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };

        let content = match &block.kind {
            BlockKind::Divider => {
                let layouts = self.layouts.clone();
                let color = theme.faint;
                div()
                    .flex_1()
                    .h(zoomed(25.))
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let thickness = zoomed(1.).to_pixels(window.rem_size()).max(px(1.));
                                let line = Bounds::new(
                                    point(bounds.left(), bounds.center().y),
                                    gpui::size(bounds.size.width, thickness),
                                );
                                window.paint_quad(fill(line, color));
                                layouts
                                    .borrow_mut()
                                    .insert(id, BlockLayout::without_text(bounds));
                            },
                        )
                        .size_full(),
                    )
                    .into_any_element()
            }
            BlockKind::Code { .. } | BlockKind::Raw => {
                let label = match &block.kind {
                    BlockKind::Code { language } => language.clone(),
                    _ => "markdown".to_string(),
                };
                div()
                    .flex_1()
                    .min_w_0()
                    .px(zoomed(12.))
                    .py(zoomed(8.))
                    .rounded(zoomed(6.))
                    .bg(theme.code_background)
                    .when(!label.is_empty(), |code| {
                        code.child(
                            div()
                                .text_size(zoomed(11.))
                                .line_height(zoomed(16.))
                                .text_color(theme.muted)
                                .child(label),
                        )
                    })
                    .child(
                        div().cursor_text().child(
                            self.render_block_text(block, theme, is_focused, span_range, cx),
                        ),
                    )
                    .into_any_element()
            }
            _ => div()
                .flex_1()
                .min_w_0()
                .cursor_text()
                .child(self.render_block_text(block, theme, is_focused, span_range, cx))
                .into_any_element(),
        };

        div()
            .w_full()
            .flex()
            .flex_row()
            .px(zoomed(8.))
            .when(highlight.selected, |row| row.bg(theme.block_selection))
            .when(highlight.first, |row| row.rounded_t(zoomed(4.)))
            .when(highlight.last, |row| row.rounded_b(zoomed(4.)))
            .children(row.quote_ancestors.iter().map(|is_quote| {
                if *is_quote {
                    quote_bar().into_any_element()
                } else {
                    div().w(INDENT).flex_none().into_any_element()
                }
            }))
            .when(block.kind == BlockKind::Quote, |row| row.child(quote_bar()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .pt(metrics.padding_top)
                    .pb(metrics.padding_bottom)
                    .text_size(metrics.font_size)
                    .line_height(metrics.line_height)
                    .text_color(theme.text)
                    .children(marker)
                    .child(content),
            )
            .into_any_element()
    }
}

/// How a row takes part in the highlight of the selected blocks. The rows of a selection
/// form one shape, so only its first and last rows have rounded corners.
#[derive(Clone, Copy, Default)]
struct RowHighlight {
    selected: bool,
    first: bool,
    last: bool,
}

/// Font size and spacing of a block type.
struct BlockMetrics {
    font_size: Rems,
    line_height: Rems,
    padding_top: Rems,
    padding_bottom: Rems,
}

impl BlockMetrics {
    fn for_kind(kind: &BlockKind) -> Self {
        let (font_size, line_height, padding_top, padding_bottom) = match kind {
            BlockKind::Heading(1) => (28., 36., 20., 4.),
            BlockKind::Heading(2) => (22., 30., 16., 4.),
            BlockKind::Heading(3) => (18., 26., 12., 3.),
            BlockKind::Heading(_) => (15., 24., 10., 3.),
            BlockKind::Code { .. } | BlockKind::Raw => (13., 20., 4., 4.),
            BlockKind::Divider => (15., 24., 0., 0.),
            BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo { .. } => (15., 24., 1., 1.),
            _ => (15., 24., 4., 4.),
        };
        Self {
            font_size: zoomed(font_size),
            line_height: zoomed(line_height),
            padding_top: zoomed(padding_top),
            padding_bottom: zoomed(padding_bottom),
        }
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let is_focused = self.focus_handle.is_focused(window);
        if self.needs_autoscroll {
            cx.on_next_frame(window, |this, _, cx| this.scroll_selection_into_view(cx));
        }
        let rows = self.document.rows();
        let mut selected_blocks = match &self.selection {
            Selection::Blocks { anchor, head } => self.document.subtree_ids(*anchor, *head),
            _ => HashSet::new(),
        };
        // Text selected across blocks highlights the selected part of each block's text. A
        // block without text (a divider) is highlighted as a whole.
        let mut span_ranges = HashMap::new();
        if let Some((start, end)) = self.span() {
            for (block, range) in self.document.span_ranges(start, end) {
                if !block.kind.has_text() {
                    selected_blocks.insert(block.id);
                    continue;
                }
                // Where the selection goes on into the next block, the highlight runs past
                // the end of the text, which shows as a line break.
                let selected_end = if block.id == end.block {
                    range.end
                } else {
                    range.end + 1
                };
                span_ranges.insert(block.id, range.start..selected_end);
            }
        }
        {
            let row_ids: HashSet<BlockId> = rows.iter().map(|row| row.block.id).collect();
            self.layouts
                .borrow_mut()
                .retain(|id, _| row_ids.contains(id));
        }

        let is_selected = |index: usize| {
            rows.get(index)
                .is_some_and(|row| selected_blocks.contains(&row.block.id))
        };
        let mut elements = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let selected = is_selected(index);
            let highlight = RowHighlight {
                selected,
                first: selected && !index.checked_sub(1).is_some_and(is_selected),
                last: selected && !is_selected(index + 1),
            };
            let span_range = span_ranges.remove(&row.block.id);
            elements.push(self.render_row(row, highlight, is_focused, span_range, &theme, cx));
        }

        let mode = match self.mode() {
            Mode::Idle => "",
            Mode::Navigation => "NAVIGATION",
            Mode::Writing => "WRITING",
        };

        div()
            .id("editor")
            .key_context(self.key_context())
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::move_word_left))
            .on_action(cx.listener(Self::move_word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::move_to_line_start))
            .on_action(cx.listener(Self::move_to_line_end))
            .on_action(cx.listener(Self::select_to_line_start))
            .on_action(cx.listener(Self::select_to_line_end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::line_break))
            .on_action(cx.listener(Self::new_block_below))
            .on_action(cx.listener(Self::open_below))
            .on_action(cx.listener(Self::open_above))
            .on_action(cx.listener(Self::write_at_start))
            .on_action(cx.listener(Self::write_at_end))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_backward))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::move_blocks_up))
            .on_action(cx.listener(Self::move_blocks_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::toggle_bold))
            .on_action(cx.listener(Self::toggle_italic))
            .on_action(cx.listener(Self::toggle_code))
            .on_action(cx.listener(Self::toggle_strikethrough))
            .on_action(cx.listener(Self::toggle_selected_todos))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        div()
                            .id("document")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(
                                div()
                                    .mx_auto()
                                    .w_full()
                                    .max_w(CONTENT_WIDTH)
                                    .px(zoomed(24.))
                                    .pt(zoomed(32.))
                                    .pb(zoomed(160.))
                                    .flex()
                                    .flex_col()
                                    .children(elements),
                            ),
                    )
                    // Over the note, and after it: it is drawn from how the note was laid out.
                    .child(self.scrollbar.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .h(zoomed(24.))
                    .px(zoomed(12.))
                    .flex()
                    .items_center()
                    .text_size(zoomed(11.))
                    .text_color(theme.muted)
                    .child(mode),
            )
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.active_text()?.text();
        let range = range_from_utf16(text, &range_utf16);
        adjusted_range.replace(range_to_utf16(text, &range));
        Some(text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.input_selection()?;
        let text = self.active_text()?.text();
        Some(UTF16Selection {
            range: range_to_utf16(text, &selection.range),
            reversed: selection.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let text = self.active_text()?.text();
        self.marked_range
            .as_ref()
            .map(|range| range_to_utf16(text, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.span().is_some() {
            // The text typed replaces what is selected across blocks.
            self.marked_range = None;
            self.insert_text(0..0, new_text, cx);
            return;
        }
        let Some(selection) = self.text_selection().cloned() else {
            return;
        };
        let Some(text) = self.active_text().map(|text| text.text()) else {
            return;
        };
        let range = range_utf16
            .map(|range_utf16| range_from_utf16(text, &range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(selection.range);
        self.marked_range = None;
        self.insert_text(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut range_utf16 = range_utf16;
        if self.span().is_some() {
            // Composing starts by deleting what is selected across blocks.
            self.delete_span(cx);
            range_utf16 = None;
        }
        let Some(selection) = self.text_selection().cloned() else {
            return;
        };
        let Some(text) = self.active_text().map(|text| text.text()) else {
            return;
        };
        let range = range_utf16
            .map(|range_utf16| range_from_utf16(text, &range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(selection.range);
        self.insert_text(range.clone(), new_text, cx);

        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        if let Some(selected_utf16) = new_selected_range_utf16 {
            let selected = range_from_utf16(new_text, &selected_utf16);
            self.selection = Selection::Text(TextSelection {
                block: selection.block,
                range: range.start + selected.start..range.start + selected.end,
                reversed: false,
            });
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let selection = self.input_selection()?;
        let range = range_from_utf16(self.active_text()?.text(), &range_utf16);
        let layouts = self.layouts.borrow();
        let layout = layouts.get(&selection.block)?;
        layout
            .range_bounds(range.clone())
            .into_iter()
            .next()
            .or_else(|| layout.caret_bounds(range.start, false))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let selection = self.input_selection()?;
        let text = self.active_text()?.text();
        let layouts = self.layouts.borrow();
        let (offset, _) = layouts.get(&selection.block)?.offset_for_point(point);
        Some(offset_to_utf16(text, offset))
    }
}

/// The selection of the text from `anchor` to `head`: a selection within a block when they
/// are in the same one, and across blocks otherwise.
fn text_selection_between(anchor: TextPosition, head: TextPosition) -> Selection {
    if anchor.block != head.block {
        return Selection::Span { anchor, head };
    }
    Selection::Text(TextSelection {
        block: anchor.block,
        range: anchor.offset.min(head.offset)..anchor.offset.max(head.offset),
        reversed: head.offset < anchor.offset,
    })
}

/// The block type that typing `marker` followed by a space at the start of a block of type
/// `current` turns it into.
fn shortcut_kind(marker: &str, current: &BlockKind) -> Option<BlockKind> {
    let task = match marker {
        "[]" | "[ ]" => Some(BlockKind::Todo { checked: false }),
        "[x]" | "[X]" => Some(BlockKind::Todo { checked: true }),
        _ => None,
    };
    match current {
        BlockKind::Paragraph => {}
        // Typing a Markdown task item goes through a bullet first: `- ` then `[ ] `.
        BlockKind::Bullet => return task,
        _ => return None,
    }
    if task.is_some() {
        return task;
    }
    match marker {
        "-" | "*" | "+" => return Some(BlockKind::Bullet),
        ">" => return Some(BlockKind::Quote),
        "```" => {
            return Some(BlockKind::Code {
                language: String::new(),
            });
        }
        _ => {}
    }
    if (1..=6).contains(&marker.len()) && marker.chars().all(|character| character == '#') {
        return Some(BlockKind::Heading(marker.len() as u8));
    }
    let digits = marker.trim_end_matches(['.', ')']);
    let is_ordinal = digits.len() + 1 == marker.len()
        && !digits.is_empty()
        && digits.len() <= 9
        && digits.chars().all(|character| character.is_ascii_digit());
    is_ordinal.then_some(BlockKind::Numbered)
}

/// The link target when the pasted text is nothing but a URL.
fn pasted_url(text: &str) -> Option<Arc<str>> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let url = url::Url::parse(text).ok()?;
    let is_link = match url.scheme() {
        "http" | "https" | "ftp" => url.has_host(),
        "mailto" => true,
        _ => false,
    };
    is_link.then(|| text.into())
}

fn previous_grapheme(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(index, _)| (index < offset).then_some(index))
        .unwrap_or(0)
}

fn next_grapheme(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(index, _)| (index > offset).then_some(index))
        .unwrap_or(text.len())
}

fn previous_word_start(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .take_while(|(index, _)| *index < offset)
        .filter(|(_, word)| !word.trim().is_empty())
        .last()
        .map_or(0, |(index, _)| index)
}

fn next_word_end(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .map(|(index, word)| (index + word.len(), word))
        .find(|(end, word)| *end > offset && !word.trim().is_empty())
        .map_or(text.len(), |(end, _)| end)
}

/// The word at `offset`, or the whitespace or punctuation there when it is not in a word.
fn word_range(text: &str, offset: usize) -> Range<usize> {
    text.split_word_bound_indices()
        .map(|(index, word)| index..index + word.len())
        .find(|range| range.contains(&offset) || range.end == text.len())
        .unwrap_or(offset..offset)
}

fn offset_from_utf16(text: &str, offset_utf16: usize) -> usize {
    let mut utf16_count = 0;
    for (index, character) in text.char_indices() {
        if utf16_count >= offset_utf16 {
            return index;
        }
        utf16_count += character.len_utf16();
    }
    text.len()
}

fn offset_to_utf16(text: &str, offset: usize) -> usize {
    text.char_indices()
        .take_while(|(index, _)| *index < offset)
        .map(|(_, character)| character.len_utf16())
        .sum()
}

fn range_from_utf16(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range_utf16.start)..offset_from_utf16(text, range_utf16.end)
}

fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}
