use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::rich_text::RichText;

pub type BlockId = u64;

/// A place in the text of a block, as a byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPosition {
    pub block: BlockId,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Bullet,
    Numbered,
    Todo {
        checked: bool,
    },
    Quote,
    Code {
        language: String,
    },
    Divider,
    /// Markdown that has no block representation (tables, HTML, front matter, footnote
    /// definitions). Its text is the verbatim source and is written back unchanged.
    Raw,
}

impl BlockKind {
    /// Only the block types whose nesting Markdown can express hold children.
    pub fn can_have_children(&self) -> bool {
        matches!(
            self,
            BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo { .. } | BlockKind::Quote
        )
    }

    pub fn has_text(&self) -> bool {
        !matches!(self, BlockKind::Divider)
    }

    /// Blocks whose text is literal: no inline styles, and Enter inserts a line break.
    pub fn is_verbatim(&self) -> bool {
        matches!(self, BlockKind::Code { .. } | BlockKind::Raw)
    }

    pub fn is_list_item(&self) -> bool {
        matches!(
            self,
            BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo { .. }
        )
    }

    /// The kind of an empty block that carries on from one of this kind: what Enter creates
    /// when it splits the block, and what is opened next to it. A list goes on with another
    /// item, anything else with a paragraph.
    pub fn continuation(&self) -> BlockKind {
        match self {
            BlockKind::Bullet => BlockKind::Bullet,
            BlockKind::Numbered => BlockKind::Numbered,
            BlockKind::Todo { .. } => BlockKind::Todo { checked: false },
            _ => BlockKind::Paragraph,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    /// Unique within its document and stable across edits. Blocks built outside a document
    /// carry a placeholder until [`Document::adopt`] gives them a real one.
    pub id: BlockId,
    pub kind: BlockKind,
    pub text: RichText,
    pub children: Vec<Block>,
}

impl Block {
    pub fn new(kind: BlockKind, text: RichText) -> Self {
        Self {
            id: 0,
            kind,
            text,
            children: Vec::new(),
        }
    }

    pub fn paragraph(text: RichText) -> Self {
        Self::new(BlockKind::Paragraph, text)
    }

    pub fn with_children(mut self, children: Vec<Block>) -> Self {
        self.children = children;
        self
    }
}

#[cfg(test)]
impl Block {
    /// The block's type and `text` on one line, as used in test outlines. The text is passed
    /// in so that callers can annotate it first.
    pub fn outline_line(&self, text: &RichText) -> String {
        let label = match &self.kind {
            BlockKind::Paragraph => "p".to_string(),
            BlockKind::Heading(level) => format!("h{level}"),
            BlockKind::Bullet => "-".to_string(),
            BlockKind::Numbered => "1.".to_string(),
            BlockKind::Todo { checked: false } => "[ ]".to_string(),
            BlockKind::Todo { checked: true } => "[x]".to_string(),
            BlockKind::Quote => ">".to_string(),
            BlockKind::Code { language } => format!("code({language})"),
            BlockKind::Divider => "---".to_string(),
            BlockKind::Raw => "raw".to_string(),
        };
        if self.kind.has_text() {
            format!("{label} {}", text.markup().replace('\n', "⏎"))
        } else {
            label
        }
    }
}

/// One block as it appears in document order, with what rendering needs to know about its
/// position in the tree.
#[derive(Clone, Debug)]
pub struct Row<'a> {
    pub block: &'a Block,
    pub depth: usize,
    /// For every ancestor, outermost first, whether it is a quote.
    pub quote_ancestors: Vec<bool>,
    /// 1-based position within a run of consecutive numbered siblings.
    pub ordinal: usize,
}

/// A contiguous run of sibling blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SiblingRange {
    parent: Vec<usize>,
    range: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct Document {
    blocks: Vec<Block>,
    next_id: BlockId,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// A document always holds at least one block, so there is always somewhere to type.
    pub fn new() -> Self {
        Self::from_blocks(Vec::new())
    }

    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        let mut document = Self {
            blocks: Vec::new(),
            next_id: 1,
        };
        document.blocks = blocks
            .into_iter()
            .map(|block| document.adopt(block))
            .collect();
        document.ensure_not_empty();
        document
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Gives `block` and its descendants identifiers that are fresh in this document.
    pub fn adopt(&mut self, mut block: Block) -> Block {
        block.id = self.next_id;
        self.next_id += 1;
        block.children = std::mem::take(&mut block.children)
            .into_iter()
            .map(|child| self.adopt(child))
            .collect();
        block
    }

    fn ensure_not_empty(&mut self) {
        if self.blocks.is_empty() {
            let block = self.adopt(Block::paragraph(RichText::new()));
            self.blocks.push(block);
        }
    }

    pub fn path_of(&self, id: BlockId) -> Option<Vec<usize>> {
        fn find(blocks: &[Block], id: BlockId, path: &mut Vec<usize>) -> bool {
            for (index, block) in blocks.iter().enumerate() {
                path.push(index);
                if block.id == id || find(&block.children, id, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        find(&self.blocks, id, &mut path).then_some(path)
    }

    fn siblings(&self, parent: &[usize]) -> Option<&Vec<Block>> {
        let mut blocks = &self.blocks;
        for &index in parent {
            blocks = &blocks.get(index)?.children;
        }
        Some(blocks)
    }

    fn siblings_mut(&mut self, parent: &[usize]) -> Option<&mut Vec<Block>> {
        let mut blocks = &mut self.blocks;
        for &index in parent {
            blocks = &mut blocks.get_mut(index)?.children;
        }
        Some(blocks)
    }

    fn block_at(&self, path: &[usize]) -> Option<&Block> {
        let (&index, parent) = path.split_last()?;
        self.siblings(parent)?.get(index)
    }

    fn block_at_mut(&mut self, path: &[usize]) -> Option<&mut Block> {
        let (&index, parent) = path.split_last()?;
        self.siblings_mut(parent)?.get_mut(index)
    }

    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.block_at(&self.path_of(id)?)
    }

    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        let path = self.path_of(id)?;
        self.block_at_mut(&path)
    }

    pub fn first(&self) -> BlockId {
        self.blocks.first().map_or(0, |block| block.id)
    }

    /// The last block of the top level, which [`Self::last`] may be nested in.
    pub fn last_top_level(&self) -> BlockId {
        self.blocks.last().map_or(0, |block| block.id)
    }

    /// The last block in document order, i.e. the one displayed at the very bottom.
    pub fn last(&self) -> BlockId {
        self.blocks
            .last()
            .map_or(0, |block| last_descendant(block).id)
    }

    pub fn parent(&self, id: BlockId) -> Option<BlockId> {
        let path = self.path_of(id)?;
        let (_, parent) = path.split_last()?;
        self.block_at(parent).map(|block| block.id)
    }

    pub fn first_child(&self, id: BlockId) -> Option<BlockId> {
        self.block(id)?.children.first().map(|child| child.id)
    }

    pub fn next_sibling(&self, id: BlockId) -> Option<BlockId> {
        let path = self.path_of(id)?;
        let (&index, parent) = path.split_last()?;
        self.siblings(parent)?.get(index + 1).map(|block| block.id)
    }

    pub fn previous_sibling(&self, id: BlockId) -> Option<BlockId> {
        let path = self.path_of(id)?;
        let (&index, parent) = path.split_last()?;
        let previous = index.checked_sub(1)?;
        self.siblings(parent)?.get(previous).map(|block| block.id)
    }

    /// The block displayed right below `id`: its first child, else its next sibling, else the
    /// next sibling of the nearest ancestor that has one.
    pub fn next(&self, id: BlockId) -> Option<BlockId> {
        if let Some(child) = self.first_child(id) {
            return Some(child);
        }
        let mut path = self.path_of(id)?;
        while let Some(index) = path.pop() {
            if let Some(block) = self.siblings(&path)?.get(index + 1) {
                return Some(block.id);
            }
        }
        None
    }

    /// The block displayed right above `id`: the last descendant of its previous sibling, or
    /// its parent when it is a first child.
    pub fn previous(&self, id: BlockId) -> Option<BlockId> {
        let path = self.path_of(id)?;
        let (&index, parent) = path.split_last()?;
        match index.checked_sub(1) {
            Some(previous) => self
                .siblings(parent)?
                .get(previous)
                .map(|block| last_descendant(block).id),
            None => self.block_at(parent).map(|block| block.id),
        }
    }

    pub fn rows(&self) -> Vec<Row<'_>> {
        fn visit<'a>(
            blocks: &'a [Block],
            quote_ancestors: &mut Vec<bool>,
            rows: &mut Vec<Row<'a>>,
        ) {
            let mut ordinal = 0;
            for block in blocks {
                ordinal = if block.kind == BlockKind::Numbered {
                    ordinal + 1
                } else {
                    0
                };
                rows.push(Row {
                    block,
                    depth: quote_ancestors.len(),
                    quote_ancestors: quote_ancestors.clone(),
                    ordinal,
                });
                quote_ancestors.push(block.kind == BlockKind::Quote);
                visit(&block.children, quote_ancestors, rows);
                quote_ancestors.pop();
            }
        }
        let mut rows = Vec::new();
        visit(&self.blocks, &mut Vec::new(), &mut rows);
        rows
    }

