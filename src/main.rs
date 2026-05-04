mod core;
mod entities;
mod systems;
mod ui;
mod net;
mod world;

use macroquad::prelude::*;
use core::animation::*;
use entities::player::*;
use entities::character::*;
use core::camera::*;
use systems::input::*;
use systems::indicators::*;
use entities::effects::*;
use ui::character_creator::*;
use net::client::NetClient;
use net::protocol::*;

pub struct Assets {
    pub icon_q: Texture2D,
    pub icon_w: Texture2D,
    pub icon_e: Texture2D,
    pub icon_r: Texture2D,
    pub spell_q: Texture2D,
    pub spell_w: Texture2D,
    pub spell_e: Texture2D,
    pub spell_r: Texture2D,
    pub dummy: Texture2D,
    pub target_mouse: Texture2D,
}

#[derive(PartialEq)]
enum GameState {
    CharacterCreation,
    Connecting,
    Playing,
    HitboxCalibration,
}

async fn load_or_fallback(path: &str, color: Color) -> Texture2D {
    let tex = load_texture(path).await.unwrap_or_else(|_| {
        let mut bytes: Vec<u8> = Vec::with_capacity(64 * 64 * 4);
        for _ in 0..(64*64) {
            bytes.push((color.r * 255.0) as u8);
            bytes.push((color.g * 255.0) as u8);
            bytes.push((color.b * 255.0) as u8);
            bytes.push((color.a * 255.0) as u8);
        }
        Texture2D::from_rgba8(64, 64, &bytes)
    });
    tex.set_filter(FilterMode::Nearest);
    tex
}

