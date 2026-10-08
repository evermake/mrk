//! Tests for the two places the caret has where inline code starts or ends: inside the code,
//! within its padding, and outside it. What is typed next goes where the caret is.

use gpui::{Bounds, Entity, Hsla, Modifiers, TestAppContext, VisualTestContext};
use pretty_assertions::assert_eq;

use super::*;

const SOURCE: &str = "a `code` b\n";
/// The text offsets in [`SOURCE`] that the code starts and ends at.
const CODE: Range<usize> = 2..6;

/// Presses `key` and returns the state after it.
fn press(key: &str, editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
    cx.simulate_keystrokes(key);
    state(editor, cx)
}

/// The layout of the text of the first block.
fn first_layout(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> BlockLayout {
    editor.read_with(cx, |editor, _| {
        editor.layouts.borrow()[&editor.document.first()].clone()
    })
}

/// The bounds of what was painted in `color` of the theme, in the units of the layouts.
fn painted(color: impl Fn(&Theme) -> Hsla, cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
    cx.run_until_parked();
    cx.update(|window, _| {
        let color = color(&Theme::for_appearance(window.appearance()));
        let scale = window.scale_factor();
        let quads = window.painted_quads();
        let quads = quads.iter();
        quads
            .filter(|quad| quad.background.as_solid() == Some(color))
            .map(|quad| quad.bounds.map(|length| px(length.0 / scale)))
            .collect()
    })
}

#[gpui::test]
fn the_caret_stops_on_both_sides_of_the_edges_of_inline_code(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    cx.simulate_keystrokes("down i");
    let forward = [
        "p aˇ <code>code</code> b\n",
        "p a ˇ<code>code</code> b\n",
        "p a <code>ˇcode</code> b\n",
        "p a <code>cˇode</code> b\n",
        "p a <code>coˇde</code> b\n",
        "p a <code>codˇe</code> b\n",
        "p a <code>codeˇ</code> b\n",
        "p a <code>code</code>ˇ b\n",
        "p a <code>code</code> ˇb\n",
        "p a <code>code</code> bˇ\n",
    ];
    for expected in forward {
        assert_eq!(press("right", &editor, &mut cx), expected);
    }
    // Going back, the caret passes the same places in the opposite order.
    for expected in forward.iter().rev().skip(1) {
        assert_eq!(press("left", &editor, &mut cx), *expected);
    }
    assert_eq!(
        press("left", &editor, &mut cx),
        "p ˇa <code>code</code> b\n"
    );
}

#[gpui::test]
fn what_is_typed_at_an_edge_of_inline_code_goes_where_the_caret_is(cx: &mut TestAppContext) {
    for (steps, expected) in [
        (2, "a x`code` b\n"),
        (3, "a `xcode` b\n"),
        (7, "a `codex` b\n"),
        (8, "a `code`x b\n"),
    ] {
        let (editor, mut cx) = open(SOURCE, cx);
        cx.simulate_keystrokes("down i");
        for _ in 0..steps {
            cx.simulate_keystrokes("right");
        }
        type_text("x", &mut cx);
        assert_eq!(saved(&editor, &mut cx), expected, "after {steps} steps");
        cx.update(|window, _| window.remove_window());
    }
}

#[gpui::test]
fn inline_code_at_the_end_of_a_block_can_be_left(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("`code`\n\nnext\n", cx);
    cx.simulate_keystrokes("down a");
    assert_eq!(state(&editor, &mut cx), "p <code>codeˇ</code>\np next\n");
    assert_eq!(
        press("right", &editor, &mut cx),
        "p <code>code</code>ˇ\np next\n"
    );
    assert_eq!(
        press("right", &editor, &mut cx),
        "p <code>code</code>\np ˇnext\n"
    );
    // Coming back from the next block, the caret is outside the code first.
    assert_eq!(
        press("left", &editor, &mut cx),
        "p <code>code</code>ˇ\np next\n"
    );
    type_text("x", &mut cx);
    assert_eq!(saved(&editor, &mut cx), "`code`x\n\nnext\n");
}

#[gpui::test]
fn inline_code_at_the_start_of_a_block_can_be_left(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("above\n\n`code`\n", cx);
    cx.simulate_keystrokes("down down i");
    assert_eq!(state(&editor, &mut cx), "p above\np <code>ˇcode</code>\n");
    assert_eq!(
        press("left", &editor, &mut cx),
        "p above\np ˇ<code>code</code>\n"
    );
    assert_eq!(
        press("left", &editor, &mut cx),
        "p aboveˇ\np <code>code</code>\n"
    );
    // Coming back from the block above, the caret is outside the code first.
    assert_eq!(
        press("right", &editor, &mut cx),
        "p above\np ˇ<code>code</code>\n"
    );
    type_text("x", &mut cx);
    assert_eq!(saved(&editor, &mut cx), "above\n\nx`code`\n");
}

#[gpui::test]
fn toggling_code_at_an_edge_moves_the_caret_to_the_other_side(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    cx.simulate_keystrokes("down i right right right right right right right");
    assert_eq!(state(&editor, &mut cx), "p a <code>codeˇ</code> b\n");
    assert_eq!(
        press("cmd-e", &editor, &mut cx),
        "p a <code>code</code>ˇ b\n"
    );
    // The caret is outside the code now, so the next step leaves the edge.
    assert_eq!(
        press("right", &editor, &mut cx),
        "p a <code>code</code> ˇb\n"
    );
}

#[gpui::test]
fn deleting_keeps_the_caret_on_its_side_of_an_edge(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    cx.simulate_keystrokes("down i right right right right right right right right");
    assert_eq!(state(&editor, &mut cx), "p a <code>code</code>ˇ b\n");
    assert_eq!(
        press("backspace", &editor, &mut cx),
        "p a <code>cod</code>ˇ b\n"
    );
    type_text("x", &mut cx);
    assert_eq!(saved(&editor, &mut cx), "a `cod`x b\n");
}

#[gpui::test]
fn selecting_does_not_stop_twice_at_an_edge(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    cx.simulate_keystrokes("down i right");
    for expected in [
        "p a« »<code>code</code> b\n",
        "p a« <code>c</code>»<code>ode</code> b\n",
    ] {
        assert_eq!(press("shift-right", &editor, &mut cx), expected);
    }
}

#[gpui::test]
fn inline_code_is_padded_and_the_caret_is_painted_on_its_side(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    cx.simulate_keystrokes("down i");
    let code = painted(|theme| theme.code_background, &mut cx);
    assert_eq!(code.len(), 1, "the code should have a background");
    let code = code[0];

    // The places of the caret at each edge: outside the code it is at the edge of the
    // background, and inside it the padding is between the two.
    let layout = first_layout(&editor, &mut cx);
    let caret = |offset, inside| layout.caret_bounds(offset, false, inside).unwrap().left();
    assert_eq!(caret(CODE.start, false), code.left());
    assert!(caret(CODE.start, true) > code.left());
    assert!(caret(CODE.end, true) < code.right());
    assert_eq!(caret(CODE.end, false), code.right());
    // Away from the edges there is one place.
    for offset in [0, 1, 3, 4, 5, 7, 8] {
        assert_eq!(caret(offset, true), caret(offset, false), "{offset}");
    }

    let mut painted_carets = Vec::new();
    for _ in 0..9 {
        cx.simulate_keystrokes("right");
        let carets = painted(|theme| theme.caret, &mut cx);
        assert_eq!(carets.len(), 1, "the caret should be painted");
        painted_carets.push(carets[0].left());
    }
    let mut expected = vec![caret(1, false), caret(2, false), caret(2, true)];
    expected.extend([3, 4, 5].map(|offset| caret(offset, false)));
    expected.extend([caret(6, true), caret(6, false), caret(7, false)]);
    assert_eq!(painted_carets, expected);
    assert!(
        painted_carets.windows(2).all(|pair| pair[0] < pair[1]),
        "every step should move the caret: {painted_carets:?}"
    );
}

#[gpui::test]
fn the_padding_of_inline_code_is_selected_with_the_code(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    let code = painted(|theme| theme.code_background, &mut cx)[0];
    let layout = first_layout(&editor, &mut cx);

    assert_eq!(layout.range_bounds(CODE), vec![code]);
    let before = layout.range_bounds(0..CODE.start);
    assert_eq!(before[0].right(), code.left());
    let after = layout.range_bounds(CODE.end..8);
    assert_eq!(after[0].left(), code.right());
}

#[gpui::test]
fn a_click_puts_the_caret_on_the_side_of_the_edge_it_is_nearer_to(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(SOURCE, cx);
    let layout = first_layout(&editor, &mut cx);
    for (offset, inside, expected) in [
        (CODE.start, false, "p a ˇ<code>code</code> b\n"),
        (CODE.start, true, "p a <code>ˇcode</code> b\n"),
        (CODE.end, true, "p a <code>codeˇ</code> b\n"),
        (CODE.end, false, "p a <code>code</code>ˇ b\n"),
    ] {
        let place = layout.caret_bounds(offset, false, inside).unwrap();
        cx.simulate_click(place.center(), Modifiers::none());
        assert_eq!(state(&editor, &mut cx), expected);
    }
}

#[gpui::test]
fn wrapped_text_with_inline_code_keeps_a_place_for_every_offset(cx: &mut TestAppContext) {
    // Code at the start, at the end and in between, some of it starting with a character
    // that the text can be wrapped before.
    let source = format!(
        "`first` {}`last`\n",
        "some `code` and `(more)` text ".repeat(12)
    );
    let (editor, mut cx) = open(&source, cx);
    let layout = first_layout(&editor, &mut cx);
    let length = editor.read_with(&cx, |editor, _| {
        let block = editor.document.block(editor.document.first()).unwrap();
        block.text.len()
    });

    let rows = layout.rows();
    assert!(rows.len() > 2, "the text should wrap");
    assert_eq!(rows[0].range.start, 0);
    assert_eq!(rows.last().unwrap().range.end, length);
    for pair in rows.windows(2) {
        assert_eq!(pair[0].range.end, pair[1].range.start);
    }
    for (index, row) in rows.iter().enumerate() {
        for offset in row.range.clone() {
            assert_eq!(layout.row_index(&rows, offset, false), index, "{offset}");
            // The place found from where an offset is shown is that offset.
            let x = layout.x_in_row(row, offset);
            assert_eq!(layout.offset_in_row(row, x).offset, offset);
            for inside in [false, true] {
                let caret = layout.caret_bounds(offset, false, inside).unwrap();
                assert!(layout.bounds.contains(&caret.origin), "{offset} {inside}");
            }
        }
    }
}
