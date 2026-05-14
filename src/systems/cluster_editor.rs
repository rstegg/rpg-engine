use std::collections::HashMap;

use macroquad::prelude::*;

use crate::world::chunk::CHUNK_SIZE_M;
use crate::world::cluster::{BIOMES, BiomeRegion, MapDocument, ModelPlacement};
use crate::world::environment::{GltfTemplate, instantiate};
use egui_macroquad::egui;

const MODEL_DIR: &str = "assets/world_models";
const CLUSTER_DIR: &str = "assets/clusters";
const SAVE_PATH: &str = "assets/maps/dev_map.json";
const SIDEBAR_WIDTH: f32 = 300.0;
const TOOLBAR_WIDTH: f32 = 180.0;
const SCALE_STEP: f32 = 0.25;
const MIN_OBJECT_SCALE: f32 = 0.25;
const MAX_OBJECT_SCALE: f32 = 10.0;
const ROTATION_STEP: f32 = std::f32::consts::FRAC_PI_2;
const GRID_STEP: f32 = 2.0;
const CHUNK_BORDER_INSET: f32 = 0.0;
const CHUNK_BORDER_EPSILON: f32 = 0.01;

#[derive(Clone)]
pub struct ModelAsset {
    pub key: String,
    pub file: String,
    pub path: String,
}

#[derive(Clone, Copy, PartialEq)]
enum EditorTab {
    Assets,
    Clusters,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum EditorTool {
    Select,
    Objects,
    Biomes,
}

pub enum EditorAction {
    None,
    Exit,
    PlayTest,
    LoadDefault,
    ClearAll,
}

pub struct ClusterEditor {
    catalog: Vec<ModelAsset>,
    selected_model: usize,
    selected_biome: usize,
    tool: EditorTool,
    rotation: f32,
    scale: f32,
    brush_radius: f32,
    blocks_movement: bool,
    search_text: String,
    search_focused: bool,
    list_scroll: usize,
    camera_target: Vec3,
    camera_height: f32,
    grid_snap_enabled: bool,
    camera: Camera3D,
    map: MapDocument,
    status: String,
    play_button_armed: bool,
    // Selection tool state
    selected_placements: Vec<(usize, usize)>,
    hovered_placement: Option<(usize, usize)>,
    is_dragging: bool,
    drag_start_pos: Vec3,
    drag_original_positions: Vec<Vec3>,
    // UI state
    active_tab: EditorTab,
    load_default_armed: bool,
    clear_all_armed: bool,
    cluster_catalog: Vec<String>,
    cluster_scroll: usize,
}

impl ClusterEditor {
    pub fn new() -> Self {
        let camera_target = Vec3::ZERO;
        let camera_height = 24.0;
        let mut editor = Self {
            catalog: discover_model_assets(),
            selected_model: 0,
            selected_biome: 0,
            tool: EditorTool::Objects,
            rotation: 0.0,
            scale: 1.0,
            brush_radius: 8.0,
            blocks_movement: true,
            search_text: String::new(),
            search_focused: false,
            list_scroll: 0,
            camera_target,
            camera_height,
            grid_snap_enabled: true,
            camera: Camera3D::default(),
            map: MapDocument::new("dev_map"),
            status: String::from("F3 exits. Tab switches Tools. WASD pans. Ctrl+S saves map."),
            play_button_armed: false,
            selected_placements: Vec::new(),
            hovered_placement: None,
            is_dragging: false,
            drag_start_pos: Vec3::ZERO,
            drag_original_positions: Vec::new(),
            active_tab: EditorTab::Assets,
            load_default_armed: false,
            clear_all_armed: false,
            cluster_catalog: discover_clusters(),
            cluster_scroll: 0,
        };
        editor.rebuild_camera();
        editor
    }

    pub fn camera(&self) -> &Camera3D {
        &self.camera
    }

    pub fn selected_asset(&self) -> Option<&ModelAsset> {
        self.catalog.get(self.selected_model)
    }

    pub fn missing_template_assets(
        &self,
        templates: &HashMap<String, GltfTemplate>,
    ) -> Vec<ModelAsset> {
        let mut missing = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for placement in self.all_placements() {
            if templates.contains_key(&placement.model) || !seen.insert(placement.model.clone()) {
                continue;
            }

            let asset = self
                .catalog
                .iter()
                .find(|asset| asset.key == placement.model || asset.file == placement.file)
                .cloned()
                .unwrap_or_else(|| ModelAsset {
                    key: placement.model.clone(),
                    file: placement.file.clone(),
                    path: format!("{MODEL_DIR}/{}", placement.file),
                });

            missing.push(asset);
        }

        missing
    }