#[macroquad::main("2.5D Indie RPG")]
async fn main() {
    let assets = Assets {
        icon_q: load_or_fallback("assets/ui/arrow-rain-icon.png", BLUE).await,
        icon_w: load_or_fallback("assets/ui/power-strike-icon.png", RED).await,
        icon_e: load_or_fallback("assets/ui/fire-claw-icon.png", ORANGE).await,
        icon_r: load_or_fallback("assets/ui/dark-void-icon.png", PURPLE).await,
        spell_q: load_or_fallback("assets/effects/arrow-projectile.png", BLUE).await,
        spell_w: load_or_fallback("assets/effects/power-strike-spell.png", RED).await,
        spell_e: load_or_fallback("assets/effects/fire-claw-spell.png", ORANGE).await,
        spell_r: load_or_fallback("assets/effects/dark-void-spell.png", PURPLE).await,
        dummy: load_or_fallback("assets/characters/layers/skins/Cyclops1.png", GRAY).await,
        target_mouse: load_or_fallback("assets/ui/target-mouse.png", RED).await,
    };

    // Discover all available character layers
    let catalog = LayerCatalog::discover();

    // Try to load saved character, or start with default
    let initial_appearance = CharacterAppearance::load_from_file("character.json")
        .unwrap_or_else(|_| CharacterAppearance::default_human());

    let mut creator = CharacterCreator::new(catalog.clone());
    creator.appearance = initial_appearance.clone();
    // Sync indices from loaded appearance
    sync_creator_indices(&mut creator);

    let config = SpriteSheetConfig {
        columns: 29,
        rows: 8,
    };

    let mut ogre_anim_timer = 0.0;
    let mut char_textures = CharacterTextures::from_appearance(&creator.appearance, &catalog).await;

    let mut hero = Hero {
        pos: vec3(0.0, 0.0, 0.0),
        target_pos: vec3(0.0, 0.0, 0.0),
        current_path: Vec::new(),
        stats: Stats::new(10, 15, 10),
        anim: AnimationManager::new(config),
        targeting_state: TargetingState::None,
        casting_timer: 0.0,
        stuck_timer: 0.0,
    };

    let mut hitbox_config: std::collections::HashMap<String, f32> = match std::fs::read_to_string("hitbox_config.json") {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => std::collections::HashMap::new(),
    };

    let world_env = world::environment::WorldEnvironment::new(20, 20, hitbox_config.clone()).await;
    let mut debug_pathfinding = false;

    let mut game_camera = GameCamera::new(hero.pos);
    let mut effect_manager = EffectManager::new();
    let mut indicator_manager = IndicatorManager::new();

    // Spawn some test dummies
    let dummies = vec![
        vec3(5.0, 0.0, 5.0),
        vec3(-5.0, 0.0, 3.0),
        vec3(2.0, 0.0, -6.0),
    ];

    let mut game_state = GameState::CharacterCreation;
    let mut net_client: Option<NetClient> = None;
    let mut server_addr = String::from("127.0.0.1:7878");
    let mut remote_textures: std::collections::HashMap<u64, Vec<Texture2D>> = std::collections::HashMap::new();
    let mut remote_anims: std::collections::HashMap<u64, AnimationManager> = std::collections::HashMap::new();
    let mut spawned_effect_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Calibration state
    let mut calibration_selected_idx = 0;
    // Ensure all templates have an entry in hitbox_config
    for key in world_env.templates.keys() {
        hitbox_config.entry(key.clone()).or_insert(1.0);
    }

    loop {
        clear_background(DARKGRAY);
        let delta_time = get_frame_time().min(0.05);

        match game_state {
            GameState::CharacterCreation => {
                if creator.needs_reload {
                    creator.preview_textures = CharacterTextures::from_appearance(&creator.appearance, &catalog).await.layers;
                    creator.needs_reload = false;
                }
                let confirmed = creator.update_and_draw(delta_time);
                if confirmed {
                    char_textures = CharacterTextures::from_appearance(&creator.appearance, &catalog).await;
                    game_state = GameState::Playing; // Skip connecting for now to test world
                }
            }
            GameState::HitboxCalibration => {
                // Clear and set 3D view
                clear_background(BLACK);
                
                let keys: Vec<String> = {
                    let mut k: Vec<String> = world_env.templates.keys().cloned().collect();
                    k.sort();
                    k
                };
                if calibration_selected_idx >= keys.len() { calibration_selected_idx = 0; }
                let current_key = keys[calibration_selected_idx].clone();
                
                // Adjust multiplier
                {
                    let mult = hitbox_config.entry(current_key.clone()).or_insert(1.0);
                    if is_key_pressed(KeyCode::Down) { calibration_selected_idx = (calibration_selected_idx + 1) % keys.len(); }
                    if is_key_pressed(KeyCode::Up) { calibration_selected_idx = (calibration_selected_idx + keys.len() - 1) % keys.len(); }
                    if is_key_down(KeyCode::Right) { *mult += 0.01; }
                    if is_key_down(KeyCode::Left) { *mult = (*mult - 0.01).max(0.01); }
                }

                let current_mult = *hitbox_config.get(&current_key).unwrap();

                if is_key_pressed(KeyCode::S) {
                    let json = serde_json::to_string_pretty(&hitbox_config).unwrap();
                    println!("--- HITBOX CONFIG EXPORT ---\n{}\n---------------------------", json);
                }
                if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F2) {
                    game_state = GameState::Playing;
                }

                // Render selected model in center
                let camera_pos = vec3(0.0, 5.0, 10.0);
                let target = vec3(0.0, 0.0, 0.0);
                set_camera(&Camera3D {
                    position: camera_pos,
                    target: target,
                    up: vec3(0.0, 1.0, 0.0),
                    fovy: 45.0,
                    ..Default::default()
                });

                draw_grid(10, 2.0, GRAY, DARKGRAY);

                if let Some(t) = world_env.templates.get(&current_key) {
                    // Draw the model
                    let rot = (get_time() * 0.5) as f32;
                    let meshes = world::environment::instantiate(t, vec3(0.0, 0.0, 0.0), rot, 2.0);
                    for m in meshes { draw_mesh(&m); }

                    // Draw the hitbox (red squares)
                    let radius = t.footprint_radius * 2.0 * current_mult;
                    let gs = world_env.grid_size;
                    let cell_radius = (radius / gs).ceil() as i32;
                    for dx in -cell_radius..=cell_radius {
                        for dz in -cell_radius..=cell_radius {
                            let wx = dx as f32 * gs;
                            let wz = dz as f32 * gs;
                            let d = (wx*wx + wz*wz).sqrt();
                            if d <= radius {
                                draw_cube(vec3(wx, 0.05, wz), vec3(gs * 0.95, 0.1, gs * 0.95), None, Color::new(1.0, 0.0, 0.0, 0.5));
                            }
                        }
                    }
                    
                    // Draw radius circle for reference
                    for i in 0..32 {
                        let a1 = (i as f32 / 32.0) * 6.28;
                        let a2 = ((i+1) as f32 / 32.0) * 6.28;
                        draw_line_3d(vec3(a1.cos()*radius, 0.1, a1.sin()*radius), vec3(a2.cos()*radius, 0.1, a2.sin()*radius), RED);
                    }
                }

                set_default_camera();
                draw_rectangle(10.0, 10.0, 300.0, 160.0, Color::new(0.0, 0.0, 0.0, 0.7));
                draw_text("HITBOX CALIBRATION", 20.0, 35.0, 24.0, YELLOW);
                draw_text(&format!("MODEL: {}", current_key), 20.0, 65.0, 20.0, WHITE);
                draw_text(&format!("RADIUS MULT: {:.2}", current_mult), 20.0, 85.0, 20.0, GREEN);
                draw_text("UP/DOWN: Select Model", 20.0, 115.0, 18.0, LIGHTGRAY);
                draw_text("LEFT/RIGHT: Adjust Radius", 20.0, 135.0, 18.0, LIGHTGRAY);
                draw_text("S: Export to Console | F2/ESC: Exit", 20.0, 155.0, 18.0, LIGHTGRAY);
            }
            GameState::Connecting => {
                // Simple connection screen
                let sw = screen_width();
                let sh = screen_height();
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.08, 0.08, 0.12, 1.0));
                draw_text("CONNECT TO SERVER", sw / 2.0 - 140.0, sh / 2.0 - 60.0, 32.0, WHITE);
                draw_text(&format!("Address: {}", server_addr), sw / 2.0 - 140.0, sh / 2.0 - 20.0, 22.0, GRAY);
                draw_text("Press ENTER to connect, or ESCAPE for offline", sw / 2.0 - 200.0, sh / 2.0 + 20.0, 18.0, Color::new(0.6, 0.6, 0.8, 1.0));

                // Type server address
                if let Some(c) = get_char_pressed() {
                    if c.is_ascii() && !c.is_control() { server_addr.push(c); }
                }
                if is_key_pressed(KeyCode::Backspace) && !server_addr.is_empty() {
                    server_addr.pop();
                }

                if is_key_pressed(KeyCode::Enter) {
                    let app = &creator.appearance;
                    let net_app = CharacterAppearanceNet {
                        skin: app.skin.clone(), shoes: app.shoes.clone(),
                        clothes: app.clothes.clone(), gloves: app.gloves.clone(),
                        hairstyle: app.hairstyle.clone(), facial_hair: app.facial_hair.clone(),
                        eye_color: app.eye_color.clone(), eyelashes: app.eyelashes.clone(),
                        headgear: app.headgear.clone(), addon: app.addon.clone(),
                    };
                    match NetClient::connect(&server_addr, "Player", net_app) {
                        Ok(client) => { net_client = Some(client); game_state = GameState::Playing; }
                        Err(e) => { eprintln!("Connection failed: {}", e); }
                    }
                }
                if is_key_pressed(KeyCode::Escape) {
                    game_state = GameState::Playing; // Offline mode
                }
            }
            GameState::Playing => {
                if is_key_pressed(KeyCode::C) {
                    creator.confirmed = false;
                    game_state = GameState::CharacterCreation;
                    continue;
                }

                // Network update
                if let Some(ref mut client) = net_client {
                    client.update();

                    // Load textures for newly joined remote players
                    let new_players: Vec<(u64, CharacterAppearanceNet)> = client.remote_appearances.iter()
                        .filter(|(id, _)| !remote_textures.contains_key(id))
                        .map(|(id, app)| (*id, app.clone()))
                        .collect();

                    for (id, app) in new_players {
                        // Convert net appearance to local CharacterAppearance for texture loading
                        let local_app = CharacterAppearance {
                            skin: app.skin.clone(), shoes: app.shoes.clone(),
                            clothes: app.clothes.clone(), gloves: app.gloves.clone(),
                            hairstyle: app.hairstyle.clone(), facial_hair: app.facial_hair.clone(),
                            eye_color: app.eye_color.clone(), eyelashes: app.eyelashes.clone(),
                            headgear: app.headgear.clone(), addon: app.addon.clone(),
                        };
                        let textures = CharacterTextures::from_appearance(&local_app, &catalog).await;
                        remote_textures.insert(id, textures.layers);
                    }

                    // Remove textures for players who left
                    remote_textures.retain(|id, _| client.remote_appearances.contains_key(id));
                    remote_anims.retain(|id, _| client.remote_appearances.contains_key(id));

                    // Apply server world state
                    if let Some(ref world) = client.latest_world {
                        if let Some(my_id) = client.my_id {
                            for ps in &world.players {
                                if ps.id == my_id {
                                    hero.pos = vec3(ps.x, 0.0, ps.z);
                                    hero.target_pos = vec3(ps.target_x, 0.0, ps.target_z);
                                    hero.casting_timer = ps.casting_timer;
                                    
                                    // Sync local animation with server authority
                                    let server_state = match ps.anim_state {
                                        0 => AnimationState::Idle,
                                        1 => AnimationState::Walk,
                                        3 => AnimationState::Sword,
                                        4 => AnimationState::Bow,
                                        5 => AnimationState::Staff,
                                        9 => AnimationState::CarryIdle,
                                        _ => AnimationState::Idle,
                                    };
                                    
                                    // Only override if the server says we are doing an action, 
                                    // or if we aren't currently playing a local action animation
                                    if ps.anim_state > 1 || hero.anim.state == AnimationState::Idle || hero.anim.state == AnimationState::Walk {
                                        hero.anim.set_state(server_state);
                                    }
                                } else {
                                    let anim = remote_anims.entry(ps.id).or_insert_with(|| {
                                        AnimationManager::new(SpriteSheetConfig { columns: 29, rows: 8 })
                                    });
                                    // Map anim_state from server to local AnimationState
                                    let state = match ps.anim_state {
                                        0 => AnimationState::Idle,
                                        1 => AnimationState::Walk,
                                        3 => AnimationState::Sword,
                                        4 => AnimationState::Bow,
                                        5 => AnimationState::Staff,
                                        9 => AnimationState::CarryIdle,
                                        _ => AnimationState::Idle,
                                    };
                                    anim.set_state(state);
                                    if ps.direction <= 7 {
                                        anim.direction = unsafe { std::mem::transmute(ps.direction) };
                                    }
                                    anim.update(delta_time, 5.0);
                                }
                            }
                        }

                        // Spawn effects from server for remote spells
                        let active_ids: std::collections::HashSet<u64> = world.effects.iter().map(|e| e.effect_id).collect();
                        spawned_effect_ids.retain(|id| active_ids.contains(id)); // Cleanup old IDs

                        for ef in &world.effects {
                            if Some(ef.caster_id) == client.my_id { continue; }
                            if spawned_effect_ids.contains(&ef.effect_id) { continue; }

                            spawned_effect_ids.insert(ef.effect_id);
                            let target = vec3(ef.x, 0.0, ef.z);
                            let spell = match ef.spell {
                                0 => SpellId::Q,
                                1 => SpellId::W,
                                2 => SpellId::E,
                                3 => SpellId::R,
                                _ => continue,
                            };
                            match spell {
                                SpellId::Q => effect_manager.spawn_arrow_rain(target, assets.spell_q.clone()),
                                _ => effect_manager.spawn_single_hit(target, match spell {
                                    SpellId::W => assets.spell_w.clone(),
                                    SpellId::E => assets.spell_e.clone(),
                                    SpellId::R => assets.spell_r.clone(),
                                    _ => assets.spell_w.clone(),
                                }, spell),
                            }
                        }
                    }
                }

                game_camera.update(hero.pos);
                let cast_event = handle_input(
                    &mut hero,
                    &game_camera,
                    &mut effect_manager,
                    &dummies,
                    &assets,
                    &world_env,
                    &mut indicator_manager,
                );

                if is_key_pressed(KeyCode::F2) {
                    game_state = GameState::HitboxCalibration;
                }

                // Send inputs to server
                if let Some(ref client) = net_client {
                    if is_mouse_button_pressed(MouseButton::Right) && hero.targeting_state == TargetingState::None {
                        if let Some(pos) = game_camera.get_mouse_ray_intersection() {
                            client.send(&ClientMessage::MoveTo { x: pos.x, z: pos.z });
                        }
                    }
                    // Send spell cast
                    if let Some(ref ev) = cast_event {
                        let spell_id = match ev.spell {
                            SpellId::Q => 0,
                            SpellId::W => 1,
                            SpellId::E => 2,
                            SpellId::R => 3,
                        };
                        client.send(&ClientMessage::CastSpell {
                            spell: spell_id,
                            target_x: ev.target_x,
                            target_z: ev.target_z,
                        });
                    }
                }

                // Local simulation (for offline or client-side prediction)
                if net_client.is_none() {
                    if hero.casting_timer > 0.0 {
                        hero.casting_timer -= delta_time;
                    } else {
                        const PLAYER_RADIUS: f32 = 0.35;
                        let speed = hero.stats.get_movement_speed();
                        let to_target = hero.target_pos - hero.pos;
                        if to_target.length() > 0.1 {
                            let desired = hero.pos + to_target.normalize() * speed * delta_time;
                            let new_pos = world::pathfinding::slide_move(
                                hero.pos, desired, PLAYER_RADIUS,
                                world_env.grid_size, world_env.width, world_env.height,
                                &world_env.walkability_grid,
                            );
                            if (new_pos - hero.pos).length() > 0.001 {
                                hero.pos = new_pos;
                                
                                // ── World Boundary Clamping ──
                                let margin = PLAYER_RADIUS + 0.1;
                                let hw = (world_env.width as f32 * world_env.grid_size) / 2.0 - margin;
                                let hh = (world_env.height as f32 * world_env.grid_size) / 2.0 - margin;
                                hero.pos.x = hero.pos.x.clamp(-hw, hw);
                                hero.pos.z = hero.pos.z.clamp(-hh, hh);

                                hero.anim.set_state(AnimationState::Walk);
                                hero.anim.set_direction(to_target);
                                hero.stuck_timer = 0.0;
                            } else {
                                // Blocked — increment stuck timer
                                hero.stuck_timer += delta_time;
                                if hero.stuck_timer > 1.0 {
                                    // Truly stuck for 1 second — clear path
                                    hero.target_pos = hero.pos;
                                    hero.current_path.clear();
                                    hero.anim.set_state(AnimationState::Idle);
                                } else {
                                    // Just stay in walk animation (running against wall)
                                    hero.anim.set_state(AnimationState::Walk);
                                }
                            }
                        } else {
                            // Reached waypoint — advance path
                            if !hero.current_path.is_empty() {
                                hero.current_path.remove(0);
                                if let Some(next) = hero.current_path.first() {
                                    hero.target_pos = *next;
                                }
                            } else {
                                hero.anim.set_state(AnimationState::Idle);
                            }
                        }
                    }
                } else {
                    // When networked, only update movement animations if NOT currently casting
                    if hero.casting_timer <= 0.0 {
                        let to_target = hero.target_pos - hero.pos;
                        if to_target.length() > 0.1 {
                            hero.anim.set_state(AnimationState::Walk);
                            hero.anim.set_direction(to_target);
                        } else {
                            // Follow path locally even in networked mode for smoothness
                            if !hero.current_path.is_empty() {
                                hero.current_path.remove(0);
                                if let Some(next) = hero.current_path.first() {
                                    hero.target_pos = *next;
                                }
                            } else {
                                hero.anim.set_state(AnimationState::Idle);
                            }
                        }
                    }
                }

                hero.anim.update(delta_time, hero.stats.get_movement_speed());
                effect_manager.update(delta_time);
                indicator_manager.update(delta_time);

                // Render
                set_camera(&game_camera.camera);
                // draw_grid(20, 1.0, BLACK, GRAY);
                world_env.draw();

                // ── F1: Pathfinding Debug Overlay ──────────────────────────
                if is_key_pressed(KeyCode::F1) { debug_pathfinding = !debug_pathfinding; }

                if debug_pathfinding {
                    let gs = world_env.grid_size;
                    let hw = world_env.width / 2;
                    let hh = world_env.height / 2;

                    // Draw blocked grid cells in semi-transparent red
                    for x in 0..world_env.width {
                        for z in 0..world_env.height {
                            if !world_env.walkability_grid[x as usize][z as usize] {
                                let wx = (x - hw) as f32 * gs;
                                let wz = (z - hh) as f32 * gs;
                                draw_cube(
                                    vec3(wx, 0.05, wz),
                                    vec3(gs * 0.95, 0.1, gs * 0.95),
                                    None,
                                    Color::new(1.0, 0.0, 0.0, 0.45),
                                );
                            }
                        }
                    }

                    // Draw current path as connected yellow lines + waypoint spheres
                    let path = &hero.current_path;
                    let mut prev = hero.pos;
                    // First segment: hero → current target
                    draw_line_3d(
                        hero.pos + vec3(0.0, 0.5, 0.0),
                        hero.target_pos + vec3(0.0, 0.5, 0.0),
                        YELLOW,
                    );
                    draw_sphere(hero.target_pos + vec3(0.0, 0.5, 0.0), 0.15, None, YELLOW);

                    for waypoint in path.iter() {
                        draw_line_3d(
                            prev + vec3(0.0, 0.5, 0.0),
                            *waypoint + vec3(0.0, 0.5, 0.0),
                            YELLOW,
                        );
                        draw_sphere(*waypoint + vec3(0.0, 0.5, 0.0), 0.12, None, ORANGE);
                        prev = *waypoint;
                    }

                    // Draw final destination
                    if let Some(dest) = path.last() {
                        draw_sphere(*dest + vec3(0.0, 0.6, 0.0), 0.2, None, LIME);
                    }

                    // Show grid lines faintly
                    for x in -hw..=hw {
                        draw_line_3d(
                            vec3(x as f32 * gs, 0.02, -hh as f32 * gs),
                            vec3(x as f32 * gs, 0.02,  hh as f32 * gs),
                            Color::new(1.0, 1.0, 1.0, 0.08),
                        );
                    }
                    for z in -hh..=hh {
                        draw_line_3d(
                            vec3(-hw as f32 * gs, 0.02, z as f32 * gs),
                            vec3( hw as f32 * gs, 0.02, z as f32 * gs),
                            Color::new(1.0, 1.0, 1.0, 0.08),
                        );
                    }
                }
                // ── End Debug Overlay ──────────────────────────────────────

                indicator_manager.draw();

                // Update Ogre animation
                ogre_anim_timer += delta_time * 3.0;
                let ogre_idle_frames = [0u32, 1u32];
                let ogre_frame_idx = (ogre_anim_timer as usize % ogre_idle_frames.len()) as u32;
                let ogre_col = ogre_idle_frames[ogre_frame_idx as usize];

                // ─── Back-to-Front Sorting for EVERYTHING ───
                // This handles Ogres, Hero, Players, and Particles in a single pass to fix all transparency issues
                enum DrawKind {
                    Billboard { tex: Texture2D, src: Rect, size: f32 },
                    Particle { index: usize },
                }
                struct SortItem {
                    pos: Vec3,
                    kind: DrawKind,
                    dist_sq: f32,
                }
                let mut sort_list: Vec<SortItem> = Vec::new();
                let cam_pos = game_camera.camera.position;

                // Add Ogres
                for d_pos in &dummies {
                    let fw = assets.dummy.width() / 29.0;
                    let fh = assets.dummy.height() / 8.0;
                    let src = Rect::new(ogre_col as f32 * fw, 0.0, fw, fh);
                    sort_list.push(SortItem {
                        pos: *d_pos,
                        kind: DrawKind::Billboard { tex: assets.dummy.clone(), src, size: 4.0 },
                        dist_sq: (cam_pos - *d_pos).length_squared(),
                    });
                }

                // Add Hero
                for tex in &char_textures.layers {
                    let src = hero.anim.get_source_rect(tex.width(), tex.height());
                    sort_list.push(SortItem {
                        pos: hero.pos,
                        kind: DrawKind::Billboard { tex: tex.clone(), src, size: 2.3 },
                        dist_sq: (cam_pos - hero.pos).length_squared(),
                    });
                }

                // Add Remote Players
                if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        for ps in &world.players {
                            if Some(ps.id) == client.my_id { continue; }
                            let pos = vec3(ps.x, 0.0, ps.z);
                            if let Some(textures) = remote_textures.get(&ps.id) {
                                if let Some(anim) = remote_anims.get(&ps.id) {
                                    for tex in textures {
                                        let src = anim.get_source_rect(tex.width(), tex.height());
                                        sort_list.push(SortItem {
                                            pos,
                                            kind: DrawKind::Billboard { tex: tex.clone(), src, size: 2.0 },
                                            dist_sq: (cam_pos - pos).length_squared(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                // Add Particles
                for (i, p) in effect_manager.particles.iter().enumerate() {
                    sort_list.push(SortItem {
                        pos: p.pos,
                        kind: DrawKind::Particle { index: i },
                        dist_sq: (cam_pos - p.pos).length_squared(),
                    });
                }

                // Sort back-to-front
                sort_list.sort_by(|a, b| b.dist_sq.partial_cmp(&a.dist_sq).unwrap());

                // Draw everything in order
                for item in sort_list {
                    match item.kind {
                        DrawKind::Billboard { tex, src, size } => {
                            draw_character_billboard_ex(item.pos, &tex, src, cam_pos, size);
                        }
                        DrawKind::Particle { index } => {
                            effect_manager.draw_particle(&effect_manager.particles[index], cam_pos);
                        }
                    }
                }

                // Draw HP bars (separately as they are solid cubes)
                for d_pos in &dummies {
                    draw_cube(*d_pos + vec3(0.0, 3.5, 0.0), vec3(1.5, 0.1, 0.1), None, RED);
                }

                match hero.targeting_state {
                    TargetingState::Aoe(_, radius) => {
                        if let Some(intersection) = game_camera.get_mouse_ray_intersection() {
                            draw_aoe_target(intersection, radius, get_time() as f32);
                        }
                    }
                    TargetingState::UnitTarget(_) => {
                        if let Some(intersection) = game_camera.get_mouse_ray_intersection() {
                            draw_cube_wires(intersection, vec3(2.5, 0.1, 2.5), RED);
                        }
                    }
                    TargetingState::None => {}
                }

                set_default_camera();
                ui::hud::draw_hud(&hero, &assets);

                // Draw ping
                if let Some(ref client) = net_client {
                    draw_text(&format!("Ping: {:.0}ms", client.ping_ms), screen_width() - 140.0, 20.0, 18.0, GREEN);
                }

                if let TargetingState::UnitTarget(_) = hero.targeting_state {
                    show_mouse(false);
                    let (mx, my) = mouse_position();
                    draw_texture_ex(&assets.target_mouse, mx - 16.0, my - 16.0, WHITE,
                        DrawTextureParams { dest_size: Some(vec2(32.0, 32.0)), ..Default::default() });
                } else {
                    show_mouse(true);
                }
            }
        }

        next_frame().await
    }
}

/// Sync the creator's selected_indices from a loaded CharacterAppearance.
fn sync_creator_indices(creator: &mut CharacterCreator) {
    let app = &creator.appearance;

    // Skin (required)
    creator.selected_indices[0] = creator.catalog.skins.iter().position(|o| o.name == app.skin);
    if creator.selected_indices[0].is_none() {
        creator.selected_indices[0] = Some(0);
    }

    // Optional layers
    let find = |opts: &[LayerOption], name: &Option<String>| -> Option<usize> {
        name.as_ref().and_then(|n| opts.iter().position(|o| o.name == *n))
    };

    creator.selected_indices[1] = find(&creator.catalog.shoes, &app.shoes);
    creator.selected_indices[2] = find(&creator.catalog.clothes, &app.clothes);
    creator.selected_indices[3] = find(&creator.catalog.gloves, &app.gloves);
    creator.selected_indices[4] = find(&creator.catalog.hairstyles, &app.hairstyle);
    creator.selected_indices[5] = find(&creator.catalog.facial_hair, &app.facial_hair);
    creator.selected_indices[6] = find(&creator.catalog.eye_colors, &app.eye_color);
    creator.selected_indices[7] = find(&creator.catalog.eyelashes, &app.eyelashes);
    creator.selected_indices[8] = find(&creator.catalog.headgears, &app.headgear);
    creator.selected_indices[9] = find(&creator.catalog.addons, &app.addon);
}
