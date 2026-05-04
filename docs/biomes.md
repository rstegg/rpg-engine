# RPG Engine Biomes

The world is procedurally generated using a noise-based biome system (planned) and template-based object placement.

## Current Biomes

### 1. The Plains (Default)
The starting area. Characterized by lush green grass, scattered trees, and small clusters of flowers.
- **Flora**: Oak trees, purple/red/yellow flowers, mushrooms.
- **Landmarks**: Occasional campsites with tents and campfires.

### 2. The Great Peaks (Rock/Mountain)
A more rugged area with large rock formations.
- **Unique Feature**: **The Great Peak (Rock_B)**. A prominent rock formation (3x scale) that serves as a primary landmark. Only one spawns per map.
- **Obstacles**: Large boulders that require pathfinding to navigate around.

### 3. The Clearings (Planned)
Open areas with fewer trees, ideal for combat encounters or village placement.

## Decoration Systems

### Object Clustering
Small flora like flowers and mushrooms no longer spawn in isolation. Instead, they form **clusters** of 3-6 individual plants, creating more natural-looking "bushes" or "flower beds."

### Landmark Spawning
Large-scale objects (like the Mountain) are spawned as unique entities outside the main procedural loop to ensure they don't overlap awkwardly with the starting path or other major features.

### Collision & Navigation
All environmental objects contribute to the **Walkability Grid**. 
- **Padded Navigation**: The A* pathfinder uses a "fattened" version of the grid to ensure characters don't scrape against the edges of boulders or trees while moving.
