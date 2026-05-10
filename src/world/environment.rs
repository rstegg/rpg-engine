use crate::world::cluster::ModelPlacement;
use gltf::Gltf;
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_HITBOX_REFERENCE_SCALE: f32 = 2.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HitboxConfigEntry {
    Legacy(f32),
    Painted(HitboxPaintedMask),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HitboxPaintedMask {
    #[serde(default = "default_hitbox_reference_scale")]
    pub reference_scale: f32,
    #[serde(default)]
    pub blocked_cells: Vec<[i32; 2]>,
}

pub type HitboxConfig = HashMap<String, HitboxConfigEntry>;

fn default_hitbox_reference_scale() -> f32 {
    DEFAULT_HITBOX_REFERENCE_SCALE
}

pub fn builtin_template_defs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("grass", "ground_grass.glb"),
        ("path", "ground_pathStraight.glb"),
        ("tree_a", "tree_default.glb"),
        ("tree_b", "tree_simple.glb"),
        ("tree_c", "tree_oak.glb"),
        ("rock_a", "rock_largeA.glb"),
        ("rock_b", "rock_tallA.glb"),
        ("rock_c", "stone_largeA.glb"),
        ("flower_a", "flower_purpleA.glb"),
        ("flower_b", "flower_redA.glb"),
        ("flower_c", "flower_yellowA.glb"),
        ("mushroom", "mushroom_red.glb"),
        ("shroom2", "mushroom_tan.glb"),
        ("plant", "plant_bush.glb"),
        ("tent", "tent_detailedOpen.glb"),
        ("campfire", "campfire_bricks.glb"),
        ("stump", "stump_round.glb"),
    ]
}

/// One primitive within a GLB — has its own flat material color.
#[derive(Clone)]
pub struct GltfPrimitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// All primitives + the XZ footprint radius derived from actual geometry.
#[derive(Clone)]
pub struct GltfTemplate {
    pub primitives: Vec<GltfPrimitive>,
    /// Maximum distance from the center on the XZ plane across ALL primitives.
    /// Used to compute how many grid cells this model actually blocks.
    pub footprint_radius: f32,
}

pub async fn load_glb_template(path: &str) -> Option<GltfTemplate> {
    let bytes = load_file(path).await.ok()?;
    parse_glb_template(&bytes)
}

pub fn load_glb_template_sync(path: &str) -> Option<GltfTemplate> {
    let bytes = std::fs::read(path).ok()?;
    parse_glb_template(&bytes)
}

pub fn parse_glb_template(bytes: &[u8]) -> Option<GltfTemplate> {
    let gltf = Gltf::from_slice(&bytes).ok()?;
    let blob = gltf.blob.as_deref()?;

    let mut primitives = Vec::new();
    let light_dir = vec3(0.5, 1.0, 0.3).normalize();
    const AMBIENT: f32 = 0.45;

    let mut all_positions: Vec<Vec3> = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|_| Some(blob));

            let positions: Vec<Vec3> = match reader.read_positions() {
                Some(p) => p.map(|v| vec3(v[0], v[1], v[2])).collect(),
                None => continue,
            };
            let n = positions.len();

            all_positions.extend_from_slice(&positions);

            let normals: Vec<Vec3> = reader
                .read_normals()
                .map(|it| it.map(|v| vec3(v[0], v[1], v[2])).collect())
                .unwrap_or_else(|| vec![Vec3::Y; n]);

            let uvs: Vec<Vec2> = reader
                .read_tex_coords(0)
                .map(|it| it.into_f32().map(|u| vec2(u[0], u[1])).collect())
                .unwrap_or_else(|| vec![Vec2::ZERO; n]);

            let base = primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_factor();
            let (mat_r, mat_g, mat_b) = (base[0], base[1], base[2]);

            let mut vertices = Vec::with_capacity(n);
            for i in 0..n {
                let diffuse = normals[i].dot(light_dir).max(0.0);
                let shade = AMBIENT + (1.0 - AMBIENT) * diffuse;
                vertices.push(Vertex {
                    position: positions[i],
                    uv: uvs[i],
                    color: [
                        ((mat_r * shade) * 255.0) as u8,
                        ((mat_g * shade) * 255.0) as u8,
                        ((mat_b * shade) * 255.0) as u8,
                        255,
                    ],
                    normal: vec4(normals[i].x, normals[i].y, normals[i].z, 0.0),
                });
            }

            let indices: Vec<u32> = match reader.read_indices() {
                Some(it) => it.into_u32().collect(),
                None => (0..n as u32).collect(),
            };

            primitives.push(GltfPrimitive { vertices, indices });
        }
    }

    if primitives.is_empty() {
        return None;
    }

    let centroid_xz = if all_positions.is_empty() {
        Vec2::ZERO
    } else {
        let sum = all_positions
            .iter()
            .fold(Vec2::ZERO, |acc, p| acc + vec2(p.x, p.z));
        sum / all_positions.len() as f32
    };

    let footprint_radius = all_positions
        .iter()
        .map(|p| {
            let dx = p.x - centroid_xz.x;
            let dz = p.z - centroid_xz.y;
            (dx * dx + dz * dz).sqrt()
        })
        .fold(0.0_f32, f32::max);

    Some(GltfTemplate {
        primitives,
        footprint_radius,
    })
}

