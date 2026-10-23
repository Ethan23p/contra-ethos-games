//! The shared platform layer. A game implements [`App`] and hands it to [`run`];
//! the engine owns the window or canvas, the GPU and the frame loop.
//!
//! Everything a player sees is drawn from our own primitives (see CLAUDE.md), so
//! this crate grows by adding primitives to [`Shapes`], not by wrapping UI kits.

mod gpu;
mod runner;
mod shapes;

#[cfg(not(target_arch = "wasm32"))]
pub mod headless;

pub use glam;
pub use runner::{App, Frame, run};
pub use shapes::{Circle, Shapes, rgb, rgba};
