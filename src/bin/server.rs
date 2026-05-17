//! RPG Engine — Headless Authoritative Server
//!
//! Run with: cargo run --bin server
//! Or with a custom port: cargo run --bin server -- 7878

use rpg_engine::net::protocol::*;

use rpg_engine::world::environment::{HitboxConfig, builtin_template_defs, load_glb_template_sync};
use rpg_engine::world::chunk::{ChunkedWorld, ChunkCoord, BiomeType};
use rpg_engine::world::pathfinding::{find_path_detailed_fn, slide_move_world, find_closest_walkable_fn};
use macroquad::math::{Vec3, vec3};

use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use postgres::{Client as PgClient, NoTls};

/// Autosave interval — WoW-style: save periodically + on disconnect.
const AUTOSAVE_INTERVAL_SECS: f64 = 60.0;
const ENEMY_SPAWNING_ENABLED: bool = true;

/// Represents a connected player on the server.
struct ServerPlayer {
    id: PlayerId,
    addr: SocketAddr,
    name: String,
    appearance: CharacterAppearanceNet,
    x: f32,
    z: f32,
    target_x: f32,
    target_z: f32,
    direction: u8,
    anim_state: u8,
    anim_frame: f32,
    casting_timer: f32,
    last_heard: Instant,
    current_hp: i32,
    max_hp: i32,
    current_mp: i32,
    max_mp: i32,
    is_dead: bool,
    revive_timer: f32,
    is_invulnerable: bool,
    loaded_chunks: HashSet<ChunkCoord>,
    current_path: Vec<Vec3>,
    // ─── Persistence ───
    account_id: i32,
    character_id: i32,
    session_token: u64,
}

struct PendingClient {
    addr: SocketAddr,
    account_id: i32,
    username: String,
    last_heard: Instant,
}

struct ActiveEffect {
    id: u64,
    effect_type: u8,
    x: f32,
    z: f32,
    timer: f32,
}

struct ServerEnemy {
    id: u64,
    race_idx: usize,
    x: f32,
    z: f32,
    target_x: f32,
    target_z: f32,
    direction: u8,
    anim_state: u8,
    health: i32,
    max_health: i32,
    current_path: Vec<Vec3>,
    next_path_recalc: f32,
    death_timer: f32,
    hurt_timer: f32,
    attack_timer: f32,
    spawn_x: f32,
    spawn_z: f32,
}

fn log_path_failure(label: &str, start: Vec3, goal: Vec3, diagnostics: &rpg_engine::world::pathfinding::PathfindDiagnostics) {
    println!(
        "[PATHFINDING:{label}] failed start=({:.2},{:.2}) goal=({:.2},{:.2}) start_grid=({}, {}) goal_grid=({}, {}) search_min=({}, {}) search_max=({}, {}) start_walkable={} goal_walkable={} expanded_nodes={}",
        start.x,
        start.z,
        goal.x,
        goal.z,
        diagnostics.start_grid.x,
        diagnostics.start_grid.z,
        diagnostics.goal_grid.x,
        diagnostics.goal_grid.z,
        diagnostics.min_search_grid.x,
        diagnostics.min_search_grid.z,
        diagnostics.max_search_grid.x,
        diagnostics.max_search_grid.z,
        diagnostics.start_walkable,
        diagnostics.goal_walkable,
        diagnostics.expanded_nodes,
    );
}