pub fn instantiate(template: &GltfTemplate, pos: Vec3, rotation: f32, scale: f32) -> Vec<Mesh> {
    let rot = Quat::from_rotation_y(rotation);
    template
        .primitives
        .iter()
        .map(|prim| {
            let vertices: Vec<Vertex> = prim
                .vertices
                .iter()
                .map(|v| {
                    let mut nv = *v;
                    nv.position = rot * (v.position * scale) + pos;
                    nv
                })
                .collect();
            let indices: Vec<u16> = prim.indices.iter().map(|&i| i as u16).collect();
            Mesh {
                vertices,
                indices,
                texture: None,
            }
        })
        .collect()
}

fn circular_hitbox_mask(
    template: &GltfTemplate,
    grid_size: f32,
    multiplier: f32,
    reference_scale: f32,
) -> HitboxPaintedMask {
    let radius = template.footprint_radius * reference_scale * multiplier;
    let cell_radius = (radius / grid_size).ceil() as i32;
    let mut blocked_cells = Vec::new();

    for dx in -cell_radius..=cell_radius {
        for dz in -cell_radius..=cell_radius {
            let wx = dx as f32 * grid_size;
            let wz = dz as f32 * grid_size;
            if (wx * wx + wz * wz).sqrt() <= radius {
                blocked_cells.push([dx, dz]);
            }
        }
    }

    HitboxPaintedMask {
        reference_scale,
        blocked_cells,
    }
}

pub fn ensure_painted_hitbox_entry<'a>(
    config: &'a mut HitboxConfig,
    key: &str,
    template: &GltfTemplate,
    grid_size: f32,
) -> &'a mut HitboxPaintedMask {
    let entry = config
        .entry(key.to_string())
        .or_insert(HitboxConfigEntry::Legacy(1.0));

    if let HitboxConfigEntry::Legacy(multiplier) = entry {
        let mask = circular_hitbox_mask(
            template,
            grid_size,
            *multiplier,
            DEFAULT_HITBOX_REFERENCE_SCALE,
        );
        *entry = HitboxConfigEntry::Painted(mask);
    }

    match entry {
        HitboxConfigEntry::Painted(mask) => mask,
        HitboxConfigEntry::Legacy(_) => unreachable!(),
    }
}

