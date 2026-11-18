//! A [`Conversation`]: the current branch of one transcript, kept up to
//! date as lines are appended.
use super::items::{current_chain, group, shown_nodes, ConvItem, ConvPage, Link};
use super::parse::{parse_node, Node, NodeBody};
use crate::transcript::TranscriptReader;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Items per page.
pub const PAGE: u32 = 100;

/// Source of every epoch in the process: a conversation dropped from a cache
/// and opened again never repeats an epoch the UI may still hold.
static EPOCHS: AtomicU32 = AtomicU32::new(0);

fn next_epoch() -> u32 {
    EPOCHS.fetch_add(1, Ordering::Relaxed)
}

/// The current branch of one transcript, kept up to date incrementally.
pub struct Conversation {
    path: PathBuf,
    reader: TranscriptReader,
    nodes: HashMap<String, Node>,
    leaf: Option<String>,
    /// Uuids of the displayed nodes, in file order.
    shown: Vec<String>,
    items: Vec<ConvItem>,
    pub(super) images: HashSet<(u64, u32)>,
    epoch: u32,
    /// How often the items were rebuilt (tests).
    #[cfg(test)]
    pub(super) rebuilds: usize,
}

impl Conversation {
    pub fn new(path: PathBuf) -> Self {
        Self {
            reader: TranscriptReader::new(path.clone()),
            path,
            nodes: HashMap::new(),
            leaf: None,
            shown: Vec::new(),
            items: Vec::new(),
            images: HashSet::new(),
            epoch: next_epoch(),
            #[cfg(test)]
            rebuilds: 0,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the lines appended since the last call. A shrunk file clears
    /// everything and starts a new epoch. A new epoch also starts when the
    /// displayed nodes no longer extend the previous ones (rewind, edit,
    /// retry), or when an item before the previous last one changed (a late
    /// tool result), since `since` only resends from the last item. Lines
    /// delivered before an io error are kept.
    pub fn update(&mut self) -> io::Result<()> {
        let mut reset = false;
        let mut fresh = Vec::new();
        let res = self.reader.read_lines_at(
            || reset = true,
            |at, l| fresh.extend(parse_node(at, l)),
        );
        if reset {
            self.nodes.clear();
            self.leaf = None;
            self.shown.clear();
            self.items.clear();
            self.epoch = next_epoch();
        }
        // Hidden lines alone change nothing shown: they never move the leaf.
        let changed = reset || fresh.iter().any(|n| n.body != NodeBody::Other);
        for n in fresh {
            if n.body != NodeBody::Other {
                self.leaf = Some(n.uuid.clone());
            }
            self.nodes.insert(n.uuid.clone(), n);
        }
        if changed {
            self.rebuild();
        }
        res
    }

    fn rebuild(&mut self) {
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        let shown = shown_nodes(&self.nodes, current_chain(&self.nodes, self.leaf.as_deref()));
        // Hidden nodes take no part in the prefix check below: one joining
        // the middle (a side line of a shown API message) shows nothing.
        let uuids: Vec<String> = shown
            .iter()
            .filter_map(|l| match l {
                Link::Node(n) if n.body != NodeBody::Other => Some(n.uuid.clone()),
                _ => None,
            })
            .collect();
        let old = std::mem::replace(&mut self.items, group(&shown));
        let keep = old.len().saturating_sub(1);
        if !uuids.starts_with(&self.shown) || self.items.get(..keep) != old.get(..keep) {
            self.epoch = next_epoch();
        }
        self.images.clear();
        for item in &self.items {
            match item {
                ConvItem::User { images, .. } => {
                    for r in images {
                        self.images.insert((r.offset, r.ordinal));
                    }
                }
                ConvItem::Steps { steps, .. } => {
                    for r in steps.iter().flat_map(|s| &s.images) {
                        self.images.insert((r.offset, r.ordinal));
                    }
                }
                _ => {}
            }
        }
        self.shown = uuids;
    }

    fn total(&self) -> u32 {
        self.items.len() as u32
    }

    fn slice(&self, start: u32, end: u32) -> ConvPage {
        ConvPage {
            epoch: self.epoch,
            total: self.total(),
            start,
            items: self.items[start as usize..end as usize].to_vec(),
        }
    }

    /// The (at most) `PAGE` items ending before index `before` (default: the end).
    pub fn page(&self, before: Option<u32>) -> ConvPage {
        let end = before.unwrap_or(self.total()).min(self.total());
        self.slice(end.saturating_sub(PAGE), end)
    }

    /// Items from index `from` on when `epoch` is still current; otherwise
    /// the newest page (the caller then replaces what it has).
    pub fn since(&self, epoch: u32, from: u32) -> ConvPage {
        if epoch != self.epoch {
            return self.page(None);
        }
        self.slice(from.min(self.total()), self.total())
    }

    /// True when the image is shown somewhere on the current branch.
    pub fn has_image(&self, offset: u64, ordinal: u32) -> bool {
        self.images.contains(&(offset, ordinal))
    }
}
