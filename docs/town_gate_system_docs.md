# Town Gate System & Hot-Reloading Documentation

This document summarizes the technical implementation of the interactive town gates and the data hot-reloading system developed during this session.

---

## 1. Dual-State Interactive Gate System

The gates in the RPG engine are now fully interactive, featuring dynamic animations and state-aware collision detection.

### Technical Implementation
- **Asset Structure**: `fence_gate.glb` does not contain a separate baked "open" mesh or an animation track. It contains a static frame mesh plus a child node named `gate` for the swinging leaf.
- **Hitbox Logic**: Gates use "Painted Masks" for precise collision. The runtime tracks `open_progress` (0.0 to 1.0) and switches between two hitbox configurations:
    - `gate`: The closed collision mask.
    - `gate_open`: The open collision mask.
- **State Preview**: The hitbox calibration tool previews the real closed/open gate states by rotating only the `gate` child node. It does not rotate the entire model.
- **Proximity Triggers**: The `ChunkedWorld::update_gates` method (on the server) scans player positions. If a player is within **5.0 meters** of a gate, it transitions to the `Open` state. It returns to `Closed` once all players leave the radius.
- **Server-Authoritative Collision**: In `src/world/chunk.rs`, the `is_walkable` function now uses `is_point_in_painted_mask` from `environment.rs`. It dynamically selects the mask based on the gate's current `open_progress`.

### Configuration
Gates are identified in `town.json` by the model name `"gate"`. Any placement with this name is automatically converted into a dynamic `Gate` object upon chunk generation.

---

## 2. Data Hot-Reloading System

To speed up iteration, the engine now supports live-reloading of world data without requiring a restart of the server or client.

### Mechanism
- **File Watching**: The `ChunkedWorld` and `WorldSimulation` structs track the `SystemTime` (mtime) of key data files:
    - `assets/clusters/town.json`
    - `hitbox_config.json`
- **Update Loop**: Every 1 second, the engine calls `check_hot_reload()`. If a file's modification time has changed, it re-parses the JSON and refreshes the internal state.
- **State Invalidation**:
    - **Town Changes**: Clears the `(0, 0)` town chunk, forcing an immediate regeneration for all players.
    - **Hitbox Changes**: Clears **all** cached chunks to ensure every collision grid is updated with the new masks.

---

## 3. Town Cluster Schema & Spawn Points

### WorldCluster Requirements
The engine strictly enforces the `WorldCluster` schema for JSON files in `assets/clusters/`:
- `name`: (String) Must match the filename (e.g., "town").
- `biome`: (String) Used for procedural blending.
- `placements`: (Array) List of `ModelPlacement` objects.

### Player Spawning
- The default spawn point has been updated to **(20.0, 20.0)**.
- This is synchronized across:
    - `migrations/001_init.sql` (Database default for new characters).
    - `server.rs` (Explicit override during character creation).
    - `town.json` (The centered town enclosure).

---

## 4. Usage Tips
- **Editing Gates**: Use the Hitbox Editor to define both `closed` and `gate_open` masks for the `fence_gate.glb` model.
- **Rebuilding Town**: The current `town.json` is a minimal valid template with 4 centered gates. Use the **Map Editor (F3)** Clusters tab to save your layout as "town" to see it live in the game.
