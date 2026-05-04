use macroquad::prelude::*;
use std::collections::HashMap;
use gltf::Gltf;
use ::rand::prelude::SliceRandom;

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

            let base = primitive.material().pbr_metallic_roughness().base_color_factor();
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

    if primitives.is_empty() { return None; }

    let centroid_xz = if all_positions.is_empty() {
        Vec2::ZERO
    } else {
        let sum = all_positions.iter().fold(Vec2::ZERO, |acc, p| acc + vec2(p.x, p.z));
        sum / all_positions.len() as f32
    };

    let footprint_radius = all_positions.iter().map(|p| {
        let dx = p.x - centroid_xz.x;
        let dz = p.z - centroid_xz.y;
        (dx * dx + dz * dz).sqrt()
    }).fold(0.0_f32, f32::max);

    Some(GltfTemplate { primitives, footprint_radius })
}

pub fn instantiate(template: &GltfTemplate, pos: Vec3, rotation: f32, scale: f32) -> Vec<Mesh> {
    let rot = Quat::from_rotation_y(rotation);
    template.primitives.iter().map(|prim| {
        let vertices: Vec<Vertex> = prim.vertices.iter().map(|v| {
            let mut nv = *v;
            nv.position = rot * (v.position * scale) + pos;
            nv
        }).collect();
        let indices: Vec<u16> = prim.indices.iter().map(|&i| i as u16).collect();
        Mesh { vertices, indices, texture: None }
    }).collect()
}

fn block_radius(
    walkability_grid: &mut Vec<Vec<bool>>,
    world_pos: Vec3,
    world_radius: f32,
    grid_size: f32,
    width: i32,
    height: i32,
) {
    let cell_radius = (world_radius / grid_size).ceil() as i32;
    let cx = ((world_pos.x / grid_size).round() + (width / 2) as f32) as i32;
    let cz = ((world_pos.z / grid_size).round() + (height / 2) as f32) as i32;

    for dx in -cell_radius..=cell_radius {
        for dz in -cell_radius..=cell_radius {
            let gx = cx + dx;
            let gz = cz + dz;
            if gx < 0 || gx >= width || gz < 0 || gz >= height { continue; }

            let cell_wx = (gx - width / 2) as f32 * grid_size;
            let cell_wz = (gz - height / 2) as f32 * grid_size;
            let dist = ((cell_wx - world_pos.x).powi(2) + (cell_wz - world_pos.z).powi(2)).sqrt();
            if dist <= world_radius {
                walkability_grid[gx as usize][gz as usize] = false;
            }
        }
    }
}

pub struct WorldEnvironment {
    pub meshes: Vec<Mesh>,
    pub templates: HashMap<String, GltfTemplate>,
    pub grid_size: f32,
    pub width: i32,
    pub height: i32,
    pub walkability_grid: Vec<Vec<bool>>,
    /// A "fattened" version of the walkability grid used for A* to keep the agent away from walls.
    pub pathfinding_grid: Vec<Vec<bool>>,
}

