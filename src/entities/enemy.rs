use crate::core::animation::{AnimationManager, AnimationState, SpriteSheetConfig};
use crate::entities::character::{CharacterAppearance, CharacterTextures, LayerCatalog};
use crate::entities::player::Stats;
use macroquad::prelude::*;

const OFFLINE_ENEMY_SPAWNING_ENABLED: bool = true;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EnemyState {
    Idle,
    Chasing,
    Attacking,
    Dead,
}

pub struct Enemy {
    pub id: u64,
    pub pos: Vec3,
    pub target_pos: Vec3,
    pub current_path: Vec<Vec3>,
    pub stats: Stats,
    pub anim: AnimationManager,
    pub state: EnemyState,
    pub scale: f32,
    pub attack_timer: f32,
    pub stuck_timer: f32,
    pub path_timer: f32,
    pub textures: Vec<Texture2D>,
    pub damage_flash_timer: f32,
}

impl Enemy {
    pub fn new(
        id: u64,
        pos: Vec3,
        stats: Stats,
        scale: f32,
        textures: Vec<Texture2D>,
        config: SpriteSheetConfig,
    ) -> Self {
        Self {
            id,
            pos,
            target_pos: pos,
            current_path: Vec::new(),
            stats,
            anim: AnimationManager::new(config),
            state: EnemyState::Idle,
            scale,
            attack_timer: 0.0,
            stuck_timer: 0.0,
            path_timer: macroquad::rand::gen_range(0.0, 0.5),
            textures,
            damage_flash_timer: 0.0,
        }
    }

    pub fn take_damage(&mut self, amount: i32) {
        if self.state == EnemyState::Dead {
            return;
        }
        self.stats.current_hp -= amount;
        self.damage_flash_timer = 0.3; // Flash + stun for 0.3s (matches server hurt_timer)
        if self.stats.current_hp <= 0 {
            self.stats.current_hp = 0;
            self.state = EnemyState::Dead;
            self.anim.set_state(AnimationState::Death);
        } else {
            // Flinch — pause AI during this time (checked in EnemyDirector::update)
            self.state = EnemyState::Idle;
            self.current_path.clear();
            self.anim.set_state(AnimationState::Hurt);
        }
    }
}

pub struct EnemyArchetype {
    pub appearance: CharacterAppearance,
    pub base_stats: Stats,
    pub scale: f32,
}

impl EnemyArchetype {
    pub fn human_bandit() -> Self {
        Self {
            appearance: CharacterAppearance {
                skin: "Human1".to_string(),
                shoes: Some("Shoes".to_string()), 
                clothes: Some("Shirt".to_string()),
                gloves: None,
                hairstyle: Some("Hair1".to_string()),
                facial_hair: None,
                eye_color: None,
                eyelashes: None,
                headgear: None,
                addon: None,
            },
            base_stats: Stats::new(5, 5, 2),
            scale: 2.0,
        }
    }

    pub fn orc_warrior() -> Self {
        Self {
            appearance: CharacterAppearance {
                skin: "Orc1".to_string(),
                shoes: None,
                clothes: None,
                gloves: None,
                hairstyle: None,
                facial_hair: None,
                eye_color: None,
                eyelashes: None,
                headgear: None,
                addon: None,
            },
            base_stats: Stats::new(12, 3, 1),
            scale: 2.5,
        }
    }

    pub fn skeleton_grunt() -> Self {
        Self {
            appearance: CharacterAppearance {
                skin: "Skeleton1".to_string(),
                shoes: None,
                clothes: None,
                gloves: None,
                hairstyle: None,
                facial_hair: None,
                eye_color: None,
                eyelashes: None,
                headgear: None,
                addon: None,
            },
            base_stats: Stats::new(4, 8, 1),
            scale: 1.8,
        }
    }

    pub fn demon_lord() -> Self {
        Self {
            appearance: CharacterAppearance {
                skin: "Demon1".to_string(),
                shoes: None,
                clothes: None,
                gloves: None,
                hairstyle: None,
                facial_hair: None,
                eye_color: None,
                eyelashes: None,
                headgear: None,
                addon: None,
            },
            base_stats: Stats::new(20, 10, 5),
            scale: 3.5,
        }
    }
    
    pub fn night_elf_hunter() -> Self {
        Self {
            appearance: CharacterAppearance {
                skin: "NightElf1".to_string(),
                shoes: None,
                clothes: None,
                gloves: None,
                hairstyle: None,
                facial_hair: None,
                eye_color: None,
                eyelashes: None,
                headgear: None,
                addon: None,
            },
            base_stats: Stats::new(6, 12, 5),
            scale: 2.1,
        }
    }

