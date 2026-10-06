//! Tests for text selected across blocks: how a selection grows past the edge of a block,
//! and what editing, copying and formatting it does.

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use pretty_assertions::assert_eq;

use super::*;

const TWO: &str = "hello world\n\nsecond line\n";

/// Selects the text from `anchor` to `head`, each given as the text of a block and an offset
/// in it.
fn select(
    editor: &Entity<Editor>,
    anchor: (&str, usize),
    head: (&str, usize),
    cx: &mut VisualTestContext,
) {
    editor.update(cx, |editor, cx| {
        let position = |(text, offset): (&str, usize)| TextPosition {
            block: editor.document.find(text),
            offset,
        };
        editor.select_text(position(anchor), position(head), cx);
    });
}

/// Opens [`TWO`] with `world` and `second` selected, from the first block into the second.
fn two_blocks_selected(cx: &mut TestAppContext) -> (Entity<Editor>, VisualTestContext) {
    let (editor, mut cx) = open(TWO, cx);
    select(&editor, ("hello world", 6), ("second line", 6), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p hello «world\np second» line\n");
    (editor, cx)
}

fn clipboard(cx: &VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn shift_left_and_right_continue_into_the_neighboring_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("ab\n\ncd\n", cx);
    cx.simulate_keystrokes("down enter shift-right");
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
    assert_eq!(state(&editor, &mut cx), "p ab«\np »cd\n");
    cx.simulate_keystrokes("shift-right");
    assert_eq!(state(&editor, &mut cx), "p ab«\np c»d\n");

    // Back where it started, it is a selection within one block again.
    cx.simulate_keystrokes("shift-left shift-left");
    assert_eq!(state(&editor, &mut cx), "p abˇ\np cd\n");

    // The same going up, from the start of a block.
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("cmd-left shift-left");
    assert_eq!(state(&editor, &mut cx), "p ab«\np »cd\n");
    cx.simulate_keystrokes("shift-left");
    assert_eq!(state(&editor, &mut cx), "p a«b\np »cd\n");
    // The head is in the first block now, and moves back down from there.
    cx.simulate_keystrokes("shift-right");
    assert_eq!(state(&editor, &mut cx), "p ab«\np »cd\n");
}

#[gpui::test]
fn word_selection_continues_into_the_neighboring_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one two\n\nthree four\n", cx);
    cx.simulate_keystrokes("down enter alt-shift-right");
    assert_eq!(state(&editor, &mut cx), "p one two«\np »three four\n");
    cx.simulate_keystrokes("alt-shift-right");
    assert_eq!(state(&editor, &mut cx), "p one two«\np three» four\n");
    cx.simulate_keystrokes("alt-shift-left");
    assert_eq!(state(&editor, &mut cx), "p one two«\np »three four\n");
    cx.simulate_keystrokes("alt-shift-left");
    assert_eq!(state(&editor, &mut cx), "p one twoˇ\np three four\n");
}

#[gpui::test]
fn line_selection_stays_in_the_block_with_the_caret(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one two\n\nthree four\n", cx);
    cx.simulate_keystrokes("down enter");
    // There is no line end to go to, so this does not leave the block.
    cx.simulate_keystrokes("cmd-shift-right");
    assert_eq!(state(&editor, &mut cx), "p one twoˇ\np three four\n");

    cx.simulate_keystrokes("alt-shift-right alt-shift-right");
    assert_eq!(state(&editor, &mut cx), "p one two«\np three» four\n");
    cx.simulate_keystrokes("cmd-shift-right");
    assert_eq!(state(&editor, &mut cx), "p one two«\np three four»\n");
    cx.simulate_keystrokes("cmd-shift-left");
    assert_eq!(state(&editor, &mut cx), "p one two«\np »three four\n");
}