fn resolved_hitbox_mask(
    template: &GltfTemplate,
    entry: Option<&HitboxConfigEntry>,
    grid_size: f32,
) -> HitboxPaintedMask {
    match entry {
        Some(HitboxConfigEntry::Painted(mask)) => mask.clone(),
        Some(HitboxConfigEntry::Legacy(multiplier)) => circular_hitbox_mask(
            template,
            grid_size,
            *multiplier,
            DEFAULT_HITBOX_REFERENCE_SCALE,
        ),
        None => circular_hitbox_mask(template, grid_size, 1.0, DEFAULT_HITBOX_REFERENCE_SCALE),
    }
}

fn block_painted_mask(
    walkability_grid: &mut [Vec<bool>],
    world_pos: Vec3,
    rotation: f32,
    scale: f32,
    mask: &HitboxPaintedMask,
    grid_size: f32,
    width: i32,
    height: i32,
) {
    if mask.blocked_cells.is_empty() {
        return;
    }

    let sin_r = rotation.sin();
    let cos_r = rotation.cos();
    let inv_sin = (-rotation).sin();
    let inv_cos = (-rotation).cos();
    let local_half_extent = grid_size * 0.5 / mask.reference_scale;
    let world_half_extent = local_half_extent * scale;

    for &[cell_x, cell_z] in &mask.blocked_cells {
        let local_center = vec2(
            cell_x as f32 * grid_size / mask.reference_scale,
            cell_z as f32 * grid_size / mask.reference_scale,
        ) * scale;
        let rotated_center = vec2(
            local_center.x * cos_r + local_center.y * sin_r,
            -local_center.x * sin_r + local_center.y * cos_r,
        );
        let cell_center = vec2(
            world_pos.x + rotated_center.x,
            world_pos.z + rotated_center.y,
        );

        let corners = [
            vec2(-world_half_extent, -world_half_extent),
            vec2(world_half_extent, -world_half_extent),
            vec2(world_half_extent, world_half_extent),
            vec2(-world_half_extent, world_half_extent),
        ]
        .map(|corner| {
            vec2(
                cell_center.x + corner.x * cos_r + corner.y * sin_r,
                cell_center.y - corner.x * sin_r + corner.y * cos_r,
            )
        });

        let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_z = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_z = corners
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);

        let min_gx = ((min_x / grid_size).round() + (width / 2) as f32) as i32 - 1;
        let max_gx = ((max_x / grid_size).round() + (width / 2) as f32) as i32 + 1;
        let min_gz = ((min_z / grid_size).round() + (height / 2) as f32) as i32 - 1;
        let max_gz = ((max_z / grid_size).round() + (height / 2) as f32) as i32 + 1;

        for gx in min_gx..=max_gx {
            for gz in min_gz..=max_gz {
                if gx < 0 || gx >= width || gz < 0 || gz >= height {
                    continue;
                }

                let point = vec2(
                    (gx - width / 2) as f32 * grid_size,
                    (gz - height / 2) as f32 * grid_size,
                );
                let delta = point - cell_center;
                let local_point = vec2(
                    delta.x * inv_cos + delta.y * inv_sin,
                    -delta.x * inv_sin + delta.y * inv_cos,
                );

                if local_point.x.abs() <= world_half_extent + 0.001
                    && local_point.y.abs() <= world_half_extent + 0.001
                {
                    walkability_grid[gx as usize][gz as usize] = false;
                }
            }
        }
    }
}

fn apply_hitbox_blocking(
    walkability_grid: &mut [Vec<bool>],
    template: &GltfTemplate,
    model_key: &str,
    world_pos: Vec3,
    rotation: f32,
    scale: f32,
    hitbox_config: &HitboxConfig,
    grid_size: f32,
    width: i32,
    height: i32,
) {
    let mask = resolved_hitbox_mask(template, hitbox_config.get(model_key), grid_size);
    block_painted_mask(
        walkability_grid,
        world_pos,
        rotation,
        scale,
        &mask,
        grid_size,
        width,
        height,
    );
}

pub struct WorldSimulation {
    pub templates: HashMap<String, GltfTemplate>,
    pub grid_size: f32,
    pub width: i32,
    pub height: i32,
    pub walkability_grid: Vec<Vec<bool>>,
    pub pathfinding_grid: Vec<Vec<bool>>,
    pub placements: Vec<ModelPlacement>,
}

