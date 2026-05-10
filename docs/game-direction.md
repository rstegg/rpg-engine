# Game Direction

## Core Pitch

Build a procedurally generated roguelike RTS with fast, readable combat, large enemy counts, and strong co-op or versus play.

The game should feel good because:

- Players are almost always fighting, repositioning, or making meaningful build/ability choices.
- Enemy density creates pressure without turning the battlefield into noise.
- Co-op amplifies chaos and teamwork instead of slowing the game down.
- Matches are short enough to replay, but varied enough to support long-term mastery.

## Fun Pillars

### 1. Combat Density

The player fantasy is not careful dueling. It is surviving and controlling overwhelming fights.

Success signals:

- Large groups of enemies on screen.
- Strong area damage, knockback, slows, summons, or control tools.
- Clear feedback when a wave is erased.
- Frequent spikes in danger followed by recovery and escalation.

### 2. Tactical Readability

This is RTS-adjacent, so the battlefield must stay legible under chaos.

Success signals:

- Units, threats, allies, and objectives are readable at a glance.
- Terrain changes decision-making instead of just decorating the map.
- Abilities have distinct battlefield jobs.
- The player can make good decisions quickly under pressure.

### 3. Social Chaos

Multiplayer should increase stories, not friction.

Success signals:

- Co-op is fun immediately without requiring strict roles.
- PvP modes create map pressure, timing windows, and comeback moments.
- Shared destruction and big spell interactions generate memorable moments.

### 4. Replayable Runs

Procedural generation should change the run, not just the scenery.

Success signals:

- Different maps change pathing, defense points, pacing, and enemy approach routes.
- Players adapt builds to run-specific conditions.
- Modes can remix the same systems into different match structures.

## Product Definition

### Genre Target

The best framing right now is:

**"A multiplayer action-RTS roguelike where players survive escalating enemy swarms on procedural maps, then branch into co-op PvE or competitive PvP modes."**

This is stronger than trying to be a general-purpose RPG engine first.

### Primary Audience Promise

- Kill a lot of enemies.
- Get stronger inside a run.
- Make tactical decisions with friends.
- Replay short-to-medium matches with different map layouts and mode rules.

## Recommended North Star Loop

1. Spawn into a generated arena or map slice.
2. Expand control through movement, positioning, and combat.
3. Survive enemy waves or pressure from another team.
4. Earn upgrades, units, or temporary run modifiers.
5. Hit a mid-run fork: specialize offense, control, economy, or mobility.
6. Reach a climactic final phase: boss, last stand, extraction, or PvP showdown.

If a feature does not improve this loop, it is probably not next-priority work.

## What The Current Project Already Has

- 3D world rendering with billboarded characters and effect playback.
- Pathfinding and movement foundations.
- Procedural environment placement.
- A map and cluster editor.
- Basic client/server transport for multiplayer.
- Character appearance customization.

## What Is Missing For "Fun"

### Missing combat game

The codebase currently has movement, target selection, and visual spell effects, but not a real combat sandbox:

- No enemy AI ecosystem.
- No damage, death, wave pressure, or threat management loop.
- No progression inside a match.
- No objectives that force tactical play.

### Missing multiplayer game rules

Networking exists, but the game does not yet define:

- Match states.
- Team structure.
- Co-op objectives.
- PvP win conditions.
- Server-owned combat simulation.

### Missing procedural gameplay generation

The world is visually procedural, but the run is not yet procedurally designed:

- No encounter director.
- No spawn logic by biome or danger level.
- No run mutators.
- No objective placement logic.

## Recommended Next Priorities

### Priority 1: Build the smallest fun PvE loop

Do this before deeper RTS, economy, or broad PvP work.

Target slice:

- 1 player or co-op.
- Procedural arena/map slice.
- Waves of enemies.
- Real damage and death.
- 3-5 distinct enemy archetypes.
- 3-4 impactful abilities with cooldowns and crowd control.
- End-of-wave rewards or level-up choices.

Success metric:

- A 10-minute run is already fun with placeholder content.

### Priority 2: Add enemy volume and battlefield control

Since "killing lots of enemies" is central, the game needs systems that scale swarm combat:

- Lightweight enemy agents.
- Spawn director with pacing phases.
- AOE-heavy abilities.
- Chokepoints, flanks, and terrain pressure.
- Performance budgeting for large encounters.

Success metric:

- The game supports "too many enemies" moments without losing clarity.

### Priority 3: Define the RTS layer

Do not overbuild the RTS layer until the core combat loop is fun.

Pick one RTS direction first:

- Hero-centric RTS: player controls one hero plus support units.
- Squad RTS: player commands a small squad with active abilities.
- Base-lite survival RTS: defend, expand, and tech during waves.

Recommended first choice:

**Hero-centric RTS with support units.**

Reason:

- It fits your current direct-control movement and spellcasting.
- It keeps the screen readable.
- It is easier to make fun quickly in co-op and PvP.

### Priority 4: Add mode structure

After the PvE loop works, build modes by recombining the same systems.

Recommended mode order:

1. Co-op survival
2. Co-op objective run
3. Team PvPvE
4. Pure PvP skirmish

This order preserves momentum because PvE pressure will help validate combat and procedural systems faster than pure PvP balance work.

## Concrete Design Recommendations

### Keep matches compact

- Aim for 10-20 minute sessions first.
- Long-form persistence is not the first fun problem to solve.

### Favor enemy archetypes over content breadth

Start with a few enemies that create positioning problems:

- Swarm melee unit
- Tank/bruiser
- Ranged harasser
- Support or summoner
- Elite miniboss

### Make upgrades run-defining

Good upgrade examples:

- Arrows split or chain
- Melee strike causes shockwave
- Fire spell leaves burning ground
- Dark void pulls enemies inward
- Movement command grants a temporary speed aura to nearby allies

### Use procedural generation for tactics

Procedural map output should alter:

- lane width
- choke frequency
- open vs dense terrain
- objective exposure
- spawn angles

### Treat PvP as a mode, not the default balance target

If you balance too early for fair duel play, you will weaken the power fantasy.

First make:

- explosive PvE combat
- readable co-op synergy
- strong run progression

Then constrain or tune those systems for PvP variants.

## Suggested Milestone Roadmap

### Milestone A: Vertical Slice

- One map generator
- One playable hero
- Four abilities
- Three enemy types
- Damage, death, waves, rewards
- Basic co-op

### Milestone B: Replayable Runs

- Upgrade draft between waves
- Encounter director
- Elite enemies
- Boss or final event
- Biome-based spawn variations

### Milestone C: RTS Identity

- Support units or summonable squads
- Selection and command for allied units
- Tactical objective control
- Role differentiation across builds

### Milestone D: Mode Expansion

- Co-op survival
- Extraction run
- Team PvPvE
- Arena PvP

## Immediate Execution Questions

Use these to keep the project focused:

1. What is the 10-minute session that should already be fun without long-term progression?
2. What battlefield decision should players make every 5-10 seconds?
3. What makes this more than "move hero and cast spells"?
4. What part of the game becomes more fun specifically because friends are present?
5. What procedural element changes strategy, not just visuals?

## Immediate Recommendation

The next real deliverable should be:

**A co-op survival prototype with enemy waves, procedural encounter layout, damage/death, and upgrade choices.**

That is the shortest path from "good engine foundation" to "actually fun game."
