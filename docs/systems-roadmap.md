# Systems Roadmap

This document outlines the technical implementation plan for the core engine systems required to reach the milestones defined in our `game-direction.md`.

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

### 3. In-Run Progression
- [ ] In-match XP distribution and leveling logic.
- [ ] Upgrade drafting UI (roguelike choice mechanic at end of waves or on level-up).
- [ ] Modular upgrade application architecture (e.g., dynamically modifying existing spell components like chaining or AoE radius).

## Phase 3: RTS Layer

### 1. Selection & Command Input
- [ ] Box selection UI and multi-unit targeting logic.
- [ ] Command queuing (Attack-move, Patrol, Hold Position, Interact).
- [ ] Network protocols for issuing squad/unit commands to the authoritative server.

### 2. Allied Units & Summons
- [ ] Support units AI (following player, assisting with targets, formation holding).
- [ ] Summon lifecycle and ownership management.

## Phase 4: Metagame & Advanced Modes

### 1. Procedural Generation Enhancements
- [ ] Objective placement generation (defend points, extraction zones).
- [ ] Dynamic chokepoint and map flow generation based on mode rules.

### 2. Mode Expansion (PvP & PvPvE)
- [ ] Team structure data models (Friendly fire rules, vision sharing, shared resources).
- [ ] Mode-specific win conditions, scoring, and end-of-match flows.
