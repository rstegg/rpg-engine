use crate::core::animation::AnimationManager;
use macroquad::prelude::*;

/// Returned when the player casts a spell, so networking can forward it.
pub struct SpellCastEvent {
    pub spell: SpellId,
    pub target_x: f32,
    pub target_z: f32,
}

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

    pub fn get_cast_speed(&self) -> f32 {
        // e.g. 10 agility = 1.5x cast speed
        1.0 + (self.agility as f32 * 0.05)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SpellId {
    Q,
    W,
    E,
    R,
}

impl SpellId {
    pub fn get_max_cooldown(&self) -> f32 {
        match self {
            SpellId::Q => 3.0,
            SpellId::W => 1.0,
            SpellId::E => 2.0,
            SpellId::R => 5.0,
        }
    }
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
    pub current_path: Vec<Vec3>,
    pub stats: Stats,
    pub anim: AnimationManager,
    pub targeting_state: TargetingState,
    pub casting_timer: f32, // Locks movement/animation while > 0
    pub stuck_timer: f32,   // Tracks how long we've been running into a wall
    pub cooldowns: std::collections::HashMap<SpellId, f32>,
    pub is_dead: bool,
    pub revive_progress: f32,
}
