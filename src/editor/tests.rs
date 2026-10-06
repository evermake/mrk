use gpui::{Entity, TestAppContext, VisualTestContext};
use pretty_assertions::assert_eq;

use super::*;

mod scrollbar;
mod selection_across_blocks;
mod typing_shortcuts;
mod word_selection;

/// Opens a window showing an editor for `source` and focuses it.
fn open(source: &str, cx: &mut TestAppContext) -> (Entity<Editor>, VisualTestContext) {
    cx.update(bind_keys);
    let window = cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let editor = cx.new(|cx| Editor::new(markdown::parse(source), cx));
            window.focus(&editor.focus_handle(cx), cx);
            editor
        })
        .unwrap()
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let editor = window.root(&mut cx).unwrap();
    cx.run_until_parked();
    (editor, cx)
}

/// The document as an outline, annotated with the selection: `ˇ` is the caret and `«»`
/// surround selected text in writing mode, even when it runs across blocks, while `*` marks
/// selected rows in navigation mode.
fn state(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |editor, _| {
        let selected_blocks = match &editor.selection {
            Selection::Blocks { anchor, head } => editor.document.subtree_ids(*anchor, *head),
            _ => HashSet::new(),
        };
        let span = editor.span();
        let mut output = String::new();
        for row in editor.document.rows() {
            let block = row.block;
            let mut text = block.text.clone();
            if let Selection::Text(selection) = &editor.selection
                && selection.block == block.id
            {
                if selection.range.is_empty() {
                    let style = text.typing_style(selection.head());
                    text.insert(selection.head(), "ˇ", style);
                } else {
                    let style = text
                        .style_at(selection.range.start)
                        .cloned()
                        .unwrap_or_default();
                    text.insert(selection.range.end, "»", style.clone());
                    text.insert(selection.range.start, "«", style);
                }
            }
            if let Some((start, end)) = span {
                if end.block == block.id {
                    let style = text.typing_style(end.offset);
                    text.insert(end.offset, "»", style);
                }
                if start.block == block.id {
                    let style = text.typing_style(start.offset);
                    text.insert(start.offset, "«", style);
                }
            }
            if editor.mode() == Mode::Navigation {
                output.push_str(if selected_blocks.contains(&block.id) {
                    "* "
                } else {
                    "  "
                });
            }
            output.push_str(&"  ".repeat(row.depth));
            output.push_str(&block.outline_line(&text));
            output.push('\n');
        }
        output
    })
}

fn mode(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Mode {
    editor.read_with(cx, |editor, _| editor.mode())
}

/// The Markdown that saving the note would write.
fn saved(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |editor, _| markdown::serialize(editor.document()))
}

/// Types `text` one character at a time, as the keyboard would. A line feed is the Enter key.
fn type_text(text: &str, cx: &mut VisualTestContext) {
    for character in text.chars() {
        match character {
            ' ' => cx.simulate_keystrokes("space"),
            '\n' => cx.simulate_keystrokes("enter"),
            _ => cx.simulate_input(&character.to_string()),
        }
    }
}

const NESTED: &str = "\
- a
  - a1
  - a2
    - a2x
- b
- c
";

#[gpui::test]
fn a_document_opens_without_a_mode(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);

    cx.simulate_keystrokes("down");
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );

    cx.simulate_keystrokes("escape");
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);

    cx.simulate_keystrokes("up");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n    - a2\n      - a2x\n  - b\n* - c\n"
    );
}

#[gpui::test]
fn the_last_block_is_the_one_shown_at_the_bottom(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- a\n  - b\n", cx);
    cx.simulate_keystrokes("up");
    assert_eq!(state(&editor, &mut cx), "  - a\n*   - b\n");
}

#[gpui::test]
fn arrows_navigate_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("down down down");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("down down");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n    - a2\n      - a2x\n* - b\n  - c\n"
    );
    cx.simulate_keystrokes("up");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n    - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("left");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("left right");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n*   - a1\n    - a2\n      - a2x\n  - b\n  - c\n"
    );
    // There is nothing further in these directions.
    cx.simulate_keystrokes("right left left up");
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
}

#[gpui::test]
fn vim_keys_work_like_arrows(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("j l j");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("k");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n*   - a1\n    - a2\n      - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("h");
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("j shift-j");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n*   - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("shift-k alt-j");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a2\n      - a2x\n*   - a1\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("alt-k");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n*   - a1\n    - a2\n      - a2x\n  - b\n  - c\n"
    );
}

#[gpui::test]
fn d_deletes_the_selected_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    // Nothing is selected yet, so there is nothing to delete.
    cx.simulate_keystrokes("d");
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);
    assert_eq!(
        state(&editor, &mut cx),
        "- a\n  - a1\n  - a2\n    - a2x\n- b\n- c\n"
    );

    // `a2` goes with what is nested in it.
    cx.simulate_keystrokes("down down down d");
    assert_eq!(state(&editor, &mut cx), "  - a\n*   - a1\n  - b\n  - c\n");
    cx.simulate_keystrokes("down shift-down d");
    assert_eq!(state(&editor, &mut cx), "* - a\n*   - a1\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "  - a\n    - a1\n* - b\n* - c\n");
}