#[gpui::test]
fn shift_up_and_down_extend_across_blocks_keeping_the_column(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("abcd\n\n---\n\nef\n\nghij\n", cx);
    cx.simulate_keystrokes("down enter left");
    assert_eq!(state(&editor, &mut cx), "p abcˇd\n---\np ef\np ghij\n");

    // The divider has no text and is passed over.
    cx.simulate_keystrokes("shift-down");
    assert_eq!(state(&editor, &mut cx), "p abc«d\n---\np ef»\np ghij\n");
    cx.simulate_keystrokes("shift-down");
    assert_eq!(state(&editor, &mut cx), "p abc«d\n---\np ef\np ghi»j\n");
    cx.simulate_keystrokes("shift-up");
    assert_eq!(state(&editor, &mut cx), "p abc«d\n---\np ef»\np ghij\n");
    cx.simulate_keystrokes("shift-up");
    assert_eq!(state(&editor, &mut cx), "p abcˇd\n---\np ef\np ghij\n");

    // Starting from the last block, the selection grows upwards.
    select(&editor, ("ghij", 3), ("ghij", 3), &mut cx);
    cx.simulate_keystrokes("shift-up");
    assert_eq!(state(&editor, &mut cx), "p abcd\n---\np ef«\np ghi»j\n");
    cx.simulate_keystrokes("shift-up");
    assert_eq!(state(&editor, &mut cx), "p abc«d\n---\np ef\np ghi»j\n");
}

#[gpui::test]
fn shift_up_and_down_stop_at_the_edges_of_the_document(app: &mut TestAppContext) {
    let (editor, mut cx) = open("ab\n\ncd\n", app);
    cx.simulate_keystrokes("down enter shift-up");
    assert_eq!(state(&editor, &mut cx), "p «ab»\np cd\n");

    let (editor, mut cx) = open("ab\n\ncd\n", app);
    cx.simulate_keystrokes("down down enter cmd-left shift-down");
    assert_eq!(state(&editor, &mut cx), "p ab\np «cd»\n");
}

#[gpui::test]
fn up_and_down_move_the_caret_from_the_head_of_a_selection(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("abcd\n\nef\n\nghij\n", cx);
    cx.simulate_keystrokes("down enter left shift-down");
    assert_eq!(state(&editor, &mut cx), "p abc«d\np ef»\np ghij\n");
    cx.simulate_keystrokes("down");
    assert_eq!(state(&editor, &mut cx), "p abcd\np ef\np ghiˇj\n");
}

#[gpui::test]
fn left_and_right_collapse_a_selection_across_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("ab\n\ncd\n", cx);
    cx.simulate_keystrokes("down enter shift-right shift-right left");
    assert_eq!(state(&editor, &mut cx), "p abˇ\np cd\n");
    cx.simulate_keystrokes("shift-right shift-right right");
    assert_eq!(state(&editor, &mut cx), "p ab\np cˇd\n");

    // Whichever end the head is at.
    cx.simulate_keystrokes("shift-left shift-left shift-left");
    assert_eq!(state(&editor, &mut cx), "p a«b\np c»d\n");
    cx.simulate_keystrokes("left");
    assert_eq!(state(&editor, &mut cx), "p aˇb\np cd\n");
    cx.simulate_keystrokes("shift-right shift-right");
    assert_eq!(state(&editor, &mut cx), "p a«b\np »cd\n");
    cx.simulate_keystrokes("right");
    assert_eq!(state(&editor, &mut cx), "p ab\np ˇcd\n");
}

