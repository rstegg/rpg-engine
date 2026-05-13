//! RPG Engine — Headless Authoritative Server
//!
//! Run with: cargo run --bin server
//! Or with a custom port: cargo run --bin server -- 7878

use rpg_engine::net::protocol::*;

use rpg_engine::world::environment::{WorldSimulation, HitboxConfig, builtin_template_defs, load_glb_template_sync};
use rpg_engine::world::pathfinding::{find_path, is_walkable_with_radius, slide_move};
use macroquad::math::{Vec3, vec3};

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use postgres::{Client as PgClient, NoTls};

/// Autosave interval — WoW-style: save periodically + on disconnect.
const AUTOSAVE_INTERVAL_SECS: f64 = 60.0;

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
    // ─── Persistence ───
    account_id: i32,
    character_id: i32,
    session_token: u64,
}

/// Tracks a client that has logged in but hasn't selected a character yet.
struct PendingClient {
    addr: SocketAddr,
    account_id: i32,
    username: String,
    last_heard: Instant,
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
    death_timer: f32,
    hurt_timer: f32,
    attack_timer: f32,
}

// ─── Database Helpers ───

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
    // Try insert, on conflict do nothing, then select
    db.execute(
        "INSERT INTO accounts (username) VALUES ($1) ON CONFLICT (username) DO NOTHING",
        &[&username],
    ).unwrap();
    let row = db.query_one("SELECT id FROM accounts WHERE username = $1", &[&username]).unwrap();
    row.get(0)
}

fn db_list_characters(db: &mut PgClient, account_id: i32) -> Vec<CharacterSummaryNet> {
    let rows = db.query(
        "SELECT id, name, appearance, current_hp, max_hp, current_mp, max_mp FROM characters WHERE account_id = $1 ORDER BY last_login DESC",
        &[&account_id],
    ).unwrap();
    rows.iter().map(|r| {
        let appearance_json: serde_json::Value = r.get(2);
        let appearance: CharacterAppearanceNet = serde_json::from_value(appearance_json).unwrap_or_else(|_| CharacterAppearanceNet {
            skin: "Human1".into(), shoes: None, clothes: None, gloves: None,
            hairstyle: None, facial_hair: None, eye_color: None, eyelashes: None,
            headgear: None, addon: None,
        });
        CharacterSummaryNet {
            id: r.get(0),
            name: r.get(1),
            appearance,
            current_hp: r.get(3),
            max_hp: r.get(4),
            current_mp: r.get(5),
            max_mp: r.get(6),
        }
    }).collect()
}

fn db_create_character(db: &mut PgClient, account_id: i32, name: &str, appearance: &CharacterAppearanceNet) -> Result<CharacterSummaryNet, String> {
    // Check limit
    let count: i64 = db.query_one("SELECT COUNT(*) FROM characters WHERE account_id = $1", &[&account_id])
        .map_err(|e| e.to_string())?.get(0);
    if count >= MAX_CHARACTERS_PER_ACCOUNT as i64 {
        return Err(format!("Maximum {} characters per account", MAX_CHARACTERS_PER_ACCOUNT));
    }
    let app_json = serde_json::to_value(appearance).map_err(|e| e.to_string())?;
    let row = db.query_one(
        "INSERT INTO characters (account_id, name, appearance) VALUES ($1, $2, $3) RETURNING id, current_hp, max_hp, current_mp, max_mp",
        &[&account_id, &name, &app_json],
    ).map_err(|e| {
        eprintln!("[DB ERROR] Character creation failed: {:?}", e);
        if let Some(code) = e.code() {
            if code == &postgres::error::SqlState::UNIQUE_VIOLATION {
                return "A character with that name already exists".to_string();
            }
        }
        format!("Database error: {}", e)
    })?;
    Ok(CharacterSummaryNet {
        id: row.get(0),
        name: name.to_string(),
        appearance: appearance.clone(),
        current_hp: row.get(1),
        max_hp: row.get(2),
        current_mp: row.get(3),
        max_mp: row.get(4),
    })
}

fn db_delete_character(db: &mut PgClient, account_id: i32, character_id: i32) -> bool {
    let rows = db.execute(
        "DELETE FROM characters WHERE id = $1 AND account_id = $2",
        &[&character_id, &account_id],
    ).unwrap_or(0);
    rows > 0
}

