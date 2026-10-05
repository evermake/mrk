//! Conversion between Markdown and the block tree.
//!
//! The conversion is deliberately not lossless: the serializer always writes one canonical
//! style (`-` bullets, ATX headings, fenced code, `*`/`**` emphasis), so a file written in
//! another style changes when saved. Constructs without a block or inline representation
//! are carried through verbatim as raw blocks and raw spans rather than dropped.

use std::ops::Range;
use std::sync::Arc;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::document::{Block, BlockKind, Document};
use crate::rich_text::{InlineStyle, RichText};

pub fn parse(source: &str) -> Document {
    Document::from_blocks(parse_blocks(source))
}

pub fn parse_blocks(source: &str) -> Vec<Block> {
    let source = source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .replace("\r\n", "\n");
    let events = Parser::new_ext(&source, parser_options())
        .into_offset_iter()
        .collect();
    BlockParser {
        source: &source,
        events,
        position: 0,
        task_marker: None,
    }
    .blocks()
}

pub fn serialize(document: &Document) -> String {
    serialize_blocks(document.blocks())
}

pub fn serialize_blocks(blocks: &[Block]) -> String {
    let mut output = sibling_lines(blocks).lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

/// Tables, footnotes and front matter are enabled so that they are recognized and can be
/// preserved as raw blocks; left disabled they would be misread as paragraphs, link
/// definitions and headings.
fn parser_options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

struct BlockParser<'a> {
    source: &'a str,
    events: Vec<(Event<'a>, Range<usize>)>,
    position: usize,
    /// The task marker seen since the innermost list item started.
    task_marker: Option<bool>,
}

impl<'a> BlockParser<'a> {
    fn peek(&self) -> Option<&(Event<'a>, Range<usize>)> {
        self.events.get(self.position)
    }

    fn next(&mut self) -> Option<(Event<'a>, Range<usize>)> {
        let event = self.events.get(self.position).cloned();
        if event.is_some() {
            self.position += 1;
        }
        event
    }

    /// Parses sibling blocks up to and including the end tag of the enclosing container.
    fn blocks(&mut self) -> Vec<Block> {
        let mut blocks = Vec::new();
        while let Some((event, range)) = self.peek().cloned() {
            match event {
                Event::End(_) => {
                    self.position += 1;
                    break;
                }
                Event::Rule => {
                    self.position += 1;
                    blocks.push(Block::new(BlockKind::Divider, RichText::new()));
                }
                Event::Start(tag) if !is_inline_tag(&tag) => {
                    self.position += 1;
                    self.block(tag, range, &mut blocks);
                }
                // Tight list items hold their text directly, without a paragraph around it.
                _ => blocks.push(Block::paragraph(self.inlines(false))),
            }
        }
        blocks
    }

    fn block(&mut self, tag: Tag<'a>, range: Range<usize>, blocks: &mut Vec<Block>) {
        match tag {
            Tag::Paragraph => blocks.push(Block::paragraph(self.inlines(true))),
            Tag::Heading { level, .. } => {
                let text = self.inlines(true);
                blocks.push(Block::new(BlockKind::Heading(level as u8), text));
            }
            Tag::BlockQuote(_) => {
                let children = self.blocks();
                blocks.push(container(BlockKind::Quote, children));
            }
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let mut code = String::new();
                while let Some((event, _)) = self.next() {
                    match event {
                        Event::Text(text) => code.push_str(&text),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                if code.ends_with('\n') {
                    code.pop();
                }
                blocks.push(Block::new(
                    BlockKind::Code { language },
                    RichText::plain(&code),
                ));
            }
            Tag::List(start) => {
                while let Some((event, _)) = self.next() {
                    match event {
                        Event::Start(Tag::Item) => blocks.push(self.item(start.is_some())),
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
            }
            _ => {
                self.skip_to_end();
                blocks.push(self.raw_block(range));
            }
        }
    }

    fn item(&mut self, ordered: bool) -> Block {
        let outer_marker = self.task_marker.take();
        let children = self.blocks();
        let marker = std::mem::replace(&mut self.task_marker, outer_marker);
        let kind = match marker {
            Some(checked) => BlockKind::Todo { checked },
            None if ordered => BlockKind::Numbered,
            None => BlockKind::Bullet,
        };
        container(kind, children)
    }

    /// Parses inline content into rich text. An explicit run is wrapped in a paragraph or
    /// heading and consumes its end tag; an implicit one (the text of a tight list item) ends
    /// in front of whatever block-level event follows it.
    fn inlines(&mut self, explicit: bool) -> RichText {
        let mut text = RichText::new();
        let mut bold = 0usize;
        let mut italic = 0usize;
        let mut strikethrough = 0usize;
        let mut links: Vec<Arc<str>> = Vec::new();

        while let Some((event, range)) = self.peek().cloned() {
            let style = InlineStyle {
                bold: bold > 0,
                italic: italic > 0,
                strikethrough: strikethrough > 0,
                link: links.last().cloned(),
                ..Default::default()
            };
            match event {
                Event::Start(tag) if is_inline_tag(&tag) => {
                    self.position += 1;
                    match tag {
                        Tag::Emphasis => italic += 1,
                        Tag::Strong => bold += 1,
                        Tag::Strikethrough => strikethrough += 1,
                        Tag::Link { dest_url, .. } => links.push(dest_url.as_ref().into()),
                        _ => {
                            self.skip_to_end();
                            let raw = InlineStyle { raw: true, ..style };
                            text.push(&self.source[range], raw);
                        }
                    }
                }
                Event::Start(_) | Event::Rule => break,
                Event::End(TagEnd::Emphasis) => {
                    self.position += 1;
                    italic = italic.saturating_sub(1);
                }
                Event::End(TagEnd::Strong) => {
                    self.position += 1;
                    bold = bold.saturating_sub(1);
                }
                Event::End(TagEnd::Strikethrough) => {
                    self.position += 1;
                    strikethrough = strikethrough.saturating_sub(1);
                }
                Event::End(TagEnd::Link) => {
                    self.position += 1;
                    links.pop();
                }
                Event::End(_) => {
                    if explicit {
                        self.position += 1;
                    }
                    break;
                }
                Event::Text(content) => {
                    self.position += 1;
                    text.push(&content, style);
                }
                Event::Code(content) => {
                    self.position += 1;
                    let code = InlineStyle {
                        code: true,
                        ..style
                    };
                    text.push(&content, code);
                }
                Event::SoftBreak => {
                    self.position += 1;
                    text.push(" ", style);
                }
                Event::HardBreak => {
                    self.position += 1;
                    text.push("\n", style);
                }
                Event::TaskListMarker(checked) => {
                    self.position += 1;
                    self.task_marker = Some(checked);
                }
                Event::Html(_)
                | Event::InlineHtml(_)
                | Event::FootnoteReference(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_) => {
                    self.position += 1;
                    let raw = InlineStyle { raw: true, ..style };
                    text.push(&self.source[range], raw);
                }
            }
        }
        text
    }

    /// Skips the events of a container whose start tag was just consumed.
    fn skip_to_end(&mut self) {
        let mut depth = 1usize;
        while let Some((event, _)) = self.next() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    /// The source of a block as it would read at the top level: lines after the first lose
    /// the indentation and quote markers of the containers the block is nested in.
    fn raw_block(&self, range: Range<usize>) -> Block {
        let line_start = self.source[..range.start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let prefix_width = self.source[line_start..range.start].chars().count();
        let source = self.source[range].trim_end_matches('\n');
        let mut text = String::new();
        for (index, line) in source.split('\n').enumerate() {
            if index == 0 {
                text.push_str(line);
                continue;
            }
            let stripped: usize = line
                .chars()
                .take(prefix_width)
                .take_while(|character| matches!(character, ' ' | '\t' | '>'))
                .map(char::len_utf8)
                .sum();
            text.push('\n');
            text.push_str(&line[stripped..]);
        }
        Block::new(BlockKind::Raw, RichText::plain(&text))
    }
}

fn is_inline_tag(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
    )
}

/// A container's own text is its leading paragraph; everything else becomes its children.
fn container(kind: BlockKind, mut children: Vec<Block>) -> Block {
    let text = match children.first() {
        Some(first) if first.kind == BlockKind::Paragraph => children.remove(0).text,
        _ => RichText::new(),
    };
    Block::new(kind, text).with_children(children)
}

struct SiblingLines<'a> {
    lines: Vec<String>,
    /// The first block that produced any output.
    first: Option<&'a Block>,
}

fn sibling_lines(blocks: &[Block]) -> SiblingLines<'_> {
    let mut result = SiblingLines {
        lines: Vec::new(),
        first: None,
    };
    let mut previous: Option<&Block> = None;
    let mut ordinal = 0;
    for block in blocks {
        let next_ordinal = if block.kind == BlockKind::Numbered {
            ordinal + 1
        } else {
            0
        };
        let lines = block_lines(block, next_ordinal);
        // Markdown has no way to write an empty paragraph.
        if lines.is_empty() {
            continue;
        }
        ordinal = next_ordinal;
        match previous {
            Some(previous) if !continues_list(previous, block) => result.lines.push(String::new()),
            Some(_) => {}
            None => result.first = Some(block),
        }
        result.lines.extend(lines);
        previous = Some(block);
    }
    result
}

/// Whether `block` can directly follow `previous` as the next item of the same list. An item
/// that ends in a non-list block needs a blank line after it to end that block.
fn continues_list(previous: &Block, block: &Block) -> bool {
    fn ends_with_item(block: &Block) -> bool {
        match block.children.last() {
            Some(last) => last.kind.is_list_item() && ends_with_item(last),
            None => true,
        }
    }
    let same_list = matches!(
        (&previous.kind, &block.kind),
        (BlockKind::Numbered, BlockKind::Numbered)
            | (
                BlockKind::Bullet | BlockKind::Todo { .. },
                BlockKind::Bullet | BlockKind::Todo { .. },
            )
    );
    same_list && ends_with_item(previous)
}

fn block_lines(block: &Block, ordinal: usize) -> Vec<String> {
    match &block.kind {
        BlockKind::Paragraph => text_lines(&block.text, false),
        BlockKind::Heading(level) => {
            let hashes = "#".repeat((*level).clamp(1, 6) as usize);
            let text = heading_markdown(&block.text);
            vec![format!("{hashes} {text}").trim_end().to_string()]
        }
        BlockKind::Bullet => container_lines(block, "- ", "  "),
        BlockKind::Numbered => {
            let marker = format!("{ordinal}. ");
            container_lines(block, &marker, &" ".repeat(marker.len()))
        }
        BlockKind::Todo { checked: false } => container_lines(block, "- [ ] ", "  "),
        BlockKind::Todo { checked: true } => container_lines(block, "- [x] ", "  "),
        BlockKind::Quote => container_lines(block, "> ", "> "),
        BlockKind::Code { language } => {
            let code = block.text.text();
            let fence = "`".repeat(longest_run(code, '`').max(2) + 1);
            let language = language.replace(['\n', '`'], " ");
            let mut lines = vec![format!("{fence}{}", language.trim())];
            if !code.is_empty() {
                lines.extend(code.split('\n').map(str::to_string));
            }
            lines.push(fence);
            lines
        }
        BlockKind::Divider => vec!["---".to_string()],
        BlockKind::Raw => {
            let text = block.text.text().trim_matches('\n');
            if text.trim().is_empty() {
                Vec::new()
            } else {
                text.split('\n').map(str::to_string).collect()
            }
        }
    }
}

fn container_lines(block: &Block, first_prefix: &str, rest_prefix: &str) -> Vec<String> {
    let is_quote = block.kind == BlockKind::Quote;
    let mut inner = text_lines(&block.text, block.kind.is_list_item());
    let children = sibling_lines(&block.children);
    if !inner.is_empty() && !children.lines.is_empty() {
        // A nested list can directly follow the item's text, unless its first line is a bare
        // marker, which Markdown would read as a heading underline for that text.
        let nested_list = children
            .first
            .is_some_and(|child| child.kind.is_list_item())
            && children
                .lines
                .first()
                .is_some_and(|line| line.contains(' '));
        if !nested_list {
            inner.push(String::new());
        }
    }
    inner.extend(children.lines);
    if inner.is_empty() {
        inner.push(String::new());
    }

    inner
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 {
                first_prefix
            } else {
                rest_prefix
            };
            if !line.is_empty() {
                format!("{prefix}{line}")
            } else if index == 0 || is_quote {
                prefix.trim_end().to_string()
            } else {
                String::new()
            }
        })
        .collect()
}

fn text_lines(text: &RichText, is_list_item: bool) -> Vec<String> {
    let markdown = inline_markdown(text, is_list_item);
    if markdown.is_empty() {
        Vec::new()
    } else {
        markdown.split('\n').map(str::to_string).collect()
    }
}

fn heading_markdown(text: &RichText) -> String {
    let mut single_line = RichText::new();
    for (range, style) in text.styled_ranges() {
        single_line.push(&text.text()[range].replace('\n', " "), style.clone());
    }
    let mut markdown = inline_markdown(&single_line, false);
    // A trailing run of `#` after a space would be read as the optional closing sequence.
    let content = markdown.trim_end_matches('#');
    if content.len() < markdown.len() && (content.is_empty() || content.ends_with(' ')) {
        markdown.insert(markdown.len() - 1, '\\');
    }
    markdown
}

fn longest_run(text: &str, target: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in text.chars() {
        current = if character == target { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    longest
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Delimiter {
    Link(Arc<str>),
    Bold,
    Italic,
    Strikethrough,
}

/// What sits next to a character in the output, which decides whether Markdown would give
/// that character a special meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Neighbor {
    /// The start or end of a line.
    Edge,
    /// Markup emitted by the serializer itself.
    Markup,
    Character(char),
}

impl Neighbor {
    fn is_space(self) -> bool {
        match self {
            Neighbor::Edge => true,
            Neighbor::Markup => false,
            Neighbor::Character(character) => character.is_whitespace(),
        }
    }

    fn is_alphanumeric(self) -> bool {
        matches!(self, Neighbor::Character(character) if character.is_alphanumeric())
    }
}

/// How often the block uses the characters that only act as markup in pairs. A lone one can
/// be left unescaped.
struct PairableCounts {
    asterisks: usize,
    underscores: usize,
    tildes: usize,
}

fn inline_markdown(text: &RichText, is_list_item: bool) -> String {
    let mut characters: Vec<(char, &InlineStyle)> = Vec::new();
    for (range, style) in text.styled_ranges() {
        for character in text.text()[range].chars() {
            characters.push((character, style));
        }
    }
    let is_content = |(character, style): &(char, &InlineStyle)| {
        !character.is_whitespace() || style.code || style.raw
    };
    let start = characters
        .iter()
        .position(is_content)
        .unwrap_or(characters.len());
    let end = characters
        .iter()
        .rposition(is_content)
        .map_or(start, |index| index + 1);
    let characters = &characters[start..end];

    let literal = |target: char| {
        characters
            .iter()
            .filter(|(character, style)| *character == target && !style.code && !style.raw)
            .count()
    };
    let emphasized = characters
        .iter()
        .any(|(_, style)| style.bold || style.italic);
    let struck = characters.iter().any(|(_, style)| style.strikethrough);
    let mut writer = InlineWriter {
        output: String::new(),
        open: Vec::new(),
        at_line_start: true,
        at_block_start: true,
        last: Neighbor::Edge,
        trailing_whitespace: None,
        is_list_item,
        counts: PairableCounts {
            asterisks: literal('*') + usize::from(emphasized),
            underscores: literal('_'),
            tildes: literal('~') + usize::from(struck),
        },
    };

    let mut index = 0;
    while index < characters.len() {
        let style = characters[index].1;
        let atom_start = index;
        while index < characters.len() && characters[index].1 == style {
            index += 1;
        }
        let atom: Vec<char> = characters[atom_start..index]
            .iter()
            .map(|(character, _)| *character)
            .collect();

        // Delimiters opened together are nested so that the one covering more text is on
        // the outside, which avoids closing and reopening it when the shorter one ends.
        let extent = |has: &dyn Fn(&InlineStyle) -> bool| {
            characters[atom_start..]
                .iter()
                .take_while(|(_, style)| has(style))
                .count()
        };
        let mut desired = Vec::new();
        if let Some(link) = &style.link {
            let length = extent(&|style| style.link.as_ref() == Some(link));
            desired.push((Delimiter::Link(link.clone()), length));
        }
        if style.bold {
            desired.push((Delimiter::Bold, extent(&|style| style.bold)));
        }
        if style.italic {
            desired.push((Delimiter::Italic, extent(&|style| style.italic)));
        }
        if style.strikethrough {
            desired.push((
                Delimiter::Strikethrough,
                extent(&|style| style.strikethrough),
            ));
        }
        let link_ends_here = desired
            .first()
            .is_some_and(|(_, length)| *length == atom.len());
        desired.sort_by_key(|(_, length)| std::cmp::Reverse(*length));
        let desired = desired
            .into_iter()
            .map(|(delimiter, _)| delimiter)
            .collect();
        writer.atom(
            &atom,
            style,
            desired,
            link_ends_here,
            index == characters.len(),
        );
    }
    writer.close_to(0);
    writer.output
}

struct InlineWriter {
    output: String,
    open: Vec<Delimiter>,
    /// Nothing but skipped whitespace has been written on the current line.
    at_line_start: bool,
    at_block_start: bool,
    last: Neighbor,
    /// Where the whitespace at the end of the output starts, if it ends in whitespace.
    /// Closing delimiters go in front of it, because they only work right after text.
    trailing_whitespace: Option<usize>,
    is_list_item: bool,
    counts: PairableCounts,
}

impl InlineWriter {
    fn atom(
        &mut self,
        atom: &[char],
        style: &InlineStyle,
        desired: Vec<Delimiter>,
        link_ends_here: bool,
        is_last: bool,
    ) {
        let kept = self
            .open
            .iter()
            .take_while(|delimiter| desired.contains(delimiter))
            .count();
        self.close_to(kept);

        // Opening delimiters only work right before text, so leading whitespace is written
        // first, and whitespace on its own opens nothing.
        let leading = if style.code || style.raw {
            0
        } else {
            atom.iter()
                .take_while(|character| character.is_whitespace())
                .count()
        };
        self.text(&atom[..leading], false);
        let atom = &atom[leading..];
        if atom.is_empty() {
            return;
        }

        if let [Delimiter::Link(link)] = desired.as_slice()
            && link_ends_here
            && self.open.is_empty()
            && !style.code
            && !style.raw
            && is_autolink(link)
            && atom.iter().collect::<String>() == **link
        {
            self.markup(&format!("<{link}>"));
            return;
        }

        for delimiter in desired {
            if !self.open.contains(&delimiter) {
                self.markup(match &delimiter {
                    Delimiter::Link(_) => "[",
                    Delimiter::Bold => "**",
                    Delimiter::Italic => "*",
                    Delimiter::Strikethrough => "~~",
                });
                self.open.push(delimiter);
            }
        }

        if style.raw {
            self.markup(&atom.iter().collect::<String>());
        } else if style.code {
            self.code_span(atom);
        } else {
            self.text(atom, is_last);
        }
    }

    fn close_to(&mut self, depth: usize) {
        let mut closers = String::new();
        while self.open.len() > depth {
            match self.open.pop() {
                Some(Delimiter::Link(url)) => {
                    closers.push_str(&format!("]({})", link_destination(&url)))
                }
                Some(Delimiter::Bold) => closers.push_str("**"),
                Some(Delimiter::Italic) => closers.push('*'),
                Some(Delimiter::Strikethrough) => closers.push_str("~~"),
                None => {}
            }
        }
        if closers.is_empty() {
            return;
        }
        match self.trailing_whitespace {
            Some(start) => {
                self.output.insert_str(start, &closers);
                self.trailing_whitespace = Some(start + closers.len());
            }
            None => {
                self.output.push_str(&closers);
                self.last = Neighbor::Markup;
            }
        }
    }

    fn markup(&mut self, markup: &str) {
        self.output.push_str(markup);
        self.at_line_start = false;
        self.at_block_start = false;
        self.last = Neighbor::Markup;
        self.trailing_whitespace = None;
    }

    fn code_span(&mut self, atom: &[char]) {
        let content: String = atom
            .iter()
            .map(|character| if *character == '\n' { ' ' } else { *character })
            .collect();
        // A code span ends at the next run of backticks as long as its opening one, so the
        // fence can be the shortest run that the content itself does not contain.
        let mut run_lengths = Vec::new();
        let mut run_length = 0;
        for character in content.chars().chain([' ']) {
            if character == '`' {
                run_length += 1;
            } else if run_length > 0 {
                run_lengths.push(run_length);
                run_length = 0;
            }
        }
        let fence_length = (1..)
            .find(|length| !run_lengths.contains(length))
            .unwrap_or(1);
        let fence = "`".repeat(fence_length);
        // The parser strips one space from each side when both are present, and a span that
        // starts or ends with a backtick needs the space to separate it from the fence.
        let needs_padding = content.starts_with('`')
            || content.ends_with('`')
            || (content.starts_with(' ') && content.ends_with(' ') && !content.trim().is_empty());
        let padding = if needs_padding { " " } else { "" };
        self.markup(&format!("{fence}{padding}{content}{padding}{fence}"));
    }

    /// Writes plain text, escaping whatever Markdown would otherwise interpret. `ends_block`
    /// tells whether the end of `text` is the end of the block rather than more markup.
    fn text(&mut self, text: &[char], ends_block: bool) {
        let in_link = self
            .open
            .iter()
            .any(|delimiter| matches!(delimiter, Delimiter::Link(_)));
        let mut line_start_escape = None;
        for (index, &character) in text.iter().enumerate() {
            if character == '\n' {
                self.trailing_whitespace.get_or_insert(self.output.len());
                self.output.push_str("\\\n");
                self.at_line_start = true;
                self.last = Neighbor::Edge;
                continue;
            }
            if character.is_whitespace() {
                // Markdown drops leading whitespace, and four leading spaces would turn the
                // line into code.
                if !self.at_line_start {
                    self.trailing_whitespace.get_or_insert(self.output.len());
                    self.output.push(character);
                    self.last = Neighbor::Character(character);
                }
                continue;
            }
            if self.at_line_start {
                let line_end = text[index..]
                    .iter()
                    .position(|character| *character == '\n')
                    .map_or(text.len(), |offset| index + offset);
                let is_item_start = self.is_list_item && self.at_block_start;
                line_start_escape = line_start_escape_offset(&text[index..line_end], is_item_start)
                    .map(|offset| index + offset);
            }

            let previous = self.last;
            let next = match text.get(index + 1) {
                Some('\n') => Neighbor::Edge,
                Some(next) => Neighbor::Character(*next),
                None if ends_block => Neighbor::Edge,
                None => Neighbor::Markup,
            };
            if line_start_escape == Some(index)
                || self.needs_escape(character, previous, next, &text[index + 1..], in_link)
            {
                self.output.push('\\');
            }
            self.output.push(character);
            self.at_line_start = false;
            self.at_block_start = false;
            self.last = Neighbor::Character(character);
            self.trailing_whitespace = None;
        }
    }

    fn needs_escape(
        &self,
        character: char,
        previous: Neighbor,
        next: Neighbor,
        rest: &[char],
        in_link: bool,
    ) -> bool {
        let spaced = previous.is_space() && next.is_space();
        match character {
            '\\' => match next {
                Neighbor::Character(next) => next.is_ascii_punctuation(),
                Neighbor::Edge | Neighbor::Markup => true,
            },
            '`' => true,
            '*' => self.counts.asterisks > 1 && !spaced,
            '_' => {
                let intraword = previous.is_alphanumeric() && next.is_alphanumeric();
                self.counts.underscores > 1 && !spaced && !intraword
            }
            '~' => self.counts.tildes > 1 && !spaced,
            '[' => in_link || next == Neighbor::Character('^'),
            ']' => {
                in_link
                    || matches!(
                        next,
                        Neighbor::Markup | Neighbor::Character('(' | '[' | ':')
                    )
            }
            '<' => matches!(
                next,
                Neighbor::Character(next)
                    if next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?')
            ),
            '&' => {
                let name_length = rest
                    .iter()
                    .take_while(|character| character.is_ascii_alphanumeric() || **character == '#')
                    .count();
                name_length > 0 && rest.get(name_length) == Some(&';')
            }
            _ => false,
        }
    }
}

/// If the line would start a block construct (heading, quote, list item, thematic break,
/// heading underline, task marker), the offset of the character to escape to prevent that.
fn line_start_escape_offset(line: &[char], is_item_start: bool) -> Option<usize> {
    let first = *line.first()?;
    let space_at = |index: usize| {
        line.get(index)
            .is_none_or(|character| character.is_whitespace())
    };
    let only = |allowed: char| {
        line.iter()
            .all(|character| *character == allowed || character.is_whitespace())
    };
    match first {
        '#' => {
            let hashes = line
                .iter()
                .take_while(|character| **character == '#')
                .count();
            (hashes <= 6 && space_at(hashes)).then_some(0)
        }
        '>' => Some(0),
        '-' => (space_at(1) || only('-')).then_some(0),
        '+' | '*' => space_at(1).then_some(0),
        '=' => only('=').then_some(0),
        '_' => (only('_') && line.iter().filter(|character| **character == '_').count() >= 3)
            .then_some(0),
        '[' if is_item_start => {
            let is_task_marker = matches!(line.get(1), Some(' ' | 'x' | 'X'))
                && line.get(2) == Some(&']')
                && space_at(3);
            is_task_marker.then_some(0)
        }
        _ if first.is_ascii_digit() => {
            let digits = line
                .iter()
                .take_while(|character| character.is_ascii_digit())
                .count();
            (digits <= 9 && matches!(line.get(digits), Some('.' | ')')) && space_at(digits + 1))
                .then_some(digits)
        }
        _ => None,
    }
}

fn is_autolink(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let mut scheme_characters = scheme.chars();
    scheme_characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && (2..=32).contains(&scheme.len())
        && scheme_characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-')
        })
        && !rest.is_empty()
        && !rest.chars().any(|character| {
            character.is_whitespace() || character.is_control() || matches!(character, '<' | '>')
        })
}

fn link_destination(url: &str) -> String {
    let needs_brackets = url.is_empty()
        || url.starts_with('<')
        || url
            .chars()
            .any(|character| character.is_whitespace() || character.is_control());
    if needs_brackets {
        let escaped: String = url
            .chars()
            .filter(|character| *character != '\n')
            .flat_map(|character| match character {
                '<' | '>' => vec!['\\', character],
                _ => vec![character],
            })
            .collect();
        return format!("<{escaped}>");
    }
    let mut depth = 0i32;
    let balanced = url.chars().all(|character| {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        depth >= 0
    }) && depth == 0;
    if balanced {
        url.to_string()
    } else {
        url.replace('(', "\\(").replace(')', "\\)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn outline(source: &str) -> String {
        parse(source).outline()
    }

    /// Asserts that `source` is already in the canonical style, i.e. opening and saving it
    /// would not change a byte.
    fn assert_canonical(source: &str) {
        assert_eq!(serialize(&parse(source)), source);
    }

    #[test]
    fn parses_block_types() {
        let source = "\
# Title

Some *italic*, **bold**, ~~struck~~ and `code`.

## Second

- bullet
- [ ] todo
- [x] done

1. first
2. second

> quoted

```rust
fn main() {}
```

---
";
        assert_eq!(
            outline(source),
            "\
h1 Title
p Some <i>italic</i>, <b>bold</b>, <s>struck</s> and <code>code</code>.
h2 Second
- bullet
[ ] todo
[x] done
1. first
1. second
> quoted
code(rust) fn main() {}
---
"
        );
        assert_canonical(source);
    }

    #[test]
    fn parses_nesting() {
        let source = "\
- parent
  - child
    - grandchild
  - second child

  A paragraph in the item.

  ```
  code in the item
  ```

- next
  1. numbered child
  2. another

> quote
> - item in quote
>
> more quote
";
        assert_eq!(
            outline(source),
            "\
- parent
  - child
    - grandchild
  - second child
  p A paragraph in the item.
  code() code in the item
- next
  1. numbered child
  1. another
> quote
  - item in quote
  p more quote
"
        );
        assert_canonical(source);
    }

    #[test]
    fn loose_and_tight_lists_parse_the_same() {
        assert_eq!(outline("- a\n- b\n"), outline("- a\n\n- b\n"));
        assert_eq!(
            outline("- [ ] a\n\n  - [x] b\n\n- [ ] c\n"),
            "[ ] a\n  [x] b\n[ ] c\n"
        );
    }

    #[test]
    fn parses_links_and_breaks() {
        assert_eq!(
            outline("See [the **docs**](https://example.com/a_b) or <https://example.com>.\n"),
            "p See <a href=\"https://example.com/a_b\">the </a>\
             <a href=\"https://example.com/a_b\"><b>docs</b></a> or \
             <a href=\"https://example.com\">https://example.com</a>.\n"
        );
        assert_canonical("See [the **docs**](https://example.com/a_b) or <https://example.com>.\n");

        assert_eq!(outline("soft\nbreak\n"), "p soft break\n");
        assert_eq!(outline("hard\\\nbreak\n"), "p hard⏎break\n");
        assert_eq!(outline("hard  \nbreak\n"), "p hard⏎break\n");
        assert_canonical("hard\\\nbreak\n");
    }

    #[test]
    fn unsupported_blocks_are_kept_verbatim() {
        let source = "\
---
title: Note
---

| a | b |
|---|---|
| 1 | 2 |

<details>
<summary>More</summary>
</details>

Text with a footnote[^1] and an ![image](pic.png \"Title\") and <kbd>inline</kbd> HTML.

[^1]: The footnote.
";
        assert_eq!(
            outline(source),
            "\
raw ---⏎title: Note⏎---
raw | a | b |⏎|---|---|⏎| 1 | 2 |
raw <details>⏎<summary>More</summary>⏎</details>
p Text with a footnote<raw>[^1]</raw> and an <raw>![image](pic.png \"Title\")</raw> and \
<raw><kbd></raw>inline<raw></kbd></raw> HTML.
raw [^1]: The footnote.
"
        );
        assert_canonical(source);
    }

    #[test]
    fn unsupported_blocks_inside_containers_lose_the_container_prefix() {
        let source = "\
- item

  | a | b |
  |---|---|
  | 1 | 2 |

> <div>
> html
> </div>
";
        assert_eq!(
            outline(source),
            "- item\n  raw | a | b |⏎|---|---|⏎| 1 | 2 |\n> \n  raw <div>⏎html⏎</div>\n"
        );
        assert_canonical(source);
    }

    #[test]
    fn other_styles_are_normalized() {
        let cases = [
            ("* a\n* b\n", "- a\n- b\n"),
            ("- a\n\n- b\n", "- a\n- b\n"),
            ("Title\n=====\n", "# Title\n"),
            ("__bold__ and _italic_\n", "**bold** and *italic*\n"),
            ("    indented code\n", "```\nindented code\n```\n"),
            ("5. five\n6. six\n", "1. five\n2. six\n"),
            ("hard-wrapped\ntext\n", "hard-wrapped text\n"),
            ("***\n", "---\n"),
            (
                "[ref]\n\n[ref]: https://example.com\n",
                "[ref](https://example.com)\n",
            ),
            ("no trailing newline", "no trailing newline\n"),
            ("windows\r\nline endings\r\n", "windows line endings\n"),
            ("\\#hashtag\n", "#hashtag\n"),
        ];
        for (source, canonical) in cases {
            assert_eq!(serialize(&parse(source)), canonical, "source: {source:?}");
            assert_canonical(canonical);
        }
    }

    #[test]
    fn empty_documents() {
        assert_eq!(outline(""), "p \n");
        assert_eq!(serialize(&parse("")), "");
        assert_eq!(serialize(&parse("\n\n")), "");
    }

    #[test]
    fn empty_blocks() {
        let mut document = parse("- a\n- b\n\n# h\n\n> q\n\n- [ ] t\n\ntext\n");
        let ids: Vec<_> = document.rows().iter().map(|row| row.block.id).collect();
        for id in ids {
            document.block_mut(id).unwrap().text = RichText::new();
        }
        let markdown = serialize(&document);
        assert_eq!(markdown, "-\n-\n\n#\n\n>\n\n- [ ]\n");
        assert_eq!(parse(&markdown).outline(), "- \n- \nh1 \n> \n[ ] \n");
    }

    #[test]
    fn nested_empty_items_do_not_become_headings() {
        let mut document = parse("- a\n  - b\n");
        let b = document.find("b");
        document.block_mut(b).unwrap().text = RichText::new();
        let markdown = serialize(&document);
        assert_eq!(parse(&markdown).outline(), "- a\n  - \n");
    }

    /// Text that Markdown would interpret must come back as the same text.
    #[test]
    fn plain_text_survives_a_round_trip() {
        let texts = [
            "plain",
            "# not a heading",
            "#hashtag",
            "> not a quote",
            "- not a bullet",
            "+ not a bullet",
            "* not a bullet",
            "1. not a list",
            "1) not a list",
            "1986. A great year",
            "--- not a rule",
            "---",
            "===",
            "___",
            "***",
            "* * *",
            "a * b * c",
            "2*3*4",
            "*not italic*",
            "**not bold**",
            "_not italic_",
            "__init__",
            "snake_case_name",
            "_private",
            "~~not struck~~",
            "~/Documents and ~5 minutes",
            "`not code`",
            "```",
            "[not a link](https://example.com)",
            "[not a reference]: definition",
            "[brackets] are fine",
            "[x] not a task",
            "![not an image](a.png)",
            "footnote-like [^1]",
            "<div>not html</div>",
            "<https://example.com>",
            "a < b > c",
            "AT&T and &amp; and &#35;",
            "C:\\Users\\name",
            "\\\\server\\share",
            "trailing backslash\\",
            "back\\*slash",
            "| not | a | table |",
            "line one\nline two",
            "line one\n# not a heading\n- not a bullet\n1. not a list\n===",
            "tabs\tinside",
            "emoji 🎉 and ünïcödé",
            "a  b   c",
            "100% & more",
            "dollar $x$ sign",
        ];
        for text in texts {
            for kind in [BlockKind::Paragraph, BlockKind::Bullet, BlockKind::Quote] {
                let block = Block::new(kind.clone(), RichText::plain(text));
                let document = Document::from_blocks(vec![block]);
                let markdown = serialize(&document);
                assert_eq!(
                    parse(&markdown).outline(),
                    document.outline(),
                    "text {text:?} as {kind:?} was written as {markdown:?}"
                );
            }
        }
    }

    #[test]
    fn common_text_is_not_escaped() {
        let texts = [
            "a * b",
            "5 - 3 = 2",
            "snake_case_name",
            "#hashtag",
            "C:\\Users\\name",
            "[1] citation",
            "AT&T",
            "a < b",
            "~/Documents",
            "price: $5 (approx.)",
            "wait... what?!",
            "100%",
            "1.5 times",
            "e.g. this",
        ];
        for text in texts {
            let document = Document::from_blocks(vec![Block::paragraph(RichText::plain(text))]);
            assert_eq!(serialize(&document), format!("{text}\n"));
        }
    }

    fn styled(parts: &[(&str, InlineStyle)]) -> RichText {
        let mut text = RichText::new();
        for (part, style) in parts {
            text.push(part, style.clone());
        }
        text
    }

    fn style(configure: impl FnOnce(&mut InlineStyle)) -> InlineStyle {
        let mut style = InlineStyle::default();
        configure(&mut style);
        style
    }

    #[test]
    fn styled_text_survives_a_round_trip() {
        let plain = InlineStyle::default();
        let bold = style(|style| style.bold = true);
        let italic = style(|style| style.italic = true);
        let both = style(|style| {
            style.bold = true;
            style.italic = true;
        });
        let code = style(|style| style.code = true);
        let link = style(|style| style.link = Some("https://example.com/(a)".into()));
        let spaced_link = style(|style| style.link = Some("a file.md".into()));

        let cases: Vec<(RichText, &str)> = vec![
            (
                styled(&[
                    ("a ", plain.clone()),
                    ("b", bold.clone()),
                    (" c", plain.clone()),
                ]),
                "a **b** c",
            ),
            (
                styled(&[
                    ("it ", italic.clone()),
                    ("both", both.clone()),
                    (" it", italic.clone()),
                ]),
                "*it **both** it*",
            ),
            (
                styled(&[("both", both.clone()), ("it", italic.clone())]),
                "***both**it*",
            ),
            (
                styled(&[
                    ("bold", bold.clone()),
                    (" ", plain.clone()),
                    ("bold", bold.clone()),
                ]),
                "**bold** **bold**",
            ),
            (
                styled(&[
                    ("in", plain.clone()),
                    ("tra", italic.clone()),
                    ("word", plain.clone()),
                ]),
                "in*tra*word",
            ),
            (
                styled(&[
                    ("x", plain.clone()),
                    ("a`b", code.clone()),
                    ("y", plain.clone()),
                ]),
                "x``a`b``y",
            ),
            (styled(&[("`tick`", code.clone())]), "`` `tick` ``"),
            (styled(&[("```rust", code.clone())]), "` ```rust `"),
            (styled(&[("a`b``c", code.clone())]), "```a`b``c```"),
            (styled(&[(" pad ", code.clone())]), "`  pad  `"),
            (
                styled(&[("see ", plain.clone()), ("the [docs]", link.clone())]),
                "see [the \\[docs\\]](https://example.com/(a))",
            ),
            (
                styled(&[("file", spaced_link.clone())]),
                "[file](<a file.md>)",
            ),
            (
                styled(&[("star * inside", italic.clone())]),
                "*star * inside*",
            ),
            (styled(&[("2*3", italic.clone())]), "*2\\*3*"),
        ];
        for (text, expected) in cases {
            let document = Document::from_blocks(vec![Block::paragraph(text)]);
            let markdown = serialize(&document);
            assert_eq!(markdown, format!("{expected}\n"));
            assert_eq!(parse(&markdown).outline(), document.outline());
        }
    }

    #[test]
    fn whitespace_at_the_edge_of_a_mark_moves_outside() {
        let plain = InlineStyle::default();
        let bold = style(|style| style.bold = true);
        let italic = style(|style| style.italic = true);
        let both = style(|style| {
            style.bold = true;
            style.italic = true;
        });
        let cases = [
            (
                styled(&[
                    ("a", plain.clone()),
                    (" b ", bold.clone()),
                    ("c", plain.clone()),
                ]),
                "a **b** c\n",
            ),
            (
                styled(&[("bold ", bold.clone()), ("both", both), (" it", italic)]),
                "**bold *both*** *it*\n",
            ),
            (
                styled(&[
                    ("a", plain.clone()),
                    (" ", bold.clone()),
                    ("c", plain.clone()),
                ]),
                "a c\n",
            ),
            (
                styled(&[("one\n", bold), ("two", plain)]),
                "**one**\\\ntwo\n",
            ),
        ];
        for (text, expected) in cases {
            let document = Document::from_blocks(vec![Block::paragraph(text)]);
            assert_eq!(serialize(&document), expected);
        }
    }

    #[test]
    fn headings_stay_on_one_line() {
        let cases = [
            ("two\nlines", "# two lines\n"),
            ("trailing #", "# trailing \\#\n"),
            ("c#", "# c#\n"),
            ("#", "# \\#\n"),
        ];
        for (text, expected) in cases {
            let block = Block::new(BlockKind::Heading(1), RichText::plain(text));
            let document = Document::from_blocks(vec![block]);
            let markdown = serialize(&document);
            assert_eq!(markdown, expected);
            assert_eq!(
                parse(&markdown).outline(),
                format!("h1 {}\n", text.replace('\n', " "))
            );
        }
    }

    #[test]
    fn code_blocks_keep_their_content() {
        for code in [
            "",
            "one",
            "one\n\ntwo\n",
            "```\nnested fence\n```",
            "  indented\n\ttabbed",
        ] {
            let block = Block::new(
                BlockKind::Code {
                    language: "text".to_string(),
                },
                RichText::plain(code),
            );
            let document = Document::from_blocks(vec![block]);
            let markdown = serialize(&document);
            assert_eq!(parse(&markdown).outline(), document.outline(), "{markdown}");
        }
    }

    #[test]
    fn serializing_is_stable() {
        let source = "\
# Notes

1. one
   - nested **bold**
   - [x] done

   para in item
2. two

> quote
>
> > nested quote
";
        let once = serialize(&parse(source));
        assert_eq!(serialize(&parse(&once)), once);
    }
}