#[gpui::test]
fn the_selection_is_in_writing_mode_where_vim_keys_are_text(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
    type_text("d", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p hello dˇ line\n");
}

#[gpui::test]
fn typing_replaces_the_selection_in_one_undo_step(app: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(app);
    type_text("X", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p hello Xˇ line\n");

    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p hello «world\np second» line\n");
    assert_eq!(saved(&editor, &mut cx), TWO);
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(state(&editor, &mut cx), "p hello Xˇ line\n");

    // What is typed right after goes into the same step.
    let (editor, mut cx) = two_blocks_selected(app);
    type_text("XY", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p hello XYˇ line\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p hello «world\np second» line\n");
}

#[gpui::test]
fn deleting_removes_the_selection_and_joins_the_blocks(cx: &mut TestAppContext) {
    for key in [
        "backspace",
        "shift-backspace",
        "delete",
        "alt-backspace",
        "cmd-backspace",
    ] {
        let (editor, mut cx) = two_blocks_selected(cx);
        cx.simulate_keystrokes(key);
        assert_eq!(state(&editor, &mut cx), "p hello ˇ line\n", "{key}");

        cx.simulate_keystrokes("cmd-z");
        assert_eq!(saved(&editor, &mut cx), TWO, "{key}");
    }
}

#[gpui::test]
fn the_joined_block_keeps_the_type_of_the_first_one(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("# Heading\n\nparagraph\n\n- one\n", cx);
    select(&editor, ("Heading", 4), ("paragraph", 4), &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "h1 Head«ing\np para»graph\n- one\n"
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "h1 Headˇgraph\n- one\n");
    assert_eq!(saved(&editor, &mut cx), "# Headgraph\n\n- one\n");
}

#[gpui::test]
fn deleting_leaves_what_is_nested_outside_the_selection(app: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, app);
    select(&editor, ("a2", 1), ("b", 0), &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "- a\n  - a1\n  - a«2\n    - a2x\n- »b\n- c\n"
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "- a\n  - a1\n  - aˇb\n- c\n");

    // The divider goes too.
    let (editor, mut cx) = open("ab\n\n---\n\ncd\n", app);
    select(&editor, ("ab", 1), ("cd", 1), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p a«b\n---\np c»d\n");
    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "p aˇd\n");
}

#[gpui::test]
fn code_is_not_mixed_with_text_when_deleting(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("```\ncode\n```\n\ntext\n", cx);
    select(&editor, ("code", 2), ("text", 2), &mut cx);
    type_text("X", &mut cx);
    assert_eq!(state(&editor, &mut cx), "code() coXˇ\np xt\n");
}

#[gpui::test]
fn enter_replaces_the_selection_with_a_new_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(state(&editor, &mut cx), "p hello \np ˇ line\n");

    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p hello «world\np second» line\n");
}

#[gpui::test]
fn shift_enter_replaces_the_selection_with_a_line_break(app: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(app);
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(state(&editor, &mut cx), "p hello ⏎ˇ line\n");

    // A heading is a single line, so nothing happens.
    let (editor, mut cx) = open("# Title\n\npara\n", app);
    select(&editor, ("Title", 2), ("para", 2), &mut cx);
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(state(&editor, &mut cx), "h1 Ti«tle\np pa»ra\n");
}

const NOTION: &str = "\
# Welcome to Notion

- [x] Create an account
- [ ] [Download the app](https://example.com/dl) to unlock **offline** mode
  - [ ] Type `/page` to add a **new page**
- [ ] Find pages

Click anywhere below
";

#[gpui::test]
fn copying_gives_the_blocks_as_markdown_cut_to_the_selection(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NOTION, cx);
    select(
        &editor,
        ("Welcome to Notion", 5),
        ("Find pages", 4),
        &mut cx,
    );
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(
        clipboard(&cx).as_deref(),
        Some(
            "\
# me to Notion

- [x] Create an account
- [ ] [Download the app](https://example.com/dl) to unlock **offline** mode
  - [ ] Type `/page` to add a **new page**
- [ ] Find
"
        )
    );
    // Copying does not change the document or the selection.
    assert_eq!(saved(&editor, &mut cx), NOTION);
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
}

#[gpui::test]
fn what_is_copied_from_inside_a_block_stays_plain_text(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("# Title\n", cx);
    cx.simulate_keystrokes("down enter shift-left shift-left");
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(&cx).as_deref(), Some("le"));
    assert_eq!(state(&editor, &mut cx), "h1 Tit«le»\n");
}

#[gpui::test]
fn copied_blocks_that_cross_nesting_levels_are_kept_nested_only_with_their_parents(
    cx: &mut TestAppContext,
) {
    let (editor, mut cx) = open(NESTED, cx);
    // `a` is not selected, so the items nested in it come out at the top level.
    select(&editor, ("a1", 1), ("b", 1), &mut cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(&cx).as_deref(), Some("- 1\n- a2\n  - a2x\n- b\n"));
    // Selected from the start of `a`, its children are selected and stay in it.
    select(&editor, ("a", 0), ("a2", 1), &mut cx);
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(&cx).as_deref(), Some("- a\n  - a1\n  - a\n"));
}

#[gpui::test]
fn cutting_copies_the_blocks_and_deletes_the_selection(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    cx.simulate_keystrokes("cmd-x");
    assert_eq!(clipboard(&cx).as_deref(), Some("world\n\nsecond\n"));
    assert_eq!(state(&editor, &mut cx), "p hello ˇ line\n");

    cx.simulate_keystrokes("cmd-z");
    assert_eq!(saved(&editor, &mut cx), TWO);
}

