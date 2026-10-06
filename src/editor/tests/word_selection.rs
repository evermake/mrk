//! Tests for selecting by words with the mouse: double-click and hold, then move the pointer
//! to select further words as a whole.

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use pretty_assertions::assert_eq;

use super::*;

/// The window position of the middle of `word`, in the block whose text is `block`.
fn on_word(
    editor: &Entity<Editor>,
    block: &str,
    word: &str,
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    editor.read_with(cx, |editor, _| {
        let id = editor.document.find(block);
        let start = editor
            .document
            .block(id)
            .unwrap()
            .text
            .text()
            .find(word)
            .unwrap();
        editor.layouts.borrow()[&id].range_bounds(start..start + word.len())[0].center()
    })
}

/// Presses the mouse button twice at `position` and keeps it held down.
fn double_click_and_hold(position: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
}

fn drag_to(position: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_move(position, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn dragging_after_a_double_click_selects_whole_words(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("alpha beta gamma delta\n", cx);
    let line = "alpha beta gamma delta";
    double_click_and_hold(on_word(&editor, line, "beta", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p alpha «beta» gamma delta\n");

    // Dragging right takes in the words up to the one under the pointer, in whole.
    drag_to(on_word(&editor, line, "gamma", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p alpha «beta gamma» delta\n");
    drag_to(on_word(&editor, line, "delta", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p alpha «beta gamma delta»\n");

    // Dragging left of the word that was double-clicked keeps that word selected too.
    drag_to(on_word(&editor, line, "alpha", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p «alpha beta» gamma delta\n");

    // Back on the word that was double-clicked, only it is selected.
    drag_to(on_word(&editor, line, "beta", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p alpha «beta» gamma delta\n");
}

#[gpui::test]
fn a_word_drag_ends_when_the_button_is_released(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("alpha beta gamma\n", cx);
    let line = "alpha beta gamma";
    double_click_and_hold(on_word(&editor, line, "alpha", &mut cx), &mut cx);
    let gamma = on_word(&editor, line, "gamma", &mut cx);
    drag_to(gamma, &mut cx);
    cx.simulate_mouse_up(gamma, MouseButton::Left, Modifiers::none());
    assert_eq!(state(&editor, &mut cx), "p «alpha beta gamma»\n");

    drag_to(on_word(&editor, line, "beta", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p «alpha beta gamma»\n");
}

#[gpui::test]
fn a_word_drag_continues_into_other_blocks(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("one two\n\nthree four\n\nfive six\n", cx);
    double_click_and_hold(on_word(&editor, "three four", "four", &mut cx), &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "p one two\np three «four»\np five six\n"
    );

    drag_to(on_word(&editor, "five six", "five", &mut cx), &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "p one two\np three «four\np five» six\n"
    );

    // Going up, the word that was double-clicked stays selected, as does the word reached.
    drag_to(on_word(&editor, "one two", "two", &mut cx), &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "p one «two\np three four»\np five six\n"
    );

    // Back within the block it started in, it is a selection within one block again.
    drag_to(on_word(&editor, "three four", "three", &mut cx), &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "p one two\np «three four»\np five six\n"
    );
}

#[gpui::test]
fn a_word_drag_stops_short_of_a_divider(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("ab cd\n\n---\n\nef gh\n", cx);
    double_click_and_hold(on_word(&editor, "ab cd", "ab", &mut cx), &mut cx);
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
    drag_to(divider, &mut cx);
    assert_eq!(state(&editor, &mut cx), "p «ab cd»\n---\np ef gh\n");

    drag_to(on_word(&editor, "ef gh", "gh", &mut cx), &mut cx);
    assert_eq!(state(&editor, &mut cx), "p «ab cd\n---\np ef gh»\n");
}

#[gpui::test]
fn dragging_after_a_single_click_still_selects_by_characters(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("alpha beta gamma\n", cx);
    let line = "alpha beta gamma";
    let from = on_word(&editor, line, "beta", &mut cx);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    drag_to(on_word(&editor, line, "gamma", &mut cx), &mut cx);
    let selected = editor.read_with(&cx, |editor, _| {
        let Selection::Text(selection) = &editor.selection else {
            return None;
        };
        let block = editor.document.block(selection.block)?;
        Some(block.text.text()[selection.range.clone()].to_string())
    });
    // From the middle of "beta" to the middle of "gamma", not to the edges of the words.
    let selected = selected.expect("a selection within the block");
    assert!(
        !selected.starts_with('b') && !selected.ends_with("gamma"),
        "{selected:?}"
    );
    assert!(selected.contains(" g"), "{selected:?}");
}
