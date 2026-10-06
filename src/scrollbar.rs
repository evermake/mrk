//! The scrollbar of the note. GPUI draws the whole window itself, so this is not the system's
//! scrollbar but mrk's own: a plain track over the right edge of the note, with a thumb that
//! is as long as the share of the note that is in view and placed where that part is.
//! Dragging the thumb scrolls, and pressing the track above or below it brings the thumb
//! there.

use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Context, CursorStyle, DispatchPhase, Hitbox, HitboxBehavior, MouseButton,
    MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollHandle, Task,
    Window, canvas, fill, point, prelude::*, px, size,
};

use crate::theme::Theme;

// The scrollbar is sized in pixels, not zoomed: it belongs to the window rather than to the
// note.
/// How wide the thumb is, and with it the track: the strip along the right edge that reacts
/// to the pointer.
pub(crate) const WIDTH: Pixels = px(14.);
/// The thumb of a long note does not get shorter than this.
const MIN_THUMB_LENGTH: Pixels = px(24.);
/// How long the scrollbar stays after it was last used, where the system hides scrollbars.
pub(crate) const HIDE_DELAY: Duration = Duration::from_secs(1);
const FADE_DURATION: Duration = Duration::from_millis(250);

/// Where the thumb is along the track, in window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Thumb {
    top: Pixels,
    length: Pixels,
    /// The top of the thumb when the note is scrolled to its start.
    start: Pixels,
    /// How far the thumb moves while the note scrolls from its start to its end.
    travel: Pixels,
    /// How far the note scrolls from its start to its end.
    scrollable: Pixels,
}

impl Thumb {
    /// The thumb on a track from `track_top` that is `track_length` long, for a note that
    /// shows `viewport` of its height and is scrolled by `scrolled` out of `scrollable`.
    /// There is none when the whole note is in view.
    fn new(
        track_top: Pixels,
        track_length: Pixels,
        viewport: Pixels,
        scrolled: Pixels,
        scrollable: Pixels,
    ) -> Option<Self> {
        if scrollable <= Pixels::ZERO || track_length <= Pixels::ZERO {
            return None;
        }
        let length = (track_length * (viewport / (viewport + scrollable)))
            .max(MIN_THUMB_LENGTH)
            .min(track_length);
        let travel = track_length - length;
        Some(Self {
            top: track_top + travel * (scrolled / scrollable).clamp(0., 1.),
            length,
            start: track_top,
            travel,
            scrollable,
        })
    }

    fn contains(&self, y: Pixels) -> bool {
        (self.top..self.top + self.length).contains(&y)
    }

    /// How far the note is scrolled when the top of the thumb is at `top`.
    fn scrolled_at(&self, top: Pixels) -> Pixels {
        if self.travel <= Pixels::ZERO {
            return Pixels::ZERO;
        }
        // Whole pixels keep the text as sharp as it is when scrolled by the wheel.
        (self.scrollable * ((top - self.start) / self.travel).clamp(0., 1.)).round()
    }
}

pub struct Scrollbar {
    scroll_handle: ScrollHandle,
    /// Where the thumb was last painted; `None` while the whole note is in view.
    thumb: Option<Thumb>,
    /// How far below its top the pointer holds the thumb, while it is being dragged.
    grip: Option<Pixels>,
    /// Whether the pointer is on the track.
    hovered: bool,
    /// How far the note was scrolled when it was last painted, to notice that it scrolled.
    painted_scroll: Option<Pixels>,
    /// Since when the scrollbar has been fading out for not being used.
    hidden_since: Option<Instant>,
    hide_task: Option<Task<()>>,
    /// Whether the scrollbar hides while it is not used. `None` follows the system setting
    /// ("Show scroll bars"), as the system's scrollbars do.
    auto_hide: Option<bool>,
}

impl Scrollbar {
    /// The scrollbar of what `scroll_handle` scrolls. It fills the right edge of its parent
    /// element, which is to be the one that scrolls or one of the same size.
    pub fn new(scroll_handle: ScrollHandle) -> Self {
        Self {
            scroll_handle,
            thumb: None,
            grip: None,
            hovered: false,
            painted_scroll: None,
            hidden_since: None,
            hide_task: None,
            auto_hide: None,
        }
    }

    fn auto_hides(&self, cx: &App) -> bool {
        self.auto_hide
            .unwrap_or_else(|| cx.should_auto_hide_scrollbars())
    }

