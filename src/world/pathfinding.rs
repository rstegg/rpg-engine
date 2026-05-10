use macroquad::prelude::*;
use pathfinding::prelude::astar;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridPos {
    pub x: i32,
    pub z: i32,
}

/// Check if a straight line (with character width) between two world positions is clear.
pub fn line_of_sight(
    start: Vec3,
    goal: Vec3,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> bool {
    let dist = (goal - start).length();
    // Sample every 0.2m or so to be safe
    let steps = ((dist / 0.2).ceil() as usize).max(2);
    const PLAYER_RADIUS: f32 = 0.35;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let p = start.lerp(goal, t);
        if !is_walkable_with_radius(p, PLAYER_RADIUS, grid_size, width, height, walkability) {
            return false;
        }
    }
    true
}

/// Check a single world point against the walkability grid.
fn sample_walkable(
    p: Vec3,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> bool {
    let gx = ((p.x / grid_size).round() + (width / 2) as f32) as i32;
    let gz = ((p.z / grid_size).round() + (height / 2) as f32) as i32;

    // Out of bounds samples return TRUE because we handle the world boundary
    // via clamping in main.rs. This prevents the character from getting
    // 'stuck' when a radius-sample point falls outside the grid.
    if gx < 0 || gx >= width || gz < 0 || gz >= height {
        return true;
    }

    walkability[gx as usize][gz as usize]
}

/// Check if the player (modelled as a small disc of `radius`) at `pos` overlaps any blocked cell.
pub fn is_walkable_with_radius(
    pos: Vec3,
    radius: f32,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> bool {
    // Sample the center plus 8 points around the perimeter
    let offsets: &[(f32, f32)] = &[
        (0.0, 0.0),
        (radius, 0.0),
        (-radius, 0.0),
        (0.0, radius),
        (0.0, -radius),
        (radius * 0.707, radius * 0.707),
        (-radius * 0.707, radius * 0.707),
        (radius * 0.707, -radius * 0.707),
        (-radius * 0.707, -radius * 0.707),
    ];
    for &(dx, dz) in offsets {
        let p = pos + vec3(dx, 0.0, dz);
        if !sample_walkable(p, grid_size, width, height, walkability) {
            return false;
        }
    }
    true
}

/// Attempt to slide along walls: try X-only and Z-only moves when the full move is blocked.
/// Returns the best achievable position (may be unchanged if fully blocked).
pub fn slide_move(
    current: Vec3,
    desired: Vec3,
    radius: f32,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> Vec3 {
    // Full move
    if is_walkable_with_radius(desired, radius, grid_size, width, height, walkability) {
        return desired;
    }

    // Check if we are ALREADY stuck. If so, try to push out.
    if !is_walkable_with_radius(current, radius, grid_size, width, height, walkability) {
        // Find nearest walkable neighbor to push toward
        let gx = ((current.x / grid_size).round() + (width / 2) as f32) as i32;
        let gz = ((current.z / grid_size).round() + (height / 2) as f32) as i32;
        for r in 1..=2 {
            for dx in -r..=r {
                for dz in -r..=r {
                    let nx = gx + dx;
                    let nz = gz + dz;
                    if nx >= 0
                        && nx < width
                        && nz >= 0
                        && nz < height
                        && walkability[nx as usize][nz as usize]
                    {
                        let target_wx = (nx - width / 2) as f32 * grid_size;
                        let target_wz = (nz - height / 2) as f32 * grid_size;
                        let push_dir =
                            (vec3(target_wx, current.y, target_wz) - current).normalize();
                        let candidate = current + push_dir * 0.1;
                        if is_walkable_with_radius(
                            candidate,
                            radius,
                            grid_size,
                            width,
                            height,
                            walkability,
                        ) {
                            return candidate;
                        }
                    }
                }
            }
        }
    }

    // Slide along X
    let x_only = vec3(desired.x, current.y, current.z);
    if is_walkable_with_radius(x_only, radius, grid_size, width, height, walkability) {
        return x_only;
    }
    // Slide along Z
    let z_only = vec3(current.x, current.y, desired.z);
    if is_walkable_with_radius(z_only, radius, grid_size, width, height, walkability) {
        return z_only;
    }
    // Fully blocked — stay put
    current
}

/// Find a path from start to goal using A*.
/// Diagonals are allowed only when BOTH adjacent cardinal cells are walkable (no corner-cutting).
/// The final waypoint is the EXACT goal position, not a snapped grid center.
pub fn find_path(
    start: Vec3,
    goal: Vec3,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> Option<Vec<Vec3>> {
    let to_grid = |v: Vec3| GridPos {
        x: ((v.x / grid_size).round() + (width / 2) as f32) as i32,
        z: ((v.z / grid_size).round() + (height / 2) as f32) as i32,
    };

    let from_grid = |g: &GridPos| {
        vec3(
            (g.x - width / 2) as f32 * grid_size,
            0.0,
            (g.z - height / 2) as f32 * grid_size,
        )
    };

    let mut start_grid = to_grid(start);
    start_grid.x = start_grid.x.clamp(0, width - 1);
    start_grid.z = start_grid.z.clamp(0, height - 1);
    
    let mut goal_grid = to_grid(goal);
    goal_grid.x = goal_grid.x.clamp(0, width - 1);
    goal_grid.z = goal_grid.z.clamp(0, height - 1);

    // Resilience: If start is blocked, find nearest walkable cell to start from
    if start_grid.x >= 0
        && start_grid.x < width
        && start_grid.z >= 0
        && start_grid.z < height
        && !walkability[start_grid.x as usize][start_grid.z as usize]
    {
        let mut best: Option<GridPos> = None;
        let mut best_dist = i32::MAX;
        for dx in -5..=5i32 {
            for dz in -5..=5i32 {
                let nx = start_grid.x + dx;
                let nz = start_grid.z + dz;
                if nx >= 0
                    && nx < width
                    && nz >= 0
                    && nz < height
                    && walkability[nx as usize][nz as usize]
                {
                    let d = dx * dx + dz * dz;
                    if d < best_dist {
                        best_dist = d;
                        best = Some(GridPos { x: nx, z: nz });
                    }
                }
            }
        }
        if let Some(s) = best {
            start_grid = s;
        }
    }

    // Resilience: If goal is blocked, find the nearest walkable neighbor
    if goal_grid.x < 0
        || goal_grid.x >= width
        || goal_grid.z < 0
        || goal_grid.z >= height
        || !walkability[goal_grid.x as usize][goal_grid.z as usize]
    {
        let mut best: Option<GridPos> = None;
        let mut best_dist = i32::MAX;
        for dx in -10..=10i32 {
            for dz in -10..=10i32 {
                let nx = goal_grid.x + dx;
                let nz = goal_grid.z + dz;
                if nx >= 0
                    && nx < width
                    && nz >= 0
                    && nz < height
                    && walkability[nx as usize][nz as usize]
                {
                    let d = dx * dx + dz * dz;
                    if d < best_dist {
                        best_dist = d;
                        best = Some(GridPos { x: nx, z: nz });
                    }
                }
            }
        }
        goal_grid = best?;
    }

    let result = astar(
        &start_grid,
        |p| {
            let mut neighbors: Vec<(GridPos, i32)> = Vec::new();
            for dx in -1..=1i32 {
                for dz in -1..=1i32 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let nx = p.x + dx;
                    let nz = p.z + dz;
                    if nx < 0 || nx >= width || nz < 0 || nz >= height {
                        continue;
                    }
                    if !walkability[nx as usize][nz as usize] {
                        continue;
                    }

                    let is_diagonal = dx != 0 && dz != 0;
                    if is_diagonal {
                        // ── No corner-cutting: both cardinal neighbors must be walkable ──
                        let card_x_ok = p.x + dx >= 0
                            && p.x + dx < width
                            && walkability[(p.x + dx) as usize][p.z as usize];
                        let card_z_ok = p.z + dz >= 0
                            && p.z + dz < height
                            && walkability[p.x as usize][(p.z + dz) as usize];
                        if !card_x_ok || !card_z_ok {
                            continue;
                        }
                    }

                    let cost = if is_diagonal { 14 } else { 10 };
                    neighbors.push((GridPos { x: nx, z: nz }, cost));
                }
            }
            neighbors
        },
        |p| ((p.x - goal_grid.x).abs() + (p.z - goal_grid.z).abs()) * 10,
        |p| *p == goal_grid,
    );

    result.map(|(path, _)| {
        let mut waypoints: Vec<Vec3> = path.iter().map(from_grid).collect();
        // Replace the last snapped waypoint with the exact click position
        if let Some(last) = waypoints.last_mut() {
            *last = goal;
        }
        // Path smoothing: remove intermediate waypoints that have line-of-sight to the one after
        smooth_path(&waypoints, grid_size, width, height, walkability)
    })
}

/// Remove redundant intermediate waypoints using a greedy string-pulling algorithm.
/// This prevents the character from taking jagged diagonal grid-hops when a straight
/// line is available (common in open areas between obstacles).
fn smooth_path(
    path: &[Vec3],
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> Vec<Vec3> {
    if path.len() <= 2 {
        return path.to_vec();
    }

    let mut smooth = vec![path[0]];
    let mut anchor = 0;

    for i in 2..path.len() {
        if !line_of_sight(path[anchor], path[i], grid_size, width, height, walkability) {
            smooth.push(path[i - 1]);
            anchor = i - 1;
        }
    }
    smooth.push(*path.last().unwrap());
    smooth
}
