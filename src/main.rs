mod core;
mod entities;
mod systems;
mod ui;
mod net;

use macroquad::prelude::*;
use core::animation::*;
use entities::player::*;
use entities::character::*;
use core::camera::*;
use systems::input::*;
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
}

async fn load_or_fallback(path: &str, color: Color) -> Texture2D {
    load_texture(path).await.unwrap_or_else(|_| {
        let mut bytes: Vec<u8> = Vec::with_capacity(64 * 64 * 4);
        for _ in 0..(64*64) {
            bytes.push((color.r * 255.0) as u8);
            bytes.push((color.g * 255.0) as u8);
            bytes.push((color.b * 255.0) as u8);
            bytes.push((color.a * 255.0) as u8);
        }
        let tex = Texture2D::from_rgba8(64, 64, &bytes);
        tex.set_filter(FilterMode::Nearest);
        tex
    })
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
        dummy: load_or_fallback("assets/characters/dummy.png", GRAY).await,
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

    // Load initial character textures
    let mut char_textures = CharacterTextures::from_appearance(&creator.appearance, &catalog).await;

    let mut hero = Hero {
        pos: vec3(0.0, 0.0, 0.0),
        target_pos: vec3(0.0, 0.0, 0.0),
        stats: Stats::new(10, 15, 10),
        anim: AnimationManager::new(config),
        targeting_state: TargetingState::None,
        casting_timer: 0.0,
    };

    let mut game_camera = GameCamera::new(hero.pos);
    let mut effect_manager = EffectManager::new();

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
                    game_state = GameState::Connecting;
                }
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
                let cast_event = handle_input(&mut hero, &game_camera, &mut effect_manager, &dummies, &assets);

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
                        let speed = hero.stats.get_movement_speed();
                        let to_target = hero.target_pos - hero.pos;
                        if to_target.length() > 0.1 {
                            hero.pos += to_target.normalize() * speed * delta_time;
                            hero.anim.set_state(AnimationState::Walk);
                            hero.anim.set_direction(to_target);
                        } else {
                            hero.anim.set_state(AnimationState::Idle);
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
                            hero.anim.set_state(AnimationState::Idle);
                        }
                    }
                }

                hero.anim.update(delta_time, hero.stats.get_movement_speed());
                effect_manager.update(delta_time);

                // Render
                set_camera(&game_camera.camera);
                draw_grid(20, 1.0, BLACK, GRAY);

                if (hero.target_pos - hero.pos).length() > 0.1 {
                    draw_cube_wires(hero.target_pos, vec3(0.5, 0.1, 0.5), GREEN);
                }

                for d_pos in &dummies {
                    let dummy_rect = Rect::new(0.0, 0.0, assets.dummy.width(), assets.dummy.height());
                    draw_character_billboard(*d_pos, &assets.dummy, dummy_rect, game_camera.camera.position);
                    draw_cube(*d_pos + vec3(0.0, 2.0, 0.0), vec3(1.0, 0.1, 0.1), None, RED);
                }

                // Draw local hero
                for tex in &char_textures.layers {
                    let source_rect = hero.anim.get_source_rect(tex.width(), tex.height());
                    draw_character_billboard(hero.pos, tex, source_rect, game_camera.camera.position);
                }

                // Draw remote players
                if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        for ps in &world.players {
                            if Some(ps.id) == client.my_id { continue; }
                            let pos = vec3(ps.x, 0.0, ps.z);
                            if let Some(textures) = remote_textures.get(&ps.id) {
                                if let Some(anim) = remote_anims.get(&ps.id) {
                                    for tex in textures {
                                        let src = anim.get_source_rect(tex.width(), tex.height());
                                        draw_character_billboard(pos, tex, src, game_camera.camera.position);
                                    }
                                }
                            } else {
                                // Fallback: draw a colored cube for players whose textures haven't loaded
                                draw_cube(pos + vec3(0.0, 1.0, 0.0), vec3(0.8, 1.6, 0.8), None, YELLOW);
                            }
                        }
                    }
                }

                effect_manager.draw(game_camera.camera.position);

                match hero.targeting_state {
                    TargetingState::Aoe(_, radius) => {
                        if let Some(intersection) = game_camera.get_mouse_ray_intersection() {
                            let segments = 32;
                            for i in 0..segments {
                                let a1 = (i as f32 / segments as f32) * std::f32::consts::PI * 2.0;
                                let a2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::PI * 2.0;
                                draw_line_3d(
                                    intersection + vec3(a1.cos() * radius, 0.1, a1.sin() * radius),
                                    intersection + vec3(a2.cos() * radius, 0.1, a2.sin() * radius), GREEN);
                            }
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