#[gpui::test]
fn space_toggles_the_selected_to_do(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- [ ] task\n", cx);
    // Nothing is selected yet, so there is nothing to toggle.
    cx.simulate_keystrokes("space");
    assert_eq!(state(&editor, &mut cx), "[ ] task\n");
    assert!(!editor.read_with(&cx, |editor, _| editor.is_dirty()));

    cx.simulate_keystrokes("down space");
    assert_eq!(state(&editor, &mut cx), "* [x] task\n");
    cx.simulate_keystrokes("space");
    assert_eq!(state(&editor, &mut cx), "* [ ] task\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "* [x] task\n");

    // In writing mode it is a character like any other.
    cx.simulate_keystrokes("enter space");
    assert_eq!(state(&editor, &mut cx), "[x] task ˇ\n");
}

#[gpui::test]
fn space_checks_the_selected_to_dos_unless_all_are_checked(cx: &mut TestAppContext) {
    for (source, keys, expected) in [
        (
            "- [ ] a\n- [ ] b\n- [ ] c\n",
            "down shift-down space",
            "* [x] a\n* [x] b\n  [ ] c\n",
        ),
        // The ones that are done already stay done, whichever end the selection started from.
        (
            "- [x] a\n- [ ] b\n- [x] c\n",
            "down shift-down shift-down space",
            "* [x] a\n* [x] b\n* [x] c\n",
        ),
        (
            "- [ ] a\n- [x] b\n- [x] c\n",
            "up shift-up shift-up space",
            "* [x] a\n* [x] b\n* [x] c\n",
        ),
        (
            "- [x] a\n- [x] b\n- [ ] c\n",
            "down shift-down space",
            "* [ ] a\n* [ ] b\n  [ ] c\n",
        ),
        (
            "- [x] a\n- [ ] b\n- [x] c\n",
            "down shift-down shift-down space space",
            "* [ ] a\n* [ ] b\n* [ ] c\n",
        ),
    ] {
        let (editor, mut cx) = open(source, cx);
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&editor, &mut cx), expected, "after {keys:?}");
    }

    // Together they are one step to undo.
    let (editor, mut cx) = open("- [x] a\n- [ ] b\n", cx);
    cx.simulate_keystrokes("down shift-down space cmd-z");
    assert_eq!(state(&editor, &mut cx), "* [x] a\n* [ ] b\n");
}

#[gpui::test]
fn space_leaves_the_to_dos_nested_in_the_selected_ones_alone(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- [ ] parent\n  - [x] done\n  - [ ] open\n- [ ] next\n", cx);
    cx.simulate_keystrokes("down space");
    assert_eq!(
        state(&editor, &mut cx),
        "* [x] parent\n*   [x] done\n*   [ ] open\n  [ ] next\n"
    );
    // `open` is not one of the selected to-dos, so all of them are checked.
    cx.simulate_keystrokes("space");
    assert_eq!(
        state(&editor, &mut cx),
        "* [ ] parent\n*   [x] done\n*   [ ] open\n  [ ] next\n"
    );
    cx.simulate_keystrokes("right shift-down space");
    assert_eq!(
        state(&editor, &mut cx),
        "  [ ] parent\n*   [x] done\n*   [x] open\n  [ ] next\n"
    );
}

#[gpui::test]
fn space_skips_the_selected_blocks_that_are_not_to_dos(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- [x] a\n- b\n  - [ ] b1\n- [ ] c\n\ntext\n", cx);
    cx.simulate_keystrokes("cmd-a space");
    assert_eq!(
        state(&editor, &mut cx),
        "* [x] a\n* - b\n*   [ ] b1\n* [x] c\n* p text\n"
    );

    // With no to-do among them nothing happens.
    cx.simulate_keystrokes("cmd-z escape down down space");
    assert_eq!(
        state(&editor, &mut cx),
        "  [x] a\n* - b\n*   [ ] b1\n  [ ] c\n  p text\n"
    );
    assert!(!editor.read_with(&cx, |editor, _| editor.is_dirty()));
}

#[gpui::test]
fn o_opens_a_block_below_and_shift_o_above(cx: &mut TestAppContext) {
    for (keys, expected) in [
        // `a`, with everything nested in it.
        ("down o", "- a\n  - a1\n  - a2\n    - a2x\n- ˇ\n- b\n- c\n"),
        (
            "down shift-o",
            "- ˇ\n- a\n  - a1\n  - a2\n    - a2x\n- b\n- c\n",
        ),
        // `a1`, the first item nested in `a`.
        (
            "down down o",
            "- a\n  - a1\n  - ˇ\n  - a2\n    - a2x\n- b\n- c\n",
        ),
        (
            "down down shift-o",
            "- a\n  - ˇ\n  - a1\n  - a2\n    - a2x\n- b\n- c\n",
        ),
        // `a2`: below it is below `a2x`, but as deep as `a2`.
        (
            "down down down o",
            "- a\n  - a1\n  - a2\n    - a2x\n  - ˇ\n- b\n- c\n",
        ),
        (
            "down down down shift-o",
            "- a\n  - a1\n  - ˇ\n  - a2\n    - a2x\n- b\n- c\n",
        ),
        // `a2x`, the deepest item.
        (
            "down down down down o",
            "- a\n  - a1\n  - a2\n    - a2x\n    - ˇ\n- b\n- c\n",
        ),
        (
            "down down down down shift-o",
            "- a\n  - a1\n  - a2\n    - ˇ\n    - a2x\n- b\n- c\n",
        ),
        // `c`, the last block.
        ("up o", "- a\n  - a1\n  - a2\n    - a2x\n- b\n- c\n- ˇ\n"),
        (
            "up shift-o",
            "- a\n  - a1\n  - a2\n    - a2x\n- b\n- ˇ\n- c\n",
        ),
    ] {
        let (editor, mut cx) = open(NESTED, cx);
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&editor, &mut cx), expected, "after {keys:?}");
        assert_eq!(mode(&editor, &mut cx), Mode::Writing, "after {keys:?}");
    }
}

