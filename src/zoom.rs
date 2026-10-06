//! Zooming the whole window, the way a browser zooms a page.
//!
//! GPUI sizes lengths given in rems by the window's rem size, so every length in the UI is
//! written with [`zoomed`] as its size in pixels at 100%, and zooming only changes the rem
//! size of the window.

use gpui::{Pixels, Rems, px, rems};

/// The rem size at 100%, which is GPUI's default.
const BASE_REM_SIZE: f32 = 16.;

/// A length that is `pixels` at 100% zoom and grows and shrinks with the zoom.
pub const fn zoomed(pixels: f32) -> Rems {
    rems(pixels / BASE_REM_SIZE)
}

/// The zoom levels, in percent, as a browser offers them.
const LEVELS: [u32; 13] = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
const ACTUAL_SIZE: usize = 5;

/// How far the window is zoomed: one of a fixed set of levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zoom(usize);

impl Default for Zoom {
    fn default() -> Self {
        Self(ACTUAL_SIZE)
    }
}

impl Zoom {
    /// The next level up, or this one at the largest.
    pub fn zoom_in(self) -> Self {
        Self((self.0 + 1).min(LEVELS.len() - 1))
    }

    /// The next level down, or this one at the smallest.
    pub fn zoom_out(self) -> Self {
        Self(self.0.saturating_sub(1))
    }

    pub fn percent(self) -> u32 {
        LEVELS[self.0]
    }

    /// How many times larger than at 100% everything is.
    pub fn factor(self) -> f32 {
        self.percent() as f32 / 100.
    }

    /// The rem size to give the window.
    pub fn rem_size(self) -> Pixels {
        px(BASE_REM_SIZE * self.factor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_at_actual_size() {
        assert_eq!(Zoom::default().percent(), 100);
        assert_eq!(Zoom::default().rem_size(), px(16.));
    }

    #[test]
    fn it_steps_through_the_levels_and_stops_at_both_ends() {
        let mut zoom = Zoom::default();
        let mut seen = vec![zoom.percent()];
        for _ in 0..LEVELS.len() {
            zoom = zoom.zoom_in();
            seen.push(zoom.percent());
        }
        assert_eq!(&seen[..4], [100, 110, 125, 150]);
        assert_eq!(seen.last(), Some(&300));
        assert_eq!(zoom.zoom_in(), zoom);

        for _ in 0..LEVELS.len() {
            zoom = zoom.zoom_out();
        }
        assert_eq!(zoom.percent(), 50);
        assert_eq!(zoom.zoom_out(), zoom);
    }

    #[test]
    fn zooming_in_and_out_comes_back() {
        let zoom = Zoom::default();
        assert_eq!(zoom.zoom_in().zoom_out(), zoom);
        assert_eq!(zoom.zoom_out().zoom_in(), zoom);
    }

    #[test]
    fn a_zoomed_length_is_its_pixels_at_actual_size() {
        let at_actual_size = Zoom::default().rem_size();
        assert_eq!(zoomed(24.).to_pixels(at_actual_size), px(24.));
        let doubled = Zoom::default()
            .zoom_in()
            .zoom_in()
            .zoom_in()
            .zoom_in()
            .zoom_in();
        assert_eq!(doubled.percent(), 200);
        assert_eq!(zoomed(24.).to_pixels(doubled.rem_size()), px(48.));
    }
}
