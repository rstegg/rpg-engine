mod core;
mod entities;
mod net;
mod systems;
mod ui;
mod world;

use core::animation::*;
use core::camera::*;
use entities::character::*;
use entities::effects::*;
use entities::player::*;
use entities::enemy::*;
use macroquad::prelude::*;
use net::client::NetClient;
use net::protocol::*;
use std::collections::BTreeMap;
use systems::cluster_editor::*;
use systems::indicators::*;
use systems::input::*;
use ui::character_creator::*;
use world::environment::{self, HitboxConfig};

const HITBOX_SIDEBAR_WIDTH: f32 = 320.0;
const HITBOX_ROW_HEIGHT: f32 = 24.0;

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
    ClusterEditor,
    GameOver,
}

async fn load_or_fallback(path: &str, color: Color) -> Texture2D {
    let tex = load_texture(path).await.unwrap_or_else(|_| {
        let mut bytes: Vec<u8> = Vec::with_capacity(64 * 64 * 4);
        for _ in 0..(64 * 64) {
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

fn ground_intersection(camera: &Camera3D) -> Option<Vec3> {
    let (mouse_x, mouse_y) = mouse_position();
    let ndc_x = (mouse_x / screen_width()) * 2.0 - 1.0;
    let ndc_y = 1.0 - (mouse_y / screen_height()) * 2.0;
    let inv_vp = camera.matrix().inverse();
    let ray_origin = inv_vp.project_point3(vec3(ndc_x, ndc_y, -1.0));
    let far_point = inv_vp.project_point3(vec3(ndc_x, ndc_y, 1.0));
    let ray_direction = (far_point - ray_origin).normalize();

    if ray_direction.y.abs() <= f32::EPSILON {
        return None;
    }

    let t = -ray_origin.y / ray_direction.y;
    if t <= 0.0 {
        return None;
    }

    let point = ray_origin + ray_direction * t;
    Some(vec3(point.x, 0.0, point.z))
}

fn rect_contains(x: f32, y: f32, w: f32, h: f32, point: (f32, f32)) -> bool {
    point.0 >= x && point.0 <= x + w && point.1 >= y && point.1 <= y + h
}

fn discover_hitbox_calibration_models() -> BTreeMap<String, String> {
    let mut models = BTreeMap::new();

    for (key, file) in environment::builtin_template_defs() {
        models.insert(key.to_string(), format!("assets/world_models/{file}"));
    }

    if let Ok(entries) = std::fs::read_dir("assets/world_models") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("glb") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            models
                .entry(stem.to_string())
                .or_insert_with(|| format!("assets/world_models/{file_name}"));
        }
    }

    models
}

async fn rebuild_world_from_current_placements(
    world_env: &mut world::environment::WorldEnvironment,
    hitbox_config: &HitboxConfig,
) {
    let placements = world_env.sim.placements.clone();
    *world_env =
        world::environment::WorldEnvironment::new(20, 20, hitbox_config.clone(), false).await;

    for placement in placements {
        if !world_env.sim.templates.contains_key(&placement.model) {
            if let Some(template) = world::environment::load_glb_template(&format!(
                "assets/world_models/{}",
                placement.file
            ))
            .await
            {
                world_env
                    .sim
                    .templates
                    .insert(placement.model.clone(), template);
            }
        }
        world_env.add_placement(&placement, hitbox_config);
    }
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
        cooldowns: std::collections::HashMap::new(),
        is_dead: false,
        revive_progress: 0.0,
    };

    let mut hitbox_config: HitboxConfig = match std::fs::read_to_string("hitbox_config.json") {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => HitboxConfig::new(),
    };
    let hitbox_models = discover_hitbox_calibration_models();

    let mut world_env =
        world::environment::WorldEnvironment::new(20, 20, hitbox_config.clone(), true).await;
    let mut debug_pathfinding = false;

    let mut game_camera = GameCamera::new(hero.pos);
    let mut effect_manager = EffectManager::new();
    let mut indicator_manager = IndicatorManager::new();
    let mut cluster_editor = ClusterEditor::new();

    let mut enemy_director = EnemyDirector::new(&catalog, config).await;
    let mut combat_text_mgr = ui::combat_text::CombatTextManager::new();

    let mut game_state = GameState::CharacterCreation;
    let mut net_client: Option<NetClient> = None;
    let mut server_addr = String::from("127.0.0.1:7878");
    let mut remote_textures: std::collections::HashMap<u64, Vec<Texture2D>> =
        std::collections::HashMap::new();
    let mut remote_anims: std::collections::HashMap<u64, AnimationManager> =
        std::collections::HashMap::new();
    let mut remote_enemy_anims: std::collections::HashMap<u64, AnimationManager> =
        std::collections::HashMap::new();
    let mut remote_player_positions: std::collections::HashMap<u64, Vec3> =
        std::collections::HashMap::new();
    let mut remote_enemy_positions: std::collections::HashMap<u64, Vec3> =
        std::collections::HashMap::new();
    let mut spawned_effect_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Calibration state
    let mut calibration_selected_idx = 0;
    let mut calibration_last_paint: Option<([i32; 2], bool)> = None;
    let mut calibration_camera_yaw = 0.0_f32;
    let mut calibration_camera_distance = 10.0_f32;
    let mut calibration_search_text = String::new();
    let mut calibration_search_focused = false;
    let mut calibration_list_scroll = 0usize;
    // Ensure all templates have an entry in hitbox_config
    for key in hitbox_models.keys() {
        hitbox_config
            .entry(key.clone())
            .or_insert(environment::HitboxConfigEntry::Legacy(1.0));
    }

    loop {
        clear_background(DARKGRAY);
        let delta_time = get_frame_time().min(0.05);

        match game_state {
            GameState::CharacterCreation => {
                show_mouse(true);
                if creator.needs_reload {
                    creator.preview_textures =
                        CharacterTextures::from_appearance(&creator.appearance, &catalog)
                            .await
                            .layers;
                    creator.needs_reload = false;
                }
                let confirmed = creator.update_and_draw(delta_time);
                if confirmed {
                    char_textures =
                        CharacterTextures::from_appearance(&creator.appearance, &catalog).await;
                    game_state = GameState::Connecting;
                }
            }
            GameState::HitboxCalibration => {
                // Clear and set 3D view
                clear_background(BLACK);
                let sw = screen_width();
                let sh = screen_height();
                let sidebar_x = sw - HITBOX_SIDEBAR_WIDTH;
                let search_y = 60.0;
                let list_y = 125.0;
                let (mx, my) = mouse_position();
                let pointer_over_sidebar = mx >= sidebar_x;
                let pointer_over_info_panel = rect_contains(10.0, 10.0, 360.0, 220.0, (mx, my));
                let pointer_over_ui = pointer_over_sidebar || pointer_over_info_panel;

                if is_mouse_button_pressed(MouseButton::Left) {
                    let clicked_search = rect_contains(
                        sidebar_x + 10.0,
                        search_y,
                        HITBOX_SIDEBAR_WIDTH - 20.0,
                        24.0,
                        (mx, my),
                    );
                    if clicked_search {
                        calibration_search_focused = true;
                    } else if !pointer_over_sidebar {
                        calibration_search_focused = false;
                    }
                }

                if calibration_search_focused {
                    while let Some(c) = get_char_pressed() {
                        if c.is_ascii() && !c.is_control() {
                            calibration_search_text.push(c.to_ascii_lowercase());
                            calibration_selected_idx = 0;
                            calibration_list_scroll = 0;
                            calibration_last_paint = None;
                        }
                    }
                    if is_key_pressed(KeyCode::Backspace) && !calibration_search_text.is_empty() {
                        calibration_search_text.pop();
                        calibration_selected_idx = 0;
                        calibration_list_scroll = 0;
                        calibration_last_paint = None;
                    }
                }

                let keys: Vec<String> = hitbox_models
                    .keys()
                    .filter(|key| {
                        calibration_search_text.is_empty()
                            || key.to_ascii_lowercase().contains(&calibration_search_text)
                    })
                    .cloned()
                    .collect();
                if keys.is_empty() {
                    set_default_camera();
                    draw_rectangle(
                        sidebar_x,
                        0.0,
                        HITBOX_SIDEBAR_WIDTH,
                        sh,
                        Color::new(0.08, 0.08, 0.08, 0.95),
                    );
                    draw_text("HITBOX CALIBRATION", sidebar_x + 10.0, 35.0, 24.0, YELLOW);
                    let search_color = if calibration_search_focused {
                        Color::new(0.18, 0.18, 0.18, 1.0)
                    } else {
                        Color::new(0.1, 0.1, 0.1, 1.0)
                    };
                    draw_text("Search:", sidebar_x + 10.0, 54.0, 14.0, GRAY);
                    draw_rectangle(
                        sidebar_x + 10.0,
                        search_y,
                        HITBOX_SIDEBAR_WIDTH - 20.0,
                        24.0,
                        search_color,
                    );
                    draw_rectangle_lines(
                        sidebar_x + 10.0,
                        search_y,
                        HITBOX_SIDEBAR_WIDTH - 20.0,
                        24.0,
                        1.0,
                        GRAY,
                    );
                    draw_text(
                        &calibration_search_text,
                        sidebar_x + 15.0,
                        search_y + 16.0,
                        14.0,
                        WHITE,
                    );
                    draw_text(
                        "No models match the current filter.",
                        sidebar_x + 10.0,
                        110.0,
                        18.0,
                        ORANGE,
                    );
                    if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F2) {
                        rebuild_world_from_current_placements(&mut world_env, &hitbox_config).await;
                        game_state = GameState::Playing;
                    }
                    continue;
                }
                if calibration_selected_idx >= keys.len() {
                    calibration_selected_idx = 0;
                }
                let mut calibration_selection_changed = false;
                let current_key = keys[calibration_selected_idx].clone();
                if !world_env.sim.templates.contains_key(&current_key) {
                    if let Some(path) = hitbox_models.get(&current_key) {
                        if let Some(template) = world::environment::load_glb_template(path).await {
                            world_env.sim.templates.insert(current_key.clone(), template);
                        }
                    }
                }

                if is_key_pressed(KeyCode::S) {
                    if let Ok(json) = serde_json::to_string_pretty(&hitbox_config) {
                        let _ = std::fs::write("hitbox_config.json", json);
                    }
                    rebuild_world_from_current_placements(&mut world_env, &hitbox_config).await;
                }
                if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F2) {
                    rebuild_world_from_current_placements(&mut world_env, &hitbox_config).await;
                    game_state = GameState::Playing;
                }
                if is_key_pressed(KeyCode::Down) {
                    calibration_selected_idx = (calibration_selected_idx + 1) % keys.len();
                    calibration_last_paint = None;
                    calibration_selection_changed = true;
                }
                if is_key_pressed(KeyCode::Up) {
                    calibration_selected_idx =
                        (calibration_selected_idx + keys.len() - 1) % keys.len();
                    calibration_last_paint = None;
                    calibration_selection_changed = true;
                }
                if is_key_pressed(KeyCode::PageDown) {
                    calibration_selected_idx = (calibration_selected_idx + 12).min(keys.len() - 1);
                    calibration_last_paint = None;
                    calibration_selection_changed = true;
                }
                if is_key_pressed(KeyCode::PageUp) {
                    calibration_selected_idx = calibration_selected_idx.saturating_sub(12);
                    calibration_last_paint = None;
                    calibration_selection_changed = true;
                }
                if is_key_pressed(KeyCode::Home) {
                    calibration_selected_idx = 0;
                    calibration_last_paint = None;
                    calibration_selection_changed = true;
                }
                if is_key_pressed(KeyCode::End) {
                    calibration_selected_idx = keys.len() - 1;
                    calibration_last_paint = None;
                    calibration_selection_changed = true;
                }

                let visible_rows = ((sh - list_y - 150.0) / HITBOX_ROW_HEIGHT).max(1.0) as usize;
                let max_scroll = keys.len().saturating_sub(visible_rows);
                calibration_list_scroll = calibration_list_scroll.min(max_scroll);
                if calibration_selection_changed {
                    if calibration_selected_idx < calibration_list_scroll {
                        calibration_list_scroll = calibration_selected_idx;
                    }
                    if calibration_selected_idx >= calibration_list_scroll + visible_rows {
                        calibration_list_scroll =
                            calibration_selected_idx.saturating_sub(visible_rows.saturating_sub(1));
                    }
                }

                if is_mouse_button_pressed(MouseButton::Left) && pointer_over_sidebar && my > list_y
                {
                    let row = ((my - list_y) / HITBOX_ROW_HEIGHT) as usize;
                    let idx = calibration_list_scroll + row;
                    if idx < keys.len() {
                        calibration_selected_idx = idx;
                        calibration_last_paint = None;
                    }
                }

                if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
                    calibration_camera_yaw -= delta_time * 1.8;
                }
                if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
                    calibration_camera_yaw += delta_time * 1.8;
                }
                let (_, wheel_y) = mouse_wheel();
                if wheel_y.abs() > 0.01 && pointer_over_sidebar {
                    if wheel_y < 0.0 {
                        calibration_list_scroll = (calibration_list_scroll + 3).min(max_scroll);
                    } else {
                        calibration_list_scroll = calibration_list_scroll.saturating_sub(3);
                    }
                } else if wheel_y.abs() > 0.01 && !pointer_over_ui {
                    calibration_camera_distance =
                        (calibration_camera_distance - wheel_y * 0.75).clamp(4.0, 20.0);
                }

                // Render selected model in center
                let camera_pos = vec3(
                    calibration_camera_yaw.sin() * calibration_camera_distance,
                    5.0,
                    calibration_camera_yaw.cos() * calibration_camera_distance,
                );
                let target = vec3(0.0, 0.0, 0.0);
                let calibration_camera = Camera3D {
                    position: camera_pos,
                    target: target,
                    up: vec3(0.0, 1.0, 0.0),
                    fovy: 45.0,
                    ..Default::default()
                };
                set_camera(&calibration_camera);

                draw_grid(10, 2.0, GRAY, DARKGRAY);

                if let Some(t) = world_env.sim.templates.get(&current_key) {
                    let grid_size = world_env.sim.grid_size;
                    let mask = environment::ensure_painted_hitbox_entry(
                        &mut hitbox_config,
                        &current_key,
                        t,
                        grid_size,
                    );
                    let hovered_cell = if pointer_over_ui {
                        None
                    } else {
                        ground_intersection(&calibration_camera).map(|pos| {
                            [
                                (pos.x / grid_size).round() as i32,
                                (pos.z / grid_size).round() as i32,
                            ]
                        })
                    };

                    let paint_mode = if pointer_over_ui {
                        None
                    } else if is_mouse_button_down(MouseButton::Left) {
                        Some(false)
                    } else if is_mouse_button_down(MouseButton::Right) {
                        Some(true)
                    } else {
                        None
                    };

                    if let (Some(cell), Some(erase)) = (hovered_cell, paint_mode) {
                        let should_apply = calibration_last_paint
                            .map(|last| last != (cell, erase))
                            .unwrap_or(true);
                        if should_apply {
                            if erase {
                                mask.blocked_cells.retain(|&blocked| blocked != cell);
                            } else if !mask.blocked_cells.contains(&cell) {
                                mask.blocked_cells.push(cell);
                            }
                            mask.blocked_cells.sort_unstable();
                            calibration_last_paint = Some((cell, erase));
                        }
                    } else {
                        calibration_last_paint = None;
                    }

                    // Draw the model
                    let meshes = world::environment::instantiate(t, vec3(0.0, 0.0, 0.0), 0.0, 2.0);
                    for m in meshes {
                        draw_mesh(&m);
                    }

                    for &[cell_x, cell_z] in &mask.blocked_cells {
                        draw_cube(
                            vec3(cell_x as f32 * grid_size, 0.05, cell_z as f32 * grid_size),
                            vec3(grid_size * 0.95, 0.1, grid_size * 0.95),
                            None,
                            Color::new(1.0, 0.0, 0.0, 0.5),
                        );
                    }

                    if let Some([cell_x, cell_z]) = hovered_cell {
                        draw_cube_wires(
                            vec3(cell_x as f32 * grid_size, 0.08, cell_z as f32 * grid_size),
                            vec3(grid_size, 0.12, grid_size),
                            if is_mouse_button_down(MouseButton::Right) {
                                ORANGE
                            } else {
                                YELLOW
                            },
                        );
                    }

                    for i in -8..=8 {
                        let line = i as f32 * grid_size;
                        draw_line_3d(
                            vec3(line, 0.01, -8.0 * grid_size),
                            vec3(line, 0.01, 8.0 * grid_size),
                            DARKGRAY,
                        );
                        draw_line_3d(
                            vec3(-8.0 * grid_size, 0.01, line),
                            vec3(8.0 * grid_size, 0.01, line),
                            DARKGRAY,
                        );
                    }
                } else {
                    calibration_last_paint = None;
                }

                let blocked_count = hitbox_config
                    .get(&current_key)
                    .and_then(|entry| match entry {
                        environment::HitboxConfigEntry::Painted(mask) => {
                            Some(mask.blocked_cells.len())
                        }
                        environment::HitboxConfigEntry::Legacy(_) => None,
                    })
                    .unwrap_or(0);

                set_default_camera();
                draw_rectangle(10.0, 10.0, 360.0, 220.0, Color::new(0.0, 0.0, 0.0, 0.7));
                draw_text("HITBOX CALIBRATION", 20.0, 35.0, 24.0, YELLOW);
                draw_text(&format!("MODEL: {}", current_key), 20.0, 65.0, 20.0, WHITE);
                draw_text(
                    &format!("BLOCKED CELLS: {}", blocked_count),
                    20.0,
                    90.0,
                    18.0,
                    GREEN,
                );
                draw_text(
                    &format!(
                        "Search: {}",
                        if calibration_search_text.is_empty() {
                            "(all)"
                        } else {
                            &calibration_search_text
                        }
                    ),
                    20.0,
                    115.0,
                    18.0,
                    LIGHTGRAY,
                );
                draw_text("LMB Paint | RMB Erase", 20.0, 145.0, 18.0, LIGHTGRAY);
                draw_text("A/D Orbit | Wheel Zoom", 20.0, 170.0, 18.0, LIGHTGRAY);
                draw_text("S Save | F2/ESC Exit", 20.0, 195.0, 18.0, LIGHTGRAY);
                draw_text(
                    "Click the right panel to search and pick models.",
                    20.0,
                    220.0,
                    18.0,
                    LIGHTGRAY,
                );

                draw_rectangle(
                    sidebar_x,
                    0.0,
                    HITBOX_SIDEBAR_WIDTH,
                    sh,
                    Color::new(0.08, 0.08, 0.08, 0.95),
                );
                draw_line(sidebar_x, 0.0, sidebar_x, sh, 1.0, GRAY);
                draw_text("HITBOX CALIBRATION", sidebar_x + 10.0, 35.0, 24.0, YELLOW);
                let search_color = if calibration_search_focused {
                    Color::new(0.18, 0.18, 0.18, 1.0)
                } else {
                    Color::new(0.1, 0.1, 0.1, 1.0)
                };
                draw_text("Search:", sidebar_x + 10.0, 54.0, 14.0, GRAY);
                draw_rectangle(
                    sidebar_x + 10.0,
                    search_y,
                    HITBOX_SIDEBAR_WIDTH - 20.0,
                    24.0,
                    search_color,
                );
                draw_rectangle_lines(
                    sidebar_x + 10.0,
                    search_y,
                    HITBOX_SIDEBAR_WIDTH - 20.0,
                    24.0,
                    1.0,
                    GRAY,
                );
                draw_text(
                    &calibration_search_text,
                    sidebar_x + 15.0,
                    search_y + 16.0,
                    14.0,
                    WHITE,
                );
                draw_text(
                    &format!("Selected: {}", current_key),
                    sidebar_x + 10.0,
                    110.0,
                    18.0,
                    WHITE,
                );
                draw_text(
                    &format!("Model {} / {}", calibration_selected_idx + 1, keys.len()),
                    sidebar_x + 10.0,
                    135.0,
                    18.0,
                    LIGHTGRAY,
                );
                for row in 0..visible_rows {
                    let idx = calibration_list_scroll + row;
                    if idx >= keys.len() {
                        break;
                    }
                    let y = list_y + row as f32 * HITBOX_ROW_HEIGHT;
                    if idx == calibration_selected_idx {
                        draw_rectangle(
                            sidebar_x + 5.0,
                            y,
                            HITBOX_SIDEBAR_WIDTH - 10.0,
                            HITBOX_ROW_HEIGHT,
                            Color::new(0.35, 0.18, 0.04, 1.0),
                        );
                    }
                    let color = if idx == calibration_selected_idx {
                        YELLOW
                    } else {
                        WHITE
                    };
                    draw_text(&keys[idx], sidebar_x + 10.0, y + 16.0, 14.0, color);
                }
            }
            GameState::Connecting => {
                // Simple connection screen
                let sw = screen_width();
                let sh = screen_height();
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.08, 0.08, 0.12, 1.0));
                draw_text(
                    "CONNECT TO SERVER",
                    sw / 2.0 - 140.0,
                    sh / 2.0 - 60.0,
                    32.0,
                    WHITE,
                );
                draw_text(
                    &format!("Address: {}", server_addr),
                    sw / 2.0 - 140.0,
                    sh / 2.0 - 20.0,
                    22.0,
                    GRAY,
                );
                draw_text(
                    "Press ENTER to connect, or ESCAPE for offline",
                    sw / 2.0 - 200.0,
                    sh / 2.0 + 20.0,
                    18.0,
                    Color::new(0.6, 0.6, 0.8, 1.0),
                );

                // Type server address
                if let Some(c) = get_char_pressed() {
                    if c.is_ascii() && !c.is_control() {
                        server_addr.push(c);
                    }
                }
                if is_key_pressed(KeyCode::Backspace) && !server_addr.is_empty() {
                    server_addr.pop();
                }

                if is_key_pressed(KeyCode::Enter) && net_client.is_none() {
                    let app = &creator.appearance;
                    let net_app = CharacterAppearanceNet {
                        skin: app.skin.clone(),
                        shoes: app.shoes.clone(),
                        clothes: app.clothes.clone(),
                        gloves: app.gloves.clone(),
                        hairstyle: app.hairstyle.clone(),
                        facial_hair: app.facial_hair.clone(),
                        eye_color: app.eye_color.clone(),
                        eyelashes: app.eyelashes.clone(),
                        headgear: app.headgear.clone(),
                        addon: app.addon.clone(),
                    };
                    match NetClient::connect(&server_addr, "Player", net_app) {
                        Ok(client) => {
                            net_client = Some(client);
                        }
                        Err(e) => {
                            eprintln!("Connection failed: {}", e);
                        }
                    }
                }

                if let Some(ref mut nc) = net_client {
                    nc.update();
                    draw_text("Waiting for world data...", sw / 2.0 - 100.0, sh / 2.0 + 60.0, 18.0, YELLOW);
                    
                    if nc.connected && nc.pending_map.is_some() {
                        let (palette, placements) = nc.pending_map.take().unwrap();
                        println!(
                            "[CLIENT] Building world from server placements ({})...",
                            placements.len()
                        );

                        world_env =
                            world::environment::WorldEnvironment::new(20, 20, hitbox_config.clone(), false)
                                .await;
                        for p in placements {
                            let (model_name, file_name) = &palette[p.model_idx as usize];
                            if !world_env.sim.templates.contains_key(model_name) {
                                if let Some(template) = world::environment::load_glb_template(
                                    &format!("assets/world_models/{}", file_name),
                                )
                                .await
                                {
                                    world_env
                                        .sim
                                        .templates
                                        .insert(model_name.clone(), template);
                                }
                            }
                            let sim_p = world::cluster::ModelPlacement {
                                model: model_name.clone(),
                                file: file_name.clone(),
                                position: p.position,
                                rotation: p.rotation,
                                scale: p.scale,
                                blocks_movement: p.blocks_movement,
                            };
                            world_env.add_placement(&sim_p, &hitbox_config);
                        }
                        game_state = GameState::Playing;
                    }
                }

                if is_key_pressed(KeyCode::Escape) {
                    game_state = GameState::Playing; // Offline mode
                }
            }
            GameState::ClusterEditor => {
                let editor_action = cluster_editor.update();

                if let Some(asset) = cluster_editor.selected_asset() {
                    if !world_env.sim.templates.contains_key(&asset.key) {
                        if let Some(template) =
                            world::environment::load_glb_template(&asset.path).await
                        {
                            world_env.sim.templates.insert(asset.key.clone(), template);
                        }
                    }
                }

                match editor_action {
                    EditorAction::Exit => {
                        game_state = GameState::Playing;
                        continue;
                    }
                    EditorAction::LoadDefault => {
                        world_env = world::environment::WorldEnvironment::new(
                            20,
                            20,
                            hitbox_config.clone(),
                            true,
                        )
                        .await;
                        cluster_editor.clear_map();
                        cluster_editor.import_placements(&world_env.sim.placements);
                    }
                    EditorAction::ClearAll => {
                        cluster_editor.clear_map();
                        world_env = world::environment::WorldEnvironment::new(
                            20,
                            20,
                            hitbox_config.clone(),
                            false,
                        )
                        .await;
                    }
                    EditorAction::PlayTest => {
                        world_env = world::environment::WorldEnvironment::new(
                            20,
                            20,
                            hitbox_config.clone(),
                            false,
                        )
                        .await;
                        for placement in cluster_editor.all_placements() {
                            if !world_env.sim.templates.contains_key(&placement.model) {
                                if let Some(template) = world::environment::load_glb_template(
                                    &format!("assets/world_models/{}", placement.file),
                                )
                                .await
                                {
                                    world_env
                                        .sim
                                        .templates
                                        .insert(placement.model.clone(), template);
                                }
                            }
                            world_env.add_placement(&placement, &hitbox_config);
                        }
                        let start = cluster_editor.playtest_start();
                        hero.pos = start;
                        hero.target_pos = start;
                        hero.current_path.clear();
                        game_state = GameState::Playing;
                    }
                    _ => {}
                }

                set_camera(cluster_editor.camera());
                cluster_editor.draw_3d(&world_env.sim.templates);

                set_default_camera();
                let template_loaded = cluster_editor
                    .selected_asset()
                    .map(|asset| world_env.sim.templates.contains_key(&asset.key))
                    .unwrap_or(false);
                cluster_editor.draw_ui(template_loaded);
            }
            GameState::Playing => {
                if is_key_pressed(KeyCode::C) {
                    creator.confirmed = false;
                    game_state = GameState::CharacterCreation;
                    continue;
                }

                if hero.is_dead {
                    hero.anim.set_state(AnimationState::Death);
                    hero.targeting_state = TargetingState::None;
                }

                for cd in hero.cooldowns.values_mut() {
                    if *cd > 0.0 {
                        *cd -= delta_time;
                    }
                }

                // Network update
                if let Some(ref mut client) = net_client {
                    client.update();

                    // Load textures for newly joined remote players
                    let new_players: Vec<(u64, CharacterAppearanceNet)> = client
                        .remote_appearances
                        .iter()
                        .filter(|(id, _)| !remote_textures.contains_key(id))
                        .map(|(id, app)| (*id, app.clone()))
                        .collect();

                    for (id, app) in new_players {
                        // Convert net appearance to local CharacterAppearance for texture loading
                        let local_app = CharacterAppearance {
                            skin: app.skin.clone(),
                            shoes: app.shoes.clone(),
                            clothes: app.clothes.clone(),
                            gloves: app.gloves.clone(),
                            hairstyle: app.hairstyle.clone(),
                            facial_hair: app.facial_hair.clone(),
                            eye_color: app.eye_color.clone(),
                            eyelashes: app.eyelashes.clone(),
                            headgear: app.headgear.clone(),
                            addon: app.addon.clone(),
                        };
                        let textures =
                            CharacterTextures::from_appearance(&local_app, &catalog).await;
                        remote_textures.insert(id, textures.layers);
                    }

                    // Remove textures for players who left
                    remote_textures.retain(|id, _| client.remote_appearances.contains_key(id));
                    remote_anims.retain(|id, _| client.remote_appearances.contains_key(id));

                    // Check for Game Over
                    if let Some(msg) = &client.latest_msg {
                        if let ServerMessage::GameOver = msg {
                            game_state = GameState::GameOver;
                        }
                    }

                    // Apply server world state
                    if let Some(ref world) = client.latest_world {
                        if let Some(my_id) = client.my_id {
                            for ps in &world.players {
                                if ps.id == my_id {
                                    let srv_pos = vec3(ps.x, 0.0, ps.z);
                                    if (hero.pos - srv_pos).length() > 2.0 {
                                        hero.pos = srv_pos;
                                    } else {
                                        hero.pos = hero.pos.lerp(srv_pos, 4.0 * delta_time);
                                    }
                                    hero.target_pos = vec3(ps.target_x, 0.0, ps.target_z);
                                    hero.casting_timer = ps.casting_timer;
                                    hero.stats.current_hp = ps.current_hp;
                                    hero.stats.max_hp = ps.max_hp;
                                    hero.stats.current_mp = ps.current_mp;
                                    hero.stats.max_mp = ps.max_mp;
                                    hero.is_dead = ps.is_dead;
                                    hero.revive_progress = ps.revive_progress;

                                    // Sync local animation with server authority
                                    let server_state = match ps.anim_state {
                                        0 => AnimationState::Idle,
                                        1 => AnimationState::Walk,
                                        2 => AnimationState::Death,
                                        3 => AnimationState::Sword,
                                        4 => AnimationState::Bow,
                                        5 => AnimationState::Staff,
                                        9 => AnimationState::CarryIdle,
                                        _ => AnimationState::Idle,
                                    };

                                    // Only override if the server says we are doing an action,
                                    // or if we aren't currently playing a local action animation
                                    if ps.anim_state > 1
                                        || hero.anim.state == AnimationState::Idle
                                        || hero.anim.state == AnimationState::Walk
                                    {
                                        hero.anim.set_state(server_state);
                                    }
                                } else {
                                    let pos = remote_player_positions
                                        .entry(ps.id)
                                        .or_insert(vec3(ps.x, 0.0, ps.z));
                                    let srv_pos = vec3(ps.x, 0.0, ps.z);
                                    if (*pos - srv_pos).length() > 2.0 {
                                        *pos = srv_pos;
                                    } else {
                                        *pos = pos.lerp(srv_pos, 15.0 * delta_time);
                                    }

                                    let anim = remote_anims.entry(ps.id).or_insert_with(|| {
                                        AnimationManager::new(SpriteSheetConfig {
                                            columns: 29,
                                            rows: 8,
                                        })
                                    });
                                    // Map anim_state from server to local AnimationState
                                    let state = match ps.anim_state {
                                        0 => AnimationState::Idle,
                                        1 => AnimationState::Walk,
                                        2 => AnimationState::Death,
                                        3 => AnimationState::Sword,
                                        4 => AnimationState::Bow,
                                        5 => AnimationState::Staff,
                                        9 => AnimationState::CarryIdle,
                                        _ => AnimationState::Idle,
                                    };
                                    anim.set_state(state);
                                    if ps.direction <= 7 {
                                        anim.direction =
                                            unsafe { std::mem::transmute(ps.direction) };
                                    }
                                    anim.update(delta_time, 5.0, 1.0);
                                }
                            }
                        }

                        // Spawn effects from server for remote spells
                        let active_ids: std::collections::HashSet<u64> =
                            world.effects.iter().map(|e| e.effect_id).collect();
                        spawned_effect_ids.retain(|id| active_ids.contains(id)); // Cleanup old IDs

                        for ef in &world.effects {
                            if Some(ef.caster_id) == client.my_id {
                                continue;
                            }
                            if spawned_effect_ids.contains(&ef.effect_id) {
                                continue;
                            }

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
                                SpellId::Q => {
                                    effect_manager.spawn_arrow_rain(target, assets.spell_q.clone())
                                }
                                _ => effect_manager.spawn_single_hit(
                                    target,
                                    match spell {
                                        SpellId::W => assets.spell_w.clone(),
                                        SpellId::E => assets.spell_e.clone(),
                                        SpellId::R => assets.spell_r.clone(),
                                        _ => assets.spell_w.clone(),
                                    },
                                    spell,
                                    None,
                                ),
                            }
                        }

                        // Cleanup remote enemy anims for enemies no longer in the snapshot
                        let current_enemy_ids: std::collections::HashSet<u64> = world.enemies.iter().map(|e| e.id).collect();
                        remote_enemy_anims.retain(|id, _| current_enemy_ids.contains(id));
                        remote_enemy_positions.retain(|id, _| current_enemy_ids.contains(id));

                        for en in &world.enemies {
                            let pos = remote_enemy_positions.entry(en.id).or_insert(vec3(en.x, 0.0, en.z));
                            let srv_pos = vec3(en.x, 0.0, en.z);
                            if (*pos - srv_pos).length() > 2.0 {
                                *pos = srv_pos;
                            } else {
                                *pos = pos.lerp(srv_pos, 15.0 * delta_time);
                            }
                        }
                    }
                }

                game_camera.update(hero.pos);
                let targets: Vec<(u64, Vec3)> = if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        world.enemies.iter()
                            .filter(|e| e.health > 0)
                            .map(|e| (e.id, *remote_enemy_positions.get(&e.id).unwrap_or(&vec3(e.x, 0.0, e.z))))
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    enemy_director.active_enemies.iter()
                        .filter(|e| e.state != crate::entities::enemy::EnemyState::Dead)
                        .map(|e| (e.id, e.pos))
                        .collect()
                };

                let cast_event = if !hero.is_dead {
                    handle_input(
                        &mut hero,
                        &game_camera,
                        &mut effect_manager,
                        &targets,
                        &mut combat_text_mgr,
                        &assets,
                        &world_env,
                        &mut indicator_manager,
                    )
                } else {
                    None
                };

                if is_key_pressed(KeyCode::F2) {
                    game_state = GameState::HitboxCalibration;
                }
                if is_key_pressed(KeyCode::F3) {
                    game_state = GameState::ClusterEditor;
                }

                // Send inputs to server
                if let Some(ref client) = net_client {
                    if is_mouse_button_pressed(MouseButton::Right)
                        && hero.targeting_state == TargetingState::None
                        && !hero.is_dead
                    {
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

                    if is_key_pressed(KeyCode::G) {
                        client.send(&ClientMessage::DebugToggleGodMode);
                    }
                }

                // ── Single Player: Apply spell damage locally ──
                if net_client.is_none() {
                    if let Some(ref ev) = cast_event {
                        match ev.spell {
                            SpellId::Q => {
                                // AoE damage (arrow rain)
                                for enemy in &mut enemy_director.active_enemies {
                                    if enemy.state == EnemyState::Dead { continue; }
                                    let dx = enemy.pos.x - ev.target_x;
                                    let dz = enemy.pos.z - ev.target_z;
                                    let dist = (dx * dx + dz * dz).sqrt();
                                    if dist <= 3.0 {
                                        let dmg = 20;
                                        enemy.take_damage(dmg);
                                        combat_text_mgr.spawn(enemy.pos, dmg, true, YELLOW);
                                    }
                                }
                            }
                            spell => {
                                // Unit-target damage (W, E, R)
                                let dmg = match spell {
                                    SpellId::W => 30,
                                    SpellId::E => 20,
                                    SpellId::R => 40,
                                    _ => 0,
                                };
                                // Find the closest enemy to the target position
                                let mut closest_idx: Option<usize> = None;
                                let mut min_dist: f32 = 1.5;
                                for (i, enemy) in enemy_director.active_enemies.iter().enumerate() {
                                    if enemy.state == EnemyState::Dead { continue; }
                                    let dx = enemy.pos.x - ev.target_x;
                                    let dz = enemy.pos.z - ev.target_z;
                                    let dist = (dx * dx + dz * dz).sqrt();
                                    if dist < min_dist {
                                        min_dist = dist;
                                        closest_idx = Some(i);
                                    }
                                }
                                if let Some(idx) = closest_idx {
                                    let hero_pos = hero.pos;
                                    let enemy = &mut enemy_director.active_enemies[idx];
                                    enemy.take_damage(dmg);
                                    combat_text_mgr.spawn(enemy.pos, dmg, true, YELLOW);
                                    // Knockback
                                    if enemy.state != EnemyState::Dead {
                                        let dx = enemy.pos.x - hero_pos.x;
                                        let dz = enemy.pos.z - hero_pos.z;
                                        let dist = (dx * dx + dz * dz).sqrt();
                                        if dist > 0.1 {
                                            enemy.pos.x += (dx / dist) * 0.5;
                                            enemy.pos.z += (dz / dist) * 0.5;
                                        }
                                    }
                                }
                            }
                        }
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
                                hero.pos,
                                desired,
                                PLAYER_RADIUS,
                                world_env.sim.grid_size,
                                world_env.sim.width,
                                world_env.sim.height,
                                &world_env.sim.walkability_grid,
                            );
                            if (new_pos - hero.pos).length() > 0.001 {
                                hero.pos = new_pos;

                                // ── World Boundary Clamping ──
                                let margin = PLAYER_RADIUS + 0.1;
                                let hw =
                                    (world_env.sim.width as f32 * world_env.sim.grid_size) / 2.0 - margin;
                                let hh =
                                    (world_env.sim.height as f32 * world_env.sim.grid_size) / 2.0 - margin;
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

                hero.anim.update(delta_time, hero.stats.get_movement_speed(), hero.stats.get_cast_speed());
                effect_manager.update(delta_time, &enemy_director.active_enemies);
                indicator_manager.update(delta_time);
                
                if net_client.is_none() {
                    enemy_director.update(delta_time, &mut hero, &mut combat_text_mgr, &world_env);
                    if hero.is_dead {
                        game_state = GameState::GameOver;
                    }
                }
                
                combat_text_mgr.update(delta_time);

                // Render
                set_camera(&game_camera.camera);
                // draw_grid(20, 1.0, BLACK, GRAY);
                world_env.draw();

                // ── F1: Pathfinding Debug Overlay ──────────────────────────
                if is_key_pressed(KeyCode::F1) {
                    debug_pathfinding = !debug_pathfinding;
                }

                if debug_pathfinding {
                    let gs = world_env.sim.grid_size;
                    let hw = world_env.sim.width / 2;
                    let hh = world_env.sim.height / 2;

                    // Draw blocked grid cells in semi-transparent red
                    for x in 0..world_env.sim.width {
                        for z in 0..world_env.sim.height {
                            if !world_env.sim.walkability_grid[x as usize][z as usize] {
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
                            vec3(x as f32 * gs, 0.02, hh as f32 * gs),
                            Color::new(1.0, 1.0, 1.0, 0.08),
                        );
                    }
                    for z in -hh..=hh {
                        draw_line_3d(
                            vec3(-hw as f32 * gs, 0.02, z as f32 * gs),
                            vec3(hw as f32 * gs, 0.02, z as f32 * gs),
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
                let _ogre_col = ogre_idle_frames[ogre_frame_idx as usize];

                // ─── Back-to-Front Sorting for EVERYTHING ───
                // This handles Ogres, Hero, Players, and Particles in a single pass to fix all transparency issues
                enum DrawKind {
                    Billboard {
                        tex: Texture2D,
                        src: Rect,
                        size: f32,
                    },
                    Particle {
                        index: usize,
                    },
                }
                struct SortItem {
                    pos: Vec3,
                    kind: DrawKind,
                    dist_sq: f32,
                    color: Color,
                }
                let mut sort_list: Vec<SortItem> = Vec::new();
                let cam_pos = game_camera.camera.position;

                // Add Active Enemies
                for enemy in &enemy_director.active_enemies {
                    for tex in &enemy.textures {
                        let src = enemy.anim.get_source_rect(tex.width(), tex.height());
                        sort_list.push(SortItem {
                            pos: enemy.pos,
                            kind: DrawKind::Billboard {
                                tex: tex.clone(),
                                src,
                                size: enemy.scale,
                            },
                            dist_sq: (cam_pos - enemy.pos).length_squared(),
                            color: WHITE,
                        });
                    }
                }

                // Add Server Enemies
                if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        for en in &world.enemies {
                            let race_idx = en.race_idx;
                            if race_idx < enemy_director.preloaded_races.len() {
                                let race = &enemy_director.preloaded_races[race_idx];
                                let anim = remote_enemy_anims.entry(en.id).or_insert_with(|| {
                                    AnimationManager::new(SpriteSheetConfig {
                                        columns: 29,
                                        rows: 8,
                                    })
                                });
                                
                                // Map anim_state from server to local AnimationState
                                let state = match en.anim_state {
                                    0 => AnimationState::Idle,
                                    1 => AnimationState::Walk,
                                    3 => AnimationState::Sword,
                                    6 => AnimationState::Hurt,
                                    7 => AnimationState::Death,
                                    _ => AnimationState::Idle,
                                };
                                anim.set_state(state);
                                if en.direction <= 7 {
                                    anim.direction = unsafe { std::mem::transmute(en.direction) };
                                }
                                anim.update(delta_time, 3.5, 1.0);

                                let pos = *remote_enemy_positions.get(&en.id).unwrap_or(&vec3(en.x, 0.0, en.z));
                                for tex in &race.textures {
                                    let src = anim.get_source_rect(tex.width(), tex.height());
                                    sort_list.push(SortItem {
                                        pos,
                                        kind: DrawKind::Billboard {
                                            tex: tex.clone(),
                                            src,
                                            size: race.archetype.scale,
                                        },
                                        dist_sq: (cam_pos - pos).length_squared(),
                                        color: WHITE,
                                    });
                                }
                            }
                        }
                    }
                }

                // Add Hero
                for tex in &char_textures.layers {
                    let src = hero.anim.get_source_rect(tex.width(), tex.height());
                    let color = if hero.is_dead {
                        Color::new(0.5, 0.5, 0.7, 0.6)
                    } else {
                        WHITE
                    };
                    sort_list.push(SortItem {
                        pos: hero.pos,
                        kind: DrawKind::Billboard {
                            tex: tex.clone(),
                            src,
                            size: 2.3,
                        },
                        dist_sq: (cam_pos - hero.pos).length_squared(),
                        color,
                    });
                }

                // Add Remote Players
                if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        for ps in &world.players {
                            if Some(ps.id) == client.my_id {
                                continue;
                            }
                            let pos = *remote_player_positions.get(&ps.id).unwrap_or(&vec3(ps.x, 0.0, ps.z));
                            if let Some(textures) = remote_textures.get(&ps.id) {
                                if let Some(anim) = remote_anims.get(&ps.id) {
                                    for tex in textures {
                                        let src = anim.get_source_rect(tex.width(), tex.height());
                                        let color = if ps.is_dead {
                                            Color::new(0.5, 0.5, 0.7, 0.6)
                                        } else {
                                            WHITE
                                        };
                                        sort_list.push(SortItem {
                                            pos,
                                            kind: DrawKind::Billboard {
                                                tex: tex.clone(),
                                                src,
                                                size: 2.0,
                                            },
                                            dist_sq: (cam_pos - pos).length_squared(),
                                            color,
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
                        dist_sq: (cam_pos - p.pos).length_squared() - 0.1, // Add bias to guarantee rendering on top
                        color: WHITE,
                    });
                }

                // Sort back-to-front
                sort_list.sort_by(|a, b| b.dist_sq.partial_cmp(&a.dist_sq).unwrap());

                // Draw everything in order
                for item in sort_list {
                    match item.kind {
                        DrawKind::Billboard { tex, src, size } => {
                            draw_character_billboard_ex(item.pos, &tex, src, cam_pos, size, item.color);
                        }
                        DrawKind::Particle { index } => {
                            effect_manager.draw_particle(&effect_manager.particles[index], cam_pos);
                        }
                    }
                }

                // Draw HP bars (separately as they are solid cubes)
                let enemy_targets: Vec<(Vec3, i32, i32, f32)> = if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        world.enemies.iter().map(|e| {
                            let pos = *remote_enemy_positions.get(&e.id).unwrap_or(&vec3(e.x, 0.0, e.z));
                            let race_scale = if e.race_idx < enemy_director.preloaded_races.len() {
                                enemy_director.preloaded_races[e.race_idx].archetype.scale
                            } else { 2.0 };
                            (pos, e.health, e.max_health, race_scale)
                        }).collect()
                    } else { vec![] }
                } else {
                    enemy_director.active_enemies.iter()
                        .map(|e| (e.pos, e.stats.current_hp, e.stats.max_hp, e.scale))
                        .collect()
                };

                let mut hp_bars_2d = Vec::new();
                for (pos, hp, max_hp, scale) in enemy_targets {
                    let hp_pct = hp as f32 / max_hp as f32;
                    if hp_pct < 1.0 && hp_pct > 0.0 {
                        let matrix = game_camera.camera.matrix();
                        let screen_pos = matrix.project_point3(pos + vec3(0.0, scale * 1.1, 0.0));
                        if screen_pos.z >= 0.0 && screen_pos.z <= 1.0 {
                            let x = (screen_pos.x + 1.0) / 2.0 * screen_width();
                            let y = (1.0 - screen_pos.y) / 2.0 * screen_height();
                            hp_bars_2d.push((x, y, hp_pct));
                        }
                    }
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
                for (x, y, hp_pct) in hp_bars_2d {
                    let bar_w = 40.0;
                    let bar_h = 6.0;
                    draw_rectangle(x - bar_w / 2.0 - 1.0, y - bar_h / 2.0 - 1.0, bar_w + 2.0, bar_h + 2.0, BLACK);
                    draw_rectangle(x - bar_w / 2.0, y - bar_h / 2.0, bar_w, bar_h, RED);
                    draw_rectangle(x - bar_w / 2.0, y - bar_h / 2.0, bar_w * hp_pct, bar_h, GREEN);
                }
                
                ui::hud::draw_hud(&hero, &assets);
                ui::hud::draw_revive_progress(hero.revive_progress);
                combat_text_mgr.draw(&game_camera);

                // Draw ping
                if let Some(ref client) = net_client {
                    draw_text(
                        &format!("Ping: {:.0}ms", client.ping_ms),
                        screen_width() - 140.0,
                        20.0,
                        18.0,
                        GREEN,
                    );
                }

                if let TargetingState::UnitTarget(_) = hero.targeting_state {
                    show_mouse(false);
                    let (mx, my) = mouse_position();
                    draw_texture_ex(
                        &assets.target_mouse,
                        mx - 16.0,
                        my - 16.0,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(32.0, 32.0)),
                            ..Default::default()
                        },
                    );
                } else {
                    show_mouse(true);
                }
            }
            GameState::GameOver => {
                show_mouse(true);
                clear_background(Color::new(0.05, 0.0, 0.0, 1.0));
                let sw = screen_width();
                let sh = screen_height();
                
                draw_text("GAME OVER", sw/2.0 - 150.0, sh/2.0 - 50.0, 60.0, RED);
                draw_text("The party has been defeated.", sw/2.0 - 160.0, sh/2.0 + 10.0, 24.0, GRAY);
                
                let btn_w = 240.0;
                let btn_h = 50.0;
                let btn_x = sw/2.0 - btn_w/2.0;
                let btn_y = sh/2.0 + 80.0;
                
                let mouse = mouse_position();
                let hovered = mouse.0 >= btn_x && mouse.0 <= btn_x + btn_w && mouse.1 >= btn_y && mouse.1 <= btn_y + btn_h;
                
                draw_rectangle(btn_x, btn_y, btn_w, btn_h, if hovered { Color::new(0.3, 0.1, 0.1, 1.0) } else { Color::new(0.2, 0.05, 0.05, 1.0) });
                draw_rectangle_lines(btn_x, btn_y, btn_w, btn_h, 2.0, if hovered { RED } else { Color::new(0.5, 0.1, 0.1, 1.0) });
                
                draw_text("RESTART", btn_x + 75.0, btn_y + 35.0, 28.0, WHITE);
                
                if hovered && is_mouse_button_pressed(MouseButton::Left) {
                    // Close connection and restart
                    net_client = None;
                    creator.confirmed = false;
                    game_state = GameState::CharacterCreation;
                    
                    // Reset hero
                    hero.is_dead = false;
                    hero.revive_progress = 0.0;
                    hero.stats.current_hp = hero.stats.max_hp;
                    hero.stats.current_mp = hero.stats.max_mp;
                    hero.pos = vec3(0.0, 0.0, 0.0);
                    hero.target_pos = vec3(0.0, 0.0, 0.0);
                    hero.current_path.clear();
                    hero.anim.set_state(AnimationState::Idle);
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
    creator.selected_indices[0] = creator
        .catalog
        .skins
        .iter()
        .position(|o| o.name == app.skin);
    if creator.selected_indices[0].is_none() {
        creator.selected_indices[0] = Some(0);
    }

    // Optional layers
    let find = |opts: &[LayerOption], name: &Option<String>| -> Option<usize> {
        name.as_ref()
            .and_then(|n| opts.iter().position(|o| o.name == *n))
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
