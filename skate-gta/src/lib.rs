//! SkateGTA: the Skate 3 Rust Engine board physics running inside GTA V
//! through ScriptHookV. GTA-independent modules are plain Rust and tested on
//! any host; `gta` holds the ScriptHookV glue and only builds for Windows.
pub mod assets;
pub mod bigstack;
pub mod colliders;
pub mod config;
pub mod coords;
pub mod far;
pub mod hash;
pub mod hud_draw;
pub mod patch;
pub mod png;
pub mod pose;
pub mod ride;
pub mod settings;
pub mod sim;
pub mod terrain;

#[cfg(windows)]
mod gta;
