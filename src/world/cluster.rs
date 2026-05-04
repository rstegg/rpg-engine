use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

pub const BIOMES: [&str; 5] = ["grassland", "forest", "wetland", "rocky", "camp"];

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MapBounds {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BiomeRegion {
    pub name: String,
    pub biome: String,
    pub center: [f32; 2],
    pub radius: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelPlacement {
    pub model: String,
    pub file: String,
    pub position: [f32; 3],
    pub rotation: f32,
    pub scale: f32,
    pub blocks_movement: bool,
}

impl ModelPlacement {
    pub fn pos_vec3(&self) -> Vec3 {
        vec3(self.position[0], self.position[1], self.position[2])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldCluster {
    pub name: String,
    pub biome: String,
    pub placements: Vec<ModelPlacement>,
}

impl WorldCluster {
    pub fn new(name: impl Into<String>, biome: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            biome: biome.into(),
            placements: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapDocument {
    pub name: String,
    pub bounds: MapBounds,
    pub biome_regions: Vec<BiomeRegion>,
    pub clusters: Vec<WorldCluster>,
}

impl MapDocument {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bounds: MapBounds {
                width: 256.0,
                height: 256.0,
            },
            biome_regions: Vec::new(),
            clusters: vec![WorldCluster::new("cluster_001", BIOMES[0])],
        }
    }

    pub fn active_cluster_mut(&mut self) -> &mut WorldCluster {
        if self.clusters.is_empty() {
            self.clusters
                .push(WorldCluster::new("cluster_001", BIOMES[0]));
        }
        &mut self.clusters[0]
    }

    pub fn active_cluster(&self) -> Option<&WorldCluster> {
        self.clusters.first()
    }
}