impl WorldSimulation {
    pub fn build(
        tile_width: i32,
        tile_height: i32,
        hitbox_config: HitboxConfig,
        procedural: bool,
        templates: HashMap<String, GltfTemplate>,
    ) -> Self {
        let tile_size = 2.0_f32;
        let grid_size = 0.5_f32; // Higher resolution for hitboxes

        // World size in meters
        let world_width_m = tile_width as f32 * tile_size;
        let world_height_m = tile_height as f32 * tile_size;

        // Grid size in cells
        let grid_width = (world_width_m / grid_size) as i32;
        let grid_height = (world_height_m / grid_size) as i32;

        let mut walkability_grid = vec![vec![true; grid_height as usize]; grid_width as usize];
        let mut placements: Vec<ModelPlacement> = Vec::new();

        use ::rand::Rng;
        let mut rng = ::rand::thread_rng();

        if procedural {
            // ── 1. Ground Tiles ──────────────────────────────────────────────────
            for x in 0..tile_width {
                for z in 0..tile_height {
                    let wx = (x - tile_width / 2) as f32 * tile_size;
                    let wz = (z - tile_height / 2) as f32 * tile_size;
                    let is_path = (x - tile_width / 2).abs() < 1;
                    let key = if is_path { "path" } else { "grass" };
                    let rot = if is_path {
                        0.0
                    } else {
                        rng.gen_range(0..4) as f32 * 1.5708
                    };
                    if let Some(_t) = templates.get(key) {
                        placements.push(ModelPlacement {
                            model: key.to_string(),
                            file: format!("ground_{}.glb", key),
                            position: [wx, 0.0, wz],
                            rotation: rot,
                            scale: tile_size,
                            blocks_movement: false,
                        });
                    }
                }
            }

            // ── 2. Procedural decoration ──────────────────────────────────────────
            let trees = ["tree_a", "tree_b", "tree_c"];
            let rocks = ["rock_a", "rock_c"]; // rock_b is now a mountain-sized landmark
            let smalls = [
                "flower_a", "flower_b", "flower_c", "mushroom", "shroom2", "plant",
            ];

            for _ in 0..180 {
                let x_tile = rng.gen_range(0..tile_width);
                let z_tile = rng.gen_range(0..tile_height);
                if (x_tile - tile_width / 2).abs() < 2 {
                    continue;
                }

                let wx =
                    (x_tile - tile_width / 2) as f32 * tile_size + rng.gen_range(-0.7_f32..0.7);
                let wz =
                    (z_tile - tile_height / 2) as f32 * tile_size + rng.gen_range(-0.7_f32..0.7);

                // Check grid walkability at the proposed spot
                let gx = ((wx / grid_size).round() + (grid_width / 2) as f32) as i32;
                let gz = ((wz / grid_size).round() + (grid_height / 2) as f32) as i32;
                if gx < 0
                    || gx >= grid_width
                    || gz < 0
                    || gz >= grid_height
                    || !walkability_grid[gx as usize][gz as usize]
                {
                    continue;
                }

                let rot = rng.gen_range(0.0_f32..6.28);
                let roll = rng.gen_range(0.0_f32..1.0);

                let (key, blocks): (&str, bool) = if roll < 0.40 {
                    (trees[rng.gen_range(0..trees.len())], true)
                } else if roll < 0.60 {
                    (rocks[rng.gen_range(0..rocks.len())], true)
                } else {
                    (smalls[rng.gen_range(0..smalls.len())], false)
                };

                if let Some(t) = templates.get(key) {
                    let base_scale = match key {
                        k if k.starts_with("tree") => 2.0 + rng.gen_range(-0.3_f32..0.5),
                        k if k.starts_with("rock") => 1.5 + rng.gen_range(-0.2_f32..0.3),
                        k if k.starts_with("stone") => 1.5 + rng.gen_range(-0.2_f32..0.3),
                        _ => 0.8 + rng.gen_range(-0.1_f32..0.2),
                    };
                    let pos = vec3(wx, 0.0, wz);

                    // If it's a small object (flower/plant), spawn a cluster
                    if !blocks && key.starts_with("flower") {
                        let cluster_count = rng.gen_range(3..6);
                        for _ in 0..cluster_count {
                            let offset =
                                vec3(rng.gen_range(-0.4..0.4), 0.0, rng.gen_range(-0.4..0.4));
                            let c_rot = rng.gen_range(0.0..6.28);
                            let c_scale = base_scale * rng.gen_range(0.8..1.2);
                            placements.push(ModelPlacement {
                                model: key.to_string(),
                                file: format!("{}.glb", key),
                                position: [(pos + offset).x, 0.0, (pos + offset).z],
                                rotation: c_rot,
                                scale: c_scale,
                                blocks_movement: false,
                            });
                        }
                    } else {
                        placements.push(ModelPlacement {
                            model: key.to_string(),
                            file: format!("{}.glb", key),
                            position: [pos.x, 0.0, pos.z],
                            rotation: rot,
                            scale: base_scale,
                            blocks_movement: blocks,
                        });
                    }

                    if blocks {
                        apply_hitbox_blocking(
                            &mut walkability_grid,
                            t,
                            key,
                            pos,
                            rot,
                            base_scale,
                            &hitbox_config,
                            grid_size,
                            grid_width,
                            grid_height,
                        );
                    }
                }
            }

            // ── 3. Landmarks (The Mountain) ──────────────────────────────────────
            if let Some(t) = templates.get("rock_b") {
                let wx = rng.gen_range(-15.0..15.0);
                let wz = rng.gen_range(-15.0..15.0);
                let pos = vec3(wx, 0.0, wz);
                let scale = 3.0; // Scaled down from mountain size
                let rot = rng.gen_range(0.0..6.28);
                placements.push(ModelPlacement {
                    model: "rock_b".to_string(),
                    file: "rock_tallA.glb".to_string(),
                    position: [pos.x, 0.0, pos.z],
                    rotation: rot,
                    scale,
                    blocks_movement: true,
                });

                apply_hitbox_blocking(
                    &mut walkability_grid,
                    t,
                    "rock_b",
                    pos,
                    rot,
                    scale,
                    &hitbox_config,
                    grid_size,
                    grid_width,
                    grid_height,
                );
            }

            // ── 4. Camp ───────────────────────────────────────────────────────────
            let tent_pos = vec3(6.0, 0.0, 4.0);
            let fire_pos = vec3(4.0, 0.0, 4.0);
            if let Some(t) = templates.get("tent") {
                placements.push(ModelPlacement {
                    model: "tent".to_string(),
                    file: "tent_detailedOpen.glb".to_string(),
                    position: [tent_pos.x, 0.0, tent_pos.z],
                    rotation: -0.7,
                    scale: 2.5,
                    blocks_movement: true,
                });
                apply_hitbox_blocking(
                    &mut walkability_grid,
                    t,
                    "tent",
                    tent_pos,
                    -0.7,
                    2.5,
                    &hitbox_config,
                    grid_size,
                    grid_width,
                    grid_height,
                );
            }
            if let Some(t) = templates.get("campfire") {
                placements.push(ModelPlacement {
                    model: "campfire".to_string(),
                    file: "campfire_bricks.glb".to_string(),
                    position: [fire_pos.x, 0.0, fire_pos.z],
                    rotation: 0.0,
                    scale: 2.0,
                    blocks_movement: true,
                });
                apply_hitbox_blocking(
                    &mut walkability_grid,
                    t,
                    "campfire",
                    fire_pos,
                    0.0,
                    2.0,
                    &hitbox_config,
                    grid_size,
                    grid_width,
                    grid_height,
                );
            }
        }

        // ── 4. Generate Pathfinding Padding ─────────────────────────────────
        let mut pathfinding_grid = walkability_grid.clone();
        for x in 0..grid_width {
            for z in 0..grid_height {
                if !walkability_grid[x as usize][z as usize] {
                    // Pad neighbors (1 cell radius = 0.5m padding)
                    for dx in -1..=1 {
                        for dz in -1..=1 {
                            let nx = x + dx;
                            let nz = z + dz;
                            if nx >= 0 && nx < grid_width && nz >= 0 && nz < grid_height {
                                pathfinding_grid[nx as usize][nz as usize] = false;
                            }
                        }
                    }
                }
            }
        }

        Self {
            templates,
            grid_size,
            width: grid_width,
            height: grid_height,
            walkability_grid,
            pathfinding_grid,
            placements,
        }
    }