    /// Shows the scrollbar, to hide it again once it has not been used for a while.
    fn reveal(&mut self, cx: &mut Context<Self>) {
        self.hidden_since = None;
        if !self.auto_hides(cx) {
            self.hide_task = None;
            return;
        }
        self.hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HIDE_DELAY).await;
            this.update(cx, |this, cx| {
                if !this.hovered && this.grip.is_none() {
                    this.hidden_since = Some(Instant::now());
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// How opaque the scrollbar is: it fades out when it hides.
    fn opacity(&self, cx: &App) -> f32 {
        match self.hidden_since {
            Some(since) if self.auto_hides(cx) => {
                1. - (since.elapsed().as_secs_f32() / FADE_DURATION.as_secs_f32()).min(1.)
            }
            _ => 1.,
        }
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered != self.hovered {
            self.hovered = hovered;
            // It stays while the pointer is on it, and for a moment after the pointer left.
            self.reveal(cx);
            cx.notify();
        }
    }

    /// The mouse button went down on the track, with the pointer at `y`.
    fn press(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let Some(thumb) = self.thumb else {
            return;
        };
        if thumb.contains(y) {
            self.grip = Some(y - thumb.top);
        } else {
            // Beside the thumb, the middle of the thumb comes to the pointer.
            self.grip = Some(thumb.length / 2.);
            self.drag_to(y, cx);
        }
        self.reveal(cx);
        cx.notify();
    }

    /// Scrolls the note so that the thumb follows the pointer, which is at `y`.
    fn drag_to(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let (Some(thumb), Some(grip)) = (self.thumb, self.grip) else {
            return;
        };
        let mut offset = self.scroll_handle.offset();
        offset.y = -thumb.scrolled_at(y - grip);
        self.scroll_handle.set_offset(offset);
        cx.notify();
    }

    fn release(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.grip = None;
        self.hovered = hovered;
        self.reveal(cx);
        cx.notify();
    }

    /// Works out where the thumb is now that the note is laid out. Returns the area that
    /// reacts to the pointer, which there is none of while the whole note is in view.
    fn layout(
        &mut self,
        track: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Hitbox> {
        let scrolled = -self.scroll_handle.offset().y;
        if self.painted_scroll != Some(scrolled) {
            self.painted_scroll = Some(scrolled);
            self.reveal(cx);
        }
        self.thumb = Thumb::new(
            track.top(),
            track.size.height,
            self.scroll_handle.bounds().size.height,
            scrolled,
            self.scroll_handle.max_offset().y,
        );
        if self.thumb.is_none() {
            self.grip = None;
            self.hovered = false;
            return None;
        }
        Some(window.insert_hitbox(track, HitboxBehavior::Normal))
    }

    #[cfg(test)]
    pub(crate) fn set_auto_hide(&mut self, auto_hide: bool) {
        self.auto_hide = Some(auto_hide);
    }

    /// Whether the scrollbar is to be seen, when there is one.
    #[cfg(test)]
    pub(crate) fn is_shown(&self) -> bool {
        self.hidden_since.is_none()
    }

    /// From where to where down the window the thumb was last painted, when there is one.
    #[cfg(test)]
    pub(crate) fn thumb(&self) -> Option<std::ops::Range<Pixels>> {
        self.thumb.map(|thumb| thumb.top..thumb.top + thumb.length)
    }
}

impl Render for Scrollbar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let track_color = theme.scrollbar_track;
        let (color, active_color) = (theme.scrollbar_thumb, theme.scrollbar_thumb_active);
        let scrollbar = cx.entity();
        canvas(
            {
                let scrollbar = scrollbar.clone();
                // The note is laid out by now, being the element before this one.
                move |track, window, cx| {
                    scrollbar.update(cx, |this, cx| this.layout(track, window, cx))
                }
            },
            move |track, hitbox, window, cx| {
                let Some(hitbox) = hitbox else {
                    return;
                };
                let this = scrollbar.read(cx);
                let Some(thumb) = this.thumb else {
                    return;
                };
                let held = this.grip.is_some();
                // The thumb stands out while the pointer is on the scrollbar or holds it.
                let color = if held || this.hovered {
                    active_color
                } else {
                    color
                };
                let opacity = this.opacity(cx);

                if opacity > 0. {
                    // The track shows where pressing scrolls.
                    window.paint_quad(fill(track, track_color.opacity(opacity)));
                    let thumb = Bounds::new(
                        point(track.left(), thumb.top),
                        size(track.size.width, thumb.length),
                    );
                    window.paint_quad(fill(thumb, color.opacity(opacity)));
                    if opacity < 1. {
                        window.request_animation_frame();
                    }
                }
                if held {
                    // The pointer may leave the track while it holds the thumb.
                    window.set_window_cursor_style(CursorStyle::Arrow);
                } else {
                    window.set_cursor_style(CursorStyle::Arrow, &hitbox);
                }

                window.on_mouse_event({
                    let scrollbar = scrollbar.clone();
                    let hitbox = hitbox.clone();
                    move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.button == MouseButton::Left
                            && hitbox.is_hovered(window)
                        {
                            scrollbar.update(cx, |this, cx| this.press(event.position.y, cx));
                            // The note under the scrollbar is not clicked.
                            cx.stop_propagation();
                        }
                    }
                });
                window.on_mouse_event({
                    let scrollbar = scrollbar.clone();
                    let hitbox = hitbox.clone();
                    move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }
                        let hovered = hitbox.is_hovered(window);
                        scrollbar.update(cx, |this, cx| {
                            if this.grip.is_none() {
                                // Dragging something else over the track does not count.
                                this.set_hovered(hovered && event.pressed_button.is_none(), cx);
                            } else if event.dragging() {
                                this.drag_to(event.position.y, cx);
                                cx.stop_propagation();
                            } else {
                                // The button was released where the window did not see it.
                                this.release(hovered, cx);
                            }
                        });
                    }
                });
                window.on_mouse_event({
                    let scrollbar = scrollbar.clone();
                    move |event: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                            let hovered = hitbox.is_hovered(window);
                            scrollbar.update(cx, |this, cx| {
                                if this.grip.is_some() {
                                    this.release(hovered, cx);
                                }
                            });
                        }
                    }
                });
                // The pointer can leave the window, which the track is at the edge of,
                // without moving off the track first.
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        scrollbar.update(cx, |this, cx| this.set_hovered(false, cx));
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(WIDTH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_that_is_all_in_view_has_no_thumb() {
        assert_eq!(Thumb::new(px(0.), px(400.), px(400.), px(0.), px(0.)), None);
    }

    #[test]
    fn the_thumb_is_as_long_as_the_share_of_the_note_in_view() {
        // A quarter of the note is in view: 400 of 1600.
        let thumb = Thumb::new(px(10.), px(400.), px(400.), px(0.), px(1200.)).unwrap();
        assert_eq!(thumb.length, px(100.));
        assert_eq!(thumb.top, px(10.));

        let halfway = Thumb::new(px(10.), px(400.), px(400.), px(600.), px(1200.)).unwrap();
        assert_eq!(halfway.top, px(160.));
        let at_end = Thumb::new(px(10.), px(400.), px(400.), px(1200.), px(1200.)).unwrap();
        assert_eq!(at_end.top + at_end.length, px(410.));
    }

    #[test]
    fn the_thumb_of_a_long_note_stays_long_enough_to_hold() {
        let thumb = Thumb::new(px(0.), px(400.), px(400.), px(0.), px(1_000_000.)).unwrap();
        assert_eq!(thumb.length, MIN_THUMB_LENGTH);
        let at_end = Thumb::new(px(0.), px(400.), px(400.), px(1_000_000.), px(1_000_000.));
        assert_eq!(at_end.unwrap().top, px(400.) - MIN_THUMB_LENGTH);

        // On a track shorter than that, the thumb fills the track.
        let tiny = Thumb::new(px(0.), px(10.), px(14.), px(50.), px(100.)).unwrap();
        assert_eq!((tiny.top, tiny.length), (px(0.), px(10.)));
        assert_eq!(tiny.scrolled_at(px(5.)), px(0.));
    }

    #[test]
    fn moving_the_thumb_scrolls_in_proportion() {
        let thumb = Thumb::new(px(10.), px(400.), px(400.), px(0.), px(1200.)).unwrap();
        assert_eq!(thumb.scrolled_at(px(10.)), px(0.));
        assert_eq!(thumb.scrolled_at(px(160.)), px(600.));
        assert_eq!(thumb.scrolled_at(px(310.)), px(1200.));
        // The thumb stops at the ends of the track.
        assert_eq!(thumb.scrolled_at(px(-50.)), px(0.));
        assert_eq!(thumb.scrolled_at(px(900.)), px(1200.));
    }
}
