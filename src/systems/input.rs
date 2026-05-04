use crate::Assets;
use crate::core::animation::AnimationState;
use crate::core::camera::GameCamera;
use crate::entities::effects::EffectManager;
use crate::entities::player::{Hero, SpellCastEvent, SpellId, TargetingState};
use crate::systems::indicators::IndicatorManager;
use crate::world::environment::WorldEnvironment;
use crate::world::pathfinding::{find_path, line_of_sight};
use macroquad::prelude::*;

pub fn handle_input(
    hero: &mut Hero,
    camera: &GameCamera,
    effect_manager: &mut EffectManager,
    dummies: &[Vec3],
    assets: &Assets,
    env: &WorldEnvironment,
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
            TargetingState::Aoe(spell, _radius) => {
                if let Some(intersection) = camera.get_mouse_ray_intersection() {
                    hero.target_pos = hero.pos;
                    hero.current_path.clear();
                    hero.anim.set_direction(intersection - hero.pos);
                    hero.casting_timer = 0.5;

                    if spell == SpellId::Q {
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
                if let Some(intersection) = camera.get_mouse_ray_intersection() {
                    let mut target_idx = None;
                    for (i, d_pos) in dummies.iter().enumerate() {
                        if (intersection - *d_pos).length() < 2.5 {
                            target_idx = Some(i);
                            break;
                        }
                    }

                    if let Some(idx) = target_idx {
                        let target_pos = dummies[idx];
                        hero.target_pos = hero.pos;
                        hero.current_path.clear();
                        hero.anim.set_direction(target_pos - hero.pos);
                        hero.casting_timer = 0.5;

                        match spell {
                            SpellId::W => {
                                hero.anim.set_state(AnimationState::Sword);
                                effect_manager.spawn_single_hit(
                                    target_pos,
                                    assets.spell_w.clone(),
                                    SpellId::W,
                                );
                            }
                            SpellId::E => {
                                hero.anim.set_state(AnimationState::Staff);
                                effect_manager.spawn_single_hit(
                                    target_pos,
                                    assets.spell_e.clone(),
                                    SpellId::E,
                                );
                            }
                            SpellId::R => {
                                hero.anim.set_state(AnimationState::CarryIdle);
                                effect_manager.spawn_single_hit(
                                    target_pos,
                                    assets.spell_r.clone(),
                                    SpellId::R,
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
                if line_of_sight(
                    hero.pos,
                    goal,
                    env.grid_size,
                    env.width,
                    env.height,
                    &env.pathfinding_grid,
                ) {
                    hero.target_pos = goal;
                    hero.current_path.clear();
                } else {
                    // Obstacle in the way — use A* (on the padded grid)
                    if let Some(path) = find_path(
                        hero.pos,
                        goal,
                        env.grid_size,
                        env.width,
                        env.height,
                        &env.pathfinding_grid,
                    ) {
                        hero.current_path = path;
                        if let Some(first) = hero.current_path.first() {
                            hero.target_pos = *first;
                        }
                    } else {
                        // No path found — move directly (slide_move will catch the hits)
                        hero.target_pos = goal;
                        hero.current_path.clear();
                    }
                }
            }
        }
    }

    cast_event
}
