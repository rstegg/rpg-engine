use std::collections::HashMap;

use macroquad::prelude::*;

use crate::world::cluster::{BIOMES, BiomeRegion, MapDocument, ModelPlacement};
use crate::world::environment::{GltfTemplate, instantiate};

const MODEL_DIR: &str = "assets/world_models";
const SAVE_PATH: &str = "assets/maps/dev_map.json";
const PANEL_WIDTH: f32 = 330.0;
const ROW_HEIGHT: f32 = 22.0;

#[derive(Clone)]
pub struct ModelAsset {
    pub key: String,
    pub file: String,
    pub path: String,
}

#[derive(Clone, Copy, PartialEq)]
enum EditorTool {
    Objects,
    Biomes,
}

pub enum EditorAction {
    None,
    Exit,
    PlayTest,
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
    camera: Camera3D,
    map: MapDocument,
    status: String,
    play_button_armed: bool,
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
            camera: Camera3D::default(),
            map: MapDocument::new("dev_map"),
            status: String::from(
                "F3 exits. Tab switches Objects/Biomes. WASD pans. Ctrl+S saves map.",
            ),
            play_button_armed: false,
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

    pub fn update(&mut self) -> EditorAction {
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F3) {
            return EditorAction::Exit;
        }
        if is_key_pressed(KeyCode::F5) {
            return EditorAction::PlayTest;
        }

        self.update_panel();
        if self.play_button_armed {
            self.play_button_armed = false;
            return EditorAction::PlayTest;
        }
        self.update_camera();

        if is_key_pressed(KeyCode::Tab) {
            self.tool = match self.tool {
                EditorTool::Objects => EditorTool::Biomes,
                EditorTool::Biomes => EditorTool::Objects,
            };
        }
        if is_key_pressed(KeyCode::B) {
            self.selected_biome = (self.selected_biome + 1) % BIOMES.len();
            self.map.active_cluster_mut().biome = BIOMES[self.selected_biome].to_string();
        }
        if is_key_pressed(KeyCode::N) {
            self.new_cluster();
        }
        if is_key_pressed(KeyCode::S) && ctrl_down() {
            self.save();
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.undo();
        }

        if !self.pointer_over_panel() {
            match self.tool {
                EditorTool::Objects => self.update_objects(),
                EditorTool::Biomes => self.update_biomes(),
            }
        }

