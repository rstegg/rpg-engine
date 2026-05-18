#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
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
use egui_macroquad::egui;
use net::client::NetClient;
use net::protocol::*;
use std::collections::BTreeMap;
use systems::cluster_editor::*;
use systems::indicators::*;
use systems::input::*;
use systems::hitbox_editor::*;
use ui::character_creator::*;
use world::environment::{self, HitboxConfig, GltfTemplate};
use world::chunk::{ChunkedWorld, ChunkCoord};
use std::time::Instant;
use world::pathfinding::{self, slide_move_world};
use arboard::Clipboard;



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
    Login,
    Connecting,
    CharacterSelect,
    CharacterCreation,
    Playing,
    Reconnecting,
    HitboxCalibration,
    ClusterEditor,
    GameOver,
}

/// Apply a dark RPG-styled theme to the egui context.
fn apply_rpg_egui_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    // Panel / window backgrounds — nearly invisible, we draw our own
    visuals.window_fill = egui::Color32::from_rgba_unmultiplied(10, 8, 20, 230);
    visuals.panel_fill = egui::Color32::TRANSPARENT;
    visuals.window_stroke = egui::Stroke::NONE;
    let cr = egui::CornerRadius::same(3);
    // Inactive widget (unfocused field)
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(14, 12, 26);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 58, 95));
    visuals.widgets.inactive.corner_radius = cr;
    // Hovered
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(22, 18, 40);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(180, 140, 60));
    visuals.widgets.hovered.corner_radius = cr;
    // Active / focused text field
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(18, 14, 34);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(210, 165, 45));
    visuals.widgets.active.corner_radius = cr;
    // Open (the state egui uses while a text field is being edited)
    visuals.widgets.open.bg_fill = egui::Color32::from_rgb(18, 14, 34);
    visuals.widgets.open.bg_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(210, 165, 45));
    visuals.widgets.open.corner_radius = cr;
    // Selection highlight (text selection)
    visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(180, 138, 40, 90);
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(210, 165, 45));
    // Cursor
    visuals.text_cursor.stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 175, 55));
    ctx.set_visuals(visuals);
    // Slightly larger default text size
    ctx.style_mut(|s| {
        s.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(16.0, egui::FontFamily::Proportional),
        );
    });
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
    models.insert(
        "gate_closed".to_string(),
        "assets/world_models/fence_gate.glb".to_string(),
    );
    models.insert(
        "gate_open".to_string(),
        "assets/world_models/fence_gate.glb".to_string(),
    );

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

    // Hide raw gate aliases in calibration mode. Gates should be painted through
    // the explicit state entries so the editor only exposes meaningful choices.
    models.remove("gate");
    models.remove("fence_gate");

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