#[gpui::test]
fn o_and_shift_o_open_a_block_around_several_selected_blocks(cx: &mut TestAppContext) {
    for (keys, expected) in [
        // `a1` and `a2` are selected, whichever of them the selection started from.
        (
            "down down shift-down o",
            "- a\n  - a1\n  - a2\n    - a2x\n  - ˇ\n- b\n- c\n",
        ),
        (
            "down down down shift-up o",
            "- a\n  - a1\n  - a2\n    - a2x\n  - ˇ\n- b\n- c\n",
        ),
        (
            "down down shift-down shift-o",
            "- a\n  - ˇ\n  - a1\n  - a2\n    - a2x\n- b\n- c\n",
        ),
        (
            "down down down shift-up shift-o",
            "- a\n  - ˇ\n  - a1\n  - a2\n    - a2x\n- b\n- c\n",
        ),
    ] {
        let (editor, mut cx) = open(NESTED, cx);
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&editor, &mut cx), expected, "after {keys:?}");
    }
}

#[gpui::test]
fn an_opened_block_continues_a_list_and_is_a_paragraph_otherwise(cx: &mut TestAppContext) {
    for (source, select, below, above) in [
        ("text\n", "down", "p text\np ˇ\n", "p ˇ\np text\n"),
        ("# Title\n", "down", "h1 Title\np ˇ\n", "p ˇ\nh1 Title\n"),
        ("- item\n", "down", "- item\n- ˇ\n", "- ˇ\n- item\n"),
        ("1. item\n", "down", "1. item\n1. ˇ\n", "1. ˇ\n1. item\n"),
        // A new to-do is still to be done.
        (
            "- [x] done\n",
            "down",
            "[x] done\n[ ] ˇ\n",
            "[ ] ˇ\n[x] done\n",
        ),
        ("> quote\n", "down", "> quote\np ˇ\n", "p ˇ\n> quote\n"),
        (
            "```\ncode\n```\n",
            "down",
            "code() code\np ˇ\n",
            "p ˇ\ncode() code\n",
        ),
        ("---\n", "down", "---\np ˇ\n", "p ˇ\n---\n"),
        // What matters is the selected block, not what it is nested in.
        (
            "- item\n\n  nested text\n",
            "down right",
            "- item\n  p nested text\n  p ˇ\n",
            "- item\n  p ˇ\n  p nested text\n",
        ),
        (
            "> quote\n>\n> - item\n",
            "down right",
            "> quote\n  - item\n  - ˇ\n",
            "> quote\n  - ˇ\n  - item\n",
        ),
    ] {
        for (key, expected) in [("o", below), ("shift-o", above)] {
            let (editor, mut cx) = open(source, cx);
            cx.simulate_keystrokes(select);
            cx.simulate_keystrokes(key);
            assert_eq!(state(&editor, &mut cx), expected, "{key} in {source:?}");
        }
    }
}

#[gpui::test]
fn with_nothing_selected_o_opens_the_end_of_the_document_and_shift_o_its_start(
    cx: &mut TestAppContext,
) {
    for (key, expected) in [
        // The document ends in `end`, which is nested, but the new block is not.
        ("o", "h1 Title\n- last\n  - end\n- ˇ\n"),
        ("shift-o", "p ˇ\nh1 Title\n- last\n  - end\n"),
    ] {
        let (editor, mut cx) = open("# Title\n\n- last\n  - end\n", cx);
        cx.simulate_keystrokes(key);
        assert_eq!(state(&editor, &mut cx), expected, "after {key:?}");
        assert_eq!(mode(&editor, &mut cx), Mode::Writing, "after {key:?}");
    }
}

#[gpui::test]
fn an_opened_block_is_written_in_and_undone_in_one_step(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("down down down o");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n*   - a2\n*     - a2x\n  - b\n  - c\n"
    );
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(
        state(&editor, &mut cx),
        "- a\n  - a1\n  - a2\n    - a2x\n  - ˇ\n- b\n- c\n"
    );

    type_text("new", &mut cx);
    assert_eq!(
        saved(&editor, &mut cx),
        "- a\n  - a1\n  - a2\n    - a2x\n  - new\n- b\n- c\n"
    );
    // Back in navigation mode, the block that was written is the selected one.
    cx.simulate_keystrokes("escape shift-o");
    type_text("above", &mut cx);
    assert_eq!(
        saved(&editor, &mut cx),
        "- a\n  - a1\n  - a2\n    - a2x\n  - above\n  - new\n- b\n- c\n"
    );
}

#[gpui::test]
fn shift_arrows_select_several_siblings(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("down shift-down");
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n* - b\n  - c\n"
    );
    cx.simulate_keystrokes("shift-down shift-down shift-up");
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n* - b\n  - c\n"
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "* - c\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        state(&editor, &mut cx),
        "* - a\n*   - a1\n*   - a2\n*     - a2x\n* - b\n  - c\n"
    );
    cx.simulate_keystrokes("cmd-shift-z delete");
    assert_eq!(state(&editor, &mut cx), "* p \n");
}