        EditorAction::None
    }

    pub fn draw_3d(&self, templates: &HashMap<String, GltfTemplate>) {
        self.draw_map_bounds();
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
            }
        }
    }

    pub fn draw_ui(&self, template_loaded: bool) {
        let tool = match self.tool {
            EditorTool::Objects => "Objects",
            EditorTool::Biomes => "Biomes",
        };
        let cluster = self.map.active_cluster();
        let cluster_name = cluster.map(|c| c.name.as_str()).unwrap_or("none");
        let placement_count = self
            .map
            .clusters
            .iter()
            .map(|cluster| cluster.placements.len())
            .sum::<usize>();

        draw_rectangle(
            0.0,
            0.0,
            PANEL_WIDTH,
            screen_height(),
            Color::new(0.0, 0.0, 0.0, 0.78),
        );
        draw_text("MAP EDITOR", 16.0, 32.0, 24.0, YELLOW);
        draw_button_rect(
            16.0,
            48.0,
            88.0,
            30.0,
            "Objects",
            self.tool == EditorTool::Objects,
        );
        draw_button_rect(
            112.0,
            48.0,
            84.0,
            30.0,
            "Biomes",
            self.tool == EditorTool::Biomes,
        );
        draw_button_rect(204.0, 48.0, 52.0, 30.0, "Save", false);
        draw_button_rect(264.0, 48.0, 52.0, 30.0, "Play", false);

        draw_text(
            &format!(
                "Tool: {}  Biome: {}  Cluster: {}",
                tool, BIOMES[self.selected_biome], cluster_name
            ),
            16.0,
            104.0,
            18.0,
            WHITE,
        );

        if let Some(asset) = self.selected_asset() {
            let state = if template_loaded { "loaded" } else { "loading" };
            draw_text(
                &format!("Model: {} ({})", asset.file, state),
                16.0,
                128.0,
                16.0,
                LIGHTGRAY,
            );
        } else {
            draw_text(
                "Model: none found in assets/world_models",
                16.0,
                128.0,
                16.0,
                RED,
            );
        }

        draw_text(
            &format!(
                "Objects: {}  Biome regions: {}  Scale: {:.2}  Brush: {:.1}",
                placement_count,
                self.map.biome_regions.len(),
                self.scale,
                self.brush_radius
            ),
            16.0,
            152.0,
            16.0,
            LIGHTGRAY,
        );

        draw_text("Search models:", 16.0, 184.0, 16.0, LIGHTGRAY);
        let search_color = if self.search_focused {
            Color::new(0.14, 0.14, 0.12, 1.0)
        } else {
            Color::new(0.08, 0.08, 0.08, 1.0)
        };
        draw_rectangle(16.0, 196.0, 298.0, 30.0, search_color);
        draw_rectangle_lines(16.0, 196.0, 298.0, 30.0, 1.0, GRAY);
        draw_text(&self.search_text, 24.0, 216.0, 16.0, WHITE);

        self.draw_model_list();

        let help_y = screen_height() - 118.0;
        draw_text(
            "Objects: [/] model, wheel scale, R/T rotate",
            16.0,
            help_y,
            16.0,
            LIGHTGRAY,
        );
        draw_text(
            "Biomes: wheel brush, B biome, left paint",
            16.0,
            help_y + 22.0,
            16.0,
            LIGHTGRAY,
        );
        draw_text(
            "Camera: WASD pan, Q/E zoom, Shift faster",
            16.0,
            help_y + 44.0,
            16.0,
            LIGHTGRAY,
        );
        draw_text(
            &format!("{}  Save: {}", self.status, SAVE_PATH),
            16.0,
            help_y + 72.0,
            14.0,
            GREEN,
        );
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

    fn update_panel(&mut self) {
        if !self.pointer_over_panel() {
            self.search_focused = false;
            return;
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            self.search_focused = rect_contains(16.0, 196.0, 298.0, 30.0, mouse_position());

            if rect_contains(16.0, 48.0, 88.0, 30.0, mouse_position()) {
                self.tool = EditorTool::Objects;
            } else if rect_contains(112.0, 48.0, 84.0, 30.0, mouse_position()) {
                self.tool = EditorTool::Biomes;
            } else if rect_contains(204.0, 48.0, 52.0, 30.0, mouse_position()) {
                self.save();
            } else if rect_contains(264.0, 48.0, 52.0, 30.0, mouse_position()) {
                self.play_button_armed = true;
            } else {
                self.click_model_list();
            }
        }

        if self.search_focused {
            while let Some(c) = get_char_pressed() {
                if c.is_ascii() && !c.is_control() {
                    self.search_text.push(c);
                    self.list_scroll = 0;
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                self.search_text.pop();
                self.list_scroll = 0;
            }
            if is_key_pressed(KeyCode::Escape) {
                self.search_focused = false;
            }
        }

        let (_, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 && self.pointer_over_model_list() {
            if wheel_y < 0.0 {
                self.list_scroll = self.list_scroll.saturating_add(3);
            } else {
                self.list_scroll = self.list_scroll.saturating_sub(3);
            }
        }
    }

    fn pointer_over_panel(&self) -> bool {
        let (mx, _) = mouse_position();
        mx <= PANEL_WIDTH
    }

    fn pointer_over_model_list(&self) -> bool {
        let (_, my) = mouse_position();
        my >= 238.0 && my <= screen_height() - 135.0 && self.pointer_over_panel()
    }

    fn filtered_model_indices(&self) -> Vec<usize> {
        let query = self.search_text.to_lowercase();
        self.catalog
            .iter()
            .enumerate()
            .filter(|(_, asset)| query.is_empty() || asset.file.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    fn click_model_list(&mut self) {
        if !self.pointer_over_model_list() {
            return;
        }

        let (_, my) = mouse_position();
        let row = ((my - 238.0) / ROW_HEIGHT).floor() as usize;
        let indices = self.filtered_model_indices();
        let Some(index) = indices.get(self.list_scroll + row).copied() else {
            return;
        };
        self.selected_model = index;
        self.tool = EditorTool::Objects;
        self.status = format!("Selected {}.", self.catalog[index].file);
    }

    fn draw_model_list(&self) {
        let top = 238.0;
        let bottom = screen_height() - 135.0;
        draw_rectangle(
            16.0,
            top,
            298.0,
            bottom - top,
            Color::new(0.04, 0.04, 0.04, 1.0),
        );
        draw_rectangle_lines(16.0, top, 298.0, bottom - top, 1.0, DARKGRAY);

        let visible_rows = ((bottom - top) / ROW_HEIGHT).max(0.0) as usize;
        let indices = self.filtered_model_indices();
        for (row, index) in indices
            .iter()
            .skip(self.list_scroll)
            .take(visible_rows)
            .enumerate()
        {
            let y = top + row as f32 * ROW_HEIGHT;
            let selected = *index == self.selected_model;
            if selected {
                draw_rectangle(
                    18.0,
                    y + 1.0,
                    294.0,
                    ROW_HEIGHT - 2.0,
                    Color::new(0.35, 0.18, 0.04, 1.0),
                );
            }
            let label = truncate_label(&self.catalog[*index].file, 34);
            draw_text(
                &label,
                24.0,
                y + 16.0,
                14.0,
                if selected { WHITE } else { LIGHTGRAY },
            );
        }
    }

    fn update_objects(&mut self) {
        if is_key_pressed(KeyCode::RightBracket) {
            self.next_model();
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.prev_model();
        }
        if is_key_down(KeyCode::R) {
            self.rotation += get_frame_time() * 1.8;
        }
        if is_key_down(KeyCode::T) {
            self.rotation -= get_frame_time() * 1.8;
        }
        if is_key_pressed(KeyCode::Space) {
            self.blocks_movement = !self.blocks_movement;
        }

        let (_, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 {
            self.scale = (self.scale + wheel_y * 0.08).clamp(0.2, 8.0);
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            if let (Some(asset), Some(pos)) =
                (self.selected_asset().cloned(), self.mouse_ground_pos())
            {
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
        if let Some(asset) = self.selected_asset() {
            if let Some(template) = templates.get(&asset.key) {
                for mesh in instantiate(
                    template,
                    vec3(cursor.x, 0.02, cursor.z),
                    self.rotation,
                    self.scale,
                ) {
                    draw_mesh(&mesh);
                }
            }
        }
        draw_cube_wires(vec3(cursor.x, 0.08, cursor.z), vec3(1.0, 0.12, 1.0), ORANGE);
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

fn rect_contains(x: f32, y: f32, w: f32, h: f32, point: (f32, f32)) -> bool {
    point.0 >= x && point.0 <= x + w && point.1 >= y && point.1 <= y + h
}

fn draw_button_rect(x: f32, y: f32, w: f32, h: f32, label: &str, active: bool) {
    let color = if active {
        Color::new(0.42, 0.22, 0.05, 1.0)
    } else {
        Color::new(0.10, 0.10, 0.10, 1.0)
    };
    draw_rectangle(x, y, w, h, color);
    draw_rectangle_lines(x, y, w, h, 1.0, GRAY);
    draw_text(label, x + 8.0, y + 20.0, 16.0, WHITE);
}

fn truncate_label(label: &str, max_chars: usize) -> String {
    if label.len() <= max_chars {
        return label.to_string();
    }
    format!("{}...", &label[..max_chars.saturating_sub(3)])
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