const MAX_PLAYERS: usize = 16;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = if args.len() > 1 { args[1].parse().unwrap_or(7878) } else { 7878 };

    // Initialize Database
    let mut db = db_connect();
    db_run_migrations(&mut db);

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).expect("Failed to bind UDP socket");
    socket.set_nonblocking(true).expect("Failed to set nonblocking");

    println!("[SERVER] RPG Engine Server listening on port {}", port);

    let mut players: HashMap<SocketAddr, ServerPlayer> = HashMap::new();
    let mut pending_clients: HashMap<SocketAddr, PendingClient> = HashMap::new();
    let mut sessions: HashMap<u64, SocketAddr> = HashMap::new();
    let mut enemies: HashMap<u64, ServerEnemy> = HashMap::new();
    let mut effects: Vec<ActiveEffect> = Vec::new();
    
    let mut next_player_id: PlayerId = 1;
    let mut next_enemy_id: u64 = 1;
    let mut next_effect_id: u64 = 1;
    let mut server_tick: u64 = 0;

    let hitbox_config: HitboxConfig = match std::fs::read_to_string("hitbox_config.json") {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => HitboxConfig::new(),
    };
    
    let mut world = ChunkedWorld::new(hitbox_config, HashMap::new(), 12345);

    // Pre-load built-in templates to avoid stuttering during chunk generation
    println!("[SERVER] Pre-loading world templates...");
    for (key, file) in builtin_template_defs() {
        if let Some(t) = load_glb_template_sync(&format!("assets/world_models/{}", file)) {
            world.templates.insert(key.to_string(), t);
        }
    }
    world.update_cached_masks();

    let mut last_tick = Instant::now();
    let mut last_broadcast = Instant::now();
    let mut last_autosave = Instant::now();
    let mut enemy_spawn_timer = 5.0;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Authoritative Enemy Spawning
        if ENEMY_SPAWNING_ENABLED {
            enemy_spawn_timer -= dt;
            if enemy_spawn_timer <= 0.0 {
                enemy_spawn_timer = 5.0; // Check every 5 seconds
                
                if enemies.len() < 30 { // Cap total enemies
                    for player in players.values() {
                        let player_chunk = ChunkCoord::from_world_pos(vec3(player.x, 0.0, player.z));
                        if world.get_biome_at(player_chunk) != BiomeType::Town {
                            // Player is outside, spawn an enemy near them
                            use rand::Rng;
                            let mut rng = rand::thread_rng();
                            let angle = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
                            let sx = player.x + angle.cos() * 10.0;
                            let sz = player.z + angle.sin() * 10.0;
                            
                            let spawn_chunk = ChunkCoord::from_world_pos(vec3(sx, 0.0, sz));
                            if world.is_walkable(vec3(sx, 0.0, sz)) && world.get_biome_at(spawn_chunk) != BiomeType::Town {
                                enemies.insert(next_enemy_id, ServerEnemy {
                                    id: next_enemy_id, race_idx: rng.gen_range(0..6) as usize, x: sx, z: sz, target_x: sx, target_z: sz, direction: 0, anim_state: 0, health: 100, max_health: 100,
                                    current_path: Vec::new(), next_path_recalc: 0.0, death_timer: 0.0, hurt_timer: 0.0, attack_timer: 0.0, spawn_x: sx, spawn_z: sz,
                                });
                                next_enemy_id += 1;
                                break; // Only spawn one enemy per timer tick to prevent performance spikes
                            }
                        }
                    }
                }
            }
        }
        
        // Hot reload world data
        world.check_hot_reload();

        // Process incoming packets
        let mut buf = [0u8; 65536];
        while let Ok((len, addr)) = socket.recv_from(&mut buf) {
            if let Some(payload) = decode_packet(&buf[..len]) {
                if let PacketPayload::Client(msg) = payload {
                    handle_client_message(&socket, addr, msg, &mut db, &mut players, &mut pending_clients, &mut sessions, &mut enemies, &mut effects, &mut next_player_id, &mut next_enemy_id, &mut next_effect_id, &mut world);
                }
            }
        }

        // Periodic Autosave
        if last_autosave.elapsed().as_secs_f64() > AUTOSAVE_INTERVAL_SECS {
            println!("[SERVER] Autosaving all players...");
            for player in players.values() {
                db_save_player(&mut db, player);
            }
            last_autosave = Instant::now();
        }

        // Update World Simulation
        {
            // Update players
            let player_addrs: Vec<SocketAddr> = players.keys().cloned().collect();
            for addr in player_addrs {
                let mut player = players.remove(&addr).unwrap();
                
                // Timeout check
                if player.last_heard.elapsed().as_secs_f64() > 10.0 {
                    println!("[SERVER] Player {} ({}) timed out — saving to DB", player.name, player.id);
                    db_save_player(&mut db, &player);
                    sessions.remove(&player.session_token);
                    broadcast(&socket, &players, &ServerMessage::PlayerLeft { id: player.id });
                    continue;
                }

                if player.is_dead {
                    player.anim_state = 2; // Death animation
                    players.insert(addr, player);
                    continue;
                }

                if player.casting_timer > 0.0 {
                    player.casting_timer -= dt;
                    if player.casting_timer <= 0.0 {
                        player.anim_state = 0; 
                    }
                    players.insert(addr, player);
                    continue;
                }

                let dx = player.target_x - player.x;
                let dz = player.target_z - player.z;
                let dist = (dx * dx + dz * dz).sqrt();

                if dist > 0.05 {
                    let speed = 5.25;
                    let move_dist = speed * dt;
                    
                    let current_pos = vec3(player.x, 0.0, player.z);
                    let dir = vec3(dx / dist, 0.0, dz / dist);
                    let desired_pos = current_pos + dir * move_dist.min(dist);

                    let new_pos = slide_move_world(
                        current_pos,
                        desired_pos,
                        0.35,
                        |p| world.is_walkable(p)
                    );

                    player.x = new_pos.x;
                    player.z = new_pos.z;

                    // Re-calculate distance after move
                    let new_dist = (vec3(player.target_x, 0.0, player.target_z) - vec3(player.x, 0.0, player.z)).length();

                    // If reached waypoint (either before or after move), move to next
                    if (dist <= move_dist || new_dist <= 0.05) && !player.current_path.is_empty() {
                        let reached = player.current_path.remove(0);
                        println!("[SERVER] Player {} reached waypoint ({:.2}, {:.2}), {} nodes remaining", player.id, reached.x, reached.z, player.current_path.len());
                        if let Some(next) = player.current_path.first() {
                            player.target_x = next.x;
                            player.target_z = next.z;
                            println!("[SERVER] Player {} next waypoint: ({:.2}, {:.2})", player.id, player.target_x, player.target_z);
                            
                            // Immediately update direction for the NEW target
                            let ndx = player.target_x - player.x;
                            let ndz = player.target_z - player.z;
                            let ndist = (ndx * ndx + ndz * ndz).sqrt();
                            if ndist > 0.01 {
                                let mut angle = f32::atan2(ndx, ndz);
                                if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                                player.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                            }
                        }
                    }

                    if dist <= move_dist && player.current_path.is_empty() {
                        player.anim_state = 0; // Idle (only if truly reached final destination)
                    } else {
                        player.anim_state = 1; // Walk (stay in walk if we have more nodes)
                        let mut angle = f32::atan2(dx, dz);
                        if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                        player.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                    }
                } else if !player.current_path.is_empty() {
                    // We are very close to current target (dist <= 0.05) but have more waypoints.
                    // Pop the reached one and set the next one.
                    player.current_path.remove(0);
                    if let Some(next) = player.current_path.first() {
                        player.target_x = next.x;
                        player.target_z = next.z;
                        
                        // Immediately update direction for the NEW target
                        let ndx = player.target_x - player.x;
                        let ndz = player.target_z - player.z;
                        let ndist = (ndx * ndx + ndz * ndz).sqrt();
                        if ndist > 0.01 {
                            let mut angle = f32::atan2(ndx, ndz);
                            if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                            player.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                        }
                    }
                    player.anim_state = 1; // Stay in Walk if we just switched to a new node
                } else {
                    player.anim_state = 0; // Idle
                }

                // ─── Chunk Streaming ───
                let current_pos = vec3(player.x, 0.0, player.z);
                let current_chunk = ChunkCoord::from_world_pos(current_pos);
                let load_radius = 2;
                for dx in -load_radius..=load_radius {
                    for dz in -load_radius..=load_radius {
                        let coord = ChunkCoord::new(current_chunk.x + dx, current_chunk.z + dz);
                        if !player.loaded_chunks.contains(&coord) {
                            world.generate_chunk(coord, false);
                            if let Some(chunk) = world.chunks.get(&coord) {
                                player.loaded_chunks.insert(coord);
                                let mut palette = Vec::new();
                                let mut palette_indices = HashMap::new();
                                for placement in &chunk.placements {
                                    palette_indices
                                        .entry(placement.model.clone())
                                        .or_insert_with(|| {
                                            let index = palette.len() as u16;
                                            palette.push((
                                                placement.model.clone(),
                                                placement.file.clone(),
                                            ));
                                            index
                                        });
                                }
                                let chunk_msg = ServerMessage::ChunkData {
                                    coord_x: coord.x,
                                    coord_z: coord.z,
                                    biome: chunk.biome.to_string(),
                                    palette: palette.clone(),
                                    placements: chunk.placements.iter().map(|p| {
                                        let model_idx = *palette_indices
                                            .get(&p.model)
                                            .expect("chunk palette missing placement model");
                                        ModelPlacementNet {
                                            model_idx,
                                            position: p.position,
                                            rotation: p.rotation,
                                            scale: p.scale,
                                            blocks_movement: p.blocks_movement,
                                        }
                                    }).collect(),
                                };
                                let packet = encode_server_message(&chunk_msg);
                                let _ = socket.send_to(&packet, player.addr);
                            }
                        }
                    }
                }

                players.insert(addr, player);
            }

            // Update enemies (chase, attack, etc.)
            for enemy in enemies.values_mut() {
                if enemy.health <= 0 {
                    enemy.death_timer += dt;
                    enemy.anim_state = 7; // Death
                    continue;
                }
                
                if enemy.hurt_timer > 0.0 {
                    enemy.hurt_timer -= dt;
                    enemy.anim_state = 6; // Hurt
                    continue;
                }
                
                if enemy.attack_timer > 0.0 {
                    enemy.attack_timer -= dt;
                }
                
                enemy.next_path_recalc -= dt;
                
                let current_chunk = ChunkCoord::from_world_pos(vec3(enemy.x, 0.0, enemy.z));
                let in_safe_zone = world.get_biome_at(current_chunk) == BiomeType::Town;

                let mut nearest_player_id: Option<SocketAddr> = None;
                let mut nearest_dist = f32::MAX;
                
                if !in_safe_zone {
                    for (addr, player) in &players {
                        if player.is_dead { continue; }
                        
                        // Enemies ignore players in Town
                        let player_chunk = ChunkCoord::from_world_pos(vec3(player.x, 0.0, player.z));
                        if world.get_biome_at(player_chunk) == BiomeType::Town { continue; }

                        let dx = player.x - enemy.x;
                        let dz = player.z - enemy.z;
                        let dist = (dx * dx + dz * dz).sqrt();
                        
                        // Aggro Range: 10.0 meters
                        if dist < 10.0 && dist < nearest_dist {
                            nearest_dist = dist;
                            nearest_player_id = Some(*addr);
                        }
                    }
                }
                
                if let Some(addr) = nearest_player_id {
                    let target = players.get(&addr).unwrap();
                    if nearest_dist <= 1.5 {
                        enemy.target_x = enemy.x;
                        enemy.target_z = enemy.z;
                        enemy.current_path.clear();
                        
                        if enemy.attack_timer <= 0.0 {
                            enemy.anim_state = 3; // Attack
                            enemy.attack_timer = 2.0;
                            let player_mut = players.get_mut(&addr).unwrap();
                            if !player_mut.is_invulnerable {
                                player_mut.current_hp -= 20;
                                if player_mut.current_hp <= 0 {
                                    player_mut.current_hp = 0;
                                    player_mut.is_dead = true;
                                }
                            }
                        } else if enemy.attack_timer <= 1.5 {
                            enemy.anim_state = 0; 
                        } else {
                            enemy.anim_state = 3; 
                        }
                    } else {
                        // Chase
                        if enemy.next_path_recalc <= 0.0 {
                            enemy.next_path_recalc = 0.5;
                            let start = vec3(enemy.x, 0.0, enemy.z);
                            let goal = vec3(target.x, 0.0, target.z);
                            let result = find_path_detailed_fn(start, goal, 0.5, |p| world.is_walkable_with_radius(p, 0.35));
                            if let Some(mut path) = result.path {
                                if path.len() > 1 && (path[0] - vec3(enemy.x, 0.0, enemy.z)).length() < 0.5 {
                                    path.remove(0);
                                }
                                enemy.current_path = path;
                            } else {
                                log_path_failure("enemy_chase", start, goal, &result.diagnostics);
                            }
                        }
                        
                        if let Some(next_node) = enemy.current_path.first() {
                            enemy.target_x = next_node.x;
                            enemy.target_z = next_node.z;
                            
                            let dx = enemy.target_x - enemy.x;
                            let dz = enemy.target_z - enemy.z;
                            let dist = (dx * dx + dz * dz).sqrt();
                            
                            if dist > 0.1 {
                                let speed = 3.5;
                                let move_dist = speed * dt;
                                let current_pos = vec3(enemy.x, 0.0, enemy.z);
                                let desired_pos = if move_dist >= dist {
                                    vec3(enemy.target_x, 0.0, enemy.target_z)
                                } else {
                                    current_pos + vec3(dx, 0.0, dz).normalize() * move_dist
                                };
                                let new_pos = slide_move_world(
                                    current_pos,
                                    desired_pos,
                                    0.35,
                                    |p| world.is_walkable(p)
                                );
                                enemy.x = new_pos.x;
                                enemy.z = new_pos.z;
                                if move_dist >= dist {
                                    enemy.current_path.remove(0);
                                }
                                enemy.anim_state = 1; // Walk
                                let mut angle = f32::atan2(dx, dz);
                                if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                                enemy.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                            } else {
                                enemy.current_path.remove(0);
                            }
                        } else {
                            enemy.anim_state = 0; 
                        }
                    }
                } else {
                    // Return to spawn or idle
                    if in_safe_zone || (enemy.x - enemy.spawn_x).abs() > 0.1 || (enemy.z - enemy.spawn_z).abs() > 0.1 {
                        enemy.target_x = enemy.spawn_x;
                        enemy.target_z = enemy.spawn_z;
                        let dx = enemy.target_x - enemy.x;
                        let dz = enemy.target_z - enemy.z;
                        let dist = (dx * dx + dz * dz).sqrt();
                        if dist > 0.1 {
                            let speed = 3.5;
                            let move_dist = speed * dt;
                            let current_pos = vec3(enemy.x, 0.0, enemy.z);
                            let desired_pos = if move_dist >= dist {
                                vec3(enemy.target_x, 0.0, enemy.target_z)
                            } else {
                                current_pos + vec3(dx, 0.0, dz).normalize() * move_dist
                            };
                            let new_pos = slide_move_world(
                                current_pos,
                                desired_pos,
                                0.35,
                                |p| world.is_walkable(p)
                            );
                            enemy.x = new_pos.x;
                            enemy.z = new_pos.z;
                            enemy.anim_state = 1; 
                            let mut angle = f32::atan2(dx, dz);
                            if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                            enemy.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                        } else {
                            enemy.anim_state = 0; 
                        }
                    } else {
                        enemy.anim_state = 0; 
                    }
                }
            }

            for effect in &mut effects {
                effect.timer -= dt;
            }
            effects.retain(|e| e.timer > 0.0);
            enemies.retain(|_, e| e.death_timer < 2.5);

            // Handle Revival
            let player_ids: Vec<SocketAddr> = players.keys().cloned().collect();
            for i in 0..player_ids.len() {
                let id_dead = player_ids[i];
                if !players[&id_dead].is_dead { continue; }
                let mut reviver_present = false;
                let dead_pos = vec3(players[&id_dead].x, 0.0, players[&id_dead].z);
                for j in 0..player_ids.len() {
                    if i == j { continue; }
                    let id_alive = player_ids[j];
                    let alive_player = &players[&id_alive];
                    if alive_player.is_dead { continue; }
                    let alive_pos = vec3(alive_player.x, 0.0, alive_player.z);
                    if (dead_pos - alive_pos).length() < 1.0 {
                        reviver_present = true;
                        break;
                    }
                }
                if reviver_present {
                    let player = players.get_mut(&id_dead).unwrap();
                    player.revive_timer += dt;
                    if player.revive_timer >= 3.0 {
                        player.is_dead = false;
                        player.current_hp = (player.max_hp / 2).max(1);
                        player.revive_timer = 0.0;
                    }
                } else {
                    players.get_mut(&id_dead).unwrap().revive_timer = 0.0;
                }
            }

            // Update gates
            let player_positions: Vec<Vec3> = players.values().map(|p| vec3(p.x, 0.0, p.z)).collect();
            world.update_gates(&player_positions, dt);

            // Check Game Over
            if !players.is_empty() && players.values().all(|p| p.is_dead) {
                broadcast(&socket, &players, &ServerMessage::GameOver);
            }
        }

        // Broadcast World State
        // Broadcast world state to all players at the defined tick rate
        if last_broadcast.elapsed().as_secs_f64() > (1.0 / SERVER_TICK_RATE as f64) {
            last_broadcast = Instant::now();
            let mut player_states = Vec::new();
            for p in players.values() {
                player_states.push(PlayerState {
                    id: p.id,
                    x: p.x,
                    z: p.z,
                    target_x: p.target_x,
                    target_z: p.target_z,
                    direction: p.direction,
                    anim_state: p.anim_state,
                    anim_frame: 0.0,
                    casting_timer: p.casting_timer,
                    current_hp: p.current_hp,
                    max_hp: p.max_hp,
                    current_mp: p.current_mp,
                    max_mp: p.max_mp,
                    is_dead: p.is_dead,
                    revive_progress: p.revive_timer / 3.0,
                    current_path: p.current_path.iter().map(|v| (v.x, v.z)).collect(),
                });
            }

            let mut enemy_states = Vec::new();
            for e in enemies.values() {
                enemy_states.push(EnemyStateNet {
                    id: e.id,
                    race_idx: e.race_idx,
                    x: e.x,
                    z: e.z,
                    target_x: e.target_x,
                    target_z: e.target_z,
                    direction: e.direction,
                    anim_state: e.anim_state,
                    health: e.health,
                    max_health: e.max_health,
                    current_path: e.current_path.iter().map(|v| (v.x, v.z)).collect(),
                });
            }

            let mut effect_states = Vec::new();
            for e in &effects {
                effect_states.push(EffectState {
                    effect_id: e.id,
                    spell: e.effect_type,
                    x: e.x,
                    z: e.z,
                    timer: e.timer,
                    caster_id: 0,
                });
            }

            let mut gate_states = Vec::new();
            for g in &world.gates {
                gate_states.push(GateStateNet {
                    x: g.position.x,
                    z: g.position.z,
                    open_progress: g.open_progress,
                });
            }

            server_tick += 1;
            let world_msg = ServerMessage::WorldState {
                tick: server_tick,
                server_time: 0.0,
                players: player_states,
                enemies: enemy_states,
                effects: effect_states,
                gates: gate_states,
            };

            let packet = encode_server_message(&world_msg);
            for player in players.values() {
                let _ = socket.send_to(&packet, player.addr);
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}

fn handle_client_message(
    socket: &UdpSocket,
    addr: SocketAddr,
    msg: ClientMessage,
    db: &mut PgClient,
    players: &mut HashMap<SocketAddr, ServerPlayer>,
    pending_clients: &mut HashMap<SocketAddr, PendingClient>,
    sessions: &mut HashMap<u64, SocketAddr>,
    enemies: &mut HashMap<u64, ServerEnemy>,
    effects: &mut Vec<ActiveEffect>,
    next_id: &mut PlayerId,
    next_enemy_id: &mut u64,
    next_effect_id: &mut u64,
    world: &mut ChunkedWorld,
) {
    match msg {
        ClientMessage::Login { version, username } => {
            if version != PROTOCOL_VERSION {
                let reject = encode_server_message(&ServerMessage::JoinRejected {
                    reason: format!("Version mismatch: server={}, client={}", PROTOCOL_VERSION, version),
                });
                let _ = socket.send_to(&reject, addr);
                return;
            }
            let account_id = db_get_or_create_account(db, &username);
            let characters = db_list_characters(db, account_id);
            pending_clients.insert(addr, PendingClient {
                addr,
                account_id,
                username,
                last_heard: Instant::now(),
            });
            let msg = encode_server_message(&ServerMessage::CharacterList { characters });
            let _ = socket.send_to(&msg, addr);
        }

        ClientMessage::SelectCharacter { character_id } => {
            let pc = match pending_clients.remove(&addr) {
                Some(pc) => pc,
                None => return,
            };
            if players.len() >= MAX_PLAYERS {
                let reject = encode_server_message(&ServerMessage::JoinRejected { reason: "Server full".to_string() });
                let _ = socket.send_to(&reject, addr);
                pending_clients.insert(addr, pc);
                return;
            }
            let char_data = match db_load_character(db, pc.account_id, character_id) {
                Some(data) => data,
                None => return,
            };
            let (name, appearance, x, z, hp, max_hp, mp, max_mp, is_dead) = char_data;

            if let Some(old_player) = players.remove(&addr) {
                broadcast(socket, players, &ServerMessage::PlayerLeft { id: old_player.id });
                sessions.remove(&old_player.session_token);
            }

            let id = *next_id;
            *next_id += 1;
            let session_token: u64 = rand::random();
            let accept = encode_server_message(&ServerMessage::JoinAccepted { your_id: id, session_token });
            let _ = socket.send_to(&accept, addr);
            for existing in players.values() {
                let msg = encode_server_message(&ServerMessage::PlayerJoined { id: existing.id, name: existing.name.clone(), appearance: existing.appearance.clone() });
                let _ = socket.send_to(&msg, addr);
            }
            broadcast(socket, players, &ServerMessage::PlayerJoined { id, name: name.clone(), appearance: appearance.clone() });
            sessions.insert(session_token, addr);
            players.insert(addr, ServerPlayer {
                id, addr, name, appearance, x, z, target_x: x, target_z: z, direction: 0, anim_state: 0, anim_frame: 0.0, casting_timer: 0.0, last_heard: Instant::now(),
                current_hp: hp, max_hp, current_mp: mp, max_mp, is_dead, revive_timer: 0.0, is_invulnerable: false, loaded_chunks: HashSet::new(),
                account_id: pc.account_id, character_id, session_token, current_path: Vec::new(),
            });
        }

        ClientMessage::CreateCharacter { name, appearance } => {
            let pc = match pending_clients.get_mut(&addr) {
                Some(pc) => pc,
                None => return,
            };
            let char_id = db_create_character(db, pc.account_id, &name, &appearance);
            let summary = CharacterSummaryNet { id: char_id, name, appearance, current_hp: 100, max_hp: 100, current_mp: 50, max_mp: 50 };
            let msg = encode_server_message(&ServerMessage::CharacterCreated { character: summary });
            let _ = socket.send_to(&msg, addr);
        }

        ClientMessage::DeleteCharacter { character_id } => {
            let pc = match pending_clients.get(&addr) { Some(pc) => pc, None => return };
            db_delete_character(db, pc.account_id, character_id);
            let msg = encode_server_message(&ServerMessage::CharacterDeleted { character_id });
            let _ = socket.send_to(&msg, addr);
        }

        ClientMessage::MoveTo { x, z } => {
            if let Some(player) = players.get_mut(&addr) {
                player.last_heard = Instant::now();
                if player.is_dead { return; }

                // Calculate path to destination
                let start = vec3(player.x, 0.0, player.z);
                let mut goal = vec3(x, 0.0, z);

                // --- SNAP TO CLOSEST WALKABLE TILE ---
                let is_walkable = world.is_walkable_with_radius(goal, 0.35);
                if !is_walkable {
                    goal = find_closest_walkable_fn(goal, 50, 0.5, |p| world.is_walkable_with_radius(p, 0.35));
                }
                // -------------------------------------

                let result = find_path_detailed_fn(start, goal, 0.5, |p| {
                    world.is_walkable_with_radius(p, 0.35)
                });

                if let Some(mut path) = result.path {
                    // Remove first node if it's too close to current position
                    if path.len() > 1 && (path[0] - start).length() < 0.4 {
                        path.remove(0);
                    }
                    println!("[SERVER] New path for player {}: {} nodes (from ({:.2}, {:.2}) to ({:.2}, {:.2}))", player.id, path.len(), start.x, start.z, goal.x, goal.z);
                    player.current_path = path;
                    if let Some(first) = player.current_path.first() {
                        player.target_x = first.x;
                        player.target_z = first.z;
                        println!("[SERVER] Player {} initial target: ({:.2}, {:.2})", player.id, player.target_x, player.target_z);
                    }
                } else {
                    log_path_failure("player_move", start, goal, &result.diagnostics);
                    player.target_x = player.x;
                    player.target_z = player.z;
                    player.current_path.clear();
                }
            }
        }

        ClientMessage::CastSpell { spell, target_x, target_z } => {
            if let Some(player) = players.get_mut(&addr) {
                if player.is_dead { return; }
                player.last_heard = Instant::now();
                player.casting_timer = 0.5;
                player.anim_state = 4; // Spell cast
                let effect_id = *next_effect_id;
                *next_effect_id += 1;
                effects.push(ActiveEffect { id: effect_id, effect_type: spell, x: target_x, z: target_z, timer: 1.0 });

                // Authoritative Damage Application
                let (radius, damage) = match spell {
                    0 => (3.5, 20), // Q: Arrow Rain
                    1 => (1.5, 30), // W: Unit Target
                    2 => (1.5, 20), // E: Unit Target
                    3 => (1.5, 40), // R: Unit Target
                    _ => (0.0, 0),
                };

                if damage > 0 {
                    let mut hits = 0;
                    for enemy in enemies.values_mut() {
                        if enemy.health <= 0 { continue; }
                        let dx = enemy.x - target_x;
                        let dz = enemy.z - target_z;
                        let dist = (dx * dx + dz * dz).sqrt();
                        if dist <= radius {
                            enemy.health -= damage;
                            enemy.hurt_timer = 0.3;
                            hits += 1;
                            
                            // Apply pushback away from target point
                            let to_enemy_x = enemy.x - target_x;
                            let to_enemy_z = enemy.z - target_z;
                            let len = (to_enemy_x * to_enemy_x + to_enemy_z * to_enemy_z).sqrt();
                            if len > 0.01 {
                                let push_dist = 0.5;
                                enemy.x += (to_enemy_x / len) * push_dist;
                                enemy.z += (to_enemy_z / len) * push_dist;
                                enemy.target_x = enemy.x;
                                enemy.target_z = enemy.z;
                                enemy.current_path.clear();
                            }
                            
                            if enemy.health <= 0 {
                                enemy.health = 0;
                                enemy.anim_state = 7; // Death animation
                            }
                        }
                    }
                    if hits > 0 {
                        println!("[SERVER] Spell {} cast by player {} hit {} enemies (Target: {:.1}, {:.1})", spell, player.id, hits, target_x, target_z);
                    }
                }

                if spell == 99 {
                    // Dev spell: Kill all enemies
                    for enemy in enemies.values_mut() {
                        enemy.health = 0;
                    }
                }
            }
        }

        ClientMessage::Ping { client_time } => {
            if let Some(player) = players.get_mut(&addr) {
                player.last_heard = Instant::now();
                let pong = ServerMessage::Pong { client_time, server_time: 0.0 };
                let packet = encode_server_message(&pong);
                let _ = socket.send_to(&packet, addr);
            }
        }

        ClientMessage::Reconnect { session_token } => {
            if let Some(&original_addr) = sessions.get(&session_token) {
                if let Some(mut player) = players.remove(&original_addr) {
                    player.addr = addr;
                    player.last_heard = Instant::now();
                    let your_id = player.id;
                    players.insert(addr, player);
                    sessions.insert(session_token, addr);
                    let accept = encode_server_message(&ServerMessage::JoinAccepted { your_id, session_token });
                    let _ = socket.send_to(&accept, addr);
                    for existing in players.values() {
                        let msg = encode_server_message(&ServerMessage::PlayerJoined { id: existing.id, name: existing.name.clone(), appearance: existing.appearance.clone() });
                        let _ = socket.send_to(&msg, addr);
                    }
                }
            }
        }

        ClientMessage::Disconnect => {
            if let Some(player) = players.remove(&addr) {
                db_save_player(db, &player);
                sessions.remove(&player.session_token);
                broadcast(socket, players, &ServerMessage::PlayerLeft { id: player.id });
            }
            pending_clients.remove(&addr);
        }

        ClientMessage::DebugToggleGodMode => {
            if let Some(player) = players.get_mut(&addr) {
                player.is_invulnerable = !player.is_invulnerable;
            }
        }

        ClientMessage::DebugForceSpawn => {
            if !ENEMY_SPAWNING_ENABLED {
                return;
            }
            if let Some(player) = players.get(&addr) {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let angle = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
                let sx = player.x + angle.cos() * 5.0;
                let sz = player.z + angle.sin() * 5.0;
                enemies.insert(*next_enemy_id, ServerEnemy {
                    id: *next_enemy_id, race_idx: rng.gen_range(0..2) as usize, x: sx, z: sz, target_x: sx, target_z: sz, direction: 0, anim_state: 0, health: 100, max_health: 100,
                    current_path: Vec::new(), next_path_recalc: 0.0, death_timer: 0.0, hurt_timer: 0.0, attack_timer: 0.0, spawn_x: sx, spawn_z: sz,
                });
                *next_enemy_id += 1;
            }
        }
    }
}

fn broadcast(socket: &UdpSocket, players: &HashMap<SocketAddr, ServerPlayer>, msg: &ServerMessage) {
    let packet = encode_server_message(msg);
    for p in players.values() { let _ = socket.send_to(&packet, p.addr); }
}

// ─── DB HELPERS ───

fn db_connect() -> PgClient {
    let conn_str = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "host=localhost user=rpg password=rpg_pass dbname=rpg_engine".to_string());
    PgClient::connect(&conn_str, NoTls).expect("Failed to connect to PostgreSQL")
}

