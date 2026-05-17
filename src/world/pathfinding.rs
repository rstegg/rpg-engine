use macroquad::prelude::*;
use pathfinding::prelude::astar;

pub trait WalkabilityMap {
    fn is_walkable(&self, pos: Vec3) -> bool;
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridPos {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathfindDiagnostics {
    pub start_grid: GridPos,
    pub goal_grid: GridPos,
    pub start_walkable: bool,
    pub goal_walkable: bool,
    pub expanded_nodes: usize,
    pub min_search_grid: GridPos,
    pub max_search_grid: GridPos,
}

#[derive(Clone, Debug)]
pub struct PathfindResult {
    pub path: Option<Vec<Vec3>>,
    pub diagnostics: PathfindDiagnostics,
}

pub fn line_of_sight(
    start: Vec3,
    goal: Vec3,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> bool {
    let dist = (goal - start).length();
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

pub fn line_of_sight_fn<F>(start: Vec3, goal: Vec3, mut is_walkable_fn: F) -> bool
where
    F: FnMut(Vec3) -> bool,
{
    let dist = (goal - start).length();
    let steps = ((dist / 0.2).ceil() as usize).max(2);
    const PLAYER_RADIUS: f32 = 0.35;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let p = start.lerp(goal, t);
        if !is_walkable_with_radius_fn(p, PLAYER_RADIUS, |p| is_walkable_fn(p)) {
            return false;
        }
    }
    true
}

pub fn find_closest_walkable_fn<F>(
    target: Vec3,
    max_radius_cells: i32,
    grid_size: f32,
    mut is_walkable_fn: F,
) -> Vec3
where
    F: FnMut(Vec3) -> bool,
{
    let target_gx = (target.x / grid_size).floor() as i32;
    let target_gz = (target.z / grid_size).floor() as i32;
    let center = vec3(
        target_gx as f32 * grid_size + grid_size * 0.5,
        target.y,
        target_gz as f32 * grid_size + grid_size * 0.5,
    );

    if is_walkable_fn(target) && is_walkable_fn(center) {
        return target;
    }

    if is_walkable_fn(center) {
        return center;
    }

    let mut closest = target;
    let mut min_dist_sq = f32::MAX;

    for r in 1..=max_radius_cells {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                
                let check_pos = vec3(
                    center.x + dx as f32 * grid_size,
                    center.y,
                    center.z + dz as f32 * grid_size,
                );
                
                if is_walkable_fn(check_pos) {
                    let dist_sq = (check_pos.x - target.x).powi(2) + (check_pos.z - target.z).powi(2);
                    if dist_sq < min_dist_sq {
                        min_dist_sq = dist_sq;
                        closest = check_pos;
                    }
                }
            }
        }
        
        if min_dist_sq < f32::MAX {
            return closest;
        }
    }

    target
}

fn sample_walkable(
    p: Vec3,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> bool {
    let gx = ((p.x / grid_size).round() + (width / 2) as f32) as i32;
    let gz = ((p.z / grid_size).round() + (height / 2) as f32) as i32;
    if gx < 0 || gx >= width || gz < 0 || gz >= height {
        return true;
    }
    walkability[gx as usize][gz as usize]
}

pub fn is_walkable_with_radius(
    pos: Vec3,
    radius: f32,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> bool {
    let offsets: &[(f32, f32)] = &[
        (0.0, 0.0), (radius, 0.0), (-radius, 0.0), (0.0, radius), (0.0, -radius),
        (radius * 0.707, radius * 0.707), (-radius * 0.707, radius * 0.707),
        (radius * 0.707, -radius * 0.707), (-radius * 0.707, -radius * 0.707),
    ];
    for &(dx, dz) in offsets {
        let p = pos + vec3(dx, 0.0, dz);
        if !sample_walkable(p, grid_size, width, height, walkability) {
            return false;
        }
    }
    true
}

pub fn is_walkable_with_radius_fn<F>(pos: Vec3, radius: f32, mut is_walkable_fn: F) -> bool
where
    F: FnMut(Vec3) -> bool,
{
    let offsets: &[(f32, f32)] = &[
        (0.0, 0.0), (radius, 0.0), (-radius, 0.0), (0.0, radius), (0.0, -radius),
        (radius * 0.707, radius * 0.707), (-radius * 0.707, radius * 0.707),
        (radius * 0.707, -radius * 0.707), (-radius * 0.707, -radius * 0.707),
    ];
    for &(dx, dz) in offsets {
        if !is_walkable_fn(pos + vec3(dx, 0.0, dz)) {
            return false;
        }
    }
    true
}

pub fn slide_move(
    current: Vec3,
    desired: Vec3,
    radius: f32,
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> Vec3 {
    if is_walkable_with_radius(desired, radius, grid_size, width, height, walkability) {
        return desired;
    }
    let x_only = vec3(desired.x, current.y, current.z);
    if is_walkable_with_radius(x_only, radius, grid_size, width, height, walkability) {
        return x_only;
    }
    let z_only = vec3(current.x, current.y, desired.z);
    if is_walkable_with_radius(z_only, radius, grid_size, width, height, walkability) {
        return z_only;
    }
    current
}

pub fn slide_move_world<F>(current: Vec3, desired: Vec3, radius: f32, mut is_walkable_fn: F) -> Vec3
where
    F: FnMut(Vec3) -> bool,
{
    if is_walkable_with_radius_fn(desired, radius, |p| is_walkable_fn(p)) {
        return desired;
    }
    let x_only = vec3(desired.x, current.y, current.z);
    if is_walkable_with_radius_fn(x_only, radius, |p| is_walkable_fn(p)) {
        return x_only;
    }
    let z_only = vec3(current.x, current.y, desired.z);
    if is_walkable_with_radius_fn(z_only, radius, |p| is_walkable_fn(p)) {
        return z_only;
    }
    current
}

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
    let mut start_grid = to_grid(start).clamp(width, height);
    let mut goal_grid = to_grid(goal).clamp(width, height);

    let result = astar(
        &start_grid,
        |p| {
            let mut neighbors: Vec<(GridPos, i32)> = Vec::new();
            for dx in -1..=1i32 {
                for dz in -1..=1i32 {
                    if dx == 0 && dz == 0 { continue; }
                    let nx = p.x + dx;
                    let nz = p.z + dz;
                    if nx < 0 || nx >= width || nz < 0 || nz >= height { continue; }
                    if !walkability[nx as usize][nz as usize] { continue; }
                    let is_diagonal = dx != 0 && dz != 0;
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
        if let Some(last) = waypoints.last_mut() { *last = goal; }
        smooth_path(&waypoints, grid_size, width, height, walkability)
    })
}

pub fn find_path_detailed_fn<F>(
    start: Vec3,
    goal: Vec3,
    grid_size: f32,
    mut is_walkable_fn: F,
) -> PathfindResult
where
    F: FnMut(Vec3) -> bool,
{
    let to_grid = |v: Vec3| GridPos {
        x: (v.x / grid_size).floor() as i32,
        z: (v.z / grid_size).floor() as i32,
    };
    let from_grid = |g: &GridPos| {
        vec3(
            g.x as f32 * grid_size + grid_size * 0.5,
            0.0,
            g.z as f32 * grid_size + grid_size * 0.5,
        )
    };
    let start_grid = to_grid(start);
    let goal_grid = to_grid(goal);
    let start_walkable = is_walkable_fn(start);
    let goal_walkable = is_walkable_fn(goal);
    let dx = (goal_grid.x - start_grid.x).abs();
    let dz = (goal_grid.z - start_grid.z).abs();
    let search_margin = ((dx.max(dz) / 2).max(12)) + 8;
    let min_search_grid = GridPos {
        x: start_grid.x.min(goal_grid.x) - search_margin,
        z: start_grid.z.min(goal_grid.z) - search_margin,
    };
    let max_search_grid = GridPos {
        x: start_grid.x.max(goal_grid.x) + search_margin,
        z: start_grid.z.max(goal_grid.z) + search_margin,
    };
    let mut expanded_nodes = 0usize;

    if !start_walkable {
        return PathfindResult {
            path: None,
            diagnostics: PathfindDiagnostics {
                start_grid,
                goal_grid,
                start_walkable,
                goal_walkable,
                expanded_nodes,
                min_search_grid,
                max_search_grid,
            },
        };
    }

    let mut closest_node = start_grid.clone();
    let goal_x = goal_grid.x;
    let goal_z = goal_grid.z;
    let mut min_h = (start_grid.x - goal_x).abs() + (start_grid.z - goal_z).abs();

    let mut current_target_grid = goal_grid.clone();
    let mut fallback_triggered = false;
    let mut result = None;

    for pass in 0..2 {
        result = astar(
            &start_grid,
            |p| {
                expanded_nodes += 1;
                let mut neighbors: Vec<(GridPos, i32)> = Vec::new();
                for dx in -1..=1i32 {
                    for dz in -1..=1i32 {
                        if dx == 0 && dz == 0 { continue; }
                        let nx = p.x + dx;
                        let nz = p.z + dz;
                        if nx < min_search_grid.x
                            || nx > max_search_grid.x
                            || nz < min_search_grid.z
                            || nz > max_search_grid.z
                        {
                            continue;
                        }

                        if dx != 0 && dz != 0 {
                            let world_x = from_grid(&GridPos { x: p.x + dx, z: p.z });
                            let world_z = from_grid(&GridPos { x: p.x, z: p.z + dz });
                            if !is_walkable_fn(world_x) || !is_walkable_fn(world_z) {
                                continue;
                            }
                        }

                        let world_pos = from_grid(&GridPos { x: nx, z: nz });
                        if !is_walkable_fn(world_pos) { continue; }

                        if pass == 0 {
                            let h = (nx - goal_x).abs() + (nz - goal_z).abs();
                            if h < min_h {
                                min_h = h;
                                closest_node = GridPos { x: nx, z: nz };
                            }
                        }

                        let cost = if dx != 0 && dz != 0 { 14 } else { 10 };
                        neighbors.push((GridPos { x: nx, z: nz }, cost));
                    }
                }
                neighbors
            },
            |p| ((p.x - current_target_grid.x).abs() + (p.z - current_target_grid.z).abs()) * 10,
            |p| *p == current_target_grid,
        );

        if result.is_some() || pass == 1 {
            break;
        }

        if closest_node == start_grid {
            break;
        }

        current_target_grid = closest_node.clone();
        fallback_triggered = true;
    }

    let path = result.map(|(path, _)| {
        let mut waypoints: Vec<Vec3> = path.iter().map(from_grid).collect();
        if !fallback_triggered && goal_walkable {
            if let Some(last) = waypoints.last_mut() { *last = goal; }
        }
        waypoints
    });

    PathfindResult {
        path,
        diagnostics: PathfindDiagnostics {
            start_grid,
            goal_grid,
            start_walkable,
            goal_walkable,
            expanded_nodes,
            min_search_grid,
            max_search_grid,
        },
    }
}

pub fn find_path_fn<F>(
    start: Vec3,
    goal: Vec3,
    grid_size: f32,
    is_walkable_fn: F,
) -> Option<Vec<Vec3>>
where
    F: FnMut(Vec3) -> bool,
{
    find_path_detailed_fn(start, goal, grid_size, is_walkable_fn).path
}

impl GridPos {
    fn clamp(mut self, width: i32, height: i32) -> Self {
        self.x = self.x.clamp(0, width - 1);
        self.z = self.z.clamp(0, height - 1);
        self
    }
}

fn smooth_path(
    path: &[Vec3],
    grid_size: f32,
    width: i32,
    height: i32,
    walkability: &[Vec<bool>],
) -> Vec<Vec3> {
    if path.len() <= 2 { return path.to_vec(); }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn open_world(_: Vec3) -> bool {
        true
    }

    #[test]
    fn detailed_pathfinding_reports_blocked_goal() {
        let result = find_path_detailed_fn(
            vec3(0.5, 0.0, 0.5),
            vec3(2.5, 0.0, 0.5),
            1.0,
            |p| !(p.x >= 2.0 && p.x < 3.0 && p.z >= 0.0 && p.z < 1.0),
        );

        assert!(result.path.is_none());
        assert!(result.diagnostics.start_walkable);
        assert!(!result.diagnostics.goal_walkable);
        assert_eq!(result.diagnostics.start_grid, GridPos { x: 0, z: 0 });
        assert_eq!(result.diagnostics.goal_grid, GridPos { x: 2, z: 0 });
        assert_eq!(result.diagnostics.expanded_nodes, 0);
    }

    #[test]
    fn pathfinding_supports_long_paths_beyond_old_search_cap() {
        let path = find_path_fn(
            vec3(0.5, 0.0, 0.5),
            vec3(60.5, 0.0, 0.5),
            1.0,
            open_world,
        )
        .expect("expected an open-world path");

        assert!(path.len() > 50, "expected many waypoints for a long path");
        let last = *path.last().expect("path should contain a goal waypoint");
        assert!((last.x - 60.5).abs() < 0.01);
        assert!((last.z - 0.5).abs() < 0.01);
    }

    #[test]
    fn pathfinding_returns_goal_in_open_space() {
        let path = find_path_fn(
            vec3(0.5, 0.0, 0.5),
            vec3(5.5, 0.0, 5.5),
            1.0,
            open_world,
        )
        .expect("expected a path through open space");

        assert!(path.len() >= 2);
        let last = *path.last().expect("path should contain a goal waypoint");
        assert!((last.x - 5.5).abs() < 0.01);
        assert!((last.z - 5.5).abs() < 0.01);
    }

    #[test]
    fn pathfinding_rejects_diagonal_corner_cutting() {
        let path = find_path_fn(
            vec3(0.5, 0.0, 0.5),
            vec3(1.5, 0.0, 1.5),
            1.0,
            |p| {
                let blocked_x = p.x >= 1.0 && p.x < 2.0 && p.z >= 0.0 && p.z < 1.0;
                let blocked_z = p.x >= 0.0 && p.x < 1.0 && p.z >= 1.0 && p.z < 2.0;
                !(blocked_x || blocked_z)
            },
        );

        assert!(path.is_none(), "diagonal corner cut should be rejected");
    }
}