#[gpui::test]
fn tab_nests_and_shift_tab_moves_out(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- a\n- b\n- c\n- d\n", cx);
    cx.simulate_keystrokes("down down tab");
    assert_eq!(state(&editor, &mut cx), "  - a\n*   - b\n  - c\n  - d\n");
    // Already nested as deep as it can be under `a`.
    cx.simulate_keystrokes("tab");
    assert_eq!(state(&editor, &mut cx), "  - a\n*   - b\n  - c\n  - d\n");

    cx.simulate_keystrokes("down shift-down tab");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - b\n*   - c\n*   - d\n"
    );

    // Moving `b` out takes the blocks below it along as its children, so nothing changes
    // place vertically.
    cx.simulate_keystrokes("up up shift-tab");
    assert_eq!(state(&editor, &mut cx), "  - a\n* - b\n*   - c\n*   - d\n");
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(state(&editor, &mut cx), "  - a\n* - b\n*   - c\n*   - d\n");
}

#[gpui::test]
fn tab_does_not_nest_under_blocks_without_nesting(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("paragraph\n\n- item\n", cx);
    cx.simulate_keystrokes("up tab");
    assert_eq!(state(&editor, &mut cx), "  p paragraph\n* - item\n");
    assert!(!editor.read_with(&cx, |editor, _| editor.is_dirty()));
}

#[gpui::test]
fn alt_arrows_reorder_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("down alt-down");
    assert_eq!(
        state(&editor, &mut cx),
        "  - b\n* - a\n*   - a1\n*   - a2\n*     - a2x\n  - c\n"
    );
    cx.simulate_keystrokes("alt-down alt-down");
    assert_eq!(
        state(&editor, &mut cx),
        "  - b\n  - c\n* - a\n*   - a1\n*   - a2\n*     - a2x\n"
    );
    cx.simulate_keystrokes("shift-up alt-up");
    assert_eq!(
        state(&editor, &mut cx),
        "* - c\n* - a\n*   - a1\n*   - a2\n*     - a2x\n  - b\n"
    );
}

#[gpui::test]
fn enter_starts_writing_at_the_end_and_escape_selects_the_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("first\n\nsecond\n", cx);
    cx.simulate_keystrokes("down enter");
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
    assert_eq!(state(&editor, &mut cx), "p firstˇ\np second\n");

    type_text(" one", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p first oneˇ\np second\n");

    cx.simulate_keystrokes("escape");
    assert_eq!(state(&editor, &mut cx), "* p first one\n  p second\n");
}

#[gpui::test]
fn i_writes_at_the_start_of_the_selected_block_and_a_at_the_end(cx: &mut TestAppContext) {
    for (source, start, end) in [
        ("text\n", "p ˇtext\n", "p textˇ\n"),
        ("# Title\n", "h1 ˇTitle\n", "h1 Titleˇ\n"),
        ("- item\n", "- ˇitem\n", "- itemˇ\n"),
        ("1. item\n", "1. ˇitem\n", "1. itemˇ\n"),
        ("- [x] done\n", "[x] ˇdone\n", "[x] doneˇ\n"),
        ("> quote\n", "> ˇquote\n", "> quoteˇ\n"),
        ("```\ncode\n```\n", "code() ˇcode\n", "code() codeˇ\n"),
    ] {
        for (key, expected) in [
            ("i", start),
            ("shift-i", start),
            ("a", end),
            ("shift-a", end),
        ] {
            let (editor, mut cx) = open(source, cx);
            cx.simulate_keystrokes("down");
            cx.simulate_keystrokes(key);
            assert_eq!(state(&editor, &mut cx), expected, "{key} in {source:?}");
            assert_eq!(mode(&editor, &mut cx), Mode::Writing, "{key} in {source:?}");
        }
    }
}

#[gpui::test]
fn i_and_a_write_in_the_block_itself_not_in_what_is_nested_in_it(cx: &mut TestAppContext) {
    for (keys, expected) in [
        // `a2` is selected together with `a2x`, which is nested in it.
        (
            "down down down i",
            "- a\n  - a1\n  - ˇa2\n    - a2x\n- b\n- c\n",
        ),
        (
            "down down down a",
            "- a\n  - a1\n  - a2ˇ\n    - a2x\n- b\n- c\n",
        ),
        // The selection ends at `a2` or at `a1`, and that is where the writing starts.
        (
            "down down shift-down i",
            "- a\n  - a1\n  - ˇa2\n    - a2x\n- b\n- c\n",
        ),
        (
            "down down shift-down a",
            "- a\n  - a1\n  - a2ˇ\n    - a2x\n- b\n- c\n",
        ),
        (
            "down down down shift-up i",
            "- a\n  - ˇa1\n  - a2\n    - a2x\n- b\n- c\n",
        ),
        (
            "down down down shift-up a",
            "- a\n  - a1ˇ\n  - a2\n    - a2x\n- b\n- c\n",
        ),
    ] {
        let (editor, mut cx) = open(NESTED, cx);
        cx.simulate_keystrokes(keys);
        assert_eq!(state(&editor, &mut cx), expected, "after {keys:?}");
        assert_eq!(mode(&editor, &mut cx), Mode::Writing, "after {keys:?}");
    }
}

#[gpui::test]
fn i_and_a_write_in_a_new_paragraph_next_to_a_divider(cx: &mut TestAppContext) {
    for (key, expected) in [("i", "p ˇ\n---\n"), ("a", "---\np ˇ\n")] {
        let (editor, mut cx) = open("---\n", cx);
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes(key);
        assert_eq!(state(&editor, &mut cx), expected, "after {key:?}");
        assert_eq!(mode(&editor, &mut cx), Mode::Writing, "after {key:?}");
    }
}

