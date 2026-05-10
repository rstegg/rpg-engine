# Procedural RPG Engine

A 2.5D multiplayer action-RTS roguelike engine built in Rust using `macroquad`.

## How to Run

### Single Player / Local Testing
To launch the game client:
```bash
cargo run --bin client
```
*(Or simply `cargo run` as the client is the default binary)*

### Multiplayer (Server & Client)
To test multiplayer functionality, you need to run the server and the client(s) separately.

1. **Start the server:**
   ```bash
   cargo run --bin server
   ```
2. **Start the client(s):**
   ```bash
   cargo run --bin client
   ```
   *When prompted in-game, hit ENTER to connect to `127.0.0.1:7878`, or press ESC to play offline.*

## Controls

### Combat & Movement
- **Right Click:** Move to a location or cancel current spell targeting.
- **Q / W / E / R:** Prime an ability (AoE or Unit Target).
- **Left Click:** Confirm casting the primed ability at the mouse cursor location or target.

### Developer Tools
- **F1:** Toggle Pathfinding Debug Overlay (shows walkability grid and current path).
- **F2:** Open Hitbox Calibration Tool (calibrate movement blockers for 3D world models).
- **F3:** Open Cluster Editor (place props, generate environment clusters, and playtest maps).

## Web Build (WIP)
Compiling to WebAssembly is planned but currently unsupported out-of-the-box due to local filesystem asset loading. Future updates will include an asset manifest system to allow `wasm32-unknown-unknown` compilation.
