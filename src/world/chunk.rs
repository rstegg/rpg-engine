use crate::world::cluster::{ModelPlacement, WorldCluster};
use crate::world::environment::{
    HitboxConfig, GltfTemplate, apply_hitbox_blocking, find_hitbox_config_entry,
    instantiate, instantiate_gate, load_glb_template_sync,
};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ::rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;

pub const TILE_SIZE: f32 = 2.0;
pub const TILES_PER_CHUNK: i32 = 20;
pub const CHUNK_SIZE_M: f32 = TILE_SIZE * TILES_PER_CHUNK as f32;
pub const GRID_SIZE: f32 = 0.5;
pub const GRID_WIDTH: i32 = (CHUNK_SIZE_M / GRID_SIZE) as i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn from_world_pos(pos: Vec3) -> Self {
        Self {
            x: (pos.x / CHUNK_SIZE_M).floor() as i32,
            z: (pos.z / CHUNK_SIZE_M).floor() as i32,
        }
    }

    pub fn world_origin(&self) -> Vec3 {
        vec3(self.x as f32 * CHUNK_SIZE_M, 0.0, self.z as f32 * CHUNK_SIZE_M)
    }

    pub fn world_center(&self) -> Vec3 {
        vec3(
            self.x as f32 * CHUNK_SIZE_M + CHUNK_SIZE_M * 0.5,
            0.0,
            self.z as f32 * CHUNK_SIZE_M + CHUNK_SIZE_M * 0.5,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BiomeType {
    Grassland,
    Forest,
    Town,
    Rocky,
    Wetland,
}

impl BiomeType {
    pub fn to_string(&self) -> String {
        match self {
            Self::Grassland => "grassland".to_string(),
            Self::Forest => "forest".to_string(),
            Self::Town => "town".to_string(),
            Self::Rocky => "rocky".to_string(),
            Self::Wetland => "wetland".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum GateState {
    Closed,
    Open,
    Locked,
}

pub struct Gate {
    pub chunk: ChunkCoord,
    pub position: Vec3,
    pub base_rotation: f32,
    pub state: GateState,
    pub open_progress: f32, // 0.0 (closed) to 1.0 (open)
}

impl Gate {
    fn mask_key(&self) -> &'static str {
        if self.open_progress >= ChunkedWorld::GATE_OPEN_COLLISION_THRESHOLD {
            "gate_open"
        } else {
            "gate"
        }
    }
}

pub struct Chunk {
    pub coord: ChunkCoord,
    pub biome: BiomeType,
    pub placements: Vec<ModelPlacement>,
    pub walkability: Vec<Vec<bool>>,
    pub pathfinding: Vec<Vec<bool>>,
}

pub struct ChunkRenderData {
    pub meshes: Vec<Mesh>,
}

pub struct ChunkedWorld {
    pub chunks: HashMap<ChunkCoord, Chunk>,
    pub render_data: HashMap<ChunkCoord, ChunkRenderData>,
    pub templates: HashMap<String, GltfTemplate>,
    pub hitbox_config: HitboxConfig,
    pub world_seed: u64,
    pub town_cluster: Option<WorldCluster>,
    pub gates: Vec<Gate>,
    pub last_hot_reload_check: std::time::Instant,
    pub town_mtime: Option<std::time::SystemTime>,
    pub hitbox_mtime: Option<std::time::SystemTime>,
}

impl ChunkedWorld {
    const GATE_OPEN_RADIUS: f32 = 5.0;
    const GATE_CLOSE_RADIUS: f32 = 6.0;
    const GATE_OPEN_COLLISION_THRESHOLD: f32 = 0.35;
    const GATE_COLLISION_CHECK_RADIUS: f32 = 4.5;

    fn is_gate_model(model: &str) -> bool {
        model == "gate"
    }

    fn gate_mask(&self, key: &str) -> Option<crate::world::environment::HitboxPaintedMask> {
        let template = self.templates.get("gate")?;
        Some(crate::world::environment::get_resolved_hitbox_mask(
            template,
            find_hitbox_config_entry(&self.hitbox_config, key),
            GRID_SIZE,
        ))
    }

    fn register_gate(&mut self, chunk: ChunkCoord, position: Vec3, base_rotation: f32) {
        if self
            .gates
            .iter()
            .any(|g| g.chunk == chunk && (g.position - position).length_squared() < 0.1)
        {
            return;
        }

        self.gates.push(Gate {
            chunk,
            position,
            base_rotation,
            state: GateState::Closed,
            open_progress: 0.0,
        });
    }

    fn prune_stale_gates(&mut self) {
        self.gates.retain(|gate| self.chunks.contains_key(&gate.chunk));
    }

    fn ensure_template_loaded(&mut self, model: &str, file: &str) {
        if self.templates.contains_key(model) {
            return;
        }

        if let Some(template) = load_glb_template_sync(&format!("assets/world_models/{file}")) {
            self.templates.insert(model.to_string(), template);
        }
    }

    pub fn new(hitbox_config: HitboxConfig, templates: HashMap<String, GltfTemplate>, seed: u64) -> Self {
        let town_cluster = match std::fs::read_to_string("assets/clusters/town.json") {
            Ok(json) => serde_json::from_str(&json).ok(),
            Err(_) => None,
        };

        let town_mtime = std::fs::metadata("assets/clusters/town.json").ok().and_then(|m| m.modified().ok());
        let hitbox_mtime = std::fs::metadata("hitbox_config.json").ok().and_then(|m| m.modified().ok());

        Self {
            chunks: HashMap::new(),
            render_data: HashMap::new(),
            templates,
            hitbox_config,
            world_seed: seed,
            town_cluster,
            gates: Vec::new(),
            last_hot_reload_check: std::time::Instant::now(),
            town_mtime,
            hitbox_mtime,
        }
    }

    pub fn get_biome_at(&self, coord: ChunkCoord) -> BiomeType {
        if coord.x == 0 && coord.z == 0 {
            return BiomeType::Town;
        }

        let mut rng = Pcg64::seed_from_u64(self.chunk_seed(coord));
        let roll = rng.gen_range(0..100);
        if roll < 40 {
            BiomeType::Grassland
        } else if roll < 70 {
            BiomeType::Forest
        } else if roll < 85 {
            BiomeType::Rocky
        } else {
            BiomeType::Wetland
        }
    }

    pub fn chunk_seed(&self, coord: ChunkCoord) -> u64 {
        let mut h = (coord.x as u64).wrapping_mul(73856093) ^ (coord.z as u64).wrapping_mul(19349663) ^ self.world_seed;
        h = h.wrapping_mul(11400714819323198485);
        h
    }

    pub fn check_hot_reload(&mut self) -> bool {
        if self.last_hot_reload_check.elapsed().as_secs_f32() < 1.0 {
            return false;
        }
        self.last_hot_reload_check = std::time::Instant::now();

        let mut changed = false;

        // Check town.json
        if let Ok(m) = std::fs::metadata("assets/clusters/town.json") {
            if let Ok(mtime) = m.modified() {
                if Some(mtime) != self.town_mtime {
                    self.town_mtime = Some(mtime);
                    if let Ok(json) = std::fs::read_to_string("assets/clusters/town.json") {
                        if let Ok(cluster) = serde_json::from_str::<WorldCluster>(&json) {
                            self.town_cluster = Some(cluster);
                            // Clear town chunk so it regenerates
                            let town_coord = ChunkCoord { x: 0, z: 0 };
                            self.chunks.remove(&town_coord);
                            self.render_data.remove(&town_coord);
                            self.gates.retain(|gate| gate.chunk != town_coord);
                            changed = true;
                            println!("[HOT RELOAD] Reloaded town.json");
                        }
                    }
                }
            }
        }

        // Check hitbox_config.json
        if let Ok(m) = std::fs::metadata("hitbox_config.json") {
            if let Ok(mtime) = m.modified() {
                if Some(mtime) != self.hitbox_mtime {
                    self.hitbox_mtime = Some(mtime);
                    if let Ok(json) = std::fs::read_to_string("hitbox_config.json") {
                        if let Ok(config) = serde_json::from_str::<HitboxConfig>(&json) {
                            self.hitbox_config = config;
                            // Clear ALL chunks because hitboxes might have changed everywhere
                            self.chunks.clear();
                            self.render_data.clear();
                            self.gates.clear();
                            changed = true;
                            println!("[HOT RELOAD] Reloaded hitbox_config.json");
                        }
                    }
                }
            }
        }

        changed
    }

    pub fn generate_chunk(&mut self, coord: ChunkCoord, generate_meshes: bool) -> bool {
        if self.chunks.contains_key(&coord) {
            return false;
        }

        let biome = self.get_biome_at(coord);
        let mut rng = Pcg64::seed_from_u64(self.chunk_seed(coord));
        let origin = coord.world_origin();

        let mut placements = Vec::new();
        let mut walkability = vec![vec![true; GRID_WIDTH as usize]; GRID_WIDTH as usize];

        // 1. Ground Tiles
        for x in 0..TILES_PER_CHUNK {
            for z in 0..TILES_PER_CHUNK {
                let wx = origin.x + x as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                let wz = origin.z + z as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                
                let is_path = if biome == BiomeType::Town {
                    (x % 10 < 2) || (z % 10 < 2)
                } else {
                    false
                };

                let key = if is_path { "path" } else { "grass" };
                let rot = if is_path { 0.0 } else { rng.gen_range(0..4) as f32 * 1.5708 };
                
                placements.push(ModelPlacement {
                    model: key.to_string(),
                    file: format!("ground_{}.glb", key),
                    position: [wx, 0.0, wz],
                    rotation: rot,
                    scale: TILE_SIZE,
                    blocks_movement: false,
                });
            }
        }

        // 2. Decoration
        // --- Structures / Clusters ---
        if let Some(ref cluster) = self.town_cluster {
            let placements_clone = cluster.placements.clone();
            for p in &placements_clone {
                let world_pos = p.pos_vec3();
                let extent = 1.0; // Max hitbox radius (MAX_HITBOX_CELL_EXTENT * GRID_SIZE)
                
                let min_pos = world_pos - vec3(extent, 0.0, extent);
                let max_pos = world_pos + vec3(extent, 0.0, extent);
                
                let min_coord = ChunkCoord::from_world_pos(min_pos);
                let max_coord = ChunkCoord::from_world_pos(max_pos);
                
                // If current chunk is within the range of chunks this object covers
                if coord.x >= min_coord.x && coord.x <= max_coord.x &&
                   coord.z >= min_coord.z && coord.z <= max_coord.z {
                    self.ensure_template_loaded(&p.model, &p.file);
                    placements.push(p.clone());

                    if p.blocks_movement && !Self::is_gate_model(&p.model) {
                        if let Some(t) = self.templates.get(&p.model) {
                            self.apply_blocking_local(
                                &mut walkability,
                                coord,
                                t,
                                &p.model,
                                world_pos,
                                p.rotation,
                                p.scale,
                            );
                        }
                    }
                }
            }
        }

        match biome {
            BiomeType::Town => {
                // Authored town cluster is handled above globally

            }
            _ => {
                let density = match biome {
                    BiomeType::Forest => 120,
                    BiomeType::Grassland => 40,
                    _ => 20,
                };

                for _ in 0..density {
                    let rx = rng.gen_range(0.0..CHUNK_SIZE_M);
                    let rz = rng.gen_range(0.0..CHUNK_SIZE_M);
                    let wx = origin.x + rx;
                    let wz = origin.z + rz;

                    let roll = rng.gen_range(0.0..1.0);
                    let (key, blocks) = if roll < 0.6 {
                        (if biome == BiomeType::Forest { "tree_c" } else { "tree_a" }, true)
                    } else if roll < 0.8 {
                        ("rock_a", true)
                    } else {
                        ("flower_a", false)
                    };

                    if let Some(t) = self.templates.get(key) {
                        let pos = vec3(wx, 0.0, wz);
                        let rot = rng.gen_range(0.0..6.28);
                        let scale = if blocks { 2.0 } else { 0.8 } + rng.gen_range(-0.2..0.4);

                        placements.push(ModelPlacement {
                            model: key.to_string(),
                            file: format!("{}.glb", key),
                            position: [pos.x, 0.0, pos.z],
                            rotation: rot,
                            scale,
                            blocks_movement: blocks,
                        });

                        if blocks {
                            self.apply_blocking_local(&mut walkability, coord, t, key, pos, rot, scale);
                        }
                    }
                }
            }
        }

        let mut pathfinding = walkability.clone();
        for x in 0..GRID_WIDTH {
            for z in 0..GRID_WIDTH {
                if !walkability[x as usize][z as usize] {
                    for dx in -1..=1 {
                        for dz in -1..=1 {
                            let nx = x + dx;
                            let nz = z + dz;
                            if nx >= 0 && nx < GRID_WIDTH && nz >= 0 && nz < GRID_WIDTH {
                                pathfinding[nx as usize][nz as usize] = false;
                            }
                        }
                    }
                }
            }
        }

        self.chunks.insert(coord, Chunk {
            coord,
            biome,
            placements: placements.clone(),
            walkability,
            pathfinding,
        });

        if generate_meshes {
            let mut meshes = Vec::new();
            for p in &placements {
                if p.model == "gate" {
                    let pos = p.pos_vec3();
                    self.register_gate(coord, pos, p.rotation);
                    continue;
                }
                if let Some(t) = self.templates.get(&p.model) {
                    meshes.extend(instantiate(t, p.pos_vec3(), p.rotation, p.scale));
                }
            }
            self.render_data.insert(coord, ChunkRenderData { meshes });
        }

        true
    }

    pub fn insert_chunk(&mut self, coord: ChunkCoord, biome: BiomeType, placements: Vec<ModelPlacement>) {
        let mut walkability = vec![vec![true; GRID_WIDTH as usize]; GRID_WIDTH as usize];
        
        for p in &placements {
            self.ensure_template_loaded(&p.model, &p.file);
            if p.blocks_movement && !Self::is_gate_model(&p.model) {
                if let Some(t) = self.templates.get(&p.model) {
                    self.apply_blocking_local(&mut walkability, coord, t, &p.model, p.pos_vec3(), p.rotation, p.scale);
                }
            }
        }

        let mut pathfinding = walkability.clone();
        // (pathfinding padding logic...)
        for x in 0..GRID_WIDTH {
            for z in 0..GRID_WIDTH {
                if !walkability[x as usize][z as usize] {
                    for dx in -1..=1 {
                        for dz in -1..=1 {
                            let nx = x + dx;
                            let nz = z + dz;
                            if nx >= 0 && nx < GRID_WIDTH && nz >= 0 && nz < GRID_WIDTH {
                                pathfinding[nx as usize][nz as usize] = false;
                            }
                        }
                    }
                }
            }
        }

        let mut meshes = Vec::new();
        for p in &placements {
            if p.model == "gate" {
                let pos = p.pos_vec3();
                self.register_gate(coord, pos, p.rotation);
                continue;
            }
            if let Some(t) = self.templates.get(&p.model) {
                meshes.extend(instantiate(t, p.pos_vec3(), p.rotation, p.scale));
            }
        }

        self.chunks.insert(coord, Chunk {
            coord,
            biome,
            placements,
            walkability,
            pathfinding,
        });
        self.render_data.insert(coord, ChunkRenderData { meshes });
    }

    fn apply_blocking_local(
        &self,
        grid: &mut [Vec<bool>],
        coord: ChunkCoord,
        template: &GltfTemplate,
        model_key: &str,
        world_pos: Vec3,
        rotation: f32,
        scale: f32,
    ) {
        let chunk_center = coord.world_center();
        let local_pos = world_pos - chunk_center;

        apply_hitbox_blocking(
            grid,
            template,
            model_key,
            local_pos,
            rotation,
            scale,
            &self.hitbox_config,
            GRID_SIZE,
            GRID_WIDTH,
            GRID_WIDTH,
        );
    }

    pub fn is_walkable(&self, pos: Vec3) -> bool {
        // 1. Check static walkability
        let coord = ChunkCoord::from_world_pos(pos);
        if let Some(chunk) = self.chunks.get(&coord) {
            let center = coord.world_center();
            let local = pos - center;
            let gx = ((local.x / GRID_SIZE).round() + (GRID_WIDTH / 2) as f32) as i32;
            let gz = ((local.z / GRID_SIZE).round() + (GRID_WIDTH / 2) as f32) as i32;

            if gx >= 0 && gx < GRID_WIDTH && gz >= 0 && gz < GRID_WIDTH {
                if !chunk.walkability[gx as usize][gz as usize] {
                    return false;
                }
            }
        }

        // 2. Check dynamic gates
        let closed_mask = self.gate_mask("gate");
        let open_mask = self.gate_mask("gate_open");

        for gate in &self.gates {
            let dx = gate.position.x - pos.x;
            let dz = gate.position.z - pos.z;
            if dx * dx + dz * dz
                > Self::GATE_COLLISION_CHECK_RADIUS * Self::GATE_COLLISION_CHECK_RADIUS
            {
                continue;
            }

            let mask = if gate.mask_key() == "gate_open" {
                open_mask.as_ref()
            } else {
                closed_mask.as_ref()
            };

            let Some(mask) = mask else {
                continue;
            };

            if crate::world::environment::is_point_in_painted_mask(
                gate.position,
                gate.base_rotation,
                2.0,
                mask,
                GRID_SIZE,
                pos,
            ) {
                return false;
            }
        }

        true
    }

    pub fn update_gates(&mut self, player_positions: &[Vec3], dt: f32) {
        for gate in &mut self.gates {
            if gate.state == GateState::Locked {
                continue;
            }

            // Proximity check
            let open_radius_sq = Self::GATE_OPEN_RADIUS * Self::GATE_OPEN_RADIUS;
            let close_radius_sq = Self::GATE_CLOSE_RADIUS * Self::GATE_CLOSE_RADIUS;
            let threshold_sq = if gate.open_progress > 0.0 {
                close_radius_sq
            } else {
                open_radius_sq
            };

            let mut player_nearby = false;
            for &p_pos in player_positions {
                let dx = gate.position.x - p_pos.x;
                let dz = gate.position.z - p_pos.z;
                if dx * dx + dz * dz <= threshold_sq {
                    player_nearby = true;
                    break;
                }
            }

            if player_nearby {
                gate.state = GateState::Open;
            } else {
                gate.state = GateState::Closed;
            }

            // Animation
            if gate.state == GateState::Open {
                gate.open_progress = (gate.open_progress + dt * 2.0).min(1.0);
            } else {
                gate.open_progress = (gate.open_progress - dt * 2.0).max(0.0);
            }
        }
    }

    pub fn update(&mut self, player_pos: Vec3, load_radius: i32, is_client: bool) {
        let center = ChunkCoord::from_world_pos(player_pos);
        
        for dx in -load_radius..=load_radius {
            for dz in -load_radius..=load_radius {
                let coord = ChunkCoord::new(center.x + dx, center.z + dz);
                self.generate_chunk(coord, is_client);
            }
        }

        if is_client {
            self.chunks.retain(|coord, _| {
                (coord.x - center.x).abs() <= load_radius + 1 &&
                (coord.z - center.z).abs() <= load_radius + 1
            });
            self.render_data.retain(|coord, _| {
                (coord.x - center.x).abs() <= load_radius + 1 &&
                (coord.z - center.z).abs() <= load_radius + 1
            });
            self.prune_stale_gates();
        }
    }

    pub fn draw(&self) {
        for render in self.render_data.values() {
            for mesh in &render.meshes {
                draw_mesh(mesh);
            }
        }

        // Draw dynamic gates
        if let Some(gate_template) = self.templates.get("gate") {
            for gate in &self.gates {
                // Determine current rotation based on animation progress
                // Door swings open by 90 degrees (1.57 radians)
                let meshes =
                    instantiate_gate(gate_template, gate.position, gate.base_rotation, 2.0, gate.open_progress);
                for mesh in meshes {
                    draw_mesh(&mesh);
                }
            }
        }
    }
}