    pub fn cyclops() -> Self {
        Self {
            appearance: CharacterAppearance {
                skin: "Cyclops1".to_string(),
                shoes: None,
                clothes: None,
                gloves: None,
                hairstyle: None,
                facial_hair: None,
                eye_color: None,
                eyelashes: None,
                headgear: None,
                addon: None,
            },
            base_stats: Stats::new(15, 2, 1),
            scale: 3.0,
        }
    }
}

pub struct PreloadedEnemyRace {
    pub archetype: EnemyArchetype,
    pub textures: Vec<Texture2D>,
}

impl PreloadedEnemyRace {
    pub async fn load(archetype: EnemyArchetype, catalog: &LayerCatalog) -> Self {
        let tex = CharacterTextures::from_appearance(&archetype.appearance, catalog).await;
        Self {
            archetype,
            textures: tex.layers,
        }
    }
}

pub struct EnemyDirector {
    pub active_enemies: Vec<Enemy>,
    pub preloaded_races: Vec<PreloadedEnemyRace>,
    pub wave_timer: f32,
    pub wave_interval: f32,
    pub next_enemy_id: u64,
    pub config: SpriteSheetConfig,
}

impl EnemyDirector {
    pub async fn new(catalog: &LayerCatalog, config: SpriteSheetConfig) -> Self {
        let mut preloaded_races = Vec::new();
        preloaded_races.push(PreloadedEnemyRace::load(EnemyArchetype::human_bandit(), catalog).await);
        preloaded_races.push(PreloadedEnemyRace::load(EnemyArchetype::orc_warrior(), catalog).await);
        preloaded_races.push(PreloadedEnemyRace::load(EnemyArchetype::skeleton_grunt(), catalog).await);
        preloaded_races.push(PreloadedEnemyRace::load(EnemyArchetype::demon_lord(), catalog).await);
        preloaded_races.push(PreloadedEnemyRace::load(EnemyArchetype::night_elf_hunter(), catalog).await);
        preloaded_races.push(PreloadedEnemyRace::load(EnemyArchetype::cyclops(), catalog).await);

        Self {
            active_enemies: Vec::new(),
            preloaded_races,
            wave_timer: 5.0,
            wave_interval: 10.0,
            next_enemy_id: 1,
            config,
        }
    }

