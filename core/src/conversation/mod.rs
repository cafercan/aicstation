//! Conversation nodes parsed from transcript lines, and on-demand image reading.
//!
//! - `parse`: one transcript line into a [`Node`].
//! - `items`: the current branch of the nodes grouped into [`ConvItem`]s.
//! - `store`: a [`Conversation`] kept up to date as the transcript grows.
//! - `image`: one image of a line, read again from the file on demand.
mod image;
mod items;
mod parse;
mod store;
#[cfg(test)]
mod tests;

pub use image::{read_image, ImageError};
pub use items::{ConvItem, ConvPage, Step};
pub use parse::{parse_node, ImageRef, Node, NodeBody};
pub use store::{Conversation, PAGE};