    /// The contiguous siblings between `first` and `second`, which must share a parent.
    fn sibling_range(&self, first: BlockId, second: BlockId) -> Option<SiblingRange> {
        let first_path = self.path_of(first)?;
        let second_path = self.path_of(second)?;
        let (&first_index, first_parent) = first_path.split_last()?;
        let (&second_index, second_parent) = second_path.split_last()?;
        if first_parent != second_parent {
            return None;
        }
        Some(SiblingRange {
            parent: first_parent.to_vec(),
            range: first_index.min(second_index)..first_index.max(second_index) + 1,
        })
    }

    /// The blocks between two siblings, inclusive, in document order.
    pub fn siblings_between(&self, first: BlockId, second: BlockId) -> Vec<BlockId> {
        let Some(selection) = self.sibling_range(first, second) else {
            return Vec::new();
        };
        self.siblings(&selection.parent)
            .and_then(|siblings| siblings.get(selection.range))
            .map(|blocks| blocks.iter().map(|block| block.id).collect())
            .unwrap_or_default()
    }

    /// Every block covered by selecting the siblings between `first` and `second`: the
    /// siblings themselves and all of their descendants.
    pub fn subtree_ids(&self, first: BlockId, second: BlockId) -> HashSet<BlockId> {
        fn collect(block: &Block, ids: &mut HashSet<BlockId>) {
            ids.insert(block.id);
            for child in &block.children {
                collect(child, ids);
            }
        }
        let mut ids = HashSet::new();
        for id in self.siblings_between(first, second) {
            if let Some(block) = self.block(id) {
                collect(block, &mut ids);
            }
        }
        ids
    }

    /// Copies of the siblings between `first` and `second`, with their subtrees.
    pub fn blocks_between(&self, first: BlockId, second: BlockId) -> Vec<Block> {
        self.siblings_between(first, second)
            .into_iter()
            .filter_map(|id| self.block(id).cloned())
            .collect()
    }

