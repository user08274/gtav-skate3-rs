//! Placeholder for the upstream ScoreModule integration, which is tied to the
//! APT HUD. Recognition is fed the same frame; scoring arrives with tricks.
use skate_core::{animation::output::attributes::AttributeName, physics::filtered_state::FilteredCategory};
use skate_data::collections::Collections;

pub type Basis = [[f32; 3]; 3];

pub struct Frame {
    pub tick: u32,
    pub dt: f32,
    pub category: FilteredCategory,
    pub state: u32,
    pub descriptor: Option<AttributeName>,
    pub grind_id: i32,
    pub flags: u32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub forward: [f32; 3],
    pub switch: bool,
    pub fakie: bool,
    pub regular: bool,
    pub player_basis: Basis,
    pub board_basis: Basis,
    pub reckoning_up: [f32; 3],
    pub body_flip: bool,
    pub front_flip: bool,
    pub suspend_air: bool,
    pub landing: skate_core::animation::landing_quality::Output,
    pub teleported: bool,
    pub reverting: bool,
}

#[derive(Default)]
pub struct Runtime;

impl Runtime {
    pub fn load(_data: &Collections) -> Result<Self, String> {
        Ok(Self)
    }
    pub fn advance(&mut self, _frame: Frame) -> Result<(), String> {
        Ok(())
    }
}
