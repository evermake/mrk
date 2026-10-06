//! The scrollbar of the note: what its thumb shows, and scrolling with it.

use gpui::{
    Entity, Modifiers, MouseExitEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    VisualTestContext,
};
use pretty_assertions::assert_eq;

use super::*;
use crate::scrollbar::{HIDE_DELAY, THUMB_MARGIN, TRACK_WIDTH};

/// A note several windows long.
fn long_note() -> String {
    (0..200)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect()
}

/// The part of the window the note scrolls in.
fn viewport(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    editor.read_with(cx, |editor, _| editor.scroll_handle.bounds())
}

/// From where to where down the window the thumb is, when there is one.
fn thumb(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Option<Range<Pixels>> {
    editor.read_with(cx, |editor, cx| editor.scrollbar.read(cx).thumb())
}

/// A point on the track of the scrollbar, `y` down the window.
fn on_track(y: Pixels, editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Point<Pixels> {
    point(viewport(editor, cx).right() - TRACK_WIDTH / 2., y)
}

/// How far the note is scrolled, and how far it can be.
fn scrolled(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> (Pixels, Pixels) {
    editor.read_with(cx, |editor, _| {
        (
            -editor.scroll_handle.offset().y,
            editor.scroll_handle.max_offset().y,
        )
    })
}

fn is_shown(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> bool {
    editor.read_with(cx, |editor, cx| editor.scrollbar.read(cx).is_shown())
}

fn turn_the_wheel(by: Pixels, cx: &mut VisualTestContext) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(300.), px(300.)),
        delta: ScrollDelta::Pixels(point(px(0.), -by)),
        ..Default::default()
    });
    cx.run_until_parked();
}

fn wait(duration: Duration, cx: &mut VisualTestContext) {
    cx.executor().advance_clock(duration);
    cx.run_until_parked();
}

#[track_caller]
fn assert_close(actual: Pixels, expected: Pixels) {
    assert!(
        (actual - expected).abs() <= px(1.),
        "{actual:?} is not {expected:?}"
    );
}

#[gpui::test]
fn only_a_note_longer_than_the_window_has_a_scrollbar(cx: &mut TestAppContext) {
    let (editor, mut cx) = open("short\n", cx);
    assert_eq!(thumb(&editor, &mut cx), None);

    // Writing more than fits brings one.
    cx.simulate_keystrokes("down enter");
    for _ in 0..80 {
        cx.simulate_keystrokes("enter");
        cx.simulate_input("more");
    }
    assert!(thumb(&editor, &mut cx).is_some());
}

#[gpui::test]
fn the_thumb_shows_which_part_of_the_note_is_in_view(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(&long_note(), cx);
    let viewport = viewport(&editor, &mut cx);
    let track = viewport.top() + THUMB_MARGIN..viewport.bottom() - THUMB_MARGIN;
    let (_, scrollable) = scrolled(&editor, &mut cx);

    // Its length is to the track what the part in view is to the note.
    let at_start = thumb(&editor, &mut cx).unwrap();
    let share = viewport.size.height / (viewport.size.height + scrollable);
    assert_eq!(at_start.start, track.start);
    assert_close(
        at_start.end - at_start.start,
        (track.end - track.start) * share,
    );

    turn_the_wheel(scrollable / 2., &mut cx);
    let halfway = thumb(&editor, &mut cx).unwrap();
    assert_close(
        (halfway.start + halfway.end) / 2.,
        (track.start + track.end) / 2.,
    );

    // The note does not scroll past its end, and neither does the thumb.
    turn_the_wheel(scrollable, &mut cx);
    let at_end = thumb(&editor, &mut cx).unwrap();
    assert_eq!(at_end.end, track.end);
    assert_close(at_end.end - at_end.start, at_start.end - at_start.start);
}