    pub fn playtest_start(&self) -> Vec3 {
        self.camera_target
    }

    pub fn all_placements(&self) -> Vec<crate::world::cluster::ModelPlacement> {
        self.map
            .clusters
            .iter()
            .flat_map(|cluster| cluster.placements.iter().cloned())
            .collect()
    }

    pub fn mouse_ground_pos(&self) -> Option<Vec3> {
        let (mouse_x, mouse_y) = mouse_position();
        let ndc_x = (mouse_x / screen_width()) * 2.0 - 1.0;
        let ndc_y = 1.0 - (mouse_y / screen_height()) * 2.0;
        let inv_vp = self.camera.matrix().inverse();
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

    pub fn update(&mut self, egui_capturing: bool) -> EditorAction {
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F3) {
            return EditorAction::Exit;
        }
        if is_key_pressed(KeyCode::F5) {
            return EditorAction::PlayTest;
        }

        if self.play_button_armed {
            self.play_button_armed = false;
            return EditorAction::PlayTest;
        }
        if self.load_default_armed {
            self.load_default_armed = false;
            return EditorAction::LoadDefault;
        }
        if self.clear_all_armed {
            self.clear_all_armed = false;
            return EditorAction::ClearAll;
        }

        self.update_camera();

        if is_key_pressed(KeyCode::Tab) {
            self.tool = match self.tool {
                EditorTool::Objects => EditorTool::Biomes,
                EditorTool::Biomes => EditorTool::Select,
                EditorTool::Select => EditorTool::Objects,
            };
            self.selected_placements.clear();
            self.hovered_placement = None;
        }
        if is_key_pressed(KeyCode::B) {
            self.selected_biome = (self.selected_biome + 1) % BIOMES.len();
            self.map.active_cluster_mut().biome = BIOMES[self.selected_biome].to_string();
        }
        if is_key_pressed(KeyCode::N) {
            self.new_cluster();
        }
        if is_key_pressed(KeyCode::G) && !ctrl_down() {
            self.grid_snap_enabled = !self.grid_snap_enabled;
            self.status = if self.grid_snap_enabled {
                format!("Grid snap enabled ({GRID_STEP:.2}m).")
            } else {
                String::from("Grid snap disabled.")
            };
        }
        if is_key_pressed(KeyCode::S) && ctrl_down() {
            self.save();
        }
        if is_key_pressed(KeyCode::Backspace) && !self.search_focused {
            self.undo();
        }
        if is_key_pressed(KeyCode::Delete) && !self.search_focused {
            if self.tool == EditorTool::Select && !self.selected_placements.is_empty() {
                self.delete_selected();
            } else {
                self.undo();
            }
        }
        if ctrl_down() && is_key_pressed(KeyCode::Delete) {
            self.clear_map();
            return EditorAction::ClearAll;
        }
        if ctrl_down() && is_key_pressed(KeyCode::G) && !self.selected_placements.is_empty() {
            self.group_selection();
        }

        if !egui_capturing {
            match self.tool {
                EditorTool::Objects => self.update_objects(),
                EditorTool::Biomes => self.update_biomes(),
                EditorTool::Select => self.update_selection(),
            }
        }

        EditorAction::None
    }

    pub fn draw_3d(&self, templates: &HashMap<String, GltfTemplate>) {
        if self.grid_snap_enabled {
            draw_grid(40, GRID_STEP, Color::new(0.25, 0.25, 0.25, 0.35), DARKGRAY);
        }
        self.draw_map_bounds();
        self.draw_spawn_indicator();
        self.draw_biome_regions();

        for cluster in &self.map.clusters {
            for placement in &cluster.placements {
                if let Some(template) = templates.get(&placement.model) {
                    for mesh in instantiate(
                        template,
                        placement.pos_vec3(),
                        placement.rotation,
                        placement.scale,
                    ) {
                        draw_mesh(&mesh);
                    }
                }
            }
        }

        if let Some(cursor) = self.mouse_ground_pos() {
            match self.tool {
                EditorTool::Objects => self.draw_object_preview(templates, cursor),
                EditorTool::Biomes => self.draw_brush(cursor),
                EditorTool::Select => self.draw_selection_highlights(templates),
            }
        } else if self.tool == EditorTool::Select {
            self.draw_selection_highlights(templates);
        }
    }