fn db_run_migrations(db: &mut PgClient) {
    let sql = include_str!("../../migrations/001_init.sql");
    db.batch_execute(sql).expect("Failed to run migrations");
    println!("[DB] Migrations applied.");
}

fn db_get_or_create_account(db: &mut PgClient, username: &str) -> i32 {
    let row = db.query_one("INSERT INTO accounts (username) VALUES ($1) ON CONFLICT (username) DO UPDATE SET username = EXCLUDED.username RETURNING id", &[&username]).unwrap();
    row.get(0)
}
fn db_list_characters(db: &mut PgClient, account_id: i32) -> Vec<CharacterSummaryNet> {
    let rows = db.query("SELECT id, name, appearance, current_hp, max_hp, current_mp, max_mp FROM characters WHERE account_id = $1 ORDER BY created_at DESC", &[&account_id]).unwrap();
    rows.into_iter().map(|r| {
        let app_val: serde_json::Value = r.get(2);
        let appearance: CharacterAppearanceNet = serde_json::from_value(app_val).unwrap_or_else(|_| CharacterAppearanceNet {
            skin: "human".to_string(), shoes: None, clothes: None, gloves: None, hairstyle: None, facial_hair: None, eye_color: None, eyelashes: None, headgear: None, addon: None
        });
        CharacterSummaryNet {
            id: r.get(0), name: r.get(1),
            appearance,
            current_hp: r.get(3), max_hp: r.get(4), current_mp: r.get(5), max_mp: r.get(6),
        }
    }).collect()
}