#[gpui::test]
fn what_is_written_after_i_and_a_lands_at_the_caret(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("middle\n", cx);
    cx.simulate_keystrokes("down i");
    type_text("start ", &mut cx);
    cx.simulate_keystrokes("escape a");
    type_text(" end", &mut cx);
    assert_eq!(saved(&editor, &mut cx), "start middle end\n");
}

#[gpui::test]
fn i_and_a_do_nothing_with_nothing_selected(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("i a shift-i shift-a");
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);
    assert_eq!(saved(&editor, &mut cx), NESTED);
}

#[gpui::test]
fn vim_keys_are_text_while_writing(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("", cx);
    cx.simulate_keystrokes("enter");
    type_text("hjkl HJKL do DO ia IA", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p hjkl HJKL do DO ia IAˇ\n");
}

#[gpui::test]
fn the_caret_moves_seamlessly_between_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("ab\n\n---\n\n- cd\n  - ef\n", cx);
    cx.simulate_keystrokes("down enter right");
    assert_eq!(state(&editor, &mut cx), "p ab\n---\n- ˇcd\n  - ef\n");
    cx.simulate_keystrokes("right right right");
    assert_eq!(state(&editor, &mut cx), "p ab\n---\n- cd\n  - ˇef\n");
    cx.simulate_keystrokes("left left");
    assert_eq!(state(&editor, &mut cx), "p ab\n---\n- cˇd\n  - ef\n");
    cx.simulate_keystrokes("left left");
    assert_eq!(state(&editor, &mut cx), "p abˇ\n---\n- cd\n  - ef\n");
}

#[gpui::test]
fn up_and_down_cross_blocks_keeping_the_column(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("abcd\n\n---\n\nef\n\nghij\n", cx);
    cx.simulate_keystrokes("down enter left");
    assert_eq!(state(&editor, &mut cx), "p abcˇd\n---\np ef\np ghij\n");
    cx.simulate_keystrokes("down");
    assert_eq!(state(&editor, &mut cx), "p abcd\n---\np efˇ\np ghij\n");
    cx.simulate_keystrokes("down");
    assert_eq!(state(&editor, &mut cx), "p abcd\n---\np ef\np ghiˇj\n");
    // Below the last row there is nowhere to go but the end of the text.
    cx.simulate_keystrokes("down");
    assert_eq!(state(&editor, &mut cx), "p abcd\n---\np ef\np ghijˇ\n");
    cx.simulate_keystrokes("up up up");
    assert_eq!(state(&editor, &mut cx), "p ˇabcd\n---\np ef\np ghij\n");
}

#[gpui::test]
fn the_caret_moves_between_lines_of_one_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one\\\ntwo\\\nthree\n\nnext\n", cx);
    cx.simulate_keystrokes("down enter up");
    assert_eq!(state(&editor, &mut cx), "p one⏎twoˇ⏎three\np next\n");
    cx.simulate_keystrokes("up");
    assert_eq!(state(&editor, &mut cx), "p oneˇ⏎two⏎three\np next\n");
    cx.simulate_keystrokes("down down down");
    assert_eq!(state(&editor, &mut cx), "p one⏎two⏎three\np nextˇ\n");
    // The first row of `next` is followed by the last row of the block above, at the column
    // the caret has kept since `three`: the selection runs across the blocks.
    cx.simulate_keystrokes("shift-up");
    assert_eq!(state(&editor, &mut cx), "p one⏎two⏎three«\np next»\n");
}

