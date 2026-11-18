//! The current branch of the parsed nodes, grouped into displayed items.
use super::parse::{prefix, ImageRef, Node, NodeBody, MAX_TEXT};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// One tool call of a [`ConvItem::Steps`] group.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub tool: String,
    pub target: String,
    /// Its tool result is on the chain.
    pub done: bool,
    pub is_error: bool,
    pub images: Vec<ImageRef>,
}

/// One displayed entry of the current conversation branch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ConvItem {
    User { text: String, truncated: bool, images: Vec<ImageRef>, at_ms: i64 },
    Assistant { text: String, truncated: bool, at_ms: i64 },
    Steps { steps: Vec<Step>, at_ms: i64 },
    Compacted { at_ms: i64 },
    /// A one-line note from the system (a background task finished); an
    /// empty text means it carried no summary.
    Notice { text: String, at_ms: i64 },
}

/// Items `start..start + items.len()` of the `total` items of the branch
/// identified by `epoch`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvPage {
    pub epoch: u32,
    pub total: u32,
    pub start: u32,
    pub items: Vec<ConvItem>,
}

pub(super) enum Link<'a> {
    Node(&'a Node),
    /// The walk crossed to a logical parent at this node, which is not a
    /// compact boundary; a divider goes right before it.
    Jump(&'a Node),
}

/// The chain from `leaf` back to its root, in file order. A missing parent
/// continues at the logical parent (compaction); a cycle ends the walk.
pub(super) fn current_chain<'a>(nodes: &'a HashMap<String, Node>, leaf: Option<&str>) -> Vec<Link<'a>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = leaf.and_then(|u| nodes.get(u));
    while let Some(n) = cur {
        if !seen.insert(n.uuid.as_str()) {
            break;
        }
        out.push(Link::Node(n));
        cur = match n.parent.as_deref().and_then(|p| nodes.get(p)) {
            Some(p) => Some(p),
            None => {
                let lp = n
                    .logical_parent
                    .as_deref()
                    .and_then(|p| nodes.get(p))
                    .filter(|p| !seen.contains(p.uuid.as_str()));
                if lp.is_some() && n.body != NodeBody::CompactBoundary {
                    out.push(Link::Jump(n));
                }
                lp
            }
        };
    }
    out.reverse();
    out
}

/// The chain plus its parallel tool calls, in file order. Claude Code chains
/// the tool_use lines of one API message and hangs each result off its own
/// tool_use, so the walk from the leaf misses all but one of them. Added:
/// assistant lines sharing a `message.id` with a chain line, and tool results
/// answering a tool_use of any displayed line. Abandoned branches carry other
/// message ids and stay out.
pub(super) fn shown_nodes<'a>(nodes: &'a HashMap<String, Node>, chain: Vec<Link<'a>>) -> Vec<Link<'a>> {
    let on_chain = |l: &Link<'a>| match l {
        Link::Node(n) => Some(*n),
        Link::Jump(_) => None,
    };
    let uuids: HashSet<&str> = chain.iter().filter_map(on_chain).map(|n| n.uuid.as_str()).collect();
    let msgs: HashSet<&str> = chain.iter().filter_map(on_chain).filter_map(|n| n.message_id.as_deref()).collect();
    let mut extra: Vec<&Node> = nodes
        .values()
        .filter(|n| !uuids.contains(n.uuid.as_str()))
        .filter(|n| n.message_id.as_deref().is_some_and(|m| msgs.contains(m)))
        .collect();
    let uses: HashSet<&str> = chain
        .iter()
        .filter_map(on_chain)
        .chain(extra.iter().copied())
        .flat_map(|n| match &n.body {
            NodeBody::ToolUses(u) => u.iter().map(|(id, _, _)| id.as_str()).collect(),
            _ => Vec::new(),
        })
        .collect();
    extra.extend(nodes.values().filter(|n| {
        !uuids.contains(n.uuid.as_str())
            && matches!(&n.body, NodeBody::ToolResults(r) if r.iter().any(|(id, _, _)| uses.contains(id.as_str())))
    }));
    let mut out = chain;
    out.extend(extra.into_iter().map(Link::Node));
    // A divider sorts right before the node it belongs to.
    out.sort_by_key(|l| match l {
        Link::Jump(n) => (n.offset, 0),
        Link::Node(n) => (n.offset, 1),
    });
    out
}

/// Groups displayed nodes into items (see [`ConvItem`]).
pub(super) fn group(chain: &[Link]) -> Vec<ConvItem> {
    let mut items = Vec::new();
    // tool_use id -> (item index, step index)
    let mut steps_at: HashMap<&str, (usize, usize)> = HashMap::new();
    for link in chain {
        let n = match link {
            Link::Jump(n) => {
                items.push(ConvItem::Compacted { at_ms: n.at_ms });
                continue;
            }
            Link::Node(n) => n,
        };
        match &n.body {
            NodeBody::UserText { text, truncated, images } => items.push(ConvItem::User {
                text: text.clone(),
                truncated: *truncated,
                images: images.clone(),
                at_ms: n.at_ms,
            }),
            NodeBody::AssistantText { text, truncated } => match items.last_mut() {
                Some(ConvItem::Assistant { text: t, truncated: tr, .. }) => {
                    let room = MAX_TEXT.saturating_sub(t.len());
                    if room >= 2 + text.len() {
                        t.push_str("\n\n");
                        t.push_str(text);
                    } else {
                        if room > 2 {
                            t.push_str("\n\n");
                            t.push_str(prefix(text, room - 2));
                            // No bare blank line where the cut fell.
                            t.truncate(t.trim_end().len());
                        }
                        *tr = true;
                    }
                    *tr |= *truncated;
                }
                _ => items.push(ConvItem::Assistant { text: text.clone(), truncated: *truncated, at_ms: n.at_ms }),
            },
            NodeBody::ToolUses(uses) => {
                if !matches!(items.last(), Some(ConvItem::Steps { .. })) {
                    items.push(ConvItem::Steps { steps: Vec::new(), at_ms: n.at_ms });
                }
                let i = items.len() - 1;
                let Some(ConvItem::Steps { steps, .. }) = items.last_mut() else { unreachable!() };
                for (id, tool, target) in uses {
                    if !id.is_empty() {
                        steps_at.insert(id, (i, steps.len()));
                    }
                    steps.push(Step {
                        tool: tool.clone(),
                        target: target.clone(),
                        done: false,
                        is_error: false,
                        images: Vec::new(),
                    });
                }
            }
            NodeBody::ToolResults(results) => {
                for (id, is_error, images) in results {
                    let Some(&(i, j)) = steps_at.get(id.as_str()) else { continue };
                    if let ConvItem::Steps { steps, .. } = &mut items[i] {
                        // A result written again for the same call replaces the first.
                        let s = &mut steps[j];
                        s.done = true;
                        s.is_error = *is_error;
                        s.images = images.clone();
                    }
                }
            }
            NodeBody::CompactBoundary => items.push(ConvItem::Compacted { at_ms: n.at_ms }),
            NodeBody::Notice { text } => items.push(ConvItem::Notice { text: text.clone(), at_ms: n.at_ms }),
            NodeBody::Other => {}
        }
    }
    items
}
