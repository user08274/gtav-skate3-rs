//! Gameplay modules of the Skate 3 Rust Engine (crates/skate-game), ported
//! without Bevy. GPL-3.0-only; see the upstream repository for history.
#![allow(dead_code, unused, private_interfaces, private_bounds, clippy::all)]
pub mod animation;
pub mod apt_display;
pub mod apt_movie;
pub mod apt_scene;
pub mod apt_text;
pub mod apt_vm;
pub mod animation_pose;
pub mod camera;
pub mod custom_difficulty;
pub mod difficulty;
pub mod graph_host;
pub mod graph_runtime;
pub mod host;
pub mod hud_runtime;
pub mod grind_world;
pub mod input;
pub mod mod_types;
pub mod physics;
pub mod scoring_hud;
pub mod scoring_runtime;
pub mod skater_animation;
