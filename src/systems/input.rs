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
    enemies: &mut Vec<crate::entities::enemy::Enemy>,
    combat_text_mgr: &mut crate::ui::combat_text::CombatTextManager,
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
                        
                        // Deal damage to enemies in AoE
                        for enemy in enemies.iter_mut() {
                            if (enemy.pos - intersection).length() <= radius {
                                let dmg = hero.stats.strength * 2;
                                enemy.take_damage(dmg);
                                combat_text_mgr.spawn(enemy.pos, dmg, false, WHITE);
                            }
                        }
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
                    for (i, enemy) in enemies.iter().enumerate() {
                        if enemy.state == crate::entities::enemy::EnemyState::Dead {
                            continue;
                        }
                        let dist = (intersection - enemy.pos).length();
                        if dist < min_dist {
                            min_dist = dist;
                            target_idx = Some(i);
                        }
                    }

                    if let Some(idx) = target_idx {
                        let target_pos = enemies[idx].pos;
                        let target_id = enemies[idx].id;
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
                                let dmg = hero.stats.strength * 3;
                                enemies[idx].take_damage(dmg);
                                combat_text_mgr.spawn(enemies[idx].pos, dmg, false, WHITE);
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
                                let dmg = hero.stats.intelligence * 2;
                                enemies[idx].take_damage(dmg);
                                combat_text_mgr.spawn(enemies[idx].pos, dmg, false, WHITE);
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
                                let dmg = hero.stats.intelligence * 4;
                                enemies[idx].take_damage(dmg);
                                combat_text_mgr.spawn(enemies[idx].pos, dmg, true, YELLOW);
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
                    env.sim.grid_size,
                    env.sim.width,
                    env.sim.height,
                    &env.sim.pathfinding_grid,
                ) {
                    hero.target_pos = goal;
                    hero.current_path.clear();
                } else {
                    // Obstacle in the way — use A* (on the padded grid)
                    if let Some(path) = find_path(
                        hero.pos,
                        goal,
                        env.sim.grid_size,
                        env.sim.width,
                        env.sim.height,
                        &env.sim.pathfinding_grid,
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