    /// Where two blocks are in document order, which is top to bottom as displayed.
    pub fn order(&self, first: BlockId, second: BlockId) -> Option<Ordering> {
        // Paths compare like document order does: a block is followed by its children.
        Some(self.path_of(first)?.cmp(&self.path_of(second)?))
    }

    /// Where two text positions are in document order.
    pub fn compare(&self, first: TextPosition, second: TextPosition) -> Option<Ordering> {
        if first.block == second.block {
            Some(first.offset.cmp(&second.offset))
        } else {
            self.order(first.block, second.block)
        }
    }

    /// `first` and `second` as (earlier, later) in document order.
    pub fn in_order(
        &self,
        first: TextPosition,
        second: TextPosition,
    ) -> Option<(TextPosition, TextPosition)> {
        Some(if self.compare(first, second)? == Ordering::Greater {
            (second, first)
        } else {
            (first, second)
        })
    }

    /// The smallest run of siblings that includes both blocks, which may be nested at
    /// different depths: what block operations act on when text is selected across blocks.
    pub fn covering_siblings(&self, first: BlockId, second: BlockId) -> Option<(BlockId, BlockId)> {
        let first_path = self.path_of(first)?;
        let second_path = self.path_of(second)?;
        let shared = first_path
            .iter()
            .zip(&second_path)
            .take_while(|(first, second)| first == second)
            .count();
        // When one block holds the other, the holder is the run.
        let depth = shared.min(first_path.len().min(second_path.len()) - 1);
        let covering = |path: &[usize]| self.block_at(&path[..=depth]).map(|block| block.id);
        Some((covering(&first_path)?, covering(&second_path)?))
    }