#[gpui::test]
fn dragging_the_thumb_scrolls_the_note(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(&long_note(), cx);
    let viewport = viewport(&editor, &mut cx);
    let at_start = thumb(&editor, &mut cx).unwrap();
    let (_, scrollable) = scrolled(&editor, &mut cx);
    // How far the thumb moves to scroll through the whole note.
    let travel = viewport.size.height - THUMB_MARGIN * 2. - (at_start.end - at_start.start);

    let held = on_track(at_start.start + px(5.), &editor, &mut cx);
    cx.simulate_mouse_move(held, None, Modifiers::none());
    cx.simulate_mouse_down(held, MouseButton::Left, Modifiers::none());
    assert_eq!(
        scrolled(&editor, &mut cx).0,
        px(0.),
        "holding it moves nothing"
    );

    let lower = held + point(px(0.), px(100.));
    cx.simulate_mouse_move(lower, MouseButton::Left, Modifiers::none());
    assert_close(
        scrolled(&editor, &mut cx).0,
        scrollable * (px(100.) / travel),
    );
    let moved = thumb(&editor, &mut cx).unwrap();
    assert_close(moved.start, at_start.start + px(100.));

    // The thumb stays held when the pointer leaves the scrollbar, and stops at the end.
    let far = point(px(40.), viewport.bottom() + px(500.));
    cx.simulate_mouse_move(far, MouseButton::Left, Modifiers::none());
    assert_eq!(scrolled(&editor, &mut cx).0, scrollable);
    cx.simulate_mouse_move(held, MouseButton::Left, Modifiers::none());
    assert_eq!(scrolled(&editor, &mut cx).0, px(0.));

    cx.simulate_mouse_move(lower, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(lower, MouseButton::Left, Modifiers::none());
    let released = scrolled(&editor, &mut cx).0;
    assert!(released > px(0.));
    cx.simulate_mouse_move(far, None, Modifiers::none());
    cx.simulate_mouse_move(far, MouseButton::Left, Modifiers::none());
    assert_eq!(scrolled(&editor, &mut cx).0, released, "it was let go of");

    // None of this reached the note under the scrollbar.
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);
}

#[gpui::test]
fn pressing_the_track_brings_the_thumb_there(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(&long_note(), cx);
    let viewport = viewport(&editor, &mut cx);
    let (_, scrollable) = scrolled(&editor, &mut cx);

    let middle = on_track(viewport.center().y, &editor, &mut cx);
    cx.simulate_mouse_down(middle, MouseButton::Left, Modifiers::none());
    let thumb_now = thumb(&editor, &mut cx).unwrap();
    assert_close((thumb_now.start + thumb_now.end) / 2., middle.y);
    assert_close(scrolled(&editor, &mut cx).0, scrollable / 2.);

    // The thumb is then held, and goes on with the pointer.
    let bottom = on_track(viewport.bottom() - px(1.), &editor, &mut cx);
    cx.simulate_mouse_move(bottom, MouseButton::Left, Modifiers::none());
    assert_eq!(scrolled(&editor, &mut cx).0, scrollable);
    cx.simulate_mouse_up(bottom, MouseButton::Left, Modifiers::none());

    let top = on_track(viewport.top() + px(1.), &editor, &mut cx);
    cx.simulate_click(top, Modifiers::none());
    assert_eq!(scrolled(&editor, &mut cx).0, px(0.));

    // The note under the scrollbar was not clicked, as it is beside it.
    assert_eq!(mode(&editor, &mut cx), Mode::Idle);
    cx.simulate_click(top - point(TRACK_WIDTH, px(0.)), Modifiers::none());
    assert_eq!(mode(&editor, &mut cx), Mode::Writing);
}

