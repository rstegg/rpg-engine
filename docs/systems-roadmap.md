# Systems Roadmap

This document outlines the technical implementation plan for the core engine systems required to reach the milestones defined in our `game-direction.md`.

> [!IMPORTANT]
> **AI Directive**: Whenever you start or complete any task in this roadmap, you MUST immediately update both the roadmap's checklist checkboxes (`[ ]`, `[-]`, `[x]`) and the **Immediate Next Steps (Context Recovery)** section below to reflect the absolute latest status of the project.

## Legend
- `[ ]` Not Started
- `[-]` In Progress / Partially Completed
- `[x]` Completed

## Immediate Next Steps (Context Recovery)
If context is lost, resume development here:
1. **Builder Unit Logic**: Buildings currently build themselves. Next step is requiring a specific "Builder" unit (e.g., Peasant) to walk to the designated construction site and build it over time.
2. **Resource Economy**: Implement Gold, Wood, and Food. Setup AI for Peasants to harvest from resource nodes (Trees/Gold Mines) and return them to the Town Hall.
3. **Box Selection & Squad Movement**: Implement drag-box multi-selection for units on the client, and update the network protocol to handle squad-level movement and attack commands.



## Phase 1: Core Combat & Entity Management (Immediate Priority)

### 1. Stats and Attributes System
- [ ] Centralized component for Health, Mana, Stamina, and Move Speed.
- [ ] Damage calculation pipeline (Base damage + modifiers - armor).
- [ ] Efficient network synchronization for dynamic and volatile stats.

### 2. Combat Interaction Sandbox
- [ ] **Hitboxes & Hurtboxes**: Accurate spatial queries for melee strikes, AoE, and cleave attacks.
- [ ] **Projectiles System**: Managed lifecycle, continuous collision detection, and network sync.
- [ ] **Status Effects / Auras**: Buffs and debuffs (Slow, Stun, Burning, Speed Aura) with duration tracking, ticking effects, and stacking rules.

### 3. Enemy AI & Behavior
- [ ] **Threat/Aggro Management**: Distance-based aggro, line-of-sight checks, and threat tables for target prioritization.
- [ ] **State Machine Expansion**: Move beyond basic follow/attack. Introduce Retreat, Patrol, Cast Spell, and Stunned states.
- [ ] **Swarm Pathfinding Optimization**: Optimized group movement and collision avoidance to handle high enemy density without excessive A* overhead.

## Phase 2: Encounter & Gameplay Loop

### 1. Spawner & Wave Director
- [ ] **Spawn Logic**: Biome and nav-mesh aware enemy placement.
- [ ] **Wave Management**: State machine for tracking wave progress, spawning schedules, and difficulty scaling.
- [ ] **Encounter Director**: Dynamic adjustment of spawns and pressure based on player performance and location.

### 2. Match State Management
- [ ] Server-owned match state machine (Lobby -> Preparation -> Wave -> Boss -> End).
- [ ] Co-op objective tracking and network event broadcasting.
- [ ] Player revive/respawn mechanics.

### 3. Hero & In-Run Progression
- [ ] **MOBA-Like Heroes**: Empower player-controlled units as "Heroes" built from any base unit class (e.g., Dwarf Valkyrie). Heroes are visually scaled up and have enhanced base stats.
- [ ] **Inventory & Equipment**: Heroes can carry and equip items. Equipping an item dynamically alters their visual `CharacterAppearance` (e.g., changing helmets or weapons).
- [ ] **Inventory UI**: Full drag-and-drop user interface for managing hero inventory, stash, and equipped items.
- [ ] **Leveling & Scaling**: In-match XP distribution, stat growth, and skill unlocking over the course of a match.
- [ ] **Roguelike Upgrades**: Upgrade drafting UI (roguelike choice mechanic at end of waves or on level-up). Modular upgrade application architecture (e.g., dynamically modifying existing spell components like chaining or AoE radius).

## Phase 3: RTS Layer

### 1. Base Building & Economy
- [ ] **Resource System**: Core resources (e.g., Gold, Wood, Food/Supply) and UI resource bar.
- [ ] **Worker Units & Harvesting**: AI states for workers to harvest resources and return them to a drop-off point.
- [-] **Building Placement**: Grid-snapping or free-placement system, validation checks (not colliding), and dynamic pathfinding/nav-mesh updates.
- [-] **Construction Lifecycle**: Builder units walking to a site, construction progress over time, and visual building states.

### 2. Selection & Command Input
- [ ] Box selection UI and multi-unit targeting logic.
- [ ] Command queuing (Attack-move, Patrol, Hold Position, Interact, Harvest, Build).
- [x] Command Card UI (bottom panel) dynamically updating based on selected entity (showing spawn actions, upgrades, building options).
- [-] Network protocols for issuing squad/unit commands to the authoritative server.

### 3. Tech Trees & Production
- [-] **Unit Spawning**: Buildings queuing and spawning units over time, with rally points.
- [ ] **Global Upgrades**: Researching upgrades at specific buildings (e.g., Blacksmith armor upgrades) that apply to all relevant allied units.
- [ ] **Prerequisites System**: Building and unit availability gated by tech tree progression (e.g., requires Barracks to build Blacksmith).

### 4. Allied Units & Summons
- [ ] Support units AI (following player, assisting with targets, formation holding).
- [ ] Summon lifecycle and ownership management.

## Phase 4: Metagame & Advanced Modes

### 1. Procedural Generation Enhancements
- [ ] Objective placement generation (defend points, extraction zones).
- [ ] Dynamic chokepoint and map flow generation based on mode rules.

### 2. Mode Expansion (PvP & PvPvE)
- [ ] Team structure data models (Friendly fire rules, vision sharing, shared resources).
- [ ] Mode-specific win conditions, scoring, and end-of-match flows.