#[gpui::test]
fn shift_arrows_select_text(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("hello world\n", cx);
    cx.simulate_keystrokes("down enter shift-left shift-left");
    assert_eq!(state(&editor, &mut cx), "p hello wor«ld»\n");
    cx.simulate_keystrokes("alt-shift-left");
    assert_eq!(state(&editor, &mut cx), "p hello «world»\n");
    cx.simulate_keystrokes("shift-right");
    assert_eq!(state(&editor, &mut cx), "p hello w«orld»\n");
    cx.simulate_keystrokes("left");
    assert_eq!(state(&editor, &mut cx), "p hello wˇorld\n");
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(state(&editor, &mut cx), "p «hello world»\n");
    type_text("x", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p xˇ\n");
}

#[gpui::test]
fn word_and_line_movement(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one two three\n", cx);
    cx.simulate_keystrokes("down enter alt-left alt-left");
    assert_eq!(state(&editor, &mut cx), "p one ˇtwo three\n");
    cx.simulate_keystrokes("alt-right");
    assert_eq!(state(&editor, &mut cx), "p one twoˇ three\n");
    cx.simulate_keystrokes("cmd-left");
    assert_eq!(state(&editor, &mut cx), "p ˇone two three\n");
    cx.simulate_keystrokes("cmd-right alt-backspace");
    assert_eq!(state(&editor, &mut cx), "p one two ˇ\n");
    cx.simulate_keystrokes("cmd-backspace");
    assert_eq!(state(&editor, &mut cx), "p ˇ\n");
}

#[gpui::test]
fn the_line_keys_have_aliases(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one two three\n", cx);
    cx.simulate_keystrokes("down enter");
    for (start, end) in [
        ("cmd-left", "cmd-right"),
        ("home", "end"),
        ("ctrl-a", "ctrl-e"),
    ] {
        cx.simulate_keystrokes(start);
        assert_eq!(state(&editor, &mut cx), "p ˇone two three\n", "{start}");
        cx.simulate_keystrokes(end);
        assert_eq!(state(&editor, &mut cx), "p one two threeˇ\n", "{end}");
    }

    cx.simulate_keystrokes("alt-left");
    for (start, end) in [
        ("cmd-shift-left", "cmd-shift-right"),
        ("shift-home", "shift-end"),
    ] {
        cx.simulate_keystrokes(start);
        assert_eq!(state(&editor, &mut cx), "p «one two »three\n", "{start}");
        cx.simulate_keystrokes("right");
        assert_eq!(state(&editor, &mut cx), "p one two ˇthree\n");
        cx.simulate_keystrokes(end);
        assert_eq!(state(&editor, &mut cx), "p one two «three»\n", "{end}");
        cx.simulate_keystrokes("left");
        assert_eq!(state(&editor, &mut cx), "p one two ˇthree\n");
    }
}

#[gpui::test]
fn words_are_selected_in_both_directions(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one two three\n", cx);
    cx.simulate_keystrokes("down enter cmd-left alt-shift-right alt-shift-right");
    assert_eq!(state(&editor, &mut cx), "p «one two» three\n");
    cx.simulate_keystrokes("alt-shift-left");
    assert_eq!(state(&editor, &mut cx), "p «one »two three\n");
}

#[gpui::test]
fn shift_backspace_deletes_like_backspace(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- ab\n", cx);
    cx.simulate_keystrokes("down enter shift-backspace");
    assert_eq!(state(&editor, &mut cx), "- aˇ\n");
    cx.simulate_keystrokes("shift-backspace shift-backspace");
    assert_eq!(state(&editor, &mut cx), "p ˇ\n");
}

#[gpui::test]
fn enter_splits_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("# Title\n\n- item\n", cx);
    cx.simulate_keystrokes("down enter left left enter");
    assert_eq!(state(&editor, &mut cx), "h1 Tit\np ˇle\n- item\n");

    cx.simulate_keystrokes("down cmd-right enter");
    assert_eq!(state(&editor, &mut cx), "h1 Tit\np le\n- item\n- ˇ\n");
    type_text("next", &mut cx);
    cx.simulate_keystrokes("cmd-left enter");
    assert_eq!(
        state(&editor, &mut cx),
        "h1 Tit\np le\n- item\n- \n- ˇnext\n"
    );
}

#[gpui::test]
fn enter_on_an_empty_item_ends_the_list(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- a\n  - b\n", cx);
    cx.simulate_keystrokes("up enter enter");
    assert_eq!(state(&editor, &mut cx), "- a\n  - b\n  - ˇ\n");
    cx.simulate_keystrokes("enter");
    assert_eq!(state(&editor, &mut cx), "- a\n  - b\n- ˇ\n");
    cx.simulate_keystrokes("enter");
    assert_eq!(state(&editor, &mut cx), "- a\n  - b\np ˇ\n");
    cx.simulate_keystrokes("enter");
    assert_eq!(state(&editor, &mut cx), "- a\n  - b\np \np ˇ\n");
}

#[gpui::test]
fn shift_enter_breaks_the_line_within_a_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- item\n\n# Title\n", cx);
    cx.simulate_keystrokes("down enter shift-enter");
    type_text("more", &mut cx);
    assert_eq!(state(&editor, &mut cx), "- item⏎moreˇ\nh1 Title\n");
    cx.simulate_keystrokes("down cmd-right shift-enter");
    assert_eq!(state(&editor, &mut cx), "- item⏎more\nh1 Titleˇ\n");
}

#[gpui::test]
fn backspace_at_the_start_unwraps_then_merges(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("first\n\n- second\n  - child\n", cx);
    cx.simulate_keystrokes("down down enter cmd-left backspace");
    assert_eq!(state(&editor, &mut cx), "p first\np ˇsecond\n- child\n");
    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "p firstˇsecond\n- child\n");
    cx.simulate_keystrokes("backspace");
    assert_eq!(state(&editor, &mut cx), "p firsˇsecond\n- child\n");
    cx.simulate_keystrokes("cmd-right delete");
    assert_eq!(state(&editor, &mut cx), "p firssecondˇchild\n");
}

#[gpui::test]
fn enter_on_a_divider_writes_below_it(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("---\n", cx);
    cx.simulate_keystrokes("down enter");
    type_text("x", &mut cx);
    assert_eq!(state(&editor, &mut cx), "---\np xˇ\n");
}

#[gpui::test]
fn bold_and_italic_toggle_on_the_selection(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("hello world\n", cx);
    cx.simulate_keystrokes("down enter alt-shift-left cmd-b");
    assert_eq!(state(&editor, &mut cx), "p hello <b>«world»</b>\n");
    cx.simulate_keystrokes("cmd-i");
    assert_eq!(state(&editor, &mut cx), "p hello <b><i>«world»</i></b>\n");
    cx.simulate_keystrokes("cmd-b");
    assert_eq!(state(&editor, &mut cx), "p hello <i>«world»</i>\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p hello <b><i>«world»</i></b>\n");
}