#[gpui::test]
fn the_scrollbar_is_used_while_writing_without_moving_the_caret(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(&long_note(), cx);
    let viewport = viewport(&editor, &mut cx);
    let (_, scrollable) = scrolled(&editor, &mut cx);
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("!");
    let written = state(&editor, &mut cx);

    // Straight from the keyboard to the scrollbar, without moving the pointer first.
    let bottom = on_track(viewport.bottom() - px(1.), &editor, &mut cx);
    cx.simulate_click(bottom, Modifiers::none());
    assert_eq!(scrolled(&editor, &mut cx).0, scrollable);

    // The wheel scrolls the note with the pointer on the scrollbar too.
    cx.simulate_event(ScrollWheelEvent {
        position: bottom,
        delta: ScrollDelta::Pixels(point(px(0.), scrollable)),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(scrolled(&editor, &mut cx).0, px(0.));

    assert_eq!(state(&editor, &mut cx), written);
    cx.simulate_input("?");
    assert!(saved(&editor, &mut cx).starts_with("paragraph 0!?\n"));
}

#[gpui::test]
fn the_scrollbar_stays_where_the_system_always_shows_scrollbars(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(&long_note(), cx);
    editor.update(&mut cx, |editor, cx| {
        editor
            .scrollbar
            .update(cx, |scrollbar, _| scrollbar.set_auto_hide(false))
    });
    turn_the_wheel(px(300.), &mut cx);
    wait(HIDE_DELAY * 3, &mut cx);
    assert!(is_shown(&editor, &mut cx));
}

#[gpui::test]
fn the_scrollbar_hides_while_unused_where_the_system_hides_scrollbars(cx: &mut TestAppContext) {
    let (editor, mut cx) = open(&long_note(), cx);
    editor.update(&mut cx, |editor, cx| {
        editor
            .scrollbar
            .update(cx, |scrollbar, _| scrollbar.set_auto_hide(true))
    });
    let viewport = viewport(&editor, &mut cx);
    let beside = viewport.center();
    let on_it = on_track(viewport.center().y, &editor, &mut cx);
    let a_moment = HIDE_DELAY / 2;

    // Scrolling shows it for a while.
    turn_the_wheel(px(300.), &mut cx);
    assert!(is_shown(&editor, &mut cx));
    wait(a_moment, &mut cx);
    assert!(is_shown(&editor, &mut cx));
    turn_the_wheel(px(300.), &mut cx);
    wait(a_moment, &mut cx);
    assert!(is_shown(&editor, &mut cx), "scrolling on keeps it");
    wait(a_moment, &mut cx);
    assert!(!is_shown(&editor, &mut cx));

    // So does scrolling with the keyboard, by moving the selection out of view.
    cx.simulate_keystrokes("up");
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert!(is_shown(&editor, &mut cx));
    wait(HIDE_DELAY, &mut cx);
    assert!(!is_shown(&editor, &mut cx));

    // The pointer brings it back, and it stays for as long as the pointer is on it.
    cx.simulate_mouse_move(on_it, None, Modifiers::none());
    assert!(is_shown(&editor, &mut cx));
    wait(HIDE_DELAY * 3, &mut cx);
    assert!(is_shown(&editor, &mut cx));
    cx.simulate_mouse_move(beside, None, Modifiers::none());
    wait(a_moment, &mut cx);
    assert!(is_shown(&editor, &mut cx));
    wait(a_moment, &mut cx);
    assert!(!is_shown(&editor, &mut cx));

    // It also stays for as long as the thumb is held, wherever the pointer is.
    cx.simulate_mouse_move(on_it, None, Modifiers::none());
    cx.simulate_mouse_down(on_it, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(beside, MouseButton::Left, Modifiers::none());
    wait(HIDE_DELAY * 3, &mut cx);
    assert!(is_shown(&editor, &mut cx));
    cx.simulate_mouse_up(beside, MouseButton::Left, Modifiers::none());
    wait(HIDE_DELAY, &mut cx);
    assert!(!is_shown(&editor, &mut cx));

    // The pointer leaving the window from the scrollbar counts as leaving the scrollbar.
    cx.simulate_mouse_move(on_it, None, Modifiers::none());
    cx.simulate_event(MouseExitEvent {
        position: on_it + point(TRACK_WIDTH, px(0.)),
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    wait(HIDE_DELAY, &mut cx);
    assert!(!is_shown(&editor, &mut cx));
}
