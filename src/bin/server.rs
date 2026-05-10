//! RPG Engine — Headless Authoritative Server
//!
//! Run with: cargo run --bin server
//! Or with a custom port: cargo run --bin server -- 7878

// We need to access the shared net module from the main crate
// Since this is a separate binary in the same crate, we use `rpg_engine::` path
use rpg_engine::net::protocol::*;

use rpg_engine::world::environment::{WorldSimulation, HitboxConfig, builtin_template_defs, load_glb_template_sync};
use rpg_engine::world::pathfinding::{find_path, is_walkable_with_radius, slide_move};
use macroquad::math::{Vec3, vec3};

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    // Stats
    current_hp: i32,
    max_hp: i32,
    current_mp: i32,
    max_mp: i32,
    is_dead: bool,
    revive_timer: f32,
}

/// Active spell effects in the world.
struct ActiveEffect {
    effect_id: u64,
    spell: u8,
    x: f32,
    z: f32,
    timer: f32,
    caster_id: PlayerId,
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
}

fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    let bind_addr = format!("0.0.0.0:{}", port);
    let socket = UdpSocket::bind(&bind_addr).expect("Failed to bind server socket");
    socket.set_nonblocking(true).expect("Failed to set nonblocking");

    println!("══════════════════════════════════════════");
    println!("  RPG ENGINE SERVER v{}", PROTOCOL_VERSION);
    println!("  Listening on {}", bind_addr);
    println!("  Max players: {}", MAX_PLAYERS);
    println!("  Tick rate: {} Hz", SERVER_TICK_RATE);
    println!("══════════════════════════════════════════");

    // Load hitbox config
    let hitbox_config: HitboxConfig = if let Ok(data) = std::fs::read_to_string("assets/world_models/hitboxes.json") {
        serde_json::from_str(&data).unwrap_or_else(|_| HitboxConfig::default())
    } else {
        HitboxConfig::default()
    };

    println!("[SERVER] Loading templates for simulation...");
    let mut templates = HashMap::new();
    let base = "assets/world_models/";
    let to_load = builtin_template_defs();
    for &(k, f) in &to_load {
        if let Some(t) = load_glb_template_sync(&format!("{base}{f}")) {
            templates.insert(k.to_string(), t);
        }
    }

    println!("[SERVER] Generating world grid...");
    let mut world_sim = WorldSimulation::build(40, 40, hitbox_config, true, templates);
    println!("[SERVER] World simulation ready ({} placements)", world_sim.placements.len());

    let mut players: HashMap<SocketAddr, ServerPlayer> = HashMap::new();
    let mut enemies: HashMap<u64, ServerEnemy> = HashMap::new();
    let mut effects: Vec<ActiveEffect> = Vec::new();
    let mut next_player_id: PlayerId = 1;
    let mut next_enemy_id: u64 = 1;
    let mut next_effect_id: u64 = 1;
    let mut tick: u64 = 0;
    
    let mut enemy_spawn_timer: f32 = 0.0;

    let tick_duration = Duration::from_millis(1000 / SERVER_TICK_RATE);
    let mut last_tick = Instant::now();

    let mut buf = [0u8; 4096];

    loop {
        // ─── 1. Receive all pending client packets ───
        loop {
            match socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    if let Some(PacketPayload::Client(msg)) = decode_packet(&buf[..len]) {
                        handle_client_message(
                            &socket,
                            addr,
                            msg,
                            &mut players,
                            &mut effects,
                            &mut next_player_id,
                            &mut next_effect_id,
                            &mut world_sim,
                        );
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("[SERVER] Socket error: {}", e);
                    break;
                }
            }
        }

        // ─── 2. Tick at fixed rate ───
        if last_tick.elapsed() >= tick_duration {
            last_tick = Instant::now();
            tick += 1;

            let dt = 1.0 / SERVER_TICK_RATE as f32;

            // Update player movement (server-authoritative)
            for player in players.values_mut() {
                if player.is_dead {
                    player.anim_state = 2; // Death animation (matches AnimationState::Death)
                    continue;
                }

                if player.casting_timer > 0.0 {
                    player.casting_timer -= dt;
                    if player.casting_timer <= 0.0 {
                        player.anim_state = 0; // Reset to Idle when cast finished
                    }
                    continue;
                }

                let dx = player.target_x - player.x;
                let dz = player.target_z - player.z;
                let dist = (dx * dx + dz * dz).sqrt();

                if dist > 0.05 {
                    let speed = 5.25; // Base movement speed
                    let move_dist = speed * dt;
                    
                    let current_pos = vec3(player.x, 0.0, player.z);
                    let dir = vec3(dx / dist, 0.0, dz / dist);
                    let desired_pos = current_pos + dir * move_dist.min(dist);

                    let new_pos = slide_move(
                        current_pos,
                        desired_pos,
                        0.35, // PLAYER_RADIUS
                        world_sim.grid_size,
                        world_sim.width,
                        world_sim.height,
                        &world_sim.walkability_grid,
                    );

                    player.x = new_pos.x;
                    player.z = new_pos.z;
                    
                    if dist <= move_dist {
                        player.anim_state = 0; // Idle
                    } else {
                        player.anim_state = 1; // Walk

                        // Calculate direction
                        let mut angle = f32::atan2(dx, dz);
                        if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                        player.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                    }
                } else {
                    player.anim_state = 0; // Idle
                }
            }

            // Spawn enemies
            enemy_spawn_timer -= dt;
            if enemy_spawn_timer <= 0.0 && enemies.len() < 10 && !players.is_empty() {
                enemy_spawn_timer = 5.0;
                
                // Pick a random player
                let random_player = players.values().next().unwrap(); // Just take the first one for now
                let px = random_player.x;
                let pz = random_player.z;
                
                // Spawn 10 units away roughly
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let angle = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
                let spawn_x = px + angle.cos() * 12.0;
                let spawn_z = pz + angle.sin() * 12.0;
                
                if is_walkable_with_radius(
                    vec3(spawn_x, 0.0, spawn_z),
                    0.35,
                    world_sim.grid_size,
                    world_sim.width,
                    world_sim.height,
                    &world_sim.walkability_grid,
                ) {
                    enemies.insert(next_enemy_id, ServerEnemy {
                        id: next_enemy_id,
                        race_idx: rng.gen_range(0..2),
                        x: spawn_x,
                        z: spawn_z,
                        target_x: spawn_x,
                        target_z: spawn_z,
                        direction: 0,
                        anim_state: 0,
                        health: 100,
                        max_health: 100,
                        current_path: Vec::new(),
                        next_path_recalc: 0.0,
                    });
                    next_enemy_id += 1;
                }
            }

            // Update enemies
            for enemy in enemies.values_mut() {
                if enemy.health <= 0 { continue; }
                
                enemy.next_path_recalc -= dt;
                
                // Find nearest player (who is alive)
                let mut nearest_player_id: Option<SocketAddr> = None;
                let mut nearest_dist = f32::MAX;
                for (addr, player) in &players {
                    if player.is_dead { continue; }
                    let dx = player.x - enemy.x;
                    let dz = player.z - enemy.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    if dist < nearest_dist {
                        nearest_dist = dist;
                        nearest_player_id = Some(*addr);
                    }
                }
                
                if let Some(addr) = nearest_player_id {
                    let target = players.get(&addr).unwrap();
                    // Attack range check
                    if nearest_dist <= 1.5 {
                        enemy.anim_state = 3; // Sword/Attack
                        enemy.target_x = enemy.x;
                        enemy.target_z = enemy.z;
                        enemy.current_path.clear();
                        
                        // Deal damage periodically (e.g. every 1 second during attack)
                        // For simplicity, we'll just deal a tiny bit every tick
                        let player_mut = players.get_mut(&addr).unwrap();
                        player_mut.current_hp -= (20.0 * dt) as i32;
                        if player_mut.current_hp <= 0 {
                            player_mut.current_hp = 0;
                            player_mut.is_dead = true;
                        }
                    } else {
                        // Chase
                        if enemy.next_path_recalc <= 0.0 {
                            enemy.next_path_recalc = 0.5; // Recalc every 500ms
                            if let Some(path) = find_path(
                                vec3(enemy.x, 0.0, enemy.z),
                                vec3(target.x, 0.0, target.z),
                                world_sim.grid_size,
                                world_sim.width,
                                world_sim.height,
                                &world_sim.walkability_grid,
                            ) {
                                enemy.current_path = path;
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
                                if move_dist >= dist {
                                    enemy.x = enemy.target_x;
                                    enemy.z = enemy.target_z;
                                    enemy.current_path.remove(0);
                                } else {
                                    enemy.x += (dx / dist) * move_dist;
                                    enemy.z += (dz / dist) * move_dist;
                                }
                                enemy.anim_state = 1; // Walk
                                
                                let mut angle = f32::atan2(dx, dz);
                                if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                                enemy.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;
                            } else {
                                enemy.current_path.remove(0);
                            }
                        } else {
                            enemy.anim_state = 0; // Idle
                        }
                    }
                } else {
                    enemy.anim_state = 0; // Idle
                }
            }

            // Update effects
            for effect in &mut effects {
                effect.timer -= dt;
            }
            effects.retain(|e| e.timer > 0.0);

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
                
                let player_dead = players.get_mut(&id_dead).unwrap();
                if reviver_present {
                    player_dead.revive_timer += dt;
                    if player_dead.revive_timer >= 5.0 {
                        player_dead.is_dead = false;
                        player_dead.current_hp = player_dead.max_hp / 2;
                        player_dead.revive_timer = 0.0;
                    }
                } else {
                    player_dead.revive_timer = (player_dead.revive_timer - dt).max(0.0);
                }
            }

            // Game Over Check
            if !players.is_empty() && players.values().all(|p| p.is_dead) {
                broadcast(&socket, &players, &ServerMessage::GameOver);
            }

            // Timeout disconnected players (no packet in 10 seconds)
            let timed_out: Vec<SocketAddr> = players.iter()
                .filter(|(_, p)| p.last_heard.elapsed().as_secs() > 10)
                .map(|(addr, _)| *addr)
                .collect();

            for addr in timed_out {
                if let Some(player) = players.remove(&addr) {
                    println!("[SERVER] Player {} ({}) timed out", player.name, player.id);
                    broadcast(&socket, &players, &ServerMessage::PlayerLeft { id: player.id });
                }
            }

            // ─── 3. Broadcast world state ───
            let server_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();

            let player_states: Vec<PlayerState> = players.values().map(|p| PlayerState {
                id: p.id,
                x: p.x,
                z: p.z,
                target_x: p.target_x,
                target_z: p.target_z,
                direction: p.direction,
                anim_state: p.anim_state,
                anim_frame: p.anim_frame,
                casting_timer: p.casting_timer,
                current_hp: p.current_hp,
                max_hp: p.max_hp,
                current_mp: p.current_mp,
                max_mp: p.max_mp,
                is_dead: p.is_dead,
                revive_progress: (p.revive_timer / 5.0).clamp(0.0, 1.0),
            }).collect();

            let effect_states: Vec<EffectState> = effects.iter().map(|e| EffectState {
                effect_id: e.effect_id,
                spell: e.spell,
                x: e.x,
                z: e.z,
                timer: e.timer,
                caster_id: e.caster_id,
            }).collect();

            let enemy_states: Vec<EnemyStateNet> = enemies.values().map(|e| EnemyStateNet {
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
            }).collect();

            let world_msg = ServerMessage::WorldState {
                tick,
                server_time,
                players: player_states,
                enemies: enemy_states,
                effects: effect_states,
            };

            let packet = encode_server_message(&world_msg);
            for player in players.values() {
                let _ = socket.send_to(&packet, player.addr);
            }
        }

        // Sleep a tiny bit to avoid burning CPU
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn handle_client_message(
    socket: &UdpSocket,
    addr: SocketAddr,
    msg: ClientMessage,
    players: &mut HashMap<SocketAddr, ServerPlayer>,
    effects: &mut Vec<ActiveEffect>,
    next_id: &mut PlayerId,
    next_effect_id: &mut u64,
    world_sim: &mut WorldSimulation,
) {
    match msg {
        ClientMessage::Join { version, name, appearance } => {
            if version != PROTOCOL_VERSION {
                let reject = encode_server_message(&ServerMessage::JoinRejected {
                    reason: format!("Version mismatch: server={}, client={}", PROTOCOL_VERSION, version),
                });
                let _ = socket.send_to(&reject, addr);
                return;
            }

            if players.len() >= MAX_PLAYERS {
                let reject = encode_server_message(&ServerMessage::JoinRejected {
                    reason: "Server is full".to_string(),
                });
                let _ = socket.send_to(&reject, addr);
                return;
            }

            // Assign ID
            let id = *next_id;
            *next_id += 1;

            // Notify the new player of acceptance
            let accept = encode_server_message(&ServerMessage::JoinAccepted { your_id: id });
            let _ = socket.send_to(&accept, addr);

            // Send map data
            let map_msg = encode_server_message(&ServerMessage::MapData {
                placements: world_sim.placements.iter().map(|p| ModelPlacementNet {
                    model: p.model.clone(),
                    file: p.file.clone(),
                    position: p.position,
                    rotation: p.rotation,
                    scale: p.scale,
                    blocks_movement: p.blocks_movement,
                }).collect(),
            });
            let _ = socket.send_to(&map_msg, addr);

            // Tell the new player about existing players
            for existing in players.values() {
                let msg = encode_server_message(&ServerMessage::PlayerJoined {
                    id: existing.id,
                    name: existing.name.clone(),
                    appearance: existing.appearance.clone(),
                });
                let _ = socket.send_to(&msg, addr);
            }

            // Tell existing players about the new player
            broadcast(socket, players, &ServerMessage::PlayerJoined {
                id,
                name: name.clone(),
                appearance: appearance.clone(),
            });

            println!("[SERVER] Player {} ({}) joined from {} [{}/{}]",
                name, id, addr, players.len() + 1, MAX_PLAYERS);

            players.insert(addr, ServerPlayer {
                id,
                addr,
                name,
                appearance,
                x: 0.0,
                z: 0.0,
                target_x: 0.0,
                target_z: 0.0,
                direction: 0,
                anim_state: 0,
                anim_frame: 0.0,
                casting_timer: 0.0,
                last_heard: Instant::now(),
                current_hp: 100,
                max_hp: 100,
                current_mp: 100,
                max_mp: 100,
                is_dead: false,
                revive_timer: 0.0,
            });
        }

        ClientMessage::MoveTo { x, z } => {
            if let Some(player) = players.get_mut(&addr) {
                player.target_x = x;
                player.target_z = z;
                player.last_heard = Instant::now();
            }
        }

        ClientMessage::CastSpell { spell, target_x, target_z } => {
            if let Some(player) = players.get_mut(&addr) {
                player.last_heard = Instant::now();
                player.target_x = player.x; // Stop moving
                player.target_z = player.z;
                player.casting_timer = 0.5;

                // Set animation based on spell
                player.anim_state = match spell {
                    0 => 4,  // Q = Bow
                    1 => 3,  // W = Sword
                    2 => 5,  // E = Staff
                    3 => 9,  // R = CarryIdle
                    _ => 0,
                };

                // Calculate direction to target
                let dx = target_x - player.x;
                let dz = target_z - player.z;
                let mut angle = f32::atan2(dx, dz);
                if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                player.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;

                // Spawn effect
                let duration = match spell {
                    0 => 1.0,  // Arrow rain
                    1 => 0.6,  // Power strike
                    2 => 0.7,  // Fire claw
                    3 => 0.9,  // Dark void
                    _ => 0.5,
                };

                effects.push(ActiveEffect {
                    effect_id: *next_effect_id,
                    spell,
                    x: target_x,
                    z: target_z,
                    timer: duration,
                    caster_id: player.id,
                });
                *next_effect_id += 1;

                println!("[SERVER] Player {} cast spell {} at ({:.1}, {:.1})",
                    player.name, spell, target_x, target_z);
            }
        }

        ClientMessage::Ping { client_time } => {
            if let Some(player) = players.get_mut(&addr) {
                player.last_heard = Instant::now();
                let server_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64();
                let pong = encode_server_message(&ServerMessage::Pong { client_time, server_time });
                let _ = socket.send_to(&pong, addr);
            }
        }

        ClientMessage::Disconnect => {
            if let Some(player) = players.remove(&addr) {
                println!("[SERVER] Player {} ({}) disconnected", player.name, player.id);
                broadcast(socket, players, &ServerMessage::PlayerLeft { id: player.id });
            }
        }
    }
}

/// Send a message to all connected players.
fn broadcast(socket: &UdpSocket, players: &HashMap<SocketAddr, ServerPlayer>, msg: &ServerMessage) {
    let packet = encode_server_message(msg);
    for player in players.values() {
        let _ = socket.send_to(&packet, player.addr);
    }
}
