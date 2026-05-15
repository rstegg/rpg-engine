use std::collections::BTreeMap;
use macroquad::prelude::*;
use egui_macroquad::egui;
use crate::world::environment::{self, HitboxConfig, GltfTemplate, HitboxPaintedMask};

pub struct HitboxEditor {
    pub models: BTreeMap<String, String>,
    pub selected_idx: usize,
    pub last_paint: Option<([i32; 2], bool)>,
    pub camera_yaw: f32,
    pub camera_distance: f32,
    pub search_text: String,
    pub search_focused: bool,
}

pub enum HitboxEditorAction {
    None,
    Exit,
    Save,
}

impl HitboxEditor {
    pub fn new(hitbox_models: BTreeMap<String, String>) -> Self {
        Self {
            models: hitbox_models,
            selected_idx: 0,
            last_paint: None,
            camera_yaw: 0.0,
            camera_distance: 10.0,
            search_text: String::new(),
            search_focused: false,
        }
    }

    pub fn update(&mut self, _egui_capturing: bool) -> HitboxEditorAction {
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F2) {
            return HitboxEditorAction::Exit;
        }

        if is_key_pressed(KeyCode::S) && (is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)) {
            return HitboxEditorAction::Save;
        }

        if !self.search_focused {
            if is_key_pressed(KeyCode::Down) {
                self.selected_idx = (self.selected_idx + 1).min(self.filtered_keys().len().saturating_sub(1));
                self.last_paint = None;
            }
            if is_key_pressed(KeyCode::Up) {
                self.selected_idx = self.selected_idx.saturating_sub(1);
                self.last_paint = None;
            }
        }

        if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            self.camera_yaw -= get_frame_time() * 1.8;
        }
        if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            self.camera_yaw += get_frame_time() * 1.8;
        }

        let (_, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 && !_egui_capturing {
            self.camera_distance = (self.camera_distance - wheel_y * 0.75).clamp(4.0, 20.0);
        }

        HitboxEditorAction::None
    }

    pub fn filtered_keys(&self) -> Vec<String> {
        self.models
            .keys()
            .filter(|key| {
                self.search_text.is_empty()
                    || key.to_ascii_lowercase().contains(&self.search_text.to_ascii_lowercase())
            })
            .cloned()
            .collect()
    }

    pub fn draw_3d(&mut self, hitbox_config: &mut HitboxConfig, templates: &mut std::collections::HashMap<String, GltfTemplate>, grid_size: f32) {
        let keys = self.filtered_keys();
        if keys.is_empty() {
            return;
        }

        if self.selected_idx >= keys.len() {
            self.selected_idx = 0;
        }

        let current_key = &keys[self.selected_idx];
        let template_key = hitbox_calibration_template_key(current_key);
        let config_key = hitbox_calibration_config_key(current_key);

        let camera_pos = vec3(
            self.camera_yaw.sin() * self.camera_distance,
            5.0,
            self.camera_yaw.cos() * self.camera_distance,
        );
        let camera = Camera3D {
            position: camera_pos,
            target: vec3(0.0, 0.0, 0.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: 45.0,
            ..Default::default()
        };
        set_camera(&camera);

        draw_grid(10, 2.0, GRAY, DARKGRAY);

        if let Some(t) = templates.get(template_key) {
            let mask = environment::ensure_painted_hitbox_entry(
                hitbox_config,
                config_key,
                t,
                grid_size,
            );

            // Handle painting
            let (mx, my) = mouse_position();
            let sw = screen_width();
            let pointer_over_sidebar = mx > sw - 320.0;

            let hovered_cell = if pointer_over_sidebar {
                None
            } else {
                ground_intersection(&camera).map(|pos| {
                    [
                        (pos.x / grid_size).round() as i32,
                        (pos.z / grid_size).round() as i32,
                    ]
                })
            };

            let paint_mode = if pointer_over_sidebar {
                None
            } else if is_mouse_button_down(MouseButton::Left) {
                Some(false) // paint (erase = false)
            } else if is_mouse_button_down(MouseButton::Right) {
                Some(true) // erase (erase = true)
            } else {
                None
            };

            if let (Some(cell), Some(erase)) = (hovered_cell, paint_mode) {
                let in_bounds = cell[0].abs() <= environment::MAX_HITBOX_CELL_EXTENT 
                             && cell[1].abs() <= environment::MAX_HITBOX_CELL_EXTENT;

                let should_apply = self.last_paint
                    .map(|last| last != (cell, erase))
                    .unwrap_or(true);

                if should_apply {
                    if erase {
                        mask.blocked_cells.retain(|&blocked| blocked != cell);
                    } else if in_bounds {
                        if !mask.blocked_cells.contains(&cell) {
                            mask.blocked_cells.push(cell);
                        }
                    }
                    mask.blocked_cells.sort_unstable();
                    self.last_paint = Some((cell, erase));
                }
            } else {
                self.last_paint = None;
            }

            // Draw model
            let meshes = if template_key == "gate" {
                environment::instantiate_gate(
                    t,
                    vec3(0.0, 0.0, 0.0),
                    0.0,
                    2.0,
                    hitbox_calibration_preview_open_progress(current_key),
                )
            } else {
                environment::instantiate(t, vec3(0.0, 0.0, 0.0), 0.0, 2.0)
            };
            for m in meshes {
                draw_mesh(&m);
            }

            // Draw hover indicator
            if let Some(cell) = hovered_cell {
                let cell_x = cell[0] as f32 * grid_size;
                let cell_z = cell[1] as f32 * grid_size;
                draw_cube_wires(
                    vec3(cell_x, 0.05, cell_z),
                    vec3(grid_size, 0.1, grid_size),
                    if paint_mode == Some(false) { GREEN } else if paint_mode == Some(true) { RED } else { YELLOW },
                );
            }

            // Draw current mask
            for cell in &mask.blocked_cells {
                draw_cube(
                    vec3(cell[0] as f32 * grid_size, 0.02, cell[1] as f32 * grid_size),
                    vec3(grid_size * 0.9, 0.05, grid_size * 0.9),
                    None,
                    Color::new(1.0, 0.0, 0.0, 0.5),
                );
            }
        }
        
        set_default_camera();
    }

    pub fn draw_egui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("hitbox_sidebar")
            .default_width(320.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.heading("Hitbox Calibration");
                    ui.add_space(10.0);
                });

                ui.label("Search:");
                let search_res = ui.add(egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Filter models...")
                    .desired_width(ui.available_width()));
                self.search_focused = search_res.has_focus();

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                let keys = self.filtered_keys();
                if keys.is_empty() {
                    ui.colored_label(egui::Color32::GOLD, "No models match the filter.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (idx, key) in keys.iter().enumerate() {
                            let is_selected = self.selected_idx == idx;
                            if ui.selectable_label(is_selected, key).clicked() {
                                self.selected_idx = idx;
                                self.last_paint = None;
                            }
                        }
                    });
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    if ui.button("Exit (Esc)").clicked() {
                        // Action handled in update, but we can set a flag if needed
                    }
                    if ui.button("Save (Ctrl+S)").clicked() {
                        // Action handled in update
                    }
                    ui.label("LMB: Paint | RMB: Erase");
                    ui.label("Arrows/WASD: Orbit Camera");
                    ui.add_space(10.0);
                });
            });

        egui::Window::new("Calibration Info")
            .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let keys = self.filtered_keys();
                if let Some(key) = keys.get(self.selected_idx) {
                    ui.horizontal(|ui| {
                        ui.label("Selected:");
                        ui.colored_label(egui::Color32::WHITE, key);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Config Key:");
                        ui.colored_label(egui::Color32::GREEN, hitbox_calibration_config_key(key));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Template:");
                        ui.colored_label(egui::Color32::CYAN, hitbox_calibration_template_key(key));
                    });
                }
            });
    }
}

// Helpers moved from main.rs
pub fn hitbox_calibration_config_key(key: &str) -> &str {
    match key {
        "gate_closed" => "gate",
        "gate_open" => "gate_open",
        _ => key,
    }
}

pub fn hitbox_calibration_template_key(key: &str) -> &str {
    match key {
        "gate_closed" | "gate_open" => "gate",
        _ => key,
    }
}

fn hitbox_calibration_preview_open_progress(key: &str) -> f32 {
    match key {
        "gate_open" => 1.0,
        _ => 0.0,
    }
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
