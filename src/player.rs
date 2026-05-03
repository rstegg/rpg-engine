use macroquad::prelude::*;
use crate::animation::AnimationManager;

pub struct Stats {
    pub strength: i32,
    pub agility: i32,
    pub intelligence: i32,
    pub max_hp: i32,
    pub current_hp: i32,
    pub max_mp: i32,
    pub current_mp: i32,
}

impl Stats {
    pub fn new(strength: i32, agility: i32, intelligence: i32) -> Self {
        Self {
            strength,
            agility,
            intelligence,
            max_hp: strength * 10,
            current_hp: strength * 10,
            max_mp: intelligence * 10,
            current_mp: intelligence * 10,
        }
    }

    pub fn get_movement_speed(&self) -> f32 {
        3.0 + (self.agility as f32 * 0.15)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SpellId {
    Q,
    W,
    E,
    R,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TargetingState {
    None,
    Aoe(SpellId, f32), // spell, radius
    UnitTarget(SpellId),
}

pub struct Hero {
    pub pos: Vec3,
    pub target_pos: Vec3,
    pub texture: Texture2D,
    pub stats: Stats,
    pub anim: AnimationManager,
    pub targeting_state: TargetingState,
    pub casting_timer: f32, // Locks movement/animation while > 0
}
