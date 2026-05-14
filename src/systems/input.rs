use crate::Assets;
use crate::core::animation::AnimationState;
use crate::core::camera::GameCamera;
use crate::entities::effects::EffectManager;
use crate::entities::player::{Hero, SpellCastEvent, SpellId, TargetingState};
use crate::systems::indicators::IndicatorManager;
use crate::world::chunk::ChunkedWorld;
use crate::world::pathfinding::{find_path_fn, line_of_sight_fn};
use macroquad::prelude::*;

pub fn handle_input(
    hero: &mut Hero,
    camera: &GameCamera,
    effect_manager: &mut EffectManager,
    targets: &[(u64, Vec3)],
    combat_text_mgr: &mut crate::ui::combat_text::CombatTextManager,
    assets: &Assets,
    world: &ChunkedWorld,
    indicators: &mut IndicatorManager,
) -> Option<SpellCastEvent> {
    let mut cast_event: Option<SpellCastEvent> = None;

    // Hotkeys
    if is_key_pressed(KeyCode::Q) {
        hero.targeting_state = TargetingState::Aoe(SpellId::Q, 3.0);
    }
    if is_key_pressed(KeyCode::W) {
        hero.targeting_state = TargetingState::UnitTarget(SpellId::W);
    }
    if is_key_pressed(KeyCode::E) {
        hero.targeting_state = TargetingState::UnitTarget(SpellId::E);
    }
    if is_key_pressed(KeyCode::R) {
        hero.targeting_state = TargetingState::UnitTarget(SpellId::R);
    }

    // Left click: Confirm Cast
    if is_mouse_button_pressed(MouseButton::Left) {
        match hero.targeting_state {
            TargetingState::Aoe(spell, radius) => {
                let cd = hero.cooldowns.get(&spell).copied().unwrap_or(0.0);
                if cd > 0.0 {
                    hero.targeting_state = TargetingState::None;
                    return None; // Cooldown not ready
                }

                if let Some(intersection) = camera.get_mouse_ray_intersection() {
                    hero.target_pos = hero.pos;
                    hero.current_path.clear();
                    hero.anim.set_direction(intersection - hero.pos);
                    hero.casting_timer = 0.5 / hero.stats.get_cast_speed();

                    if spell == SpellId::Q {
                        hero.cooldowns.insert(SpellId::Q, 3.0);
                        hero.anim.set_state(AnimationState::Bow);
                        effect_manager.spawn_arrow_rain(intersection, assets.spell_q.clone());
                    }
                    cast_event = Some(SpellCastEvent {
                        spell,
                        target_x: intersection.x,
                        target_z: intersection.z,
                    });
                    hero.targeting_state = TargetingState::None;
                }
            }
            TargetingState::UnitTarget(spell) => {
                let cd = hero.cooldowns.get(&spell).copied().unwrap_or(0.0);
                if cd > 0.0 {
                    hero.targeting_state = TargetingState::None;
                    return None; // Cooldown not ready
                }

                if let Some(intersection) = camera.get_mouse_ray_intersection() {
                    let mut target_idx = None;
                    let mut min_dist = 2.5; // Only target units within this radius
                    for (i, target) in targets.iter().enumerate() {
                        let dist = (intersection - target.1).length();
                        if dist < min_dist {
                            min_dist = dist;
                            target_idx = Some(i);
                        }
                    }

                    if let Some(idx) = target_idx {
                        let target_pos = targets[idx].1;
                        let target_id = targets[idx].0;
                        hero.target_pos = hero.pos;
                        hero.current_path.clear();
                        hero.anim.set_direction(target_pos - hero.pos);
                        hero.casting_timer = 0.5 / hero.stats.get_cast_speed();

                        match spell {
                            SpellId::W => {
                                hero.cooldowns.insert(SpellId::W, 1.0);
                                hero.anim.set_state(AnimationState::Sword);
                                effect_manager.spawn_single_hit(
                                    target_pos,
                                    assets.spell_w.clone(),
                                    SpellId::W,
                                    Some(target_id),
                                );
                            }
                            SpellId::E => {
                                hero.cooldowns.insert(SpellId::E, 2.0);
                                hero.anim.set_state(AnimationState::Staff);
                                effect_manager.spawn_single_hit(
                                    target_pos,
                                    assets.spell_e.clone(),
                                    SpellId::E,
                                    Some(target_id),
                                );
                            }
                            SpellId::R => {
                                hero.cooldowns.insert(SpellId::R, 5.0);
                                hero.anim.set_state(AnimationState::CarryIdle);
                                effect_manager.spawn_single_hit(
                                    target_pos,
                                    assets.spell_r.clone(),
                                    SpellId::R,
                                    Some(target_id),
                                );
                            }
                            _ => {}
                        }
                        cast_event = Some(SpellCastEvent {
                            spell,
                            target_x: target_pos.x,
                            target_z: target_pos.z,
                        });
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
            hero.targeting_state = TargetingState::None;
        } else if hero.casting_timer <= 0.0 {
            if let Some(goal) = camera.get_mouse_ray_intersection() {
                indicators.spawn_move_marker(goal);
                // Try direct movement first (Warcraft 3 style)
                // Use pathfinding_grid (padded) so direct line only happens if there is enough clearance
                if line_of_sight_fn(
                    hero.pos,
                    goal,
                    |p| world.is_walkable(p)
                ) {
                    hero.target_pos = goal;
                    hero.current_path.clear();
                } else {
                    // Obstacle in the way — use A*
                    if let Some(path) = find_path_fn(
                        hero.pos,
                        goal,
                        0.5,
                        |p| world.is_walkable(p)
                    ) {
                        hero.current_path = path;
                        if let Some(first) = hero.current_path.first() {
                            hero.target_pos = *first;
                        }
                    } else {
                        hero.target_pos = goal;
                        hero.current_path.clear();
                    }
                }
            }
        }
    }

    cast_event
}
