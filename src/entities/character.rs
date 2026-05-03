use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

/// Serializable character appearance — this is what gets saved to JSON,
/// sent over the network in multiplayer, and used to spawn NPCs/enemies.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharacterAppearance {
    pub skin: String,
    pub shoes: Option<String>,
    pub clothes: Option<String>,
    pub gloves: Option<String>,
    pub hairstyle: Option<String>,
    pub facial_hair: Option<String>,
    pub eye_color: Option<String>,
    pub eyelashes: Option<String>,
    pub headgear: Option<String>,
    pub addon: Option<String>,
}

impl CharacterAppearance {
    pub fn default_human() -> Self {
        Self {
            skin: "Human1".to_string(),
            shoes: None,
            clothes: None,
            gloves: None,
            hairstyle: None,
            facial_hair: None,
            eye_color: None,
            eyelashes: None,
            headgear: None,
            addon: None,
        }
    }

    /// Save appearance to a JSON file
    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Load appearance from a JSON file
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}

/// Represents all available options discovered from the assets directory.
#[derive(Clone, Debug)]
pub struct LayerCatalog {
    pub skins: Vec<LayerOption>,
    pub shoes: Vec<LayerOption>,
    pub clothes: Vec<LayerOption>,
    pub gloves: Vec<LayerOption>,
    pub hairstyles: Vec<LayerOption>,
    pub facial_hair: Vec<LayerOption>,
    pub eye_colors: Vec<LayerOption>,
    pub eyelashes: Vec<LayerOption>,
    pub headgears: Vec<LayerOption>,
    pub addons: Vec<LayerOption>,
}

#[derive(Clone, Debug)]
pub struct LayerOption {
    pub name: String,
    pub path: String, // Relative path for loading
}

impl LayerCatalog {
    /// Scan the assets/characters/layers/ directory and discover all available options.
    pub fn discover() -> Self {
        let base = "assets/characters/layers";

        Self {
            skins: Self::scan_flat(&format!("{}/skins", base)),
            shoes: Self::scan_flat(&format!("{}/shoes", base)),
            clothes: Self::scan_recursive(&format!("{}/clothes", base)),
            gloves: Self::scan_flat(&format!("{}/gloves", base)),
            hairstyles: Self::scan_recursive(&format!("{}/hairstyles/Hairstyles", base)),
            facial_hair: Self::scan_recursive(&format!("{}/hairstyles/Facial Hairstyles", base)),
            eye_colors: Self::scan_recursive(&format!("{}/eyes/Eye Color", base)),
            eyelashes: Self::scan_recursive(&format!("{}/eyes/Eyelashes", base)),
            headgears: Self::scan_recursive(&format!("{}/headgears", base)),
            addons: Self::scan_recursive(&format!("{}/addons", base)),
        }
    }

    /// Scan a directory for .png files directly inside it.
    fn scan_flat(dir: &str) -> Vec<LayerOption> {
        let mut results = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "png").unwrap_or(false) {
                    let name = path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    // Use forward slashes for macroquad compatibility
                    let rel_path = path.to_string_lossy().replace('\\', "/");
                    results.push(LayerOption { name, path: rel_path });
                }
            }
        }
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    /// Recursively scan subdirectories for .png files.
    fn scan_recursive(dir: &str) -> Vec<LayerOption> {
        let mut results = Vec::new();
        Self::scan_recursive_inner(dir, &mut results);
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    fn scan_recursive_inner(dir: &str, results: &mut Vec<LayerOption>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::scan_recursive_inner(&path.to_string_lossy(), results);
                } else if path.extension().map(|e| e == "png").unwrap_or(false) {
                    let name = path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let rel_path = path.to_string_lossy().replace('\\', "/");
                    results.push(LayerOption { name, path: rel_path });
                }
            }
        }
    }

    /// Get the file path for a given layer name, searching the appropriate category.
    pub fn find_path(&self, category: LayerCategory, name: &str) -> Option<String> {
        let list = match category {
            LayerCategory::Skin => &self.skins,
            LayerCategory::Shoes => &self.shoes,
            LayerCategory::Clothes => &self.clothes,
            LayerCategory::Gloves => &self.gloves,
            LayerCategory::Hairstyle => &self.hairstyles,
            LayerCategory::FacialHair => &self.facial_hair,
            LayerCategory::EyeColor => &self.eye_colors,
            LayerCategory::Eyelashes => &self.eyelashes,
            LayerCategory::Headgear => &self.headgears,
            LayerCategory::Addon => &self.addons,
        };
        list.iter().find(|o| o.name == name).map(|o| o.path.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayerCategory {
    Skin,
    Shoes,
    Clothes,
    Gloves,
    Hairstyle,
    FacialHair,
    EyeColor,
    Eyelashes,
    Headgear,
    Addon,
}

impl LayerCategory {
    pub const ALL: [LayerCategory; 10] = [
        LayerCategory::Skin,
        LayerCategory::Shoes,
        LayerCategory::Clothes,
        LayerCategory::Gloves,
        LayerCategory::Hairstyle,
        LayerCategory::FacialHair,
        LayerCategory::EyeColor,
        LayerCategory::Eyelashes,
        LayerCategory::Headgear,
        LayerCategory::Addon,
    ];

    pub fn label(&self) -> &str {
        match self {
            LayerCategory::Skin => "Skin",
            LayerCategory::Shoes => "Shoes",
            LayerCategory::Clothes => "Clothes",
            LayerCategory::Gloves => "Gloves",
            LayerCategory::Hairstyle => "Hairstyle",
            LayerCategory::FacialHair => "Facial Hair",
            LayerCategory::EyeColor => "Eye Color",
            LayerCategory::Eyelashes => "Eyelashes",
            LayerCategory::Headgear => "Headgear",
            LayerCategory::Addon => "Add-on",
        }
    }
}

/// Holds the loaded textures for a character's current appearance.
/// These get drawn in layer order to composite the final sprite.
pub struct CharacterTextures {
    pub layers: Vec<Texture2D>, // Drawn bottom-to-top
}

impl CharacterTextures {
    /// Load all textures for a given appearance from the catalog.
    pub async fn from_appearance(appearance: &CharacterAppearance, catalog: &LayerCatalog) -> Self {
        let mut layers = Vec::new();

        // Helper: try to find and load a texture by name from a category
        async fn try_load(catalog: &LayerCatalog, category: LayerCategory, name: &Option<String>) -> Option<Texture2D> {
            if let Some(n) = name {
                if let Some(path) = catalog.find_path(category, n) {
                    if let Ok(tex) = load_texture(&path).await {
                        tex.set_filter(FilterMode::Nearest);
                        return Some(tex);
                    }
                }
            }
            None
        }

        // Layer 0: Skin (required)
        if let Some(path) = catalog.find_path(LayerCategory::Skin, &appearance.skin) {
            if let Ok(tex) = load_texture(&path).await {
                tex.set_filter(FilterMode::Nearest);
                layers.push(tex);
            }
        }

        // Layers 1-7 in draw order
        if let Some(t) = try_load(catalog, LayerCategory::Shoes, &appearance.shoes).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::Clothes, &appearance.clothes).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::Gloves, &appearance.gloves).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::EyeColor, &appearance.eye_color).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::Eyelashes, &appearance.eyelashes).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::Hairstyle, &appearance.hairstyle).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::FacialHair, &appearance.facial_hair).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::Addon, &appearance.addon).await { layers.push(t); }
        if let Some(t) = try_load(catalog, LayerCategory::Headgear, &appearance.headgear).await { layers.push(t); }

        Self { layers }
    }
}