    fn blocks_in_order(&self) -> Vec<&Block> {
        fn visit<'a>(blocks: &'a [Block], order: &mut Vec<&'a Block>) {
            for block in blocks {
                order.push(block);
                visit(&block.children, order);
            }
        }
        let mut order = Vec::new();
        visit(&self.blocks, &mut order);
        order
    }

    /// Every block from `start` to `end` (in document order) with the part of its text that
    /// lies between them. That is all of the text of the blocks in between, and nothing for
    /// blocks that have none.
    pub fn span_ranges(
        &self,
        start: TextPosition,
        end: TextPosition,
    ) -> Vec<(&Block, Range<usize>)> {
        let order = self.blocks_in_order();
        let first = order.iter().position(|block| block.id == start.block);
        let last = order.iter().position(|block| block.id == end.block);
        let Some(blocks) = first
            .zip(last)
            .and_then(|(first, last)| order.get(first..=last))
        else {
            return Vec::new();
        };
        blocks
            .iter()
            .map(|block| {
                let length = block.text.len();
                let from = if block.id == start.block {
                    start.offset.min(length)
                } else {
                    0
                };
                let to = if block.id == end.block {
                    end.offset.min(length)
                } else {
                    length
                };
                (*block, from..to.max(from))
            })
            .collect()
    }

    /// Copies of the blocks between `start` and `end`, cut to the text in between. A block
    /// keeps the children that are in the span too; the children of a block that is not in
    /// it are taken up by its nearest ancestor that is, or end up at the top level.
    pub fn copy_span(&self, start: TextPosition, end: TextPosition) -> Vec<Block> {
        fn copy(blocks: &[Block], ranges: &HashMap<BlockId, Range<usize>>) -> Vec<Block> {
            let mut copies = Vec::new();
            for block in blocks {
                let children = copy(&block.children, ranges);
                match ranges.get(&block.id) {
                    Some(range) => copies.push(
                        Block::new(block.kind.clone(), block.text.slice(range.clone()))
                            .with_children(children),
                    ),
                    None => copies.extend(children),
                }
            }
            copies
        }
        let ranges = self
            .span_ranges(start, end)
            .into_iter()
            .map(|(block, range)| (block.id, range))
            .collect();
        copy(&self.blocks, &ranges)
    }

    /// Deletes the text between `start` and `end`, which are in different blocks and in
    /// document order. The blocks between them go, and so does the text up to `end` in the
    /// last block; then the rest of the last block joins the first block, which keeps its
    /// type. They stay apart when only one of them holds literal text (a code block). What
    /// is nested in a block that goes stays, in its place. Returns where the caret belongs
    /// afterwards.
    pub fn delete_span(&mut self, start: TextPosition, end: TextPosition) -> Option<TextPosition> {
        let between: Vec<BlockId> = {
            let order = self.blocks_in_order();
            let first = order.iter().position(|block| block.id == start.block)?;
            let last = order.iter().position(|block| block.id == end.block)?;
            if first >= last {
                return None;
            }
            order[first + 1..last]
                .iter()
                .map(|block| block.id)
                .collect()
        };
        for id in between {
            self.take_promoting_children(id);
        }

        let last = self.block_mut(end.block)?;
        let end_offset = last.text.clamp(end.offset);
        last.text.delete(0..end_offset);
        let first = self.block_mut(start.block)?;
        let offset = first.text.clamp(start.offset);
        let length = first.text.len();
        first.text.delete(offset..length);

        let (first, last) = (self.block(start.block)?, self.block(end.block)?);
        let joins = first.kind.has_text()
            && last.kind.has_text()
            && first.kind.is_verbatim() == last.kind.is_verbatim();
        if joins {
            let removed = self.take_promoting_children(end.block)?;
            self.block_mut(start.block)?.text.append(removed.text);
        }
        Some(TextPosition {
            block: start.block,
            offset,
        })
    }

    /// Nests the selected siblings under the sibling right above them.
    pub fn indent(&mut self, first: BlockId, second: BlockId) -> bool {
        let Some(selection) = self.sibling_range(first, second) else {
            return false;
        };
        let Some(siblings) = self.siblings_mut(&selection.parent) else {
            return false;
        };
        let Some(new_parent_index) = selection.range.start.checked_sub(1) else {
            return false;
        };
        if !siblings[new_parent_index].kind.can_have_children() {
            return false;
        }
        let moved: Vec<Block> = siblings.drain(selection.range).collect();
        siblings[new_parent_index].children.extend(moved);
        true
    }

    /// Moves the selected siblings out of their parent, placing them right after it. Blocks
    /// that followed the selection inside the parent stay below it: they become children of
    /// the last moved block, or move out alongside it when it cannot hold children.
    pub fn outdent(&mut self, first: BlockId, second: BlockId) -> bool {
        let Some(selection) = self.sibling_range(first, second) else {
            return false;
        };
        let Some((&parent_index, grandparent)) = selection.parent.split_last() else {
            return false;
        };
        let Some(siblings) = self.siblings_mut(&selection.parent) else {
            return false;
        };
        let following = siblings.split_off(selection.range.end);
        let mut moved = siblings.split_off(selection.range.start);
        match moved.last_mut() {
            Some(last) if last.kind.can_have_children() => last.children.extend(following),
            _ => moved.extend(following),
        }
        let Some(outer) = self.siblings_mut(grandparent) else {
            return false;
        };
        outer.splice(parent_index + 1..parent_index + 1, moved);
        true
    }

    pub fn move_up(&mut self, first: BlockId, second: BlockId) -> bool {
        let Some(selection) = self.sibling_range(first, second) else {
            return false;
        };
        let Some(siblings) = self.siblings_mut(&selection.parent) else {
            return false;
        };
        if selection.range.start == 0 {
            return false;
        }
        siblings[selection.range.start - 1..selection.range.end].rotate_left(1);
        true
    }

    pub fn move_down(&mut self, first: BlockId, second: BlockId) -> bool {
        let Some(selection) = self.sibling_range(first, second) else {
            return false;
        };
        let Some(siblings) = self.siblings_mut(&selection.parent) else {
            return false;
        };
        if selection.range.end >= siblings.len() {
            return false;
        }
        siblings[selection.range.start..selection.range.end + 1].rotate_right(1);
        true
    }

    /// Deletes the selected siblings with their subtrees and returns the block that should be
    /// selected afterwards.
    pub fn delete(&mut self, first: BlockId, second: BlockId) -> Option<BlockId> {
        let selection = self.sibling_range(first, second)?;
        let siblings = self.siblings_mut(&selection.parent)?;
        siblings.drain(selection.range.clone());
        let neighbor = siblings
            .get(selection.range.start)
            .or_else(|| siblings.get(selection.range.start.checked_sub(1)?))
            .map(|block| block.id);
        let parent = self.block_at(&selection.parent).map(|block| block.id);
        self.ensure_not_empty();
        Some(neighbor.or(parent).unwrap_or_else(|| self.first()))
    }

    pub fn remove(&mut self, id: BlockId) -> Option<Block> {
        let path = self.path_of(id)?;
        let (&index, parent) = path.split_last()?;
        let block = self.siblings_mut(parent)?.remove(index);
        self.ensure_not_empty();
        Some(block)
    }

    /// Inserts `blocks` as siblings right after `id` and returns their new identifiers.
    pub fn insert_after(&mut self, id: BlockId, blocks: Vec<Block>) -> Vec<BlockId> {
        self.insert_relative(id, 1, blocks)
    }

    pub fn insert_before(&mut self, id: BlockId, blocks: Vec<Block>) -> Vec<BlockId> {
        self.insert_relative(id, 0, blocks)
    }

    fn insert_relative(&mut self, id: BlockId, offset: usize, blocks: Vec<Block>) -> Vec<BlockId> {
        let Some(path) = self.path_of(id) else {
            return Vec::new();
        };
        let Some((&index, parent)) = path.split_last() else {
            return Vec::new();
        };
        let blocks: Vec<Block> = blocks.into_iter().map(|block| self.adopt(block)).collect();
        let ids = blocks.iter().map(|block| block.id).collect();
        match self.siblings_mut(parent) {
            Some(siblings) => {
                siblings.splice(index + offset..index + offset, blocks);
                ids
            }
            None => Vec::new(),
        }
    }

    /// Splits the block at `offset`, as pressing Enter there does, and returns the block the
    /// caret belongs in afterwards (at offset 0).
    pub fn split(&mut self, id: BlockId, offset: usize) -> Option<BlockId> {
        let block = self.block(id)?;
        let kind = block.kind.continuation();
        if offset == 0 && !block.text.is_empty() {
            // Enter at the very start pushes the block down instead of moving its text into a
            // new block, so the block keeps its type and children.
            self.insert_before(id, vec![Block::new(kind, RichText::new())]);
            return Some(id);
        }
        let has_children = !block.children.is_empty();
        let block = self.block_mut(id)?;
        let tail = block.text.split_off(offset);
        let new_block = Block::new(kind, tail);
        if has_children {
            // The new block goes right below the text it was split from, which is above the
            // existing children.
            let new_block = self.adopt(new_block);
            let new_id = new_block.id;
            self.block_mut(id)?.children.insert(0, new_block);
            Some(new_id)
        } else {
            self.insert_after(id, vec![new_block]).first().copied()
        }
    }

    /// Changes the type of a block. Children that the new type cannot hold are moved out to
    /// follow the block, and text loses its styles when the new type is verbatim.
    pub fn set_kind(&mut self, id: BlockId, kind: BlockKind) -> bool {
        let Some(block) = self.block_mut(id) else {
            return false;
        };
        if kind.is_verbatim() && !block.kind.is_verbatim() {
            block.text = block.text.to_plain();
        }
        if !kind.has_text() {
            block.text = RichText::new();
        }
        let orphans = if kind.can_have_children() {
            Vec::new()
        } else {
            std::mem::take(&mut block.children)
        };
        block.kind = kind;
        self.insert_existing_after(id, orphans);
        true
    }

    fn insert_existing_after(&mut self, id: BlockId, blocks: Vec<Block>) {
        if blocks.is_empty() {
            return;
        }
        let Some(path) = self.path_of(id) else {
            return;
        };
        let Some((&index, parent)) = path.split_last() else {
            return;
        };
        if let Some(siblings) = self.siblings_mut(parent) {
            siblings.splice(index + 1..index + 1, blocks);
        }
    }

    /// Joins the text of `id` onto the end of the block above it, as Backspace at the start
    /// of a block does. Returns the block and offset where the caret belongs afterwards.
    pub fn merge_into_previous(&mut self, id: BlockId) -> Option<(BlockId, usize)> {
        let previous_id = self.previous(id)?;
        let previous = self.block(previous_id)?;
        let block = self.block(id)?;
        if !previous.kind.has_text() {
            // An empty block gives way itself, and the caret moves over the divider to the
            // text above it. A block with text stays, and so does one with no text above
            // the divider to move to: there it is the divider that goes.
            if block.text.is_empty()
                && block.children.is_empty()
                && let Some(above) = self.previous_text_block(previous_id)
            {
                let offset = self.block(above)?.text.len();
                self.remove(id);
                return Some((above, offset));
            }
            self.remove(previous_id);
            return Some((id, 0));
        }
        if previous.kind.is_verbatim() || block.kind.is_verbatim() {
            if !block.text.is_empty() || !block.children.is_empty() {
                return None;
            }
            let offset = previous.text.len();
            self.remove(id);
            return Some((previous_id, offset));
        }
        let offset = previous.text.len();
        let removed = self.take_promoting_children(id)?;
        self.block_mut(previous_id)?.text.append(removed.text);
        Some((previous_id, offset))
    }

    /// The nearest block displayed above `id` that has text.
    fn previous_text_block(&self, id: BlockId) -> Option<BlockId> {
        let mut current = id;
        loop {
            current = self.previous(current)?;
            if self.block(current)?.kind.has_text() {
                return Some(current);
            }
        }
    }

    /// Joins the text of the block below `id` onto its end, as Delete at the end of a block
    /// does.
    pub fn merge_next(&mut self, id: BlockId) -> bool {
        let Some(next_id) = self.next(id) else {
            return false;
        };
        let (Some(block), Some(next)) = (self.block(id), self.block(next_id)) else {
            return false;
        };
        if !next.kind.has_text() {
            return self.remove(next_id).is_some();
        }
        if block.kind.is_verbatim() || next.kind.is_verbatim() || !block.kind.has_text() {
            return false;
        }
        let Some(removed) = self.take_promoting_children(next_id) else {
            return false;
        };
        match self.block_mut(id) {
            Some(block) => {
                block.text.append(removed.text);
                true
            }
            None => false,
        }
    }

    /// Removes a block, leaving its children in its place.
    fn take_promoting_children(&mut self, id: BlockId) -> Option<Block> {
        let path = self.path_of(id)?;
        let (&index, parent) = path.split_last()?;
        let siblings = self.siblings_mut(parent)?;
        let mut block = siblings.remove(index);
        let children = std::mem::take(&mut block.children);
        siblings.splice(index..index, children);
        self.ensure_not_empty();
        Some(block)
    }
}

