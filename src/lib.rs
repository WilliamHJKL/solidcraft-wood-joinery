//! Woodworking joints for SolidCraft, as a sandboxed command plugin.
//!
//! The plugin measures the two bodies, plans the joint from their bounding boxes and adds
//! ordinary box features: one joined to the rail, one cut from the stile. One call is one undo
//! step, and the result needs no plugin to reopen.

pub mod joint;

#[cfg(target_arch = "wasm32")]
mod guest;