    pub fn add_placement(&mut self, placement: &ModelPlacement, hitbox_config: &HitboxConfig) {
        if let Some(template) = self.templates.get(&placement.model) {
            let pos = placement.pos_vec3();
            if placement.blocks_movement {
                apply_hitbox_blocking(
                    &mut self.walkability_grid,
                    template,
                    &placement.model,
                    pos,
                    placement.rotation,
                    placement.scale,
                    hitbox_config,
                    self.grid_size,
                    self.width,
                    self.height,
                );
                self.rebuild_pathfinding_padding();
            }

            self.placements.push(placement.clone());
        }
    }

    fn rebuild_pathfinding_padding(&mut self) {
        self.pathfinding_grid = self.walkability_grid.clone();
        for x in 0..self.width {
            for z in 0..self.height {
                if !self.walkability_grid[x as usize][z as usize] {
                    for dx in -1..=1 {
                        for dz in -1..=1 {
                            let nx = x + dx;
                            let nz = z + dz;
                            if nx >= 0 && nx < self.width && nz >= 0 && nz < self.height {
                                self.pathfinding_grid[nx as usize][nz as usize] = false;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn is_walkable(&self, world_pos: Vec3) -> bool {
        let x = ((world_pos.x / self.grid_size).round() + (self.width / 2) as f32) as i32;
        let z = ((world_pos.z / self.grid_size).round() + (self.height / 2) as f32) as i32;
        if x < 0 || x >= self.width || z < 0 || z >= self.height {
            return false;
        }
        self.walkability_grid[x as usize][z as usize]
    }
}

pub struct WorldEnvironment {
    pub sim: WorldSimulation,
    pub meshes: Vec<Mesh>,
}

impl WorldEnvironment {
    pub async fn new(
        tile_width: i32,
        tile_height: i32,
        hitbox_config: HitboxConfig,
        procedural: bool,
    ) -> Self {
        let base = "assets/world_models/";
        let to_load = builtin_template_defs();

        let mut templates = HashMap::new();
        for &(k, f) in &to_load {
            if let Some(t) = load_glb_template(&format!("{base}{f}")).await {
                templates.insert(k.to_string(), t);
            }
        }

        let sim = WorldSimulation::build(tile_width, tile_height, hitbox_config, procedural, templates);

        let mut meshes = Vec::new();
        for placement in &sim.placements {
            if let Some(t) = sim.templates.get(&placement.model) {
                meshes.extend(instantiate(
                    t,
                    placement.pos_vec3(),
                    placement.rotation,
                    placement.scale,
                ));
            }
        }

        Self { sim, meshes }
    }

    pub fn draw(&self) {
        for mesh in &self.meshes {
            draw_mesh(mesh);
        }
    }

    pub fn add_placement(&mut self, placement: &ModelPlacement, hitbox_config: &HitboxConfig) {
        self.sim.add_placement(placement, hitbox_config);
        if let Some(template) = self.sim.templates.get(&placement.model) {
            self.meshes.extend(instantiate(
                template,
                placement.pos_vec3(),
                placement.rotation,
                placement.scale,
            ));
        }
    }
}