#[gpui::test]
fn pasting_replaces_the_selection_in_one_undo_step(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    cx.write_to_clipboard(ClipboardItem::new_string("X".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(state(&editor, &mut cx), "p hello Xˇ line\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p hello «world\np second» line\n");

    cx.write_to_clipboard(ClipboardItem::new_string("# H\n\n- one\n".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(state(&editor, &mut cx), "p hello  line\nh1 H\n- oneˇ\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p hello «world\np second» line\n");
}

#[gpui::test]
fn pasting_a_url_over_the_selection_links_the_text_in_every_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    cx.write_to_clipboard(ClipboardItem::new_string("https://example.com".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        saved(&editor, &mut cx),
        "hello [world](https://example.com)\n\n[second](https://example.com) line\n"
    );
    // The selection stays, and undoing removes the links.
    assert_eq!(
        state(&editor, &mut cx),
        "p hello «<a href=\"https://example.com\">world</a>\np <a href=\"https://example.com\">second</a>» line\n"
    );
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(saved(&editor, &mut cx), TWO);
}

#[gpui::test]
fn formatting_applies_to_the_selected_text_of_every_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    cx.simulate_keystrokes("cmd-b");
    assert_eq!(
        saved(&editor, &mut cx),
        "hello **world**\n\n**second** line\n"
    );
    // It is still selected, and toggling again removes the mark.
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
    cx.simulate_keystrokes("cmd-b");
    assert_eq!(saved(&editor, &mut cx), TWO);

    // Marked in one place only, the mark is set everywhere first, and removed the next time.
    select(&editor, ("second line", 0), ("second line", 6), &mut cx);
    cx.simulate_keystrokes("cmd-i");
    assert_eq!(saved(&editor, &mut cx), "hello world\n\n*second* line\n");
    select(&editor, ("hello world", 6), ("second line", 6), &mut cx);
    cx.simulate_keystrokes("cmd-i");
    assert_eq!(saved(&editor, &mut cx), "hello *world*\n\n*second* line\n");
    cx.simulate_keystrokes("cmd-i");
    assert_eq!(saved(&editor, &mut cx), TWO);
}

#[gpui::test]
fn formatting_leaves_code_alone(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one\n\n```\ncode\n```\n\ntwo\n", cx);
    select(&editor, ("one", 1), ("two", 2), &mut cx);
    cx.simulate_keystrokes("cmd-b");
    assert_eq!(
        saved(&editor, &mut cx),
        "o**ne**\n\n```\ncode\n```\n\n**tw**o\n"
    );
}

#[gpui::test]
fn escape_selects_the_blocks_the_text_reaches_into(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    select(&editor, ("a2x", 0), ("b", 1), &mut cx);
    cx.simulate_keystrokes("escape");
    // `a2x` is nested in `a`, which holds the rest of what is between the two.
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n* - b\n  - c\n"
    );
}

#[gpui::test]
fn tab_and_alt_arrows_move_the_blocks_the_text_reaches_into(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- a\n- b\n- c\n", cx);
    select(&editor, ("b", 0), ("c", 1), &mut cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(state(&editor, &mut cx), "- a\n  - «b\n  - c»\n");
    cx.simulate_keystrokes("shift-tab alt-up");
    assert_eq!(state(&editor, &mut cx), "- «b\n- c»\n- a\n");
}

#[gpui::test]
fn select_all_goes_from_the_block_to_the_whole_document(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("ab\n\n---\n\ncd\n\nef\n", cx);
    cx.simulate_keystrokes("down enter cmd-a");
    assert_eq!(state(&editor, &mut cx), "p «ab»\n---\np cd\np ef\n");
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(state(&editor, &mut cx), "p «ab\n---\np cd\np ef»\n");
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(state(&editor, &mut cx), "p «ab\n---\np cd\np ef»\n");

    // A selection across some of the blocks becomes all of them.
    cx.simulate_keystrokes("right");
    select(&editor, ("cd", 1), ("ef", 1), &mut cx);
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(state(&editor, &mut cx), "p «ab\n---\np cd\np ef»\n");

    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "p ˇ\n");
}

#[gpui::test]
fn select_all_in_a_note_with_one_block_stays_in_the_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("only\n", cx);
    cx.simulate_keystrokes("down enter cmd-a cmd-a");
    assert_eq!(state(&editor, &mut cx), "p «only»\n");
}

/// The window position of the left edge of the text of `block`, one row down.
fn start_of(editor: &Entity<Editor>, text: &str, cx: &mut VisualTestContext) -> Point<Pixels> {
    editor.read_with(cx, |editor, _| {
        let bounds = editor.layouts.borrow()[&editor.document.find(text)].bounds;
        point(bounds.left() + px(1.), bounds.center().y)
    })
}

/// A position past the end of the text of `block`, which is the end of its text.
fn end_of(editor: &Entity<Editor>, text: &str, cx: &mut VisualTestContext) -> Point<Pixels> {
    editor.read_with(cx, |editor, _| {
        let bounds = editor.layouts.borrow()[&editor.document.find(text)].bounds;
        point(bounds.right() + px(40.), bounds.center().y)
    })
}

#[gpui::test]
fn dragging_the_mouse_selects_across_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(TWO, cx);
    let (from, to) = (
        start_of(&editor, "hello world", &mut cx),
        end_of(&editor, "second line", &mut cx),
    );
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p ˇhello world\np second line\n");
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p «hello world\np second line»\n");
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());

    // Dragging back into the block it started in gives a selection within it again.
    let inside = end_of(&editor, "hello world", &mut cx);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(inside, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p «hello world»\np second line\n");
}