    pub fn draw_egui(&mut self, ctx: &egui::Context, template_loaded: bool) {
        // 1. Left Toolbar
        egui::SidePanel::left("editor_toolbar")
            .resizable(false)
            .default_width(TOOLBAR_WIDTH)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.heading("Tools");
                    ui.add_space(10.0);

                    if ui.selectable_label(self.tool == EditorTool::Select, "Select").clicked() {
                        self.tool = EditorTool::Select;
                    }
                    if ui.selectable_label(self.tool == EditorTool::Objects, "Object").clicked() {
                        self.tool = EditorTool::Objects;
                    }
                    if ui.selectable_label(self.tool == EditorTool::Biomes, "Biome").clicked() {
                        self.tool = EditorTool::Biomes;
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        if ui.button("Default").clicked() {
                            self.load_default_armed = true;
                        }
                        if ui.button("Clear").clicked() {
                            self.clear_map();
                            self.clear_all_armed = true;
                        }
                        if ui.button("Play").clicked() {
                            self.play_button_armed = true;
                        }
                        if ui.button("Save").clicked() {
                            self.save();
                        }
                        ui.add_space(10.0);

                        // Inspector
                        if self.selected_placements.len() == 1 {
                            ui.separator();
                            ui.add_space(10.0);
                            ui.heading("Inspector");
                            ui.add_space(5.0);
                            
                            let (c_idx, p_idx) = self.selected_placements[0];
                            if let Some(p) = self.map.clusters.get_mut(c_idx).and_then(|c| c.placements.get_mut(p_idx)) {
                                ui.horizontal(|ui| {
                                    ui.label("Model:");
                                    ui.label(egui::RichText::new(&p.model).color(egui::Color32::YELLOW));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("File:");
                                    ui.label(egui::RichText::new(&p.file).small());
                                });
                                
                                ui.add_space(5.0);
                                ui.label("Position:");
                                ui.horizontal(|ui| {
                                    ui.label("X");
                                    ui.add(egui::DragValue::new(&mut p.position[0]).speed(0.1));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Y");
                                    ui.add(egui::DragValue::new(&mut p.position[1]).speed(0.1));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Z");
                                    ui.add(egui::DragValue::new(&mut p.position[2]).speed(0.1));
                                });

                                ui.add_space(5.0);
                                ui.horizontal(|ui| {
                                    ui.label("Rotation:");
                                    ui.add(egui::DragValue::new(&mut p.rotation).speed(0.05));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Scale:");
                                    ui.add(egui::DragValue::new(&mut p.scale).speed(0.05).range(0.1..=10.0));
                                });

                                if ui.button("Delete").clicked() {
                                    if let Some(c) = self.map.clusters.get_mut(c_idx) {
                                        c.placements.remove(p_idx);
                                    }
                                    self.selected_placements.clear();
                                }
                            }
                        } else if self.selected_placements.len() > 1 {
                            ui.separator();
                            ui.add_space(10.0);
                            ui.label(format!("{} selected", self.selected_placements.len()));
                            if ui.button("Delete All").clicked() {
                                let mut indices = self.selected_placements.clone();
                                indices.sort_unstable_by(|a, b| b.cmp(a));
                                for (c_idx, p_idx) in indices {
                                    if let Some(c) = self.map.clusters.get_mut(c_idx) {
                                        c.placements.remove(p_idx);
                                    }
                                }
                                self.selected_placements.clear();
                            }
                        }
                    });
                });
            });

        // 2. Right Sidebar
        egui::SidePanel::right("editor_sidebar")
            .resizable(true)
            .default_width(SIDEBAR_WIDTH)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_tab, EditorTab::Assets, "Assets");
                    ui.selectable_value(&mut self.active_tab, EditorTab::Clusters, "Clusters");
                    ui.selectable_value(&mut self.active_tab, EditorTab::Settings, "Settings");
                });

                ui.separator();

                match self.active_tab {
                    EditorTab::Assets => {
                        ui.label("Search:");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.search_text)
                                .hint_text("Filter assets...")
                                .desired_width(ui.available_width()),
                        );
                        if response.changed() {
                            self.list_scroll = 0;
                        }
                        self.search_focused = response.has_focus();

                        if let Some(asset) = self.selected_asset() {
                            let state = if template_loaded { "loaded" } else { "loading" };
                            ui.colored_label(egui::Color32::from_rgb(255, 255, 0), format!("Active: {} ({})", asset.file, state));
                        }

                        ui.separator();

                        let filtered: Vec<_> = self.catalog.iter().enumerate().filter(|(_, a)| {
                            a.key.to_lowercase().contains(&self.search_text.to_lowercase()) ||
                            a.file.to_lowercase().contains(&self.search_text.to_lowercase())
                        }).collect();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (orig_idx, asset) in filtered {
                                let is_selected = self.selected_model == orig_idx;
                                if ui.selectable_label(is_selected, &asset.file).clicked() {
                                    self.selected_model = orig_idx;
                                    self.tool = EditorTool::Objects;
                                    self.status = format!("Selected {}.", asset.file);
                                }
                            }
                        });
                    }
                    EditorTab::Clusters => {
                        ui.heading("Clusters");
                        if ui.button("SAVE ACTIVE AS FILE").clicked() {
                            self.save_active_cluster();
                        }
                        ui.separator();
                        ui.label("Available Files:");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let mut to_load = None;
                            for cluster in &self.cluster_catalog {
                                if ui.button(cluster).clicked() {
                                    to_load = Some(cluster.clone());
                                }
                            }
                            if let Some(name) = to_load {
                                self.load_cluster(&name);
                            }
                        });
                    }
                    EditorTab::Settings => {
                        ui.heading("Properties");
                        ui.label(format!("Scale: {:.2}", self.scale));
                        ui.label(format!("Rotation: {:.1}", self.rotation));
                        ui.label(format!("Brush: {:.1}", self.brush_radius));
                        ui.checkbox(&mut self.grid_snap_enabled, "Grid Snap (G)");
                        ui.label(format!("Grid Step: {:.2}", GRID_STEP));
                        ui.label("Rotate: R/T");
                    }
                }
            });

        // 3. Status Bar (Bottom)
        egui::TopBottomPanel::bottom("editor_status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(0, 255, 0), &self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tool_name = match self.tool {
                        EditorTool::Objects => "Object Paint",
                        EditorTool::Biomes => "Biome Brush",
                        EditorTool::Select => "Selection Tool",
                    };
                    ui.label(format!("Tool: {}", tool_name));
                });
            });
        });
    }

    fn update_camera(&mut self) {
        let dt = get_frame_time();
        let speed_multiplier = if is_key_down(KeyCode::LeftShift) {
            2.6
        } else {
            1.0
        };
        let pan_speed = self.camera_height * 0.65 * dt * speed_multiplier;

        if is_key_down(KeyCode::A) {
            self.camera_target.x -= pan_speed;
        }
        if is_key_down(KeyCode::D) {
            self.camera_target.x += pan_speed;
        }
        if is_key_down(KeyCode::W) {
            self.camera_target.z -= pan_speed;
        }
        if is_key_down(KeyCode::S) && !ctrl_down() {
            self.camera_target.z += pan_speed;
        }
        if is_key_down(KeyCode::Q) {
            self.camera_height = (self.camera_height + pan_speed).clamp(8.0, 80.0);
        }
        if is_key_down(KeyCode::E) {
            self.camera_height = (self.camera_height - pan_speed).clamp(8.0, 80.0);
        }

        self.rebuild_camera();
    }

    pub fn import_placements(&mut self, placements: &[ModelPlacement]) {
        self.map
            .active_cluster_mut()
            .placements
            .extend_from_slice(placements);
        self.status = format!("Imported {} objects from environment.", placements.len());
    }

    fn update_objects(&mut self) {
        if is_key_pressed(KeyCode::RightBracket) {
            self.next_model();
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.prev_model();
        }
        if is_key_pressed(KeyCode::R) {
            self.rotation += ROTATION_STEP;
        }
        if is_key_pressed(KeyCode::T) {
            self.rotation -= ROTATION_STEP;
        }
        if is_key_pressed(KeyCode::Space) {
            self.blocks_movement = !self.blocks_movement;
        }

        let (_, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 {
            let current_steps = (self.scale / SCALE_STEP).round() as i32;
            let next_steps = if wheel_y > 0.0 {
                current_steps + 1
            } else {
                current_steps - 1
            };
            let min_steps = (MIN_OBJECT_SCALE / SCALE_STEP) as i32;
            let max_steps = (MAX_OBJECT_SCALE / SCALE_STEP) as i32;
            self.scale = (next_steps.clamp(min_steps, max_steps) as f32) * SCALE_STEP;
        }
        if is_key_pressed(KeyCode::S) && ctrl_down() {
            self.save();
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            if let (Some(asset), Some(cursor)) =
                (self.selected_asset().cloned(), self.mouse_ground_pos())
            {
                let pos = self.placement_pos(cursor);
                self.map
                    .active_cluster_mut()
                    .placements
                    .push(ModelPlacement {
                        model: asset.key,
                        file: asset.file,
                        position: [pos.x, 0.0, pos.z],
                        rotation: self.rotation,
                        scale: self.scale,
                        blocks_movement: self.blocks_movement,
                    });
                self.status = String::from("Placed object.");
            }
        }
    }

    fn update_biomes(&mut self) {
        let (_, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 {
            self.brush_radius = (self.brush_radius + wheel_y * 0.75).clamp(2.0, 48.0);
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(pos) = self.mouse_ground_pos() {
                let index = self.map.biome_regions.len() + 1;
                self.map.biome_regions.push(BiomeRegion {
                    name: format!("{}_{}", BIOMES[self.selected_biome], index),
                    biome: BIOMES[self.selected_biome].to_string(),
                    center: [pos.x, pos.z],
                    radius: self.brush_radius,
                });
                self.status = String::from("Painted biome region.");
            }
        }
    }

    fn draw_object_preview(&self, templates: &HashMap<String, GltfTemplate>, cursor: Vec3) {
        let snapped = self.placement_pos(cursor);
        if let Some(asset) = self.selected_asset() {
            if let Some(template) = templates.get(&asset.key) {
                for mesh in instantiate(
                    template,
                    vec3(snapped.x, 0.02, snapped.z),
                    self.rotation,
                    self.scale,
                ) {
                    draw_mesh(&mesh);
                }
            }
        }
        let preview_size = if self.grid_snap_enabled {
            GRID_STEP
        } else {
            1.0
        };
        draw_cube_wires(
            vec3(snapped.x, 0.08, snapped.z),
            vec3(preview_size, 0.12, preview_size),
            ORANGE,
        );
    }

    fn draw_biome_regions(&self) {
        for region in &self.map.biome_regions {
            let color = biome_color(&region.biome);
            draw_disc(
                vec3(region.center[0], 0.035, region.center[1]),
                region.radius,
                color,
            );
            draw_ring(
                vec3(region.center[0], 0.055, region.center[1]),
                region.radius,
                48,
                ORANGE,
            );
        }
    }

    fn draw_brush(&self, cursor: Vec3) {
        let color = biome_color(BIOMES[self.selected_biome]);
        draw_disc(vec3(cursor.x, 0.04, cursor.z), self.brush_radius, color);
        draw_ring(
            vec3(cursor.x, 0.07, cursor.z),
            self.brush_radius,
            64,
            YELLOW,
        );
    }

    fn draw_map_bounds(&self) {
        let half_w = self.map.bounds.width * 0.5;
        let half_h = self.map.bounds.height * 0.5;
        let y = 0.025;
        let corners = [
            vec3(-half_w, y, -half_h),
            vec3(half_w, y, -half_h),
            vec3(half_w, y, half_h),
            vec3(-half_w, y, half_h),
        ];
        for i in 0..4 {
            draw_line_3d(corners[i], corners[(i + 1) % 4], GRAY);
        }
    }

    fn draw_spawn_indicator(&self) {
        let p = self.camera_target;
        draw_ring(vec3(p.x, 0.05, p.z), 1.0, 32, SKYBLUE);
        draw_ring(vec3(p.x, 0.08, p.z), 0.8, 24, BLUE);
        draw_line_3d(vec3(p.x, 0.0, p.z), vec3(p.x, 1.5, p.z), SKYBLUE);
        draw_sphere(vec3(p.x, 1.5, p.z), 0.15, None, SKYBLUE);
    }

    fn new_cluster(&mut self) {
        let index = self.map.clusters.len() + 1;
        self.map.clusters.insert(
            0,
            crate::world::cluster::WorldCluster::new(
                format!("cluster_{:03}", index),
                BIOMES[self.selected_biome],
            ),
        );
        self.status = format!("Created cluster_{:03}.", index);
    }

    fn undo(&mut self) {
        match self.tool {
            EditorTool::Objects => {
                self.map.active_cluster_mut().placements.pop();
                self.status = String::from("Removed last object from active cluster.");
            }
            EditorTool::Biomes => {
                self.map.biome_regions.pop();
                self.status = String::from("Removed last biome region.");
            }
            EditorTool::Select => {
                self.selected_placements.clear();
                self.status = String::from("Cleared selection.");
            }
        }
    }

    fn next_model(&mut self) {
        if !self.catalog.is_empty() {
            self.selected_model = (self.selected_model + 1) % self.catalog.len();
        }
    }

    fn prev_model(&mut self) {
        if !self.catalog.is_empty() {
            self.selected_model =
                (self.selected_model + self.catalog.len() - 1) % self.catalog.len();
        }
    }

    fn rebuild_camera(&mut self) {
        self.camera = Camera3D {
            position: vec3(
                self.camera_target.x,
                self.camera_height,
                self.camera_target.z + self.camera_height * 0.78,
            ),
            target: self.camera_target,
            up: Vec3::Y,
            fovy: 45.0,
            ..Default::default()
        };
    }

    fn save(&mut self) {
        if let Err(err) = std::fs::create_dir_all("assets/maps") {
            self.status = format!("Save failed: {}", err);
            return;
        }

        match serde_json::to_string_pretty(&self.map)
            .map_err(|err| err.to_string())
            .and_then(|json| std::fs::write(SAVE_PATH, json).map_err(|err| err.to_string()))
        {
            Ok(_) => self.status = String::from("Saved map."),
            Err(err) => self.status = format!("Save failed: {}", err),
        }
    }

    fn save_active_cluster(&mut self) {
        let cluster = self.map.active_cluster_mut();
        let name = cluster.name.clone();
        if let Err(err) = std::fs::create_dir_all(CLUSTER_DIR) {
            self.status = format!("Save failed: {}", err);
            return;
        }
        let path = format!("{}/{}.json", CLUSTER_DIR, name);
        match serde_json::to_string_pretty(cluster)
            .map_err(|err| err.to_string())
            .and_then(|json| std::fs::write(&path, json).map_err(|err| err.to_string()))
        {
            Ok(_) => {
                self.status = format!("Saved cluster to {}.", name);
                self.cluster_catalog = discover_clusters();
            }
            Err(err) => self.status = format!("Save failed: {}", err),
        }
    }

    fn load_cluster(&mut self, filename: &str) {
        let path = format!("{}/{}", CLUSTER_DIR, filename);
        match std::fs::read_to_string(&path) {
            Ok(json) => {
                match serde_json::from_str::<crate::world::cluster::WorldCluster>(&json) {
                    Ok(cluster) => {
                        self.map.clusters[0] = cluster;
                        self.status = format!("Loaded cluster {}.", filename);
                    }
                    Err(err) => self.status = format!("Load failed: {}", err),
                }
            }
            Err(err) => self.status = format!("Load failed: {}", err),
        }
    }

    fn update_selection(&mut self) {
        let cursor = self.mouse_ground_pos();

        if !self.selected_placements.is_empty() {
            if is_key_pressed(KeyCode::R) {
                self.rotate_selected(ROTATION_STEP);
            }
            if is_key_pressed(KeyCode::T) {
                self.rotate_selected(-ROTATION_STEP);
            }
            if is_key_pressed(KeyCode::Y) {
                self.scale_selected(SCALE_STEP);
            }
            if is_key_pressed(KeyCode::H) {
                self.scale_selected(-SCALE_STEP);
            }

            let (_, wheel_y) = mouse_wheel();
            if wheel_y.abs() > 0.01 {
                if wheel_y > 0.0 {
                    self.scale_selected(SCALE_STEP);
                } else {
                    self.scale_selected(-SCALE_STEP);
                }
            }
        }

        // 1. Hover detection
        self.hovered_placement = None;
        if let Some(pos) = cursor {
            let mut closest_dist_sq = 4.0; // 2.0 units radius
            for (c_idx, cluster) in self.map.clusters.iter().enumerate() {
                for (p_idx, placement) in cluster.placements.iter().enumerate() {
                    let p_pos = placement.pos_vec3();
                    let d2 = (p_pos - pos).length_squared();
                    if d2 < closest_dist_sq {
                        closest_dist_sq = d2;
                        self.hovered_placement = Some((c_idx, p_idx));
                    }
                }
            }
        }

        // 2. Click to select
        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(hovered) = self.hovered_placement {
                let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                if shift {
                    // Toggle selection
                    if let Some(pos) = self.selected_placements.iter().position(|&p| p == hovered) {
                        self.selected_placements.remove(pos);
                    } else {
                        self.selected_placements.push(hovered);
                    }
                } else {
                    // If not already selected, clear and select this one
                    if !self.selected_placements.contains(&hovered) {
                        self.selected_placements.clear();
                        self.selected_placements.push(hovered);
                    }
                }

                // Start dragging if we have a selection
                if !self.selected_placements.is_empty() {
                    self.is_dragging = true;
                    self.drag_start_pos = cursor.unwrap_or(Vec3::ZERO);
                    self.drag_original_positions = self
                        .selected_placements
                        .iter()
                        .map(|&(c, p)| self.map.clusters[c].placements[p].pos_vec3())
                        .collect();
                }
            } else {
                // Clicked empty ground
                self.selected_placements.clear();
            }
        }

        // 3. Dragging
        if self.is_dragging {
            if is_mouse_button_down(MouseButton::Left) {
                if let Some(current_pos) = cursor {
                    let drag_pos = if self.grid_snap_enabled {
                        self.snap_world_pos(current_pos, GRID_STEP)
                    } else {
                        current_pos
                    };
                    let drag_start = if self.grid_snap_enabled {
                        self.snap_world_pos(self.drag_start_pos, GRID_STEP)
                    } else {
                        self.drag_start_pos
                    };
                    let delta = drag_pos - drag_start;
                    for (i, &(c, p)) in self.selected_placements.iter().enumerate() {
                        let orig_pos = self.drag_original_positions[i];
                        let new_pos = orig_pos + delta;
                        self.map.clusters[c].placements[p].position = [new_pos.x, 0.0, new_pos.z];
                    }
                }
            } else {
                self.is_dragging = false;
                self.status = String::from("Moved selection.");
            }
        }
    }

    fn draw_selection_highlights(&self, _templates: &HashMap<String, GltfTemplate>) {
        // Draw hovered highlight
        if let Some((c, p)) = self.hovered_placement {
            let placement = &self.map.clusters[c].placements[p];
            draw_cube_wires(
                placement.pos_vec3() + vec3(0.0, 0.5, 0.0),
                vec3(1.5, 1.0, 1.5) * placement.scale,
                ORANGE,
            );
        }

        // Draw selected highlights
        for &(c, p) in &self.selected_placements {
            let placement = &self.map.clusters[c].placements[p];
            draw_cube_wires(
                placement.pos_vec3() + vec3(0.0, 0.6, 0.0),
                vec3(1.6, 1.2, 1.6) * placement.scale,
                YELLOW,
            );
        }
    }

    fn delete_selected(&mut self) {
        let mut by_cluster: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(c, p) in &self.selected_placements {
            by_cluster.entry(c).or_default().push(p);
        }

        for (c_idx, mut p_indices) in by_cluster {
            p_indices.sort_unstable_by(|a, b| b.cmp(a));
            for p_idx in p_indices {
                if c_idx < self.map.clusters.len()
                    && p_idx < self.map.clusters[c_idx].placements.len()
                {
                    self.map.clusters[c_idx].placements.remove(p_idx);
                }
            }
        }

        self.status = format!("Deleted {} objects.", self.selected_placements.len());
        self.selected_placements.clear();
        self.hovered_placement = None;
    }

    pub fn clear_map(&mut self) {
        self.map = MapDocument::new("dev_map");
        self.selected_placements.clear();
        self.hovered_placement = None;
        self.status = String::from("Map cleared.");
    }

    fn group_selection(&mut self) {
        let count = self.selected_placements.len();
        let mut new_placements = Vec::new();

        let mut by_cluster: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(c, p) in &self.selected_placements {
            by_cluster.entry(c).or_default().push(p);
        }

        for (c_idx, mut p_indices) in by_cluster {
            p_indices.sort_unstable_by(|a, b| b.cmp(a));
            for p_idx in p_indices {
                if c_idx < self.map.clusters.len()
                    && p_idx < self.map.clusters[c_idx].placements.len()
                {
                    new_placements.push(self.map.clusters[c_idx].placements.remove(p_idx));
                }
            }
        }

        let cluster_name = format!("group_{}", get_time() as u32);
        let mut new_cluster = crate::world::cluster::WorldCluster::new(
            cluster_name.clone(),
            BIOMES[self.selected_biome],
        );
        new_cluster.placements = new_placements;
        self.map.clusters.insert(0, new_cluster);

        self.selected_placements.clear();
        self.status = format!("Grouped {} objects into {}.", count, cluster_name);
    }

    fn rotate_selected(&mut self, delta: f32) {
        for &(c, p) in &self.selected_placements {
            if let Some(placement) = self
                .map
                .clusters
                .get_mut(c)
                .and_then(|cluster| cluster.placements.get_mut(p))
            {
                placement.rotation += delta;
            }
        }
        self.status = String::from("Rotated selection.");
    }

    fn scale_selected(&mut self, delta: f32) {
        for &(c, p) in &self.selected_placements {
            if let Some(placement) = self
                .map
                .clusters
                .get_mut(c)
                .and_then(|cluster| cluster.placements.get_mut(p))
            {
                placement.scale = (placement.scale + delta).clamp(MIN_OBJECT_SCALE, MAX_OBJECT_SCALE);
            }
        }
        self.status = String::from("Scaled selection.");
    }

    fn snap_world_pos(&self, pos: Vec3, step: f32) -> Vec3 {
        vec3(
            self.avoid_chunk_border((pos.x / step).round() * step),
            pos.y,
            self.avoid_chunk_border((pos.z / step).round() * step),
        )
    }

    fn placement_pos(&self, pos: Vec3) -> Vec3 {
        if self.grid_snap_enabled {
            self.snap_world_pos(pos, GRID_STEP)
        } else {
            vec3(
                self.avoid_chunk_border(pos.x),
                pos.y,
                self.avoid_chunk_border(pos.z),
            )
        }
    }

    fn avoid_chunk_border(&self, coord: f32) -> f32 {
        let remainder = coord.rem_euclid(CHUNK_SIZE_M);
        if remainder <= CHUNK_BORDER_EPSILON
            || (CHUNK_SIZE_M - remainder) <= CHUNK_BORDER_EPSILON
        {
            if coord.abs() <= CHUNK_BORDER_EPSILON {
                CHUNK_BORDER_INSET
            } else if coord > 0.0 {
                coord - CHUNK_BORDER_INSET
            } else {
                coord + CHUNK_BORDER_INSET
            }
        } else {
            coord
        }
    }
}