fn db_load_character(db: &mut PgClient, account_id: i32, char_id: i32) -> Option<(String, CharacterAppearanceNet, f32, f32, i32, i32, i32, i32, bool)> {
    let row = db.query_opt("SELECT name, appearance, x, z, current_hp, max_hp, current_mp, max_mp, is_dead FROM characters WHERE account_id = $1 AND id = $2", &[&account_id, &char_id]).unwrap()?;
    let app_val: serde_json::Value = row.get(1);
    let appearance: CharacterAppearanceNet = serde_json::from_value(app_val).unwrap_or_else(|_| CharacterAppearanceNet {
        skin: "human".to_string(), shoes: None, clothes: None, gloves: None, hairstyle: None, facial_hair: None, eye_color: None, eyelashes: None, headgear: None, addon: None
    });
    Some((row.get(0), appearance, row.get(2), row.get(3), row.get(4), row.get(5), row.get(6), row.get(7), row.get(8)))
}

fn db_save_player(db: &mut PgClient, p: &ServerPlayer) {
    db.execute("UPDATE characters SET x = $1, z = $2, current_hp = $3, current_mp = $4, is_dead = $5 WHERE id = $6", &[&p.x, &p.z, &p.current_hp, &p.current_mp, &p.is_dead, &p.character_id]).unwrap();
}

fn db_create_character(db: &mut PgClient, account_id: i32, name: &str, app: &CharacterAppearanceNet) -> i32 {
    let app_val = serde_json::to_value(app).unwrap();
    let row = db.query_one("INSERT INTO characters (account_id, name, appearance, x, z) VALUES ($1, $2, $3, 20.0, 20.0) RETURNING id",
        &[&account_id, &name, &app_val]).unwrap();
    row.get(0)
}

fn db_delete_character(db: &mut PgClient, account_id: i32, char_id: i32) {
    db.execute("DELETE FROM characters WHERE account_id = $1 AND id = $2", &[&account_id, &char_id]).unwrap();
}
