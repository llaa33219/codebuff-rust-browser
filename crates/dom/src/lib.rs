//! DOM crate — Document Object Model
//!
//! Arena-based DOM tree with event dispatch.
//! Uses generational indices from the `arena` crate instead of Rc/RefCell.

pub mod node;
pub mod tree;
pub mod event;

pub use node::*;
pub use tree::Dom;
pub use event::*;