fn window_conf() -> Conf {
    Conf {
        window_title: "2.5D Indie RPG".to_owned(),
        fullscreen: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
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
        name: String::new(),
        pos: vec3(0.0, 0.0, 0.0),
        target_pos: vec3(0.0, 0.0, 0.0),
        current_path: Vec::new(),
        stats: Stats::new(10, 15, 10),
        scale: 1.0,
        base_appearance: initial_appearance.clone(),
        current_appearance: initial_appearance.clone(),
        equipment: crate::entities::item::Equipment::new(),
        anim: AnimationManager::new(config),
        targeting_state: TargetingState::None,
        casting_timer: 0.0,
        stuck_timer: 0.0,
        cooldowns: std::collections::HashMap::new(),
        is_dead: false,
        revive_progress: 0.0,
        last_click_time: Instant::now(),
    };

    let mut hitbox_config: HitboxConfig = match std::fs::read_to_string("hitbox_config.json") {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => HitboxConfig::new(),
    };
    let hitbox_models = discover_hitbox_calibration_models();

    let mut chunk_world = ChunkedWorld::new(hitbox_config.clone(), std::collections::HashMap::new(), 12345);
    let mut world_env = world::environment::WorldEnvironment::new(20, 20, hitbox_config.clone(), false).await;
    // Pre-load built-in templates into chunk_world
    for (key, file) in environment::builtin_template_defs() {
        if let Some(t) = environment::load_glb_template(&format!("assets/world_models/{}", file)).await {
            chunk_world.templates.insert(key.to_string(), t);
        }
    }
    let mut debug_pathfinding = false;

    let mut game_camera = GameCamera::new(hero.pos);
    let mut previous_mouse_pos = mouse_position();
    let mut effect_manager = EffectManager::new();
    let mut indicator_manager = IndicatorManager::new();
    let mut cluster_editor = ClusterEditor::new();

    let mut enemy_director = EnemyDirector::new(&catalog, config).await;
    let mut combat_text_mgr = ui::combat_text::CombatTextManager::new();

    let mut game_state = GameState::Login;
    let mut net_client: Option<NetClient> = None;
    let mut server_addr = String::from("71.127.210.74:7878");
    let mut login_username = String::new();
    let mut is_local_server = false;
    let mut login_error: Option<String> = None;
    let mut character_list: Vec<net::protocol::CharacterSummaryNet> = Vec::new();
    let mut char_select_scroll = 0usize;
    let mut reconnect_timer = 0.0_f32;
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
    let mut hitbox_editor = HitboxEditor::new(hitbox_models.clone());
    // Ensure all templates have an entry in hitbox_config
    for key in hitbox_models.keys() {
        hitbox_config
            .entry(hitbox_calibration_config_key(key).to_string())
            .or_insert(environment::HitboxConfigEntry::Legacy(1.0));
    }

    let mut clipboard = Clipboard::new().ok();
    let mut show_escape_menu = false;

    let mut selected_building_id: Option<u64> = None;
    let mut placement_mode: Option<String> = None;

    loop {
        clear_background(DARKGRAY);
        let delta_time = get_frame_time().min(0.05);

        // ── egui UI pass (single call per frame) ──
        // Capture any values we need back from the closure.
        let mut egui_login_changed = false;
        let mut egui_wants_pointer = false;
        egui_macroquad::ui(|ctx| {
            egui_wants_pointer = ctx.wants_pointer_input() || ctx.is_pointer_over_area();
            apply_rpg_egui_theme(ctx);

            // egui -> macroquad (Copy)
            let copy_text = ctx.output(|o| o.copied_text.clone());
            if !copy_text.is_empty() {
                if let Some(ref mut cb) = clipboard {
                    let _ = cb.set_text(copy_text);
                }
            }
            // macroquad -> egui (Paste)
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::V)) {
                if let Some(ref mut cb) = clipboard {
                    if let Ok(text) = cb.get_text() {
                        ctx.input_mut(|i| i.events.push(egui::Event::Text(text)));
                    }
                }
            }

            if game_state == GameState::Login {
                let sw = screen_width();
                let sh = screen_height();
                let field_w = 320.0_f32;
                // Position a transparent, frame-less Area over where the labels + fields go
                egui::Area::new(egui::Id::new("login_fields"))
                    .fixed_pos(egui::pos2((sw - field_w) / 2.0, sh / 2.0 - 55.0))
                    .show(ctx, |ui| {
                        ui.set_width(field_w);
                        ui.visuals_mut().extreme_bg_color = egui::Color32::from_rgb(10, 8, 20);

                        ui.label(egui::RichText::new("Username").color(egui::Color32::from_rgb(180, 170, 210)).size(14.0));
                        let ur = ui.add(
                            egui::TextEdit::singleline(&mut login_username)
                                .desired_width(field_w)
                                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                                .hint_text("Enter username…")
                        );
                        if ur.changed() { egui_login_changed = true; }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut is_local_server, true, "Local Server");
                            ui.radio_value(&mut is_local_server, false, "Online Server");
                        });

                        if is_local_server {
                            server_addr = String::from("127.0.0.1:7878");
                        } else {
                            server_addr = String::from("71.127.210.74:7878");
                        }
                    });
            }

            if game_state == GameState::CharacterCreation {
                let sw = screen_width();
                // Replicate the layout math from update_and_draw to position the name field
                let margin = 20.0_f32;
                let usable_w = sw - margin * 2.0;
                let panel_left_w = (usable_w * 0.18).max(160.0);
                let preview_x = margin + panel_left_w + 15.0;
                let preview_w = (usable_w * 0.28).max(200.0);
                let name_field_x = preview_x + 10.0;
                let name_field_y = 70.0 + 28.0; // top_y + offset
                let name_field_w = preview_w - 20.0;

                egui::Area::new(egui::Id::new("char_name_field"))
                    .fixed_pos(egui::pos2(name_field_x, name_field_y))
                    .show(ctx, |ui| {
                        ui.set_width(name_field_w);
                        ui.visuals_mut().extreme_bg_color = egui::Color32::from_rgb(10, 8, 20);
                        ui.add(
                            egui::TextEdit::singleline(&mut creator.character_name)
                                .desired_width(name_field_w)
                                .font(egui::FontId::new(17.0, egui::FontFamily::Proportional))
                                .hint_text("Enter Name…")
                        );
                    });

                if creator.show_class_popup {
                    egui::Window::new("Save Unit Definition")
                        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                        .collapsible(false)
                        .resizable(false)
                        .show(ctx, |ui| {
                            ui.set_width(300.0);
                            
                            ui.horizontal(|ui| {
                                ui.label("Class (Race):");
                                ui.text_edit_singleline(&mut creator.class_name_input);
                            });
                            ui.horizontal(|ui| {
                                ui.label("Unit Name:");
                                ui.text_edit_singleline(&mut creator.unit_name_input);
                            });
                            ui.add_space(5.0);
                            
                            ui.horizontal(|ui| {
                                ui.label("Cost:");
                                ui.add(egui::DragValue::new(&mut creator.cost_input).speed(1));
                            });
                            ui.add_space(5.0);

                            ui.label("Base Stats:");
                            ui.horizontal(|ui| {
                                ui.label("Strength:");
                                ui.add(egui::DragValue::new(&mut creator.str_input).speed(1));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Agility:");
                                ui.add(egui::DragValue::new(&mut creator.agi_input).speed(1));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Intelligence:");
                                ui.add(egui::DragValue::new(&mut creator.int_input).speed(1));
                            });
                            
                            ui.add_space(5.0);
                            ui.horizontal(|ui| {
                                ui.label("Scale:");
                                ui.add(egui::Slider::new(&mut creator.scale_input, 0.5..=3.0));
                            });
                            ui.checkbox(&mut creator.is_hero_input, "Is MOBA Hero");
                            
                            ui.add_space(15.0);
                            ui.horizontal(|ui| {
                                if ui.button("Cancel").clicked() {
                                    creator.show_class_popup = false;
                                }
                                if ui.button("Save Unit").clicked() {
                                    let def = UnitDefinition {
                                        unit_name: creator.unit_name_input.clone(),
                                        unit_class: creator.class_name_input.clone(),
                                        cost: creator.cost_input,
                                        strength: creator.str_input,
                                        agility: creator.agi_input,
                                        intelligence: creator.int_input,
                                        scale: creator.scale_input,
                                        is_hero: creator.is_hero_input,
                                        appearance: creator.appearance.clone(),
                                    };
                                    let class_folder = if def.unit_class.is_empty() {
                                        "UnknownClass".to_string()
                                    } else {
                                        def.unit_class.replace(" ", "_")
                                    };
                                    let filename = if def.unit_name.is_empty() {
                                        "UnnamedUnit".to_string()
                                    } else {
                                        def.unit_name.replace(" ", "_")
                                    };
                                    
                                    let dir_path = format!("classes/{}", class_folder);
                                    let _ = std::fs::create_dir_all(&dir_path);
                                    let _ = def.save_to_file(&format!("{}/{}.json", dir_path, filename));
                                    creator.show_class_popup = false;
                                }
                            });
                        });
                }
            }

            if game_state == GameState::ClusterEditor {
                let template_loaded = cluster_editor
                    .selected_asset()
                    .map(|asset| world_env.sim.templates.contains_key(&asset.key))
                    .unwrap_or(false);
                cluster_editor.draw_egui(ctx, template_loaded);
            }

            if game_state == GameState::HitboxCalibration {
                hitbox_editor.draw_egui(ctx);
            }

            if show_escape_menu {
                egui::Window::new("Escape Menu")
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgba_unmultiplied(15, 12, 28, 245)))
                    .show(ctx, |ui| {
                        ui.set_width(240.0);
                        ui.vertical_centered(|ui| {
                            ui.add_space(15.0);
                            ui.heading(egui::RichText::new("SYSTEM MENU").color(egui::Color32::from_rgb(200, 160, 80)).strong());
                            ui.add_space(25.0);

                            let btn_size = egui::vec2(180.0, 32.0);

                            if ui.add_sized(btn_size, egui::Button::new(egui::RichText::new("RESUME").size(16.0))).clicked() {
                                show_escape_menu = false;
                            }

                            ui.add_space(12.0);

                            if ui.add_sized(btn_size, egui::Button::new(egui::RichText::new("CHARACTER SELECT").size(16.0))).clicked() {
                                if let Some(ref mut nc) = net_client {
                                    nc.send(&ClientMessage::Disconnect);
                                    nc.reset_for_login();
                                    // Send login again to get back to character list
                                    nc.send(&ClientMessage::Login { 
                                        version: PROTOCOL_VERSION, 
                                        username: login_username.clone() 
                                    });
                                }
                                game_state = GameState::Connecting;
                                show_escape_menu = false;
                            }

                            ui.add_space(12.0);

                            if ui.add_sized(btn_size, egui::Button::new(egui::RichText::new("LOGOUT").size(16.0).color(egui::Color32::from_rgb(220, 80, 80)))).clicked() {
                                if let Some(ref nc) = net_client {
                                    nc.disconnect();
                                }
                                net_client = None;
                                game_state = GameState::Login;
                                show_escape_menu = false;
                            }

                            ui.add_space(15.0);
                        });
                    });
            }

            if game_state == GameState::Playing && !show_escape_menu {
                if let Some(building_id) = selected_building_id {
                    let mut b_type = None;
                    let mut b_hp = 0;
                    let mut b_max_hp = 1;
                    let mut b_prog = 0.0;
                    let mut b_queue = false;
                    if let Some(ref client) = net_client {
                        if let Some(ref world) = client.latest_world {
                            if let Some(b) = world.buildings.iter().find(|b| b.id == building_id) {
                                b_type = Some(b.building_type.clone());
                                b_hp = b.health;
                                b_max_hp = b.max_health;
                                b_prog = b.construction_progress;
                                b_queue = b.active_production.is_some();
                            }
                        }
                    }
                    if let Some(b_type) = b_type {
                        egui::Window::new("Command Card")
                            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
                            .collapsible(false)
                            .resizable(false)
                            .title_bar(false)
                            .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgba_unmultiplied(15, 12, 28, 245)))
                            .show(ctx, |ui| {
                                ui.set_width(400.0);
                                ui.vertical_centered(|ui| {
                                    ui.heading(if b_type == "tent_detailedOpen.glb" { "Barracks" } else { "Building" });
                                    ui.add_space(5.0);
                                    if b_prog < 1.0 {
                                        ui.label(format!("Constructing: {:.0}%", b_prog * 100.0));
                                    } else {
                                        ui.label(format!("HP: {}/{}", b_hp, b_max_hp));
                                        if b_queue {
                                            ui.label(egui::RichText::new("Producing unit...").color(egui::Color32::YELLOW));
                                        }
                                        ui.add_space(10.0);
                                        ui.horizontal(|ui| {
                                            if b_type == "tent_detailedOpen.glb" {
                                                if ui.button("Spawn Unit").clicked() {
                                                    if let Some(ref nc) = net_client {
                                                        nc.send(&ClientMessage::QueueProduction {
                                                            building_id,
                                                            unit_type: "Hunter".to_string(),
                                                        });
                                                    }
                                                }
                                            }
                                        });
                                    }
                                });
                            });
                    } else {
                        // Deselect if building no longer exists
                        selected_building_id = None;
                    }
                }
            }
        });

        // ── Character Buffer Management ──
        // Drain macroquad char buffer in states where egui is handling input,
        // and in gameplay to prevent WASD leaking into fields.
        // NOTE: ClusterEditor now uses egui, so we drain it there too if needed, 
        // but macroquad still has its own search_text in ClusterEditor. 
        // We moved that to egui::TextEdit so it's safe to drain.
        if game_state != GameState::HitboxCalibration && game_state != GameState::ClusterEditor {
            while let Some(_) = get_char_pressed() {}
        }

        match game_state {
            GameState::Login => {
                show_mouse(true);
                let sw = screen_width();
                let sh = screen_height();
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.06, 0.06, 0.10, 1.0));

                let title = "RPG ENGINE";
                let tw = measure_text(title, None, 48, 1.0).width;
                draw_text(title, (sw - tw) / 2.0, sh / 2.0 - 120.0, 48.0, WHITE);

                let subtitle = "Enter your credentials to connect";
                let stw = measure_text(subtitle, None, 18, 1.0).width;
                draw_text(subtitle, (sw - stw) / 2.0, sh / 2.0 - 80.0, 18.0, Color::new(0.5, 0.5, 0.7, 1.0));

                let (mx, my) = mouse_position();
                let clicked = is_mouse_button_pressed(MouseButton::Left);

                // egui renders the Username and Server fields — no manual drawing needed here.
                // Clear login error when the user edits either field.
                if egui_login_changed {
                    login_error = None;
                }

                // Error message
                if let Some(ref err) = login_error {
                    let etw = measure_text(err, None, 18, 1.0).width;
                    draw_text(err, (sw - etw) / 2.0, sh / 2.0 + 65.0, 18.0, RED);
                }

                // Connect button
                let btn_w = 200.0;
                let btn_h = 45.0;
                let btn_x = (sw - btn_w) / 2.0;
                let btn_y = sh / 2.0 + 80.0;
                let btn_hover = mx >= btn_x && mx <= btn_x + btn_w && my >= btn_y && my <= btn_y + btn_h;
                draw_rectangle(btn_x, btn_y, btn_w, btn_h, if btn_hover { Color::new(0.2, 0.35, 0.2, 1.0) } else { Color::new(0.12, 0.25, 0.12, 1.0) });
                draw_rectangle_lines(btn_x, btn_y, btn_w, btn_h, 2.0, GREEN);
                let ct = "PLAY ONLINE";
                let ctw = measure_text(ct, None, 24, 1.0).width;
                draw_text(ct, btn_x + (btn_w - ctw) / 2.0, btn_y + 30.0, 24.0, WHITE);

                if (btn_hover && clicked) || is_key_pressed(KeyCode::Enter) {
                    if !login_username.is_empty() {
                        match NetClient::connect(&server_addr, &login_username) {
                            Ok(client) => {
                                net_client = Some(client);
                                game_state = GameState::Connecting;
                                login_error = None;
                            }
                            Err(e) => {
                                eprintln!("Connection failed: {}", e);
                                login_error = Some(format!("Connection failed: {}", e));
                            }
                        }
                    } else {
                        login_error = Some("Please enter a username".to_string());
                    }
                }

                // Play Offline button
                let off_btn_y = btn_y + btn_h + 15.0;
                let off_btn_hover = mx >= btn_x && mx <= btn_x + btn_w && my >= off_btn_y && my <= off_btn_y + btn_h;
                draw_rectangle(btn_x, off_btn_y, btn_w, btn_h, if off_btn_hover { Color::new(0.2, 0.2, 0.25, 1.0) } else { Color::new(0.12, 0.12, 0.18, 1.0) });
                draw_rectangle_lines(btn_x, off_btn_y, btn_w, btn_h, 1.0, GRAY);
                let ot = "PLAY OFFLINE";
                let otw = measure_text(ot, None, 20, 1.0).width;
                draw_text(ot, btn_x + (btn_w - otw) / 2.0, off_btn_y + 30.0, 20.0, LIGHTGRAY);

                if off_btn_hover && clicked {
                    net_client = None;
                    game_state = GameState::Playing;
                }

                // Quit button
                let quit_btn_y = off_btn_y + btn_h + 15.0;
                let quit_btn_hover = mx >= btn_x && mx <= btn_x + btn_w && my >= quit_btn_y && my <= quit_btn_y + btn_h;
                draw_rectangle(btn_x, quit_btn_y, btn_w, btn_h, if quit_btn_hover { Color::new(0.3, 0.1, 0.1, 1.0) } else { Color::new(0.2, 0.08, 0.08, 1.0) });
                draw_rectangle_lines(btn_x, quit_btn_y, btn_w, btn_h, 1.0, Color::new(0.5, 0.2, 0.2, 1.0));
                let qt = "QUIT GAME";
                let qtw = measure_text(qt, None, 20, 1.0).width;
                draw_text(qt, btn_x + (btn_w - qtw) / 2.0, quit_btn_y + 30.0, 20.0, Color::new(0.8, 0.4, 0.4, 1.0));

                if quit_btn_hover && clicked {
                    std::process::exit(0);
                }
            }
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
                if confirmed && !creator.confirmed_already_sent {
                    if creator.character_name.trim().is_empty() {
                        creator.error_message = Some("Please enter a character name".to_string());
                        creator.confirmed = false;
                    } else {
                        if let Some(ref nc) = net_client {
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
                            nc.send(&ClientMessage::CreateCharacter {
                                name: creator.character_name.trim().to_string(),
                                appearance: net_app,
                            });
                            creator.confirmed_already_sent = true;
                            creator.error_message = None;
                        } else {
                            if let Ok(_) = creator.appearance.save_to_file("character.json") {
                                hero.name = creator.character_name.trim().to_string();
                                creator.confirmed = true;
                            } else {
                                creator.error_message = Some("Failed to save character".to_string());
                                creator.confirmed = false;
                            }
                        }
                    }
                }

                if creator.confirmed {
                    if let Some(ref mut nc) = net_client {
                        nc.update();
                        if let Some(created) = nc.pending_character_created.take() {
                            character_list.push(created);
                            game_state = GameState::CharacterSelect;
                            creator.confirmed = false;
                            creator.confirmed_already_sent = false;
                            creator.character_name.clear();
                            creator.error_message = None;
                        }
                        if let Some(err) = nc.pending_create_error.take() {
                            eprintln!("Create failed: {}", err);
                            creator.error_message = Some(format!("Create failed: {}", err));
                            creator.confirmed = false;
                            creator.confirmed_already_sent = false;
                        }
                    } else {
                        hero.base_appearance = creator.appearance.clone();
                        hero.update_appearance_from_equipment();
                        char_textures = CharacterTextures::from_appearance(&hero.current_appearance, &catalog).await;
                        game_state = GameState::Playing;
                        creator.confirmed = false;
                        creator.confirmed_already_sent = false;
                        creator.character_name.clear();
                        creator.error_message = None;
                    }
                }
                if is_key_pressed(KeyCode::Escape) {
                    if net_client.is_some() {
                        game_state = GameState::CharacterSelect;
                    } else {
                        game_state = GameState::Playing;
                    }
                }
            }
            GameState::HitboxCalibration => {
                let action = hitbox_editor.update(egui_wants_pointer);
                match action {
                    HitboxEditorAction::Exit => {
                        rebuild_world_from_current_placements(&mut world_env, &hitbox_config).await;
                        game_state = GameState::Playing;
                    }
                    HitboxEditorAction::Save => {
                        if let Ok(json) = serde_json::to_string_pretty(&hitbox_config) {
                            let _ = std::fs::write("hitbox_config.json", json);
                        }
                        rebuild_world_from_current_placements(&mut world_env, &hitbox_config).await;
                    }
                    HitboxEditorAction::None => {}
                }

                hitbox_editor.draw_3d(&mut hitbox_config, &mut world_env.sim.templates, world_env.sim.grid_size);
            }
            GameState::Connecting => {
                let sw = screen_width();
                let sh = screen_height();
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.06, 0.06, 0.10, 1.0));

                if let Some(ref mut nc) = net_client {
                    nc.update();

                    if nc.connection_timed_out {
                        draw_text("CONNECTION FAILED", sw / 2.0 - 130.0, sh / 2.0 - 20.0, 28.0, RED);
                        draw_text("Server unreachable. Press ENTER to retry or ESC to go back.", sw / 2.0 - 250.0, sh / 2.0 + 20.0, 18.0, GRAY);
                        if is_key_pressed(KeyCode::Enter) {
                            match NetClient::connect(&server_addr, &login_username) {
                                Ok(client) => { net_client = Some(client); }
                                Err(e) => eprintln!("Retry failed: {}", e),
                            }
                        }
                        if is_key_pressed(KeyCode::Escape) {
                            net_client = None;
                            game_state = GameState::Login;
                        }
                    } else {
                        let elapsed = nc.connecting_elapsed();
                        draw_text("CONNECTING...", sw / 2.0 - 80.0, sh / 2.0 - 20.0, 28.0, WHITE);
                        draw_text(&format!("{:.1}s", elapsed), sw / 2.0 - 15.0, sh / 2.0 + 15.0, 18.0, YELLOW);

                        // Check for character list response
                        if let Some(chars) = nc.pending_characters.take() {
                            character_list = chars;
                            game_state = GameState::CharacterSelect;
                        }
                    }
                }

                if is_key_pressed(KeyCode::Escape) {
                    net_client = None;
                    game_state = GameState::Login;
                }
            }
            GameState::CharacterSelect => {
                show_mouse(true);
                let sw = screen_width();
                let sh = screen_height();
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.06, 0.06, 0.10, 1.0));

                // Poll for updates
                if let Some(ref mut nc) = net_client {
                    nc.update();
                    if let Some(chars) = nc.pending_characters.take() {
                        character_list = chars;
                    }
                    if let Some(created) = nc.pending_character_created.take() {
                        character_list.push(created);
                    }
                    if let Some(del_id) = nc.pending_character_deleted.take() {
                        character_list.retain(|c| c.id != del_id);
                    }
                }

                let title = "CHARACTER SELECT";
                let tw = measure_text(title, None, 36, 1.0).width;
                draw_text(title, (sw - tw) / 2.0, 50.0, 36.0, WHITE);

                draw_text(&format!("Logged in as: {}", login_username), 20.0, 50.0, 18.0, Color::new(0.5, 0.5, 0.7, 1.0));

                let (mx, my) = mouse_position();
                let clicked = is_mouse_button_pressed(MouseButton::Left);

                // Character cards
                let card_w = 500.0;
                let card_h = 70.0;
                let card_x = (sw - card_w) / 2.0;
                let start_y = 90.0;

                for (i, ch) in character_list.iter().enumerate() {
                    let cy = start_y + i as f32 * (card_h + 10.0);
                    let hovered = mx >= card_x && mx <= card_x + card_w && my >= cy && my <= cy + card_h;
                    let bg = if hovered { Color::new(0.15, 0.18, 0.28, 1.0) } else { Color::new(0.10, 0.10, 0.18, 1.0) };
                    draw_rectangle(card_x, cy, card_w, card_h, bg);
                    draw_rectangle_lines(card_x, cy, card_w, card_h, 1.0, if hovered { WHITE } else { Color::new(0.25, 0.25, 0.4, 1.0) });

                    draw_text(&ch.name, card_x + 15.0, cy + 28.0, 22.0, WHITE);
                    draw_text(&format!("HP: {}/{}", ch.current_hp, ch.max_hp), card_x + 15.0, cy + 50.0, 16.0, Color::new(0.4, 0.8, 0.4, 1.0));
                    draw_text(&format!("MP: {}/{}", ch.current_mp, ch.max_mp), card_x + 160.0, cy + 50.0, 16.0, Color::new(0.4, 0.6, 0.9, 1.0));

                    // Delete button
                    let del_x = card_x + card_w - 80.0;
                    let del_y = cy + 10.0;
                    let del_hover = mx >= del_x && mx <= del_x + 65.0 && my >= del_y && my <= del_y + 25.0;
                    draw_rectangle(del_x, del_y, 65.0, 25.0, if del_hover { Color::new(0.5, 0.1, 0.1, 1.0) } else { Color::new(0.3, 0.08, 0.08, 1.0) });
                    draw_text("Delete", del_x + 8.0, del_y + 18.0, 14.0, RED);
                    if del_hover && clicked {
                        if let Some(ref nc) = net_client {
                            nc.send(&ClientMessage::DeleteCharacter { character_id: ch.id });
                        }
                    }

                    // Select on click (not on delete button)
                    if hovered && clicked && !del_hover {
                        if let Some(ref nc) = net_client {
                            nc.send(&ClientMessage::SelectCharacter { character_id: ch.id });
                            hero.name = ch.name.clone();
                        }
                    }
                }

                // "Create New" button
                let new_y = start_y + character_list.len() as f32 * (card_h + 10.0) + 10.0;
                let new_hover = mx >= card_x && mx <= card_x + card_w && my >= new_y && my <= new_y + 50.0;
                draw_rectangle(card_x, new_y, card_w, 50.0, if new_hover { Color::new(0.12, 0.25, 0.12, 1.0) } else { Color::new(0.08, 0.15, 0.08, 1.0) });
                draw_rectangle_lines(card_x, new_y, card_w, 50.0, 2.0, GREEN);
                let nt = "+ CREATE NEW CHARACTER";
                let ntw = measure_text(nt, None, 22, 1.0).width;
                draw_text(nt, card_x + (card_w - ntw) / 2.0, new_y + 32.0, 22.0, GREEN);

                if new_hover && clicked {
                    creator.reset();
                    creator.needs_reload = true;
                    game_state = GameState::CharacterCreation;
                }

                // Check if server accepted a character selection (JoinAccepted)
                if let Some(ref mut nc) = net_client {
                    if nc.connected {
                        println!("[CLIENT] Character selection accepted, entering world...");
                        // Clear chunks and entities for fresh start, but preserve templates
                        let old_templates = std::mem::take(&mut chunk_world.templates);
                        chunk_world = ChunkedWorld::new(hitbox_config.clone(), old_templates, 12345);
                        
                        // Ensure built-in templates are present (if they weren't already)
                        for (key, file) in environment::builtin_template_defs() {
                            if !chunk_world.templates.contains_key(key) {
                                if let Some(t) = environment::load_glb_template(&format!("assets/world_models/{}", file)).await {
                                    chunk_world.templates.insert(key.to_string(), t);
                                }
                            }
                        }

                        char_textures = CharacterTextures::from_appearance(&creator.appearance, &catalog).await;
                        game_state = GameState::Playing;
                    }
                }

                // Logout
                if is_key_pressed(KeyCode::Escape) {
                    if let Some(ref nc) = net_client { nc.disconnect(); }
                    net_client = None;
                    game_state = GameState::Login;
                }
            }
            GameState::Reconnecting => {
                let sw = screen_width();
                let sh = screen_height();
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.05, 0.0, 0.0, 0.85));

                if let Some(ref nc) = net_client {
                    let attempt = nc.reconnect_attempts;
                    let max = nc.max_reconnect_attempts;
                    draw_text("CONNECTION LOST", sw / 2.0 - 120.0, sh / 2.0 - 40.0, 32.0, RED);
                    draw_text(&format!("Reconnecting... (attempt {}/{})", attempt, max), sw / 2.0 - 150.0, sh / 2.0 + 10.0, 20.0, YELLOW);
                }

                reconnect_timer -= delta_time;
                if reconnect_timer <= 0.0 {
                    reconnect_timer = 3.0;
                    if let Some(ref mut nc) = net_client {
                        if nc.reconnect_attempts >= nc.max_reconnect_attempts {
                            // Give up
                            draw_text("Unable to reconnect.", sw / 2.0 - 100.0, sh / 2.0 + 50.0, 20.0, GRAY);
                            net_client = None;
                            game_state = GameState::Login;
                        } else {
                            let _ = nc.reconnect();
                        }
                    }
                }

                // Check if reconnected
                if let Some(ref mut nc) = net_client {
                    nc.update();
                    if nc.connected {
                        println!("[CLIENT] Reconnected successfully.");
                        game_state = GameState::Playing;
                    }
                }

                if is_key_pressed(KeyCode::Escape) {
                    net_client = None;
                    game_state = GameState::Login;
                }
            }
            GameState::ClusterEditor => {
                let editor_action = cluster_editor.update(egui_wants_pointer);

                for asset in cluster_editor.missing_template_assets(&world_env.sim.templates) {
                    if let Some(template) = world::environment::load_glb_template(&asset.path).await
                    {
                        world_env.sim.templates.insert(asset.key, template);
                    }
                }

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
                        chunk_world = ChunkedWorld::new(hitbox_config.clone(), std::collections::HashMap::new(), 12345);
                        // Pre-load templates
                        for placement in cluster_editor.all_placements() {
                            if !chunk_world.templates.contains_key(&placement.model) {
                                if let Some(template) = world::environment::load_glb_template(
                                    &format!("assets/world_models/{}", placement.file),
                                )
                                .await
                                {
                                    chunk_world.templates.insert(placement.model.clone(), template);
                                }
                            }
                        }
                        // Group placements by chunk
                        let mut chunks_map: std::collections::HashMap<ChunkCoord, Vec<world::cluster::ModelPlacement>> = std::collections::HashMap::new();
                        for p in cluster_editor.all_placements() {
                            let coord = ChunkCoord::from_world_pos(p.pos_vec3());
                            chunks_map.entry(coord).or_default().push(p);
                        }

                        for (coord, placements) in chunks_map {
                            chunk_world.insert_chunk(coord, world::chunk::BiomeType::Grassland, placements);
                        }
                        
                        let start = cluster_editor.playtest_start();
                        hero.pos = start;
                        hero.target_pos = start;
                        hero.current_path.clear();
                        game_state = GameState::Playing;
                    }
                    _ => {}
                }

                gl_use_default_material();
                set_camera(cluster_editor.camera());
                cluster_editor.draw_3d(&world_env.sim.templates);

                set_default_camera();
                let template_loaded = cluster_editor
                    .selected_asset()
                    .map(|asset| world_env.sim.templates.contains_key(&asset.key))
                    .unwrap_or(false);
                // cluster_editor.draw_ui is now legacy
            }
            GameState::Playing => {
                // Hot reload world data
                chunk_world.check_hot_reload();
                world_env.check_hot_reload();

                if is_key_pressed(KeyCode::Escape) {
                    if placement_mode.is_some() {
                        placement_mode = None;
                    } else if selected_building_id.is_some() {
                        selected_building_id = None;
                    } else {
                        show_escape_menu = !show_escape_menu;
                    }
                }

                if is_key_pressed(KeyCode::B) && !show_escape_menu && !egui_wants_pointer {
                    placement_mode = Some("tent_detailedOpen.glb".to_string());
                }

                if is_key_pressed(KeyCode::C) && !show_escape_menu {
                    creator.reset();
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

                    // Detect disconnect → transition to reconnect
                    if client.server_lost {
                        reconnect_timer = 0.0; // try immediately
                        game_state = GameState::Reconnecting;
                    }

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

                    // ─── Chunk Streaming ───
                    while !client.pending_chunks.is_empty() {
                        let chunk_data = client.pending_chunks.remove(0);
                        let coord = ChunkCoord::new(chunk_data.coord_x, chunk_data.coord_z);
                        for (name, file) in &chunk_data.palette {
                            if !chunk_world.templates.contains_key(name) {
                                if let Some(t) = world::environment::load_glb_template(&format!("assets/world_models/{}", file)).await {
                                    chunk_world.templates.insert(name.clone(), t);
                                }
                            }
                        }
                        let placements = chunk_data.placements.iter().map(|p| {
                            let (model_name, file_name) = &chunk_data.palette[p.model_idx as usize];
                            world::cluster::ModelPlacement {
                                model: model_name.clone(), file: file_name.clone(),
                                position: p.position, rotation: p.rotation,
                                scale: p.scale, blocks_movement: p.blocks_movement,
                            }
                        }).collect();
                        let biome = match chunk_data.biome.as_str() {
                            "Town" => world::chunk::BiomeType::Town,
                            "Forest" => world::chunk::BiomeType::Forest,
                            "Rocky" => world::chunk::BiomeType::Rocky,
                            "Wetland" => world::chunk::BiomeType::Wetland,
                            _ => world::chunk::BiomeType::Grassland,
                        };
                        chunk_world.insert_chunk(coord, biome, placements);
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
                                        // Use faster lerp for local player to keep pathfinding starts accurate
                                        hero.pos = hero.pos.lerp(srv_pos, 20.0 * delta_time);
                                    }
                                     // Authoritative sync: Always follow the server's current target for animations
                                     let srv_target = vec3(ps.target_x, 0.0, ps.target_z);
                                     
                                     // If the server's final destination has changed, sync the whole path
                                     let srv_dest = ps.current_path.last().map(|(x, z)| vec3(*x, 0.0, *z));
                                     let my_dest = hero.current_path.last().copied();
                                     
                                     if srv_dest != my_dest || hero.current_path.is_empty() {
                                         if let Some(dest) = srv_dest {
                                             hero.current_path = ps.current_path.iter().map(|(x, z)| vec3(*x, 0.0, *z)).collect();
                                         } else {
                                             hero.current_path.clear();
                                         }
                                     }

                                     // Always use the server's authoritative target for the look-direction,
                                     // BUT only if we haven't clicked very recently (allow 150ms latency window)
                                     // OR if the server has clearly already updated to a new target
                                     if hero.last_click_time.elapsed().as_secs_f32() > 0.15 || (srv_target - hero.target_pos).length() > 0.1 {
                                         hero.target_pos = srv_target;
                                     }

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
                                    // Cleanup remote player data for players no longer in the snapshot
                                    let current_player_ids: std::collections::HashSet<PlayerId> = world.players.iter().map(|p| p.id).collect();
                                    remote_player_positions.retain(|id, _| current_player_ids.contains(id));
                                    remote_anims.retain(|id, _| current_player_ids.contains(id));
                                    remote_textures.retain(|id, _| current_player_ids.contains(id));

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

                            // Store paths for debug rendering (if we had a map for it)
                            // For now let's just focus on the hero.
                        }

                        // Sync Gates
                        for gs in &world.gates {
                            let srv_pos = vec3(gs.x, 0.0, gs.z);
                            if let Some(gate) = chunk_world.gates.iter_mut().find(|g| (g.position - srv_pos).length_squared() < 0.1) {
                                gate.open_progress = gs.open_progress;
                                if gate.open_progress > 0.9 {
                                    gate.state = world::chunk::GateState::Open;
                                } else if gate.open_progress < 0.1 {
                                    gate.state = world::chunk::GateState::Closed;
                                }
                            }
                        }
                    }
                }

                // Local chunk generation (single player, or client-side missing chunks)
                chunk_world.update(hero.pos, 2, true);

                let current_mouse_pos = mouse_position();
                game_camera.update(hero.pos);
                if debug_pathfinding && is_mouse_button_down(MouseButton::Middle) {
                    let delta_x = current_mouse_pos.0 - previous_mouse_pos.0;
                    let delta_y = current_mouse_pos.1 - previous_mouse_pos.1;
                    if delta_x.abs() > f32::EPSILON || delta_y.abs() > f32::EPSILON {
                        game_camera.orbit(delta_x, delta_y, hero.pos);
                    }
                }
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

                let cast_event = if !hero.is_dead && !show_escape_menu {
                    handle_input(
                        &mut hero,
                        &game_camera,
                        &mut effect_manager,
                        &targets,
                        &mut combat_text_mgr,
                        &assets,
                        &chunk_world,
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
                    if is_mouse_button_pressed(MouseButton::Left) && !show_escape_menu && !egui_wants_pointer {
                        if let Some(ref p_type) = placement_mode {
                            if let Some(pos) = game_camera.get_mouse_ray_intersection() {
                                let gs = world::chunk::GRID_SIZE;
                                let snap_x = (pos.x / gs).round() * gs;
                                let snap_z = (pos.z / gs).round() * gs;
                                client.send(&ClientMessage::StartConstruction {
                                    building_type: p_type.clone(),
                                    x: snap_x,
                                    z: snap_z,
                                });
                                placement_mode = None;
                            }
                        } else {
                            // Selection
                            if let Some(pos) = game_camera.get_mouse_ray_intersection() {
                                let mut selected = None;
                                if let Some(ref world) = client.latest_world {
                                    for b in &world.buildings {
                                        let dx = b.x - pos.x;
                                        let dz = b.z - pos.z;
                                        if (dx * dx + dz * dz).sqrt() < 3.0 { // 3.0 radius for selection
                                            selected = Some(b.id);
                                            break;
                                        }
                                    }
                                }
                                selected_building_id = selected;
                            }
                        }
                    }

                    if is_mouse_button_pressed(MouseButton::Right)
                        && hero.targeting_state == TargetingState::None
                        && !hero.is_dead
                        && !show_escape_menu
                    {
                        if let Some(mut pos) = game_camera.get_mouse_ray_intersection() {
                            let is_walkable = chunk_world.is_walkable_with_radius(pos, 0.35);
                            if !is_walkable {
                                use rpg_engine::world::pathfinding::find_closest_walkable_fn;
                                pos = find_closest_walkable_fn(pos, 50, 0.5, |p| chunk_world.is_walkable_with_radius(p, 0.35));
                            }
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
                        println!("[CLIENT] Sending CastSpell {} to ({:.1}, {:.1})", spell_id, ev.target_x, ev.target_z);
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
                                    if dist <= 3.5 {
                                        let dmg = 20;
                                        enemy.take_damage(dmg);
                                        combat_text_mgr.spawn(enemy.pos, dmg, true, YELLOW);
                                    }
                                }
                            }
                            _ => {
                                // Unit-target damage (W, E, R)
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
                                    let dmg = match ev.spell {
                                        SpellId::W => 30,
                                        SpellId::E => 20,
                                        SpellId::R => 40,
                                        _ => 0,
                                    };
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

                // Gate animation can be predicted locally even in networked play.
                chunk_world.update_gates(&[hero.pos], delta_time);

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
                            let new_pos = world::pathfinding::slide_move_world(
                                hero.pos,
                                desired,
                                PLAYER_RADIUS,
                                |p| chunk_world.is_walkable(p)
                            );
                            if (new_pos - hero.pos).length() > 0.001 {
                                hero.pos = new_pos;

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
                        // Aggressively prune nodes that we've already reached or passed locally
                        // This ensures the debug path stays ahead of the character visually
                        while !hero.current_path.is_empty() {
                            let dist_to_node = (hero.current_path[0] - hero.pos).length();
                            // If we are close to the first node, or if the server's target is already past it
                            if dist_to_node < 0.6 || (hero.current_path[0] - hero.target_pos).length() < 0.1 {
                                hero.current_path.remove(0);
                            } else {
                                break;
                            }
                        }

                        let to_target = hero.target_pos - hero.pos;
                        if to_target.length() > 0.1 {
                            hero.anim.set_state(AnimationState::Walk);
                            hero.anim.set_direction(to_target);
                        } else {
                            hero.anim.set_state(AnimationState::Idle);
                        }
                    }
                }

                hero.anim.update(delta_time, hero.stats.get_movement_speed(), hero.stats.get_cast_speed());
                effect_manager.update(delta_time, &enemy_director.active_enemies);
                indicator_manager.update(delta_time);
                
                if net_client.is_none() {
                    enemy_director.update(delta_time, &mut hero, &mut combat_text_mgr, &chunk_world);
                    if hero.is_dead {
                        game_state = GameState::GameOver;
                    }
                }
                
                combat_text_mgr.update(delta_time);

                // Quick Debug hotkey to equip an item and update visuals
                if is_key_pressed(KeyCode::I) {
                    let debug_helm = crate::entities::item::Item::new(
                        "item_kheshig_helm",
                        "Kheshig Helm",
                        crate::entities::item::ItemSlot::Head,
                        "KheshigHelm",
                    );
                    hero.equipment.equip(debug_helm);
                    hero.update_appearance_from_equipment();
                    char_textures = CharacterTextures::from_appearance(&hero.current_appearance, &catalog).await;
                }

                // Render
                set_camera(&game_camera.camera);
                // draw_grid(20, 1.0, BLACK, GRAY);
                chunk_world.draw();

                // Draw buildings
                if let Some(ref client) = net_client {
                    if let Some(ref world) = client.latest_world {
                        for b in &world.buildings {
                            if let Some(template) = chunk_world.templates.get(&b.building_type) {
                                let scale = if b.building_type == "tent_detailedOpen.glb" { 2.0 } else { 1.0 };
                                let color = if b.construction_progress < 1.0 {
                                    Color::new(0.5, 0.5, 0.8, 0.8) // Transparent blue-ish while building
                                } else {
                                    WHITE
                                };
                                world::environment::draw_template(template, vec3(b.x, 0.0, b.z), 0.0, scale, color);

                                // Selection Ring
                                if Some(b.id) == selected_building_id {
                                    for i in 0..16 {
                                        let angle1 = (i as f32 / 16.0) * std::f32::consts::PI * 2.0;
                                        let angle2 = ((i + 1) as f32 / 16.0) * std::f32::consts::PI * 2.0;
                                        draw_line_3d(
                                            vec3(b.x + angle1.cos() * 3.0, 0.1, b.z + angle1.sin() * 3.0),
                                            vec3(b.x + angle2.cos() * 3.0, 0.1, b.z + angle2.sin() * 3.0),
                                            GREEN
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Draw Placement Ghost
                if let Some(ref p_type) = placement_mode {
                    if let Some(pos) = game_camera.get_mouse_ray_intersection() {
                        let gs = world::chunk::GRID_SIZE;
                        let snap_x = (pos.x / gs).round() * gs;
                        let snap_z = (pos.z / gs).round() * gs;
                        if let Some(template) = chunk_world.templates.get(p_type) {
                            let scale = if p_type == "tent_detailedOpen.glb" { 2.0 } else { 1.0 };
                            world::environment::draw_template(template, vec3(snap_x, 0.0, snap_z), 0.0, scale, Color::new(0.0, 1.0, 0.0, 0.5));
                        }
                    }
                }
                
                // (Note: Debug pathfinding overlay disabled for infinite world for now)

                // ── F1: Pathfinding Debug Overlay ──────────────────────────
                if is_key_pressed(KeyCode::F1) {
                    debug_pathfinding = !debug_pathfinding;
                    if !debug_pathfinding {
                        game_camera.reset_view(hero.pos);
                    }
                }

                if debug_pathfinding {
                    let gs = world::chunk::GRID_SIZE;

                    // Draw blocked grid cells from all loaded chunks in semi-transparent red.
                    for (coord, chunk) in &chunk_world.chunks {
                        let center = coord.world_center();
                        for x in 0..world::chunk::GRID_WIDTH {
                            for z in 0..world::chunk::GRID_WIDTH {
                                if !chunk.walkability[x as usize][z as usize] {
                                    let wx = center.x
                                        + (x - world::chunk::GRID_WIDTH / 2) as f32 * gs;
                                    let wz = center.z
                                        + (z - world::chunk::GRID_WIDTH / 2) as f32 * gs;
                                    draw_cube(
                                        vec3(wx, 0.05, wz),
                                        vec3(gs * 0.95, 0.1, gs * 0.95),
                                        None,
                                        Color::new(1.0, 0.0, 0.0, 0.45),
                                    );
                                }
                            }
                        }
                    }

                    for gate in &chunk_world.gates {
                        if gate.open_progress < 0.6 {
                            draw_cube(
                                gate.position + vec3(0.0, 0.12, 0.0),
                                vec3(0.5, 0.24, 2.0),
                                None,
                                Color::new(1.0, 0.55, 0.0, 0.35),
                            );
                        }
                    }                    // Draw current path as connected yellow lines + waypoint spheres
                    let path = &hero.current_path;
                    
                    // First segment: hero → current target (only if far enough to matter)
                    let dist_to_target = (hero.target_pos - hero.pos).length();
                    if dist_to_target > 0.15 {
                        draw_line_3d(
                            hero.pos + vec3(0.0, 0.5, 0.0),
                            hero.target_pos + vec3(0.0, 0.5, 0.0),
                            YELLOW,
                        );
                        draw_sphere(hero.target_pos + vec3(0.0, 0.5, 0.0), 0.15, None, YELLOW);
                    }

                    let mut prev = hero.target_pos;
                    // Skip waypoints that are effectively our current target_pos
                    for waypoint in path.iter() {
                        if (*waypoint - hero.target_pos).length() < 0.1 { continue; }
                        
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

                    // Show grid lines faintly around the player.
                    let grid_radius = 20;
                    let hero_gx = (hero.pos.x / gs).round() as i32;
                    let hero_gz = (hero.pos.z / gs).round() as i32;
                    for x in (hero_gx - grid_radius)..=(hero_gx + grid_radius) {
                        draw_line_3d(
                            vec3(x as f32 * gs, 0.02, (hero_gz - grid_radius) as f32 * gs),
                            vec3(x as f32 * gs, 0.02, (hero_gz + grid_radius) as f32 * gs),
                            Color::new(1.0, 1.0, 1.0, 0.08),
                        );
                    }
                    for z in (hero_gz - grid_radius)..=(hero_gz + grid_radius) {
                        draw_line_3d(
                            vec3((hero_gx - grid_radius) as f32 * gs, 0.02, z as f32 * gs),
                            vec3((hero_gx + grid_radius) as f32 * gs, 0.02, z as f32 * gs),
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
                            size: 2.3 * hero.scale,
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
                let mut nameplates_2d = Vec::new();
                for (pos, hp, max_hp, scale) in enemy_targets {
                    let hp_pct = hp as f32 / max_hp as f32;
                    if hp_pct < 1.0 && hp_pct > 0.0 {
                        let matrix = game_camera.camera.matrix();
                        let screen_pos = matrix.project_point3(pos + vec3(0.0, scale * 1.1, 0.0));
                        if screen_pos.z >= -1.0 && screen_pos.z <= 1.0 {
                            let x = (screen_pos.x + 1.0) / 2.0 * screen_width();
                            let y = (1.0 - screen_pos.y) / 2.0 * screen_height();
                            hp_bars_2d.push((x, y, hp_pct));
                        }
                    }
                }

                // Project player nameplates
                {
                    let matrix = game_camera.camera.matrix();
                    // Local Hero
                    if !hero.name.is_empty() {
                        let screen_pos = matrix.project_point3(hero.pos + vec3(0.0, 2.5, 0.0));
                        if screen_pos.z >= -1.0 && screen_pos.z <= 1.0 {
                            let x = (screen_pos.x + 1.0) / 2.0 * screen_width();
                            let y = (1.0 - screen_pos.y) / 2.0 * screen_height();
                            let hp_pct = hero.stats.current_hp as f32 / hero.stats.max_hp as f32;
                            nameplates_2d.push((x, y, hero.name.clone(), WHITE, hp_pct));
                        }
                    }
                    // Remote Players
                    if let Some(ref client) = net_client {
                        if let Some(ref world) = client.latest_world {
                            for ps in &world.players {
                                if Some(ps.id) == client.my_id { continue; }
                                if let Some(name) = client.remote_names.get(&ps.id) {
                                    let pos = *remote_player_positions.get(&ps.id).unwrap_or(&vec3(ps.x, 0.0, ps.z));
                                    let screen_pos = matrix.project_point3(pos + vec3(0.0, 2.3, 0.0));
                                    if screen_pos.z >= -1.0 && screen_pos.z <= 1.0 {
                                        let x = (screen_pos.x + 1.0) / 2.0 * screen_width();
                                        let y = (1.0 - screen_pos.y) / 2.0 * screen_height();
                                        let hp_pct = ps.current_hp as f32 / ps.max_hp as f32;
                                        nameplates_2d.push((x, y, name.clone(), Color::new(0.8, 0.9, 1.0, 1.0), hp_pct));
                                    }
                                }
                            }
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

                for (x, y, name, color, hp_pct) in nameplates_2d {
                    let font_size = 20.0;
                    let tw = measure_text(&name, None, font_size as u16, 1.0).width;
                    let bg_w = tw.max(40.0) + 10.0;
                    let bg_h = 32.0; // Taller to fit HP bar
                    
                    // Background
                    draw_rectangle(x - bg_w / 2.0, y - bg_h / 2.0, bg_w, bg_h, Color::new(0.0, 0.0, 0.0, 0.6));
                    
                    // Name
                    draw_text(&name, x - tw / 2.0, y + 2.0, font_size, color);
                    
                    // HP Bar
                    let hp_bar_w = bg_w - 10.0;
                    let hp_bar_h = 4.0;
                    let hp_bar_y = y + 8.0;
                    draw_rectangle(x - hp_bar_w / 2.0, hp_bar_y, hp_bar_w, hp_bar_h, RED);
                    draw_rectangle(x - hp_bar_w / 2.0, hp_bar_y, hp_bar_w * hp_pct.clamp(0.0, 1.0), hp_bar_h, GREEN);
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
                    // Disconnect cleanly → back to login
                    if let Some(ref nc) = net_client {
                        nc.disconnect();
                    }
                    net_client = None;
                    game_state = GameState::Login;
                    
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

        // ── Render egui on top of everything ──
        egui_macroquad::draw();

        previous_mouse_pos = mouse_position();
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
}
