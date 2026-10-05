use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    /// Markdown source that has no rich representation (images, inline HTML, footnote
    /// references). It is shown as-is and written back verbatim, without escaping.
    pub raw: bool,
    pub link: Option<Arc<str>>,
}

impl InlineStyle {
    pub fn has(&self, mark: Mark) -> bool {
        match mark {
            Mark::Bold => self.bold,
            Mark::Italic => self.italic,
            Mark::Strikethrough => self.strikethrough,
            Mark::Code => self.code,
        }
    }

    pub fn set(&mut self, mark: Mark, on: bool) {
        match mark {
            Mark::Bold => self.bold = on,
            Mark::Italic => self.italic = on,
            Mark::Strikethrough => self.strikethrough = on,
            Mark::Code => self.code = on,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Bold,
    Italic,
    Strikethrough,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub len: usize,
    pub style: InlineStyle,
}

/// Text with inline styles, stored as runs that always cover the whole text. Offsets are
/// UTF-8 byte offsets; callers are expected to pass char boundaries, and offsets that are
/// not are rounded down to the nearest one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RichText {
    text: String,
    runs: Vec<Run>,
}

impl RichText {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plain(text: &str) -> Self {
        Self::styled(text, InlineStyle::default())
    }

    pub fn styled(text: &str, style: InlineStyle) -> Self {
        let mut rich_text = Self::new();
        rich_text.push(text, style);
        rich_text
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Iterates over `(byte range, style)` for every run.
    pub fn styled_ranges(&self) -> impl Iterator<Item = (Range<usize>, &InlineStyle)> {
        let mut start = 0;
        self.runs.iter().map(move |run| {
            let range = start..start + run.len;
            start = range.end;
            (range, &run.style)
        })
    }

    pub fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn clamp_range(&self, range: Range<usize>) -> Range<usize> {
        let start = self.clamp(range.start);
        let end = self.clamp(range.end).max(start);
        start..end
    }

    pub fn push(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        self.text.push_str(text);
        match self.runs.last_mut() {
            Some(last) if last.style == style => last.len += text.len(),
            _ => self.runs.push(Run {
                len: text.len(),
                style,
            }),
        }
    }

    pub fn append(&mut self, other: RichText) {
        self.text.push_str(&other.text);
        self.runs.extend(other.runs);
        self.normalize();
    }

    pub fn insert(&mut self, offset: usize, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        let offset = self.clamp(offset);
        let run_index = self.split_runs_at(offset);
        self.text.insert_str(offset, text);
        self.runs.insert(
            run_index,
            Run {
                len: text.len(),
                style,
            },
        );
        self.normalize();
    }

    pub fn insert_rich(&mut self, offset: usize, other: RichText) {
        let offset = self.clamp(offset);
        let run_index = self.split_runs_at(offset);
        self.text.insert_str(offset, &other.text);
        self.runs.splice(run_index..run_index, other.runs);
        self.normalize();
    }

    pub fn delete(&mut self, range: Range<usize>) {
        let range = self.clamp_range(range);
        if range.is_empty() {
            return;
        }
        let first = self.split_runs_at(range.start);
        let last = self.split_runs_at(range.end);
        self.runs.drain(first..last);
        self.text.replace_range(range, "");
        self.normalize();
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str, style: InlineStyle) {
        let range = self.clamp_range(range);
        self.delete(range.clone());
        self.insert(range.start, text, style);
    }

    /// Removes and returns everything from `offset` to the end.
    pub fn split_off(&mut self, offset: usize) -> RichText {
        let offset = self.clamp(offset);
        let run_index = self.split_runs_at(offset);
        RichText {
            text: self.text.split_off(offset),
            runs: self.runs.split_off(run_index),
        }
    }

    /// Drops all inline styles, keeping only the text.
    pub fn to_plain(&self) -> RichText {
        RichText::plain(&self.text)
    }

    /// The style of the character starting at `offset`, if there is one.
    pub fn style_at(&self, offset: usize) -> Option<&InlineStyle> {
        self.styled_ranges()
            .find(|(range, _)| range.contains(&offset))
            .map(|(_, style)| style)
    }

    /// The style newly typed text should get at `offset`. Marks continue from the character
    /// before the caret, while links and raw source only continue when the caret is strictly
    /// inside them, so that typing after a link produces normal text.
    pub fn typing_style(&self, offset: usize) -> InlineStyle {
        let offset = self.clamp(offset);
        let before = self.text[..offset]
            .chars()
            .next_back()
            .and_then(|character| self.style_at(offset - character.len_utf8()));
        let after = self.style_at(offset);
        let mut style = before.or(after).cloned().unwrap_or_default();
        match (before, after) {
            (Some(before), Some(after)) => {
                if before.link != after.link {
                    style.link = None;
                }
                style.raw = before.raw && after.raw;
            }
            _ => {
                style.link = None;
                style.raw = false;
            }
        }
        style
    }

    pub fn link_at(&self, offset: usize) -> Option<&Arc<str>> {
        self.style_at(offset).and_then(|style| style.link.as_ref())
    }

    /// Whether every character in `range` carries `mark`. Empty ranges never do.
    pub fn has_mark(&self, range: Range<usize>, mark: Mark) -> bool {
        let range = self.clamp_range(range);
        if range.is_empty() {
            return false;
        }
        self.styled_ranges()
            .filter(|(run_range, _)| run_range.start < range.end && run_range.end > range.start)
            .all(|(_, style)| style.has(mark) || style.raw)
    }

    pub fn set_mark(&mut self, range: Range<usize>, mark: Mark, on: bool) {
        self.update_styles(range, |style| {
            if !style.raw {
                style.set(mark, on);
            }
        });
    }

    /// Returns whether the mark is set after toggling.
    pub fn toggle_mark(&mut self, range: Range<usize>, mark: Mark) -> bool {
        let on = !self.has_mark(range.clone(), mark);
        self.set_mark(range, mark, on);
        on
    }

    pub fn set_link(&mut self, range: Range<usize>, link: Option<Arc<str>>) {
        self.update_styles(range, |style| {
            if !style.raw {
                style.link = link.clone();
            }
        });
    }

    fn update_styles(&mut self, range: Range<usize>, update: impl Fn(&mut InlineStyle)) {
        let range = self.clamp_range(range);
        if range.is_empty() {
            return;
        }
        let first = self.split_runs_at(range.start);
        let last = self.split_runs_at(range.end);
        for run in &mut self.runs[first..last] {
            update(&mut run.style);
        }
        self.normalize();
    }

    /// Makes sure a run boundary exists at `offset` and returns the index of the run that
    /// starts there (or `runs.len()` when `offset` is the end of the text).
    fn split_runs_at(&mut self, offset: usize) -> usize {
        let mut start = 0;
        for index in 0..self.runs.len() {
            if start == offset {
                return index;
            }
            let end = start + self.runs[index].len;
            if offset < end {
                let tail = Run {
                    len: end - offset,
                    style: self.runs[index].style.clone(),
                };
                self.runs[index].len = offset - start;
                self.runs.insert(index + 1, tail);
                return index + 1;
            }
            start = end;
        }
        self.runs.len()
    }

    /// The text with styles spelled out as HTML-like tags, for assertions in tests.
    #[cfg(test)]
    pub fn markup(&self) -> String {
        let mut output = String::new();
        for (range, style) in self.styled_ranges() {
            let mut tags = Vec::new();
            if let Some(link) = &style.link {
                output.push_str(&format!("<a href=\"{link}\">"));
                tags.push("a");
            }
            for (enabled, tag) in [
                (style.bold, "b"),
                (style.italic, "i"),
                (style.strikethrough, "s"),
                (style.code, "code"),
                (style.raw, "raw"),
            ] {
                if enabled {
                    output.push_str(&format!("<{tag}>"));
                    tags.push(tag);
                }
            }
            output.push_str(&self.text[range]);
            for tag in tags.iter().rev() {
                output.push_str(&format!("</{tag}>"));
            }
        }
        output
    }

    fn normalize(&mut self) {
        let mut merged: Vec<Run> = Vec::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            if run.len == 0 {
                continue;
            }
            match merged.last_mut() {
                Some(last) if last.style == run.style => last.len += run.len,
                _ => merged.push(run),
            }
        }
        self.runs = merged;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn bold() -> InlineStyle {
        InlineStyle {
            bold: true,
            ..Default::default()
        }
    }

    fn link(url: &str) -> InlineStyle {
        InlineStyle {
            link: Some(url.into()),
            ..Default::default()
        }
    }

    fn describe(text: &RichText) -> Vec<(&str, bool)> {
        text.styled_ranges()
            .map(|(range, style)| (&text.text()[range], style.bold))
            .collect()
    }

    fn assert_invariants(text: &RichText) {
        let total: usize = text.runs.iter().map(|run| run.len).sum();
        assert_eq!(total, text.len());
        assert!(text.runs.iter().all(|run| run.len > 0));
        assert!(
            text.runs
                .windows(2)
                .all(|pair| pair[0].style != pair[1].style)
        );
    }

    #[test]
    fn insert_and_delete_keep_runs_consistent() {
        let mut text = RichText::plain("hello world");
        text.set_mark(6..11, Mark::Bold, true);
        assert_eq!(describe(&text), vec![("hello ", false), ("world", true)]);

        text.insert(6, "big ", InlineStyle::default());
        assert_eq!(
            describe(&text),
            vec![("hello big ", false), ("world", true)]
        );

        text.insert(12, "X", bold());
        assert_eq!(
            describe(&text),
            vec![("hello big ", false), ("woXrld", true)]
        );

        text.delete(3..13);
        assert_eq!(describe(&text), vec![("hel", false), ("rld", true)]);
        assert_invariants(&text);

        text.delete(0..text.len());
        assert!(text.is_empty());
        assert!(text.runs.is_empty());
    }

    #[test]
    fn toggle_mark_sets_unless_whole_range_is_marked() {
        let mut text = RichText::plain("abcdef");
        assert!(text.toggle_mark(0..3, Mark::Bold));
        assert!(text.toggle_mark(2..5, Mark::Bold));
        assert_eq!(describe(&text), vec![("abcde", true), ("f", false)]);
        assert!(!text.toggle_mark(1..4, Mark::Bold));
        assert_eq!(
            describe(&text),
            vec![("a", true), ("bcd", false), ("e", true), ("f", false)]
        );
        assert_invariants(&text);
    }

    #[test]
    fn split_off_and_append_round_trip() {
        let mut text = RichText::plain("one two three");
        text.set_mark(2..9, Mark::Bold, true);
        let original = text.clone();

        let tail = text.split_off(6);
        assert_eq!(text.text(), "one tw");
        assert_eq!(tail.text(), "o three");
        assert_invariants(&text);
        assert_invariants(&tail);

        text.append(tail);
        assert_eq!(text, original);
    }

    #[test]
    fn typing_style_extends_marks_but_not_links() {
        let mut text = RichText::plain("a bold link z");
        text.set_mark(2..6, Mark::Bold, true);
        text.set_link(7..11, Some("https://example.com".into()));

        assert!(text.typing_style(6).bold);
        assert!(text.typing_style(4).bold);
        assert!(!text.typing_style(2).bold);

        assert_eq!(text.typing_style(9), link("https://example.com"));
        assert_eq!(text.typing_style(11), InlineStyle::default());
        assert_eq!(text.typing_style(7), InlineStyle::default());
    }

    #[test]
    fn offsets_inside_multibyte_characters_are_rounded_down() {
        let mut text = RichText::plain("añb");
        text.insert(2, "-", InlineStyle::default());
        assert_eq!(text.text(), "a-ñb");
        text.delete(2..3);
        assert_eq!(text.text(), "a-ñb");
        text.delete(1..4);
        assert_eq!(text.text(), "ab");
        assert_invariants(&text);
    }
}
