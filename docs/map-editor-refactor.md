# Map Editor UI & Hitbox System Refactor

This document summarizes the changes made during the May 2026 refactor of the RPG Map Editor.

## Goals
- **Modernize UI**: Migrate from manual `macroquad` rendering to `egui` to improve usability and fix interaction bugs.
*   **Improve Text Input**: Implement a functional search field with standard text editing features (focus, cursor, keyboard shortcuts).
*   **Enhance Hitbox System**: Support state-dependent hitboxes (e.g., Open vs. Closed gates) in the calibration tool and gameplay.
*   **Fix PlayTest Mode**: Ensure hitboxes are correctly generated for all objects regardless of their world coordinates.

## Technical Changes

### 1. Egui Migration (`src/systems/cluster_editor.rs`)
- **Panel-Based Layout**: The editor now uses `egui::SidePanel` for the left toolbar and right asset/cluster panels.
- **Interactive Search**: Replaced manual character buffer handling with `egui::TextEdit`. This resolved the "unclickable search" bug and added support for pasting, cursor movement, and filtering.
- **State Integration**: 
    - `ClusterEditor::update` now accepts an `egui_capturing` flag from `main.rs` to prevent 3D tool interactions when clicking on UI panels.
    - `ClusterEditor::draw_egui` handles the rendering pass, while `draw_3d` focuses on the world view.
- **Cleanup**: Removed ~400 lines of legacy Macroquad UI code (`draw_ui`, `draw_tool_button`, manual rect checks, etc.).

### 2. Hitbox Calibration Improvements
- **State-Dependent Masks**: Added `gate_open` as a virtual model key in the Hitbox Calibration tool.
- **Gate Logic**: 
    - The calibration tool correctly reuses the gate model for visualization but saves the painted mask under the `gate_open` key in `hitbox_config.json`.
    - `src/world/chunk.rs` now switches dynamically between `gate` and `gate_open` masks based on the `gate.open_progress` state.

### 3. Rendering & Depth Fixes (`src/main.rs`)
- **Depth State Reset**: Added `gl_use_default_material()` before the 3D draw call in the editor. This ensures that the depth testing state disabled by `egui` is restored for 3D objects, preventing fences from rendering "under" tiles.
- **Input Reconciliation**: Fixed the main loop's character buffer drainage to allow `egui` to capture keystrokes in the editor mode.

### 4. PlayTest Mode Correction
- **Multi-Chunk Support**: Refactored the `PlayTest` initialization to group placements by their `ChunkCoord`. Placements are now correctly inserted into multiple chunks instead of being forced into chunk `(0,0)`, which fixed missing hitboxes for objects at negative or distant coordinates.

## How to Use
- **Search**: Click the "Search" field in the Assets tab to filter models. Standard text shortcuts (Ctrl+A, etc.) work.
- **Gate Hitboxes**: In Hitbox Calibration mode, select `gate_open` to paint the walkability for an open gate.
- **Play**: Click the "Play" button in the toolbar to test the current cluster with full collision and gate interactions.