/// Load a character's full data from DB. Returns (name, appearance, x, z, hp, max_hp, mp, max_mp, is_dead).
fn db_load_character(db: &mut PgClient, account_id: i32, character_id: i32) -> Option<(String, CharacterAppearanceNet, f32, f32, i32, i32, i32, i32, bool)> {
    let row = db.query_opt(
        "SELECT name, appearance, x, z, current_hp, max_hp, current_mp, max_mp, is_dead FROM characters WHERE id = $1 AND account_id = $2",
        &[&character_id, &account_id],
    ).ok()??;
    let app_json: serde_json::Value = row.get(1);
    let appearance: CharacterAppearanceNet = serde_json::from_value(app_json).ok()?;
    Some((row.get(0), appearance, row.get(2), row.get(3), row.get(4), row.get(5), row.get(6), row.get(7), row.get(8)))
}

fn db_save_player(db: &mut PgClient, p: &ServerPlayer) {
    let _ = db.execute(
        "UPDATE characters SET x = $1, z = $2, current_hp = $3, max_hp = $4, current_mp = $5, max_mp = $6, is_dead = $7, last_login = NOW() WHERE id = $8",
        &[&p.x, &p.z, &p.current_hp, &p.max_hp, &p.current_mp, &p.max_mp, &p.is_dead, &p.character_id],
    );
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

    // Connect to PostgreSQL
    println!("[SERVER] Connecting to database...");
    let mut db = db_connect();
    db_run_migrations(&mut db);
    println!("[SERVER] Database ready.");

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
    let mut pending_clients: HashMap<SocketAddr, PendingClient> = HashMap::new();
    let mut sessions: HashMap<u64, SocketAddr> = HashMap::new(); // session_token -> addr
    let mut enemies: HashMap<u64, ServerEnemy> = HashMap::new();
    let mut effects: Vec<ActiveEffect> = Vec::new();
    let mut next_player_id: PlayerId = 1;
    let mut next_enemy_id: u64 = 1;
    let mut next_effect_id: u64 = 1;
    let mut tick: u64 = 0;
    
    let mut enemy_spawn_timer: f32 = 0.0;
    let mut last_autosave = Instant::now();

    let tick_duration = Duration::from_millis(1000 / SERVER_TICK_RATE);
    let mut last_tick = Instant::now();

    let mut buf = [0u8; 65536];

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
                            &mut db,
                            &mut players,
                            &mut pending_clients,
                            &mut sessions,
                            &mut enemies,
                            &mut effects,
                            &mut next_player_id,
                            &mut next_enemy_id,
                            &mut next_effect_id,
                            &mut world_sim,
                        );
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                    // Ignore Windows UDP "os error 10054" (ICMP Port Unreachable)
                    continue;
                }
                Err(e) => {
                    eprintln!("[SERVER] Socket error: {}", e);
                    continue;
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
                        death_timer: 0.0,
                        hurt_timer: 0.0,
                        attack_timer: 0.0,
                    });
                    next_enemy_id += 1;
                }
            }

            // Update enemies
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
                        enemy.target_x = enemy.x;
                        enemy.target_z = enemy.z;
                        enemy.current_path.clear();
                        
                        if enemy.attack_timer <= 0.0 {
                            enemy.anim_state = 3; // Sword/Attack
                            enemy.attack_timer = 2.0; // Attack cooldown
                            let player_mut = players.get_mut(&addr).unwrap();
                            if !player_mut.is_invulnerable {
                                player_mut.current_hp -= 20;
                                if player_mut.current_hp <= 0 {
                                    player_mut.current_hp = 0;
                                    player_mut.is_dead = true;
                                }
                            }
                        } else if enemy.attack_timer <= 1.5 {
                            enemy.anim_state = 0; // Return to idle after animation
                        } else {
                            enemy.anim_state = 3; // Keep playing attack animation
                        }
                    } else {
                        // Chase
                        if enemy.next_path_recalc <= 0.0 {
                            enemy.next_path_recalc = 0.5; // Recalc every 500ms
                            if let Some(mut path) = find_path(
                                vec3(enemy.x, 0.0, enemy.z),
                                vec3(target.x, 0.0, target.z),
                                world_sim.grid_size,
                                world_sim.width,
                                world_sim.height,
                                &world_sim.walkability_grid,
                            ) {
                                if path.len() > 1 && (path[0] - vec3(enemy.x, 0.0, enemy.z)).length() < world_sim.grid_size {
                                    path.remove(0);
                                }
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
            
            // Clean up dead enemies after death animation finishes (e.g. 2.5 secs)
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

            // Timeout disconnected players (no packet in 10 seconds) — save to DB first
            let timed_out: Vec<SocketAddr> = players.iter()
                .filter(|(_, p)| p.last_heard.elapsed().as_secs() > 10)
                .map(|(addr, _)| *addr)
                .collect();

            for addr in timed_out {
                if let Some(player) = players.remove(&addr) {
                    println!("[SERVER] Player {} ({}) timed out — saving to DB", player.name, player.id);
                    db_save_player(&mut db, &player);
                    sessions.remove(&player.session_token);
                    broadcast(&socket, &players, &ServerMessage::PlayerLeft { id: player.id });
                }
            }

            // Timeout pending clients (no character selected in 30 seconds)
            pending_clients.retain(|_, pc| pc.last_heard.elapsed().as_secs() < 30);

            // ─── Periodic autosave (WoW-style, every 60s) ───
            if last_autosave.elapsed().as_secs_f64() > AUTOSAVE_INTERVAL_SECS {
                last_autosave = Instant::now();
                let count = players.len();
                if count > 0 {
                    for p in players.values() {
                        db_save_player(&mut db, p);
                    }
                    println!("[SERVER] Autosaved {} player(s) to DB", count);
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
    db: &mut PgClient,
    players: &mut HashMap<SocketAddr, ServerPlayer>,
    pending_clients: &mut HashMap<SocketAddr, PendingClient>,
    sessions: &mut HashMap<u64, SocketAddr>,
    enemies: &mut HashMap<u64, ServerEnemy>,
    effects: &mut Vec<ActiveEffect>,
    next_id: &mut PlayerId,
    next_enemy_id: &mut u64,
    next_effect_id: &mut u64,
    world_sim: &mut WorldSimulation,
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
            println!("[SERVER] Login from '{}' (account_id={}, {} chars)", username, account_id, characters.len());

            // Store as pending (not yet in world)
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
                let reject = encode_server_message(&ServerMessage::JoinRejected {
                    reason: "Server is full".to_string(),
                });
                let _ = socket.send_to(&reject, addr);
                pending_clients.insert(addr, pc); // put back
                return;
            }

            let char_data = match db_load_character(db, pc.account_id, character_id) {
                Some(data) => data,
                None => {
                    let reject = encode_server_message(&ServerMessage::JoinRejected {
                        reason: "Character not found".to_string(),
                    });
                    let _ = socket.send_to(&reject, addr);
                    pending_clients.insert(addr, pc);
                    return;
                }
            };

            let (name, appearance, x, z, hp, max_hp, mp, max_mp, is_dead) = char_data;

            let id = *next_id;
            *next_id += 1;

            // Generate session token
            let session_token: u64 = rand::random();

            // Send acceptance with session token
            let accept = encode_server_message(&ServerMessage::JoinAccepted { your_id: id, session_token });
            let _ = socket.send_to(&accept, addr);

            // Send map data
            send_map_data(socket, addr, world_sim);

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

            println!("[SERVER] Player '{}' (id={}) entered world from {} [{}/{}]",
                name, id, addr, players.len() + 1, MAX_PLAYERS);

            sessions.insert(session_token, addr);

            players.insert(addr, ServerPlayer {
                id,
                addr,
                name,
                appearance,
                x,
                z,
                target_x: x,
                target_z: z,
                direction: 0,
                anim_state: 0,
                anim_frame: 0.0,
                casting_timer: 0.0,
                last_heard: Instant::now(),
                current_hp: hp,
                max_hp,
                current_mp: mp,
                max_mp,
                is_dead,
                revive_timer: 0.0,
                is_invulnerable: false,
                account_id: pc.account_id,
                character_id,
                session_token,
            });
        }

        ClientMessage::CreateCharacter { name, appearance } => {
            let pc = match pending_clients.get_mut(&addr) {
                Some(pc) => { pc.last_heard = Instant::now(); pc.account_id },
                None => return,
            };

            match db_create_character(db, pc, &name, &appearance) {
                Ok(summary) => {
                    println!("[SERVER] Created character '{}' for account {}", name, pc);
                    let msg = encode_server_message(&ServerMessage::CharacterCreated { character: summary });
                    let _ = socket.send_to(&msg, addr);
                }
                Err(reason) => {
                    let msg = encode_server_message(&ServerMessage::CharacterCreateFailed { reason });
                    let _ = socket.send_to(&msg, addr);
                }
            }
        }

        ClientMessage::DeleteCharacter { character_id } => {
            let pc = match pending_clients.get_mut(&addr) {
                Some(pc) => { pc.last_heard = Instant::now(); pc.account_id },
                None => return,
            };

            if db_delete_character(db, pc, character_id) {
                println!("[SERVER] Deleted character id={} for account {}", character_id, pc);
                let msg = encode_server_message(&ServerMessage::CharacterDeleted { character_id });
                let _ = socket.send_to(&msg, addr);
            }
        }

        ClientMessage::Reconnect { session_token } => {
            // Find the old player by session token
            if let Some(old_addr) = sessions.remove(&session_token) {
                if let Some(mut player) = players.remove(&old_addr) {
                    println!("[SERVER] Player '{}' reconnected from {}", player.name, addr);
                    player.addr = addr;
                    player.last_heard = Instant::now();

                    // Send acceptance
                    let accept = encode_server_message(&ServerMessage::JoinAccepted {
                        your_id: player.id,
                        session_token,
                    });
                    let _ = socket.send_to(&accept, addr);

                    // Resend map data
                    send_map_data(socket, addr, world_sim);

                    // Resend existing player info
                    for existing in players.values() {
                        let msg = encode_server_message(&ServerMessage::PlayerJoined {
                            id: existing.id,
                            name: existing.name.clone(),
                            appearance: existing.appearance.clone(),
                        });
                        let _ = socket.send_to(&msg, addr);
                    }

                    sessions.insert(session_token, addr);
                    players.insert(addr, player);
                    return;
                }
            }

            // Session expired or invalid — reject, client should re-login
            let reject = encode_server_message(&ServerMessage::JoinRejected {
                reason: "Session expired — please login again".to_string(),
            });
            let _ = socket.send_to(&reject, addr);
        }

        ClientMessage::MoveTo { x, z } => {
            if let Some(player) = players.get_mut(&addr) {
                player.target_x = x;
                player.target_z = z;
                player.last_heard = Instant::now();
            }
        }

        ClientMessage::CastSpell { spell, target_x, target_z } => {
            if spell == 99 {
                println!("[SERVER] Player at {} cast KILL ALL", addr);
                for enemy in enemies.values_mut() {
                    enemy.health = 0;
                }
                return;
            }

            if let Some(player) = players.get_mut(&addr) {
                player.last_heard = Instant::now();
                player.target_x = player.x;
                player.target_z = player.z;
                player.casting_timer = 0.5;

                player.anim_state = match spell {
                    0 => 4,
                    1 => 3,
                    2 => 5,
                    3 => 9,
                    _ => 0,
                };

                let player_x = player.x;
                let player_z = player.z;

                let dx = target_x - player.x;
                let dz = target_z - player.z;
                let mut angle = f32::atan2(dx, dz);
                if angle < 0.0 { angle += std::f32::consts::PI * 2.0; }
                player.direction = ((angle / (std::f32::consts::PI / 4.0)).round() as u8) % 8;

                let duration = match spell {
                    0 => 1.0,
                    1 => 0.6,
                    2 => 0.7,
                    3 => 0.9,
                    _ => 0.5,
                };

                if spell == 0 {
                    for enemy in enemies.values_mut() {
                        let dx_e = enemy.x - target_x;
                        let dz_e = enemy.z - target_z;
                        let dist = (dx_e * dx_e + dz_e * dz_e).sqrt();
                        if dist <= 3.0 && enemy.health > 0 {
                            enemy.health = (enemy.health - 20).max(0);
                            if enemy.health <= 0 {
                                enemy.anim_state = 7;
                            } else {
                                enemy.anim_state = 6;
                                enemy.hurt_timer = 0.3;
                            }
                        }
                    }
                } else if spell == 1 || spell == 2 || spell == 3 {
                    let dmg = match spell { 1 => 30, 2 => 20, 3 => 40, _ => 0 };
                    let mut closest_enemy = None;
                    let mut min_dist = 1.5;
                    for enemy in enemies.values_mut() {
                        if enemy.health > 0 {
                            let dx_e = enemy.x - target_x;
                            let dz_e = enemy.z - target_z;
                            let dist = (dx_e * dx_e + dz_e * dz_e).sqrt();
                            if dist < min_dist {
                                min_dist = dist;
                                closest_enemy = Some(enemy);
                            }
                        }
                    }
                    if let Some(enemy) = closest_enemy {
                        enemy.health = (enemy.health - dmg).max(0);
                        if enemy.health <= 0 {
                            enemy.anim_state = 7;
                        } else {
                            enemy.anim_state = 6;
                            enemy.hurt_timer = 0.3;
                            let dx_e = enemy.x - player_x;
                            let dz_e = enemy.z - player_z;
                            let dist = (dx_e * dx_e + dz_e * dz_e).sqrt();
                            if dist > 0.1 {
                                enemy.x += (dx_e / dist) * 0.5;
                                enemy.z += (dz_e / dist) * 0.5;
                            }
                        }
                    }
                }

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
            // Update last_heard for pending clients too
            if let Some(pc) = pending_clients.get_mut(&addr) {
                pc.last_heard = Instant::now();
            }
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
                println!("[SERVER] Player {} ({}) disconnected — saving to DB", player.name, player.id);
                db_save_player(db, &player);
                sessions.remove(&player.session_token);
                broadcast(socket, players, &ServerMessage::PlayerLeft { id: player.id });
            }
            pending_clients.remove(&addr);
        }

        ClientMessage::DebugToggleGodMode => {
            if let Some(player) = players.get_mut(&addr) {
                player.is_invulnerable = !player.is_invulnerable;
                println!("[SERVER] Player {} god mode: {}", player.name, player.is_invulnerable);
            }
        }

        ClientMessage::DebugForceSpawn => {
            if let Some(player) = players.get(&addr) {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let angle = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
                let spawn_x = player.x + angle.cos() * 5.0;
                let spawn_z = player.z + angle.sin() * 5.0;
                
                enemies.insert(*next_enemy_id, ServerEnemy {
                    id: *next_enemy_id,
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
                    death_timer: 0.0,
                    hurt_timer: 0.0,
                    attack_timer: 0.0,
                });
                println!("[SERVER] Forced spawn enemy ID {} near player {}", next_enemy_id, player.name);
                *next_enemy_id += 1;
            }
        }
    }
}

/// Send map data to a specific client.
fn send_map_data(socket: &UdpSocket, addr: SocketAddr, world_sim: &WorldSimulation) {
    let mut palette = Vec::new();
    let mut palette_map = HashMap::new();
    let net_placements: Vec<ModelPlacementNet> = world_sim.placements.iter().map(|p| {
        let key = (p.model.clone(), p.file.clone());
        let model_idx = *palette_map.entry(key.clone()).or_insert_with(|| {
            let idx = palette.len() as u16;
            palette.push(key);
            idx
        });
        ModelPlacementNet {
            model_idx,
            position: p.position,
            rotation: p.rotation,
            scale: p.scale,
            blocks_movement: p.blocks_movement,
        }
    }).collect();

    let map_msg = encode_server_message(&ServerMessage::MapData {
        palette,
        placements: net_placements,
    });
    let _ = socket.send_to(&map_msg, addr);
}

/// Send a message to all connected players.
fn broadcast(socket: &UdpSocket, players: &HashMap<SocketAddr, ServerPlayer>, msg: &ServerMessage) {
    let packet = encode_server_message(msg);
    for player in players.values() {
        let _ = socket.send_to(&packet, player.addr);
    }
}