#[gpui::test]
fn dragging_upwards_selects_across_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(TWO, cx);
    let (from, to) = (
        end_of(&editor, "second line", &mut cx),
        start_of(&editor, "hello world", &mut cx),
    );
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p «hello world\np second line»\n");
    // The selection is anchored where the drag began: moving on with the keyboard
    // shrinks it from the top.
    cx.simulate_keystrokes("shift-right");
    assert_eq!(state(&editor, &mut cx), "p h«ello world\np second line»\n");
}

#[gpui::test]
fn a_drag_stops_short_of_a_divider(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("ab\n\n---\n\ncd\n", cx);
    let from = start_of(&editor, "ab", &mut cx);
    let divider = editor.read_with(&cx, |editor, _| {
        let divider = editor
            .document
            .rows()
            .iter()
            .find(|row| row.block.kind == BlockKind::Divider)
            .map(|row| row.block.id)
            .unwrap();
        editor.layouts.borrow()[&divider].bounds.center()
    });
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(divider, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p «ab»\n---\np cd\n");

    let to = end_of(&editor, "cd", &mut cx);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p «ab\n---\np cd»\n");
}

#[gpui::test]
fn shift_click_extends_the_selection_into_another_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(TWO, cx);
    let (from, to) = (
        start_of(&editor, "hello world", &mut cx),
        end_of(&editor, "second line", &mut cx),
    );
    cx.simulate_click(from, Modifiers::none());
    cx.simulate_click(to, Modifiers::shift());
    assert_eq!(state(&editor, &mut cx), "p «hello world\np second line»\n");

    // A click without Shift starts over.
    cx.simulate_click(to, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p hello world\np second lineˇ\n");
}

#[gpui::test]
fn the_head_of_a_selection_is_scrolled_into_view(cx: &mut TestAppContext) {
    let source: String = (0..200)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect();
    let (editor, mut cx) = open(&source, cx);
    select(&editor, ("paragraph 0", 3), ("paragraph 199", 3), &mut cx);
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let is_visible = editor.read_with(&cx, |editor, _| {
        let Selection::Span { head, .. } = &editor.selection else {
            return false;
        };
        let viewport = editor.scroll_handle.bounds();
        let block = editor.layouts.borrow()[&head.block].bounds;
        block.top() >= viewport.top() && block.bottom() <= viewport.bottom()
    });
    assert!(is_visible, "the last block should be scrolled to");
}

#[gpui::test]
fn an_input_method_composing_over_the_selection_replaces_it(cx: &mut TestAppContext) {
    let (editor, mut cx) = two_blocks_selected(cx);
    // The input method is told about the part of the selection in the block with the caret.
    let selected = editor.update_in(&mut cx, |editor, window, cx| {
        editor.selected_text_range(false, window, cx)
    });
    let selected = selected.expect("the input method is given a selection");
    assert_eq!(selected.range, 0..6);
    assert!(!selected.reversed);

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.replace_and_mark_text_in_range(None, "n", Some(1..1), window, cx)
    });
    assert_eq!(state(&editor, &mut cx), "p hello nˇ line\n");
    assert_eq!(
        editor.read_with(&cx, |editor, _| editor.marked_range.clone()),
        Some(6..7)
    );
    editor.update_in(&mut cx, |editor, window, cx| {
        editor.replace_text_in_range(None, "ñ", window, cx)
    });
    assert_eq!(state(&editor, &mut cx), "p hello ñˇ line\n");
}
