//! Woodworking joints for SolidCraft, as a sandboxed command plugin.
//!
//! Each entry measures the bodies it is given, plans the cut from their bounding boxes and adds
//! ordinary box and cylinder features. One call is one undo step, and the result needs no plugin to reopen.

pub mod boards;
pub mod dowel;
pub mod finger;
pub mod groove;
pub mod housing;
pub mod joint;
pub mod lap;
pub mod loose;
pub mod miter;

#[cfg(target_arch = "wasm32")]
mod guest;
