# Sprite Sheet Description

This document defines the layout and animations for the 2.5D RPG character sprite sheet. The sprite sheet is organized into **8 rows** and **29 columns**.

## Rows (Directional Auto-Facing)

The character can face 8 different directions. The rows map to these directions starting from looking "down" (South) and moving clockwise.

| Row Index (0-based) | Direction | Vector (X, Z plane) |
| :--- | :--- | :--- |
| 0 | South (Down) | Looking straight at the camera |
| 1 | South-East (Down-Right) | |
| 2 | East (Right) | |
| 3 | North-East (Up-Right) | |
| 4 | North (Up) | Looking away from the camera |
| 5 | North-West (Up-Left) | |
| 6 | West (Left) | |
| 7 | South-West (Down-Left) | |

## Columns (Animations)

The animations are split across columns 1 through 29 (1-indexed for description, these map to 0-28 in code).

| Columns (1-Indexed) | Code Indices (0-Indexed) | Animation State | Notes |
| :--- | :--- | :--- | :--- |
| 1 - 2 | 0 - 1 | **Idle** | Default resting state |
| 3 - 5 | 2 - 4 | **Walk** | Loops continuously while moving |
| 6 - 9 | 5 - 8 | **Sword Attack** | Melee swing animation |
| 10 - 13 | 9 - 12 | **Bow Attack** | Ranged bow animation |
| 14 - 16 | 13 - 15 | **Staff Magic** | Casting spell animation |
| 17 - 19 | 16 - 18 | **Punch / Throw** | Unarmed attack or throwing item |
| 20 - 22 | 19 - 21 | **Hurt** | Taking damage |
| 23 - 25 | 22 - 24 | **Death** | Character dies (halts on last frame) |
| 26 - 28 | 25 - 27 | **Carry** | Hands up holding item |
| - *26* | - *25* | *- Carry Idle* | *Standing still while holding an item* |
| - *27, 26, 28, 26, 27* | - *26, 25, 27, 25, 26* | *- Carry Walk* | *Sequence plays out of order to simulate walk cycle* |
| 29 | 28 | **Jump** | Mid-air single frame |

## Technical Implementation Details

- **Size Calculation**: The code calculates the width and height of each individual frame dynamically by dividing the entire texture's width by `29` and the height by `8`.
- **Rendering Method**: The character is drawn as a "billboard", meaning it is a flat 3D plane that constantly rotates on the Y-axis to perfectly face the top-down camera.
- **UV Mapping**: The `AnimationManager` returns a `Rect` indicating the exact slice of the texture to sample. The top-left corner of the `Rect` maps to the intersection of the current Column and Row.