    pub fn update(
        &mut self,
        dt: f32,
        hero: &mut crate::entities::player::Hero,
        combat_text: &mut crate::ui::combat_text::CombatTextManager,
        world: &crate::world::chunk::ChunkedWorld,
    ) {
        if OFFLINE_ENEMY_SPAWNING_ENABLED {
            self.wave_timer -= dt;
            if self.wave_timer <= 0.0 {
                self.wave_timer = self.wave_interval;
                self.spawn_wave(hero.pos, world);
            }
        }

        let hero_pos = hero.pos;

        for enemy in &mut self.active_enemies {
            if enemy.damage_flash_timer > 0.0 {
                enemy.damage_flash_timer -= dt;
            }
            if enemy.attack_timer > 0.0 {
                enemy.attack_timer -= dt;
            }

            if enemy.damage_flash_timer > 0.0 && enemy.state != EnemyState::Dead {
                enemy.anim.set_state(AnimationState::Hurt);
                enemy.anim.update(dt, enemy.stats.get_movement_speed(), 1.0);
                continue;
            }

            if enemy.state != EnemyState::Dead && enemy.state != EnemyState::Attacking {
                let to_hero = hero_pos - enemy.pos;
                let dist = to_hero.length();

                if dist < 1.5 {
                    if enemy.attack_timer <= 0.0 {
                        enemy.state = EnemyState::Attacking;
                        enemy.anim.set_state(AnimationState::Sword);
                        enemy.anim.set_direction(to_hero);
                        enemy.attack_timer = 1.0;
                        
                        let dmg = enemy.stats.strength;
                        hero.stats.current_hp -= dmg;
                        if hero.stats.current_hp <= 0 {
                            hero.stats.current_hp = 0;
                            hero.is_dead = true;
                        }
                        combat_text.spawn(hero.pos, dmg, false, RED);
                    } else {
                        enemy.state = EnemyState::Idle;
                        enemy.anim.set_state(AnimationState::Idle);
                    }
                } else if dist < 15.0 {
                    // Town Biome Safe-Zone Logic
                    let chunk_coord = crate::world::chunk::ChunkCoord::from_world_pos(enemy.pos);
                    let is_safe_zone = world.chunks.get(&chunk_coord)
                        .map(|c| c.biome == crate::world::chunk::BiomeType::Town)
                        .unwrap_or(false);

                    if is_safe_zone {
                        enemy.state = EnemyState::Idle;
                        enemy.anim.set_state(AnimationState::Idle);
                        enemy.current_path.clear();
                        continue;
                    }

                    enemy.state = EnemyState::Chasing;
                    let speed = enemy.stats.get_movement_speed();

                    enemy.path_timer -= dt;
                    if enemy.path_timer <= 0.0 {
                        enemy.path_timer = macroquad::rand::gen_range(0.5, 1.0);

                        if crate::world::pathfinding::line_of_sight_fn(
                            enemy.pos,
                            hero.pos,
                            |p| world.is_walkable(p)
                        ) {
                            enemy.current_path.clear();
                            enemy.target_pos = hero.pos;
                        } else {
                            if let Some(path) = crate::world::pathfinding::find_path_fn(
                                enemy.pos,
                                hero.pos,
                                0.5,
                                |p| world.is_walkable(p)
                            ) {
                                enemy.current_path = path;
                                if let Some(first) = enemy.current_path.first() {
                                    enemy.target_pos = *first;
                                }
                            } else {
                                enemy.target_pos = hero.pos;
                            }
                        }
                    }

                    if enemy.current_path.is_empty() {
                        enemy.target_pos = hero.pos;
                    }

                    let to_target = enemy.target_pos - enemy.pos;
                    let dist_to_target = to_target.length();

                    if dist_to_target > 0.1 {
                        let desired = enemy.pos + to_target.normalize() * speed * dt;
                        let new_pos = crate::world::pathfinding::slide_move_world(
                            enemy.pos,
                            desired,
                            0.35,
                            |p| world.is_walkable(p)
                        );
                        enemy.pos = new_pos;
                        enemy.anim.set_state(AnimationState::Walk);
                        enemy.anim.set_direction(to_target);
                    } else if !enemy.current_path.is_empty() {
                        enemy.current_path.remove(0);
                        if let Some(next) = enemy.current_path.first() {
                            enemy.target_pos = *next;
                        }
                    }
                } else {
                    enemy.state = EnemyState::Idle;
                    enemy.anim.set_state(AnimationState::Idle);
                }
            } else if enemy.state == EnemyState::Attacking {
                let frames = enemy.anim.get_frame_indices();
                if enemy.anim.current_frame >= (frames.len() as f32 - 0.1) {
                    enemy.state = EnemyState::Idle;
                }
            }
            enemy.anim.update(dt, enemy.stats.get_movement_speed(), 1.0);
        }

        self.active_enemies.retain(|e| {
            if e.state == EnemyState::Dead {
                let frames = e.anim.get_frame_indices();
                let is_anim_finished = e.anim.current_frame >= (frames.len() as f32 - 1.0);
                !is_anim_finished 
            } else {
                true
            }
        });
    }

    fn spawn_wave(&mut self, center_pos: Vec3, world: &crate::world::chunk::ChunkedWorld) {
        if self.preloaded_races.is_empty() {
            return;
        }

        let num_to_spawn = 3 + macroquad::rand::gen_range(0, 3);
        for _ in 0..num_to_spawn {
            let race_idx = macroquad::rand::gen_range(0, self.preloaded_races.len() as u32) as usize;
            let race = &self.preloaded_races[race_idx];
            
            let mut spawn_pos = center_pos;

            for _ in 0..10 {
                let angle = macroquad::rand::gen_range(0.0, std::f32::consts::PI * 2.0);
                let distance = macroquad::rand::gen_range(8.0, 15.0);
                let candidate_pos = center_pos + vec3(angle.cos() * distance, 0.0, angle.sin() * distance);
                
                let chunk_coord = crate::world::chunk::ChunkCoord::from_world_pos(candidate_pos);
                let is_town = world.get_biome_at(chunk_coord) == crate::world::chunk::BiomeType::Town;

                if is_town {
                    continue;
                }

                if crate::world::pathfinding::is_walkable_with_radius_fn(
                    candidate_pos,
                    0.35,
                    |p| world.is_walkable(p)
                ) {
                    spawn_pos = candidate_pos;
                    break;
                }
            }

            let new_enemy = Enemy::new(
                self.next_enemy_id,
                spawn_pos,
                Stats::new(
                    race.archetype.base_stats.strength,
                    race.archetype.base_stats.agility,
                    race.archetype.base_stats.intelligence,
                ),
                race.archetype.scale,
                race.textures.clone(),
                SpriteSheetConfig {
                    columns: self.config.columns,
                    rows: self.config.rows,
                },
            );

            self.active_enemies.push(new_enemy);
            self.next_enemy_id += 1;
        }
    }
}