fn last_descendant(block: &Block) -> &Block {
    let mut block = block;
    while let Some(last) = block.children.last() {
        block = last;
    }
    block
}

#[cfg(test)]
impl Document {
    /// One line per block, indented by depth, for asserting on document structure.
    pub fn outline(&self) -> String {
        let mut output = String::new();
        for row in self.rows() {
            output.push_str(&"  ".repeat(row.depth));
            output.push_str(&row.block.outline_line(&row.block.text));
            output.push('\n');
        }
        output
    }

    /// The first block whose text is exactly `text`.
    pub fn find(&self, text: &str) -> BlockId {
        fn visit(blocks: &[Block], text: &str) -> Option<BlockId> {
            blocks.iter().find_map(|block| {
                if block.text.text() == text {
                    Some(block.id)
                } else {
                    visit(&block.children, text)
                }
            })
        }
        visit(&self.blocks, text).unwrap_or_else(|| panic!("no block with text {text:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown;
    use pretty_assertions::assert_eq;

    fn document(source: &str) -> Document {
        markdown::parse(source)
    }

    const NESTED: &str = "\
- a
  - a1
  - a2
    - a2x
  - a3
- b
- c
";

    #[test]
    fn document_order_navigation() {
        let document = document(NESTED);
        let texts = |ids: Vec<Option<BlockId>>| -> Vec<String> {
            ids.into_iter()
                .map(|id| match id.and_then(|id| document.block(id)) {
                    Some(block) => block.text.text().to_string(),
                    None => "none".to_string(),
                })
                .collect()
        };
        let a = document.find("a");
        let a2 = document.find("a2");
        let a2x = document.find("a2x");
        let a3 = document.find("a3");
        let c = document.find("c");

        assert_eq!(
            texts(vec![
                document.next(a),
                document.next(a2),
                document.next(a2x),
                document.next(a3),
                document.next(c),
            ]),
            ["a1", "a2x", "a3", "b", "none"]
        );
        assert_eq!(
            texts(vec![
                document.previous(a),
                document.previous(a2),
                document.previous(a2x),
                document.previous(a3),
                document.previous(document.find("b")),
            ]),
            ["none", "a1", "a2", "a2x", "a3"]
        );
        assert_eq!(
            texts(vec![
                document.parent(a2x),
                document.first_child(a),
                document.first_child(c),
                Some(document.first()),
                Some(document.last()),
            ]),
            ["a2", "a1", "none", "a", "c"]
        );
    }

    #[test]
    fn last_block_is_the_deepest_last_descendant() {
        let document = document("- a\n  - b\n    - c\n");
        assert_eq!(document.last(), document.find("c"));
    }

    #[test]
    fn indent_nests_under_the_sibling_above() {
        let mut document = document("- a\n- b\n- c\n- d\n");
        let (b, c) = (document.find("b"), document.find("c"));
        assert!(document.indent(c, b));
        assert_eq!(document.outline(), "- a\n  - b\n  - c\n- d\n");

        assert!(!document.indent(document.find("a"), document.find("a")));
        assert!(!document.indent(b, b));
    }

    #[test]
    fn indent_requires_a_parent_that_supports_nesting() {
        let mut document = document("para\n\n- item\n");
        let item = document.find("item");
        assert!(!document.indent(item, item));
        assert_eq!(document.outline(), "p para\n- item\n");
    }

    #[test]
    fn outdent_keeps_vertical_order_by_adopting_following_siblings() {
        let mut document = document(NESTED);
        let a2 = document.find("a2");
        assert!(document.outdent(a2, a2));
        assert_eq!(
            document.outline(),
            "- a\n  - a1\n- a2\n  - a2x\n  - a3\n- b\n- c\n"
        );
        assert!(!document.outdent(a2, a2));
    }

    #[test]
    fn outdent_moves_following_siblings_out_when_they_cannot_be_adopted() {
        let mut document = document("- a\n\n  one\n\n  two\n\n  - three\n");
        let one = document.find("one");
        assert!(document.outdent(one, one));
        assert_eq!(document.outline(), "- a\np one\np two\n- three\n");
    }

    #[test]
    fn outdent_of_several_blocks() {
        let mut document = document(NESTED);
        assert!(document.outdent(document.find("a1"), document.find("a2")));
        assert_eq!(
            document.outline(),
            "- a\n- a1\n- a2\n  - a2x\n  - a3\n- b\n- c\n"
        );
    }

    #[test]
    fn move_up_and_down_swap_with_siblings() {
        let mut document = document(NESTED);
        let (a, b, c) = (document.find("a"), document.find("b"), document.find("c"));
        assert!(document.move_down(a, a));
        assert_eq!(
            document.outline(),
            "- b\n- a\n  - a1\n  - a2\n    - a2x\n  - a3\n- c\n"
        );
        assert!(document.move_up(a, c));
        assert_eq!(
            document.outline(),
            "- a\n  - a1\n  - a2\n    - a2x\n  - a3\n- c\n- b\n"
        );
        assert!(!document.move_up(a, c));
        assert!(!document.move_down(b, b));
    }

    #[test]
    fn delete_removes_subtrees_and_picks_a_neighbor() {
        let mut document = document(NESTED);
        let a2 = document.find("a2");
        assert_eq!(document.delete(a2, a2), Some(document.find("a3")));
        assert_eq!(document.outline(), "- a\n  - a1\n  - a3\n- b\n- c\n");

        let a3 = document.find("a3");
        assert_eq!(document.delete(a3, a3), Some(document.find("a1")));
        let a1 = document.find("a1");
        assert_eq!(document.delete(a1, a1), Some(document.find("a")));

        let (a, c) = (document.find("a"), document.find("c"));
        let remaining = document.delete(a, c);
        assert_eq!(document.outline(), "p \n");
        assert_eq!(remaining, Some(document.first()));
    }

    #[test]
    fn selection_covers_subtrees_of_siblings() {
        let document = document(NESTED);
        let ids = document.subtree_ids(document.find("b"), document.find("a"));
        let mut texts: Vec<_> = ids
            .iter()
            .filter_map(|id| document.block(*id))
            .map(|block| block.text.text())
            .collect();
        texts.sort();
        assert_eq!(texts, ["a", "a1", "a2", "a2x", "a3", "b"]);
        assert!(
            document
                .subtree_ids(document.find("a1"), document.find("b"))
                .is_empty()
        );
    }

    fn at(document: &Document, text: &str, offset: usize) -> TextPosition {
        TextPosition {
            block: document.find(text),
            offset,
        }
    }

    #[test]
    fn blocks_are_ordered_as_displayed() {
        let document = document(NESTED);
        let (a, a2x, b) = (document.find("a"), document.find("a2x"), document.find("b"));
        assert_eq!(document.order(a, a2x), Some(Ordering::Less));
        assert_eq!(document.order(b, a2x), Some(Ordering::Greater));
        assert_eq!(document.order(b, b), Some(Ordering::Equal));
        assert_eq!(document.order(b, 999), None);

        let (early, late) = (at(&document, "a2", 1), at(&document, "b", 0));
        assert_eq!(document.in_order(early, late), Some((early, late)));
        assert_eq!(document.in_order(late, early), Some((early, late)));
        let (before, after) = (at(&document, "b", 0), at(&document, "b", 1));
        assert_eq!(document.in_order(after, before), Some((before, after)));
    }

    #[test]
    fn covering_siblings_lift_both_blocks_to_one_level() {
        let document = document(NESTED);
        let find = |text| document.find(text);
        for ((first, second), expected) in [
            (("a2x", "b"), ("a", "b")),
            (("a1", "a3"), ("a1", "a3")),
            (("a2x", "a1"), ("a2", "a1")),
            // A block holding the other one is the run.
            (("a", "a2x"), ("a", "a")),
            (("a2x", "a"), ("a", "a")),
            (("b", "b"), ("b", "b")),
        ] {
            assert_eq!(
                document.covering_siblings(find(first), find(second)),
                Some((find(expected.0), find(expected.1))),
                "{first} and {second}"
            );
        }
    }

    #[test]
    fn span_ranges_cover_the_text_between_two_positions() {
        let document = document("one\n\n---\n\n- two\n  - three\n\nfour\n");
        let ranges = document.span_ranges(at(&document, "one", 1), at(&document, "three", 2));
        let described: Vec<(String, Range<usize>)> = ranges
            .into_iter()
            .map(|(block, range)| (block.text.text().to_string(), range))
            .collect();
        assert_eq!(
            described,
            [
                ("one".to_string(), 1..3),
                (String::new(), 0..0),
                ("two".to_string(), 0..3),
                ("three".to_string(), 0..2),
            ]
        );
        // Backwards, or between blocks that do not exist, there is nothing.
        assert!(
            document
                .span_ranges(at(&document, "four", 0), at(&document, "one", 0))
                .is_empty()
        );
    }

    #[test]
    fn delete_span_joins_the_ends_and_removes_what_is_between() {
        let mut document = document(NESTED);
        let a2 = document.find("a2");
        let caret = document.delete_span(at(&document, "a2", 1), at(&document, "b", 0));
        // `a2x` and `a3` were between the ends, and the text of `b` joined `a2`.
        assert_eq!(document.outline(), "- a\n  - a1\n  - ab\n- c\n");
        assert_eq!(
            caret,
            Some(TextPosition {
                block: a2,
                offset: 1
            })
        );
    }

    #[test]
    fn delete_span_keeps_the_type_of_the_first_block() {
        let mut document = document("# Title\n\n- item\n");
        let caret = document.delete_span(at(&document, "Title", 2), at(&document, "item", 2));
        assert_eq!(document.outline(), "h1 Tiem\n");
        assert_eq!(caret.map(|caret| caret.offset), Some(2));
    }

    #[test]
    fn delete_span_leaves_nested_blocks_outside_the_span_in_place() {
        // `a2x` is inside the span, and `a3` is not.
        let mut document = document(NESTED);
        document.delete_span(at(&document, "a1", 1), at(&document, "a2x", 1));
        assert_eq!(document.outline(), "- a\n  - a2x\n  - a3\n- b\n- c\n");

        // `a2` has children outside the span, which stay where its text was.
        let mut document = self::document(NESTED);
        document.delete_span(at(&document, "a", 1), at(&document, "a2", 1));
        assert_eq!(document.outline(), "- a2\n  - a2x\n  - a3\n- b\n- c\n");

        // `x` is between the ends and holds the end block, so what was nested in it moves up.
        let mut document = self::document("para\n\n- x\n  - y\n  - z\n");
        document.delete_span(at(&document, "para", 2), at(&document, "y", 1));
        assert_eq!(document.outline(), "p pa\n- z\n");
    }

    #[test]
    fn delete_span_removes_dividers_between_the_ends() {
        let mut document = document("ab\n\n---\n\ncd\n");
        document.delete_span(at(&document, "ab", 1), at(&document, "cd", 1));
        assert_eq!(document.outline(), "p ad\n");
    }

    #[test]
    fn delete_span_does_not_mix_code_and_text() {
        let mut document = document("```\ncode\n```\n\ntext\n");
        let caret = document.delete_span(at(&document, "code", 2), at(&document, "text", 2));
        assert_eq!(document.outline(), "code() co\np xt\n");
        assert_eq!(caret.map(|caret| caret.offset), Some(2));

        let mut document = self::document("```\none\n```\n\n```\ntwo\n```\n");
        document.delete_span(at(&document, "one", 1), at(&document, "two", 1));
        assert_eq!(document.outline(), "code() owo\n");
    }

    #[test]
    fn delete_span_ignores_positions_that_are_not_in_order() {
        let mut document = document("one\n\ntwo\n");
        let (one, two) = (at(&document, "one", 0), at(&document, "two", 0));
        assert_eq!(document.delete_span(two, one), None);
        assert_eq!(document.delete_span(one, one), None);
        assert_eq!(document.outline(), "p one\np two\n");
    }

    #[test]
    fn copy_span_cuts_the_ends_and_keeps_the_nesting() {
        let document = document(NESTED);
        let copy = |start, end| Document::from_blocks(document.copy_span(start, end)).outline();

        // `a` is not in the span, so what is nested in it comes out at the top level.
        assert_eq!(
            copy(at(&document, "a1", 1), at(&document, "b", 1)),
            "- 1\n- a2\n  - a2x\n- a3\n- b\n"
        );
        // A block in the span keeps the children that are in it.
        assert_eq!(
            copy(at(&document, "a", 0), at(&document, "a2", 1)),
            "- a\n  - a1\n  - a\n"
        );
        // The divider has no text but is part of the copy.
        let document = self::document("one\n\n---\n\n# two\n");
        assert_eq!(
            Document::from_blocks(
                document.copy_span(at(&document, "one", 1), at(&document, "two", 2))
            )
            .outline(),
            "p ne\n---\nh1 tw\n"
        );
    }

    #[test]
    fn split_continues_lists_and_ends_headings() {
        let mut document = document("# Title\n\n- item\n\n- [x] done\n");
        let title = document.find("Title");
        let item = document.find("item");
        let done = document.find("done");
        document.split(title, 3);
        document.split(item, 4);
        document.split(done, 2);
        assert_eq!(
            document.outline(),
            "h1 Tit\np le\n- item\n- \n[x] do\n[ ] ne\n"
        );
    }

    #[test]
    fn split_at_start_inserts_an_empty_block_above() {
        let mut document = document("# Title\n\n- item\n  - child\n");
        let title = document.find("Title");
        let item = document.find("item");
        assert_eq!(document.split(title, 0), Some(title));
        assert_eq!(document.split(item, 0), Some(item));
        assert_eq!(document.outline(), "p \nh1 Title\n- \n- item\n  - child\n");
    }

    #[test]
    fn split_of_a_block_with_children_stays_above_them() {
        let mut document = document("- parent\n  - child\n\n> quote\n>\n> more\n");
        let parent = document.find("parent");
        let quote = document.find("quote");
        let new_item = document.split(parent, 3);
        let new_paragraph = document.split(quote, 5);
        assert_eq!(
            document.outline(),
            "- par\n  - ent\n  - child\n> quote\n  p \n  p more\n"
        );
        assert_eq!(new_item, Some(document.find("ent")));
        assert_eq!(
            new_paragraph.and_then(|id| document.parent(id)),
            Some(quote)
        );
    }

    #[test]
    fn set_kind_moves_out_children_the_new_kind_cannot_hold() {
        let mut document = document("- a\n  - a1\n  - a2\n- b\n");
        let a = document.find("a");
        document.set_kind(a, BlockKind::Paragraph);
        assert_eq!(document.outline(), "p a\n- a1\n- a2\n- b\n");

        let b = document.find("b");
        document.set_kind(b, BlockKind::Todo { checked: false });
        assert_eq!(document.outline(), "p a\n- a1\n- a2\n[ ] b\n");
    }

    #[test]
    fn merge_into_previous_joins_text() {
        let mut document = document("one **two**\n\nthree\n");
        let three = document.find("three");
        let one = document.first();
        assert_eq!(document.merge_into_previous(three), Some((one, 7)));
        assert_eq!(document.outline(), "p one <b>two</b>three\n");
        assert_eq!(document.merge_into_previous(one), None);
    }

    #[test]
    fn merge_into_previous_crosses_nesting_levels() {
        let mut document = document("- a\n  - a1\n\nlast\n");
        let last = document.find("last");
        let a1 = document.find("a1");
        assert_eq!(document.merge_into_previous(last), Some((a1, 2)));
        assert_eq!(document.outline(), "- a\n  - a1last\n");

        let a = document.find("a");
        assert_eq!(document.merge_into_previous(a1), Some((a, 1)));
        assert_eq!(document.outline(), "- aa1last\n");
    }

    #[test]
    fn merge_into_previous_removes_a_divider_first() {
        let mut document = document("above\n\n---\n\nbelow\n");
        let below = document.find("below");
        assert_eq!(document.merge_into_previous(below), Some((below, 0)));
        assert_eq!(document.outline(), "p above\np below\n");
    }

    #[test]
    fn merge_into_previous_removes_an_empty_block_below_a_divider() {
        let mut document = document("above\n\n---\n\n---\n\nbelow\n");
        let above = document.first();
        let below = document.find("below");
        document.block_mut(below).unwrap().text = RichText::new();
        assert_eq!(document.merge_into_previous(below), Some((above, 5)));
        assert_eq!(document.outline(), "p above\n---\n---\n");
    }

    #[test]
    fn merge_into_previous_removes_a_divider_with_no_text_above_it() {
        let mut document = document("---\n\nbelow\n");
        let below = document.find("below");
        document.block_mut(below).unwrap().text = RichText::new();
        assert_eq!(document.merge_into_previous(below), Some((below, 0)));
        assert_eq!(document.outline(), "p \n");
    }

    #[test]
    fn merge_into_previous_finds_the_text_above_a_nested_divider() {
        let mut document = document("- item\n\n  ---\n\n  below\n");
        let item = document.find("item");
        let below = document.find("below");
        assert_eq!(document.outline(), "- item\n  ---\n  p below\n");
        document.block_mut(below).unwrap().text = RichText::new();
        assert_eq!(document.merge_into_previous(below), Some((item, 4)));
        assert_eq!(document.outline(), "- item\n  ---\n");
    }

    #[test]
    fn merge_does_not_mix_text_into_code() {
        let mut document = document("```\ncode\n```\n\ntext\n\n```\nmore\n```\n");
        let text = document.find("text");
        assert_eq!(document.merge_into_previous(text), None);
        assert!(!document.merge_next(text));

        document.block_mut(text).unwrap().text = RichText::new();
        let code = document.find("code");
        assert_eq!(document.merge_into_previous(text), Some((code, 4)));
        assert_eq!(document.outline(), "code() code\ncode() more\n");
    }

    #[test]
    fn merge_next_keeps_the_children_of_the_merged_block() {
        let mut document = document("first\n\n- second\n  - child\n");
        let first = document.find("first");
        assert!(document.merge_next(first));
        assert_eq!(document.outline(), "p firstsecond\n- child\n");
    }

    #[test]
    fn rows_number_consecutive_items_and_track_quotes() {
        let document = document("1. a\n2. b\n\npara\n\n1. c\n\n> q\n>\n> - in quote\n");
        let rows = document.rows();
        let ordinals: Vec<usize> = rows.iter().map(|row| row.ordinal).collect();
        assert_eq!(ordinals, [1, 2, 0, 1, 0, 0]);
        let last = rows.last().unwrap();
        assert_eq!(last.depth, 1);
        assert_eq!(last.quote_ancestors, [true]);
    }

    #[test]
    fn adopted_blocks_get_unique_ids() {
        let mut document = document("- a\n  - b\n");
        let copies = document.blocks_between(document.first(), document.first());
        let inserted = document.insert_after(document.first(), copies);
        assert_eq!(document.outline(), "- a\n  - b\n- a\n  - b\n");
        let mut ids: Vec<BlockId> = document.rows().iter().map(|row| row.block.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4);
        assert_eq!(inserted.len(), 1);
    }
}