#[gpui::test]
fn code_and_strikethrough_toggle_on_the_selection(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("hello world\n", cx);
    cx.simulate_keystrokes("down enter alt-shift-left cmd-e");
    assert_eq!(state(&editor, &mut cx), "p hello <code>«world»</code>\n");
    assert_eq!(saved(&editor, &mut cx), "hello `world`\n");

    cx.simulate_keystrokes("cmd-e cmd-shift-x");
    assert_eq!(state(&editor, &mut cx), "p hello <s>«world»</s>\n");
    assert_eq!(saved(&editor, &mut cx), "hello ~~world~~\n");

    cx.simulate_keystrokes("cmd-shift-x");
    assert_eq!(state(&editor, &mut cx), "p hello «world»\n");
}

#[gpui::test]
fn bold_without_a_selection_applies_to_what_is_typed_next(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("", cx);
    cx.simulate_keystrokes("enter");
    type_text("a ", &mut cx);
    cx.simulate_keystrokes("cmd-b");
    type_text("bold", &mut cx);
    cx.simulate_keystrokes("cmd-b");
    type_text(" c", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p a <b>bold</b> cˇ\n");
    assert_eq!(
        editor.read_with(&cx, |editor, _| markdown::serialize(editor.document())),
        "a **bold** c\n"
    );
}

#[gpui::test]
fn pasting_a_url_over_a_selection_links_it(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("see the docs\n", cx);
    cx.simulate_keystrokes("down enter alt-shift-left");
    cx.write_to_clipboard(ClipboardItem::new_string("https://example.com/docs".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        state(&editor, &mut cx),
        "p see the <a href=\"https://example.com/docs\">«docs»</a>\n"
    );

    // Without a selection, and for text that is not a URL, pasting inserts text.
    cx.simulate_keystrokes("right");
    cx.simulate_keystrokes("cmd-v");
    cx.simulate_keystrokes("alt-shift-left");
    cx.write_to_clipboard(ClipboardItem::new_string("not a url".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        editor.read_with(&cx, |editor, _| markdown::serialize(editor.document())),
        "see the [docs](https://example.com/docs)https://example.com/not a url\n"
    );
}

#[gpui::test]
fn pasting_several_lines_inserts_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("intro\n", cx);
    cx.simulate_keystrokes("down enter");
    cx.write_to_clipboard(ClipboardItem::new_string(
        "# Heading\n\n- one\n- two\n".into(),
    ));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        state(&editor, &mut cx),
        "p intro\nh1 Heading\n- one\n- twoˇ\n"
    );
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p introˇ\n");
}

#[gpui::test]
fn blocks_are_copied_and_pasted_as_markdown(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(NESTED, cx);
    cx.simulate_keystrokes("down cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("- a\n  - a1\n  - a2\n    - a2x\n".to_string())
    );
    cx.simulate_keystrokes("down down down down down cmd-x");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n    - a2\n      - a2x\n* - b\n"
    );
    cx.simulate_keystrokes("up up cmd-v");
    assert_eq!(
        state(&editor, &mut cx),
        "  - a\n    - a1\n    - a2\n      - a2x\n*   - c\n  - b\n"
    );
}

#[gpui::test]
fn typing_is_undone_in_one_step_per_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("", cx);
    cx.simulate_keystrokes("enter");
    type_text("one", &mut cx);
    cx.simulate_keystrokes("enter");
    type_text("two", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p one\np twoˇ\n");

    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p one\np ˇ\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p oneˇ\n");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(state(&editor, &mut cx), "p ˇ\n");
    cx.simulate_keystrokes("cmd-shift-z cmd-shift-z cmd-shift-z");
    assert_eq!(state(&editor, &mut cx), "p one\np twoˇ\n");
}

#[gpui::test]
fn undoing_back_to_the_saved_state_clears_the_dirty_flag(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("text\n", cx);
    let is_dirty = |cx: &mut VisualTestContext| editor.read_with(cx, |editor, _| editor.is_dirty());
    assert!(!is_dirty(&mut cx));

    cx.simulate_keystrokes("down enter");
    assert!(!is_dirty(&mut cx));
    type_text("!", &mut cx);
    assert!(is_dirty(&mut cx));

    editor.update(&mut cx, |editor, cx| {
        let revision = editor.revision();
        editor.mark_saved(revision, cx);
    });
    assert!(!is_dirty(&mut cx));

    cx.simulate_keystrokes("cmd-z");
    assert!(is_dirty(&mut cx));
    cx.simulate_keystrokes("cmd-shift-z");
    assert!(!is_dirty(&mut cx));
}

#[gpui::test]
fn tab_while_writing_nests_the_block_and_keeps_the_caret(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- a\n- b\n", cx);
    cx.simulate_keystrokes("up enter left tab");
    assert_eq!(state(&editor, &mut cx), "- a\n  - ˇb\n");
    cx.simulate_keystrokes("shift-tab alt-up");
    assert_eq!(state(&editor, &mut cx), "- ˇb\n- a\n");
}

#[test]
fn recognizes_pasted_urls() {
    assert!(pasted_url("https://example.com/a?b=c#d").is_some());
    assert!(pasted_url(" http://localhost:3000 \n").is_some());
    assert!(pasted_url("mailto:someone@example.com").is_some());
    for text in [
        "example.com",
        "not a url",
        "note: text",
        "https://a b",
        "",
        "file:///etc",
    ] {
        assert!(pasted_url(text).is_none(), "{text:?}");
    }
}