impl WorldEnvironment {
    pub async fn new(tile_width: i32, tile_height: i32, radius_overrides: HashMap<String, f32>) -> Self {
        let base = "GLTF format/";
        let mut to_load: Vec<(&str, &str)> = vec![
            ("grass",    "ground_grass.glb"),
            ("path",     "ground_pathStraight.glb"),
            ("tree_a",   "tree_default.glb"),
            ("tree_b",   "tree_simple.glb"),
            ("tree_c",   "tree_oak.glb"),
            ("rock_a",   "rock_largeA.glb"),
            ("rock_b",   "rock_tallA.glb"),
            ("rock_c",   "stone_largeA.glb"),
            ("flower_a", "flower_purpleA.glb"),
            ("flower_b", "flower_redA.glb"),
            ("flower_c", "flower_yellowA.glb"),
            ("mushroom", "mushroom_red.glb"),
            ("shroom2",  "mushroom_tan.glb"),
            ("plant",    "plant_bush.glb"),
            ("tent",     "tent_detailedOpen.glb"),
            ("campfire", "campfire_bricks.glb"),
            ("stump",    "stump_round.glb"),
        ];

        let mut templates = HashMap::new();
        for &(k, f) in &to_load {
            if let Some(t) = load_glb_template(&format!("{base}{f}")).await {
                templates.insert(k.to_string(), t);
            }
        }

        let mut meshes: Vec<Mesh> = Vec::new();
        let tile_size = 2.0_f32;
        let grid_size = 0.5_f32; // Higher resolution for hitboxes
        
        // World size in meters
        let world_width_m = tile_width as f32 * tile_size;
        let world_height_m = tile_height as f32 * tile_size;
        
        // Grid size in cells
        let grid_width = (world_width_m / grid_size) as i32;
        let grid_height = (world_height_m / grid_size) as i32;
        
        let mut walkability_grid = vec![vec![true; grid_height as usize]; grid_width as usize];

        use ::rand::Rng;
        let mut rng = ::rand::thread_rng();

        // ── 1. Ground Tiles ──────────────────────────────────────────────────
        for x in 0..tile_width {
            for z in 0..tile_height {
                let wx = (x - tile_width/2) as f32 * tile_size;
                let wz = (z - tile_height/2) as f32 * tile_size;
                let is_path = (x - tile_width/2).abs() < 1;
                let key = if is_path { "path" } else { "grass" };
                let rot = if is_path { 0.0 } else { rng.gen_range(0..4) as f32 * 1.5708 };
                if let Some(t) = templates.get(key) {
                    meshes.extend(instantiate(t, vec3(wx, 0.0, wz), rot, tile_size));
                }
            }
        }

        // ── 2. Procedural decoration ──────────────────────────────────────────
        let trees  = ["tree_a", "tree_b", "tree_c"];
        let rocks  = ["rock_a", "rock_b", "rock_c"];
        let smalls = ["flower_a", "flower_b", "flower_c", "mushroom", "shroom2", "plant"];

        for _ in 0..180 {
            let x_tile = rng.gen_range(0..tile_width);
            let z_tile = rng.gen_range(0..tile_height);
            if (x_tile - tile_width/2).abs() < 2 { continue; }

            let wx = (x_tile - tile_width/2) as f32 * tile_size + rng.gen_range(-0.7_f32..0.7);
            let wz = (z_tile - tile_height/2) as f32 * tile_size + rng.gen_range(-0.7_f32..0.7);
            
            // Check grid walkability at the proposed spot
            let gx = ((wx / grid_size).round() + (grid_width / 2) as f32) as i32;
            let gz = ((wz / grid_size).round() + (grid_height / 2) as f32) as i32;
            if gx < 0 || gx >= grid_width || gz < 0 || gz >= grid_height || !walkability_grid[gx as usize][gz as usize] {
                continue;
            }

            let rot  = rng.gen_range(0.0_f32..6.28);
            let roll = rng.gen_range(0.0_f32..1.0);

            let (key, blocks): (&str, bool) = if roll < 0.30 {
                (trees[rng.gen_range(0..trees.len())], true)
            } else if roll < 0.50 {
                (rocks[rng.gen_range(0..rocks.len())], true)
            } else if roll < 0.80 {
                (smalls[rng.gen_range(0..smalls.len())], false)
            } else {
                ("stump", false)
            };

            if let Some(t) = templates.get(key) {
                let base_scale = match key {
                    k if k.starts_with("tree")  => 2.0 + rng.gen_range(-0.3_f32..0.5),
                    k if k.starts_with("rock")  => 1.5 + rng.gen_range(-0.2_f32..0.3),
                    k if k.starts_with("stone") => 1.5 + rng.gen_range(-0.2_f32..0.3),
                    "stump"                      => 1.2 + rng.gen_range(-0.1_f32..0.2),
                    _                            => 0.8 + rng.gen_range(-0.1_f32..0.2),
                };
                let pos = vec3(wx, 0.0, wz);
                meshes.extend(instantiate(t, pos, rot, base_scale));

                if blocks {
                    let multiplier = radius_overrides.get(key).cloned().unwrap_or(1.0);
                    let world_radius = t.footprint_radius * base_scale * multiplier;
                    block_radius(&mut walkability_grid, pos, world_radius, grid_size, grid_width, grid_height);
                }
            }
        }

        // ── 3. Camp ───────────────────────────────────────────────────────────
        let tent_pos = vec3(6.0, 0.0, 4.0);
        let fire_pos = vec3(4.0, 0.0, 4.0);
        if let Some(t) = templates.get("tent") {
            meshes.extend(instantiate(t, tent_pos, -0.7, 2.5));
            let mult = radius_overrides.get("tent").cloned().unwrap_or(1.0);
            let r = t.footprint_radius * 2.5 * mult;
            block_radius(&mut walkability_grid, tent_pos, r, grid_size, grid_width, grid_height);
        }
        if let Some(t) = templates.get("campfire") {
            meshes.extend(instantiate(t, fire_pos, 0.0, 2.0));
            let mult = radius_overrides.get("campfire").cloned().unwrap_or(1.0);
            let r = t.footprint_radius * 2.0 * mult;
            block_radius(&mut walkability_grid, fire_pos, r, grid_size, grid_width, grid_height);
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

        Self { meshes, templates, grid_size, width: grid_width, height: grid_height, walkability_grid, pathfinding_grid }
    }

    pub fn draw(&self) {
        for mesh in &self.meshes {
            draw_mesh(mesh);
        }
    }

    pub fn is_walkable(&self, world_pos: Vec3) -> bool {
        let x = ((world_pos.x / self.grid_size).round() + (self.width / 2) as f32) as i32;
        let z = ((world_pos.z / self.grid_size).round() + (self.height / 2) as f32) as i32;
        if x < 0 || x >= self.width || z < 0 || z >= self.height { return false; }
        self.walkability_grid[x as usize][z as usize]
    }
}