fn discover_model_assets() -> Vec<ModelAsset> {
    let mut assets = Vec::new();
    if let Ok(entries) = std::fs::read_dir(MODEL_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("glb") {
                continue;
            }
            let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            assets.push(ModelAsset {
                key: stem.to_string(),
                file: file.to_string(),
                path: format!("{}/{}", MODEL_DIR, file),
            });
        }
    }
    assets.sort_by(|a, b| a.file.cmp(&b.file));
    assets
}

fn discover_clusters() -> Vec<String> {
    let mut clusters = Vec::new();
    if let Ok(entries) = std::fs::read_dir(CLUSTER_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Some(file) = path.file_name().and_then(|name| name.to_str()) {
                clusters.push(file.to_string());
            }
        }
    }
    clusters.sort();
    clusters
}

fn biome_color(biome: &str) -> Color {
    match biome {
        "forest" => Color::new(0.05, 0.32, 0.12, 0.34),
        "wetland" => Color::new(0.05, 0.22, 0.36, 0.34),
        "rocky" => Color::new(0.34, 0.32, 0.30, 0.34),
        "camp" => Color::new(0.45, 0.26, 0.08, 0.34),
        _ => Color::new(0.12, 0.42, 0.10, 0.30),
    }
}

fn ctrl_down() -> bool {
    is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
}

fn draw_ring(center: Vec3, radius: f32, segments: usize, color: Color) {
    for i in 0..segments {
        let a1 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        draw_line_3d(
            center + vec3(a1.cos() * radius, 0.0, a1.sin() * radius),
            center + vec3(a2.cos() * radius, 0.0, a2.sin() * radius),
            color,
        );
    }
}

fn draw_disc(center: Vec3, radius: f32, color: Color) {
    let segments = 48;
    let mut vertices = Vec::with_capacity(segments + 1);
    vertices.push(macroquad::models::Vertex::new2(center, Vec2::ZERO, color));
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        vertices.push(macroquad::models::Vertex::new2(
            center + vec3(angle.cos() * radius, 0.0, angle.sin() * radius),
            Vec2::ZERO,
            color,
        ));
    }

    let mut indices = Vec::with_capacity(segments * 3);
    for i in 1..=segments {
        let next = if i == segments { 1 } else { i + 1 };
        indices.extend_from_slice(&[0, i as u16, next as u16]);
    }

    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}