#[test]
fn word_boundaries() {
    let text = "one, two  three";
    assert_eq!(previous_word_start(text, 15), 10);
    assert_eq!(previous_word_start(text, 10), 5);
    assert_eq!(previous_word_start(text, 2), 0);
    assert_eq!(next_word_end(text, 0), 3);
    assert_eq!(next_word_end(text, 3), 4);
    assert_eq!(next_word_end(text, 8), 15);
    assert_eq!(word_range(text, 6), 5..8);
    assert_eq!(word_range(text, 15), 10..15);
}

/// The window position of the middle of the text of the block whose text is `text`.
fn text_center(editor: &Entity<Editor>, text: &str, cx: &mut VisualTestContext) -> Point<Pixels> {
    editor.read_with(cx, |editor, _| {
        let block = editor.document.find(text);
        editor.layouts.borrow()[&block].bounds.center()
    })
}

#[gpui::test]
fn clicking_text_starts_writing_there(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("first\n\n- second\n", cx);
    let position = text_center(&editor, "second", &mut cx);
    cx.simulate_click(position, gpui::Modifiers::none());
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
    type_text("X", &mut cx);
    let markdown = editor.read_with(&cx, |editor, _| markdown::serialize(editor.document()));
    assert!(markdown.starts_with("first\n\n- "), "{markdown}");
    assert!(markdown.contains('X'), "{markdown}");

    // A second click on the same spot selects the word under it.
    let position = text_center(&editor, "first", &mut cx);
    cx.simulate_mouse_down(position, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: gpui::Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    assert!(state(&editor, &mut cx).starts_with("p «first»\n"));
}

#[gpui::test]
fn clicking_a_checkbox_toggles_it(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("- [ ] task\n", cx);
    let text = editor.read_with(&cx, |editor, _| {
        editor.layouts.borrow()[&editor.document.first()].bounds
    });
    let indent = cx.update(|window, _| INDENT.to_pixels(window.rem_size()));
    let checkbox = point(text.left() - indent + px(7.), text.center().y);
    cx.simulate_click(checkbox, gpui::Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "[x] task\n");
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);
    cx.simulate_click(checkbox, gpui::Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "[ ] task\n");
}

#[gpui::test]
fn the_selection_is_scrolled_into_view(cx: &mut TestAppContext) {
    let source: String = (0..200)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect();
    let (editor, mut cx) = open(&source, cx);
    let is_visible = |cx: &mut VisualTestContext| {
        editor.read_with(cx, |editor, _| {
            let Selection::Blocks { head, .. } = &editor.selection else {
                return false;
            };
            let viewport = editor.scroll_handle.bounds();
            let block = editor.layouts.borrow()[head].bounds;
            block.top() >= viewport.top() && block.bottom() <= viewport.bottom()
        })
    };

    cx.simulate_keystrokes("up");
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert!(is_visible(&mut cx), "the last block should be scrolled to");

    cx.simulate_keystrokes("escape down");
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert!(
        is_visible(&mut cx),
        "the first block should be scrolled back to"
    );
}

#[gpui::test]
fn everything_is_sized_by_the_rem_size(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("# title\n\nparagraph\n\n- bullet\n", cx);
    let measure = |cx: &mut VisualTestContext| {
        editor.read_with(cx, |editor, _| {
            let ids: Vec<_> = editor
                .document
                .rows()
                .iter()
                .map(|row| row.block.id)
                .collect();
            let layouts = editor.layouts.borrow();
            let heading = layouts[&ids[0]].bounds;
            let paragraph = layouts[&ids[1]].bounds;
            let bullet = layouts[&ids[2]].bounds;
            // The heading and a line of text, and how far a bullet is indented.
            (
                heading.size.height,
                paragraph.size.height,
                bullet.left() - paragraph.left(),
            )
        })
    };
    let (heading, paragraph, indent) = measure(&mut cx);
    assert_eq!((heading, paragraph, indent), (px(36.), px(24.), px(24.)));

    cx.update(|window, _| window.set_rem_size(px(32.)));
    cx.simulate_keystrokes("down");
    let (heading, paragraph, indent) = measure(&mut cx);
    assert_eq!((heading, paragraph, indent), (px(72.), px(48.), px(48.)));
}

#[gpui::test]
fn rescaling_keeps_the_scrolled_part_in_view(cx: &mut TestAppContext) {
    let source: String = (0..200)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect();
    let (editor, mut cx) = open(&source, cx);
    let first_visible = |cx: &mut VisualTestContext| {
        editor.read_with(cx, |editor, _| {
            let viewport = editor.scroll_handle.bounds();
            let layouts = editor.layouts.borrow();
            editor
                .document
                .rows()
                .iter()
                .position(|row| layouts[&row.block.id].bounds.bottom() > viewport.top())
        })
    };
    editor.update(&mut cx, |editor, cx| {
        let mut offset = editor.scroll_handle.offset();
        offset.y = px(-2000.);
        editor.scroll_handle.set_offset(offset);
        cx.notify();
    });
    cx.run_until_parked();
    let before = first_visible(&mut cx).unwrap();
    assert!(before > 10, "the note should have been scrolled");

    cx.update(|window, _| window.set_rem_size(px(32.)));
    editor.update(&mut cx, |editor, cx| editor.rescale_scroll(2., cx));
    cx.run_until_parked();
    assert_eq!(first_visible(&mut cx), Some(before));

    cx.update(|window, _| window.set_rem_size(px(8.)));
    editor.update(&mut cx, |editor, cx| editor.rescale_scroll(0.25, cx));
    cx.run_until_parked();
    assert_eq!(first_visible(&mut cx), Some(before));
}
