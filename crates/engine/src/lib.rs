//! The shared platform layer. A game implements [`App`] and hands it to [`run`];
//! the engine owns the window or canvas, the GPU and the frame loop.
//!
//! Everything a player sees is drawn from our own primitives (see CLAUDE.md), so
//! this crate grows by adding primitives to [`Shapes`], not by wrapping UI kits.

mod gpu;
mod input;
mod panel;
mod runner;
mod shapes;
mod text;
pub mod tune;

#[cfg(not(target_arch = "wasm32"))]
pub mod headless;

pub use input::{GestureConfig, Input, Phase, Pointer};
pub use runner::{App, Frame, run};
pub use shapes::{Shapes, Styled, View, rgb, rgba};
pub use tune::{Schema, Tunable, Tuner};
pub use {egui, glam};
