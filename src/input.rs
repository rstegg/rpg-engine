use macroquad::prelude::*;
use crate::player::{Hero, TargetingState, SpellId};
use crate::camera::GameCamera;
use crate::effects::EffectManager;
use crate::animation::AnimationState;
use crate::Assets;

pub fn handle_input(
    hero: &mut Hero,
    camera: &GameCamera,
    effect_manager: &mut EffectManager,
    dummies: &[Vec3],
    assets: &Assets,
) {
    // Hotkeys
    if is_key_pressed(KeyCode::Q) { hero.targeting_state = TargetingState::Aoe(SpellId::Q, 3.0); }
    if is_key_pressed(KeyCode::W) { hero.targeting_state = TargetingState::UnitTarget(SpellId::W); }
    if is_key_pressed(KeyCode::E) { hero.targeting_state = TargetingState::UnitTarget(SpellId::E); }
    if is_key_pressed(KeyCode::R) { hero.targeting_state = TargetingState::UnitTarget(SpellId::R); }

    // Left click: Confirm Cast
    if is_mouse_button_pressed(MouseButton::Left) {
        match hero.targeting_state {
            TargetingState::Aoe(spell, _radius) => {
                if let Some(intersection) = camera.get_mouse_ray_intersection() {
                    hero.target_pos = hero.pos; // Stop moving
                    hero.anim.set_direction(intersection - hero.pos);
                    hero.casting_timer = 0.5; // lock animation

                    if spell == SpellId::Q {
                        hero.anim.set_state(AnimationState::Bow);
                        effect_manager.spawn_arrow_rain(intersection, assets.spell_q.clone());
                    }
                    hero.targeting_state = TargetingState::None;
                }
            }
            TargetingState::UnitTarget(spell) => {
                if let Some(intersection) = camera.get_mouse_ray_intersection() {
                    // Find closest dummy
                    let mut target_idx = None;
                    for (i, d_pos) in dummies.iter().enumerate() {
                        if (intersection - *d_pos).length() < 2.5 {
                            target_idx = Some(i);
                            break;
                        }
                    }

                    if let Some(idx) = target_idx {
                        let target_pos = dummies[idx];
                        hero.target_pos = hero.pos; // Stop moving
                        hero.anim.set_direction(target_pos - hero.pos);
                        hero.casting_timer = 0.5;

                        match spell {
                            SpellId::W => {
                                hero.anim.set_state(AnimationState::Sword);
                                effect_manager.spawn_single_hit(target_pos, assets.spell_w.clone(), SpellId::W);
                            }
                            SpellId::E => {
                                hero.anim.set_state(AnimationState::Staff);
                                effect_manager.spawn_single_hit(target_pos, assets.spell_e.clone(), SpellId::E);
                            }
                            SpellId::R => {
                                hero.anim.set_state(AnimationState::CarryIdle);
                                effect_manager.spawn_single_hit(target_pos, assets.spell_r.clone(), SpellId::R);
                            }
                            _ => {}
                        }
                        hero.targeting_state = TargetingState::None;
                    }
                }
            }
            TargetingState::None => {}
        }
    }

    // Right click: Move or Cancel Targeting
    if is_mouse_button_pressed(MouseButton::Right) {
        if hero.targeting_state != TargetingState::None {
            hero.targeting_state = TargetingState::None; // Cancel
        } else if hero.casting_timer <= 0.0 {
            if let Some(intersection) = camera.get_mouse_ray_intersection() {
                hero.target_pos = intersection;
            }
        }
    }
}
