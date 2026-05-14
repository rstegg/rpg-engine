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

pub fn find_path_fn<F>(
    start: Vec3,
    goal: Vec3,
    grid_size: f32,
    mut is_walkable_fn: F,
) -> Option<Vec<Vec3>>
where
    F: FnMut(Vec3) -> bool,
{
    let to_grid = |v: Vec3| GridPos {
        x: (v.x / grid_size).round() as i32,
        z: (v.z / grid_size).round() as i32,
    };
    let from_grid = |g: &GridPos| {
        vec3(g.x as f32 * grid_size, 0.0, g.z as f32 * grid_size)
    };
    let start_grid = to_grid(start);
    let goal_grid = to_grid(goal);
    let max_range = 100;

    let result = astar(
        &start_grid,
        |p| {
            let mut neighbors: Vec<(GridPos, i32)> = Vec::new();
            if (p.x - start_grid.x).abs() > max_range || (p.z - start_grid.z).abs() > max_range {
                return neighbors;
            }
            for dx in -1..=1i32 {
                for dz in -1..=1i32 {
                    if dx == 0 && dz == 0 { continue; }
                    let nx = p.x + dx;
                    let nz = p.z + dz;
                    let world_pos = from_grid(&GridPos { x: nx, z: nz });
                    if !is_walkable_fn(world_pos) { continue; }
                    let cost = if dx != 0 && dz != 0 { 14 } else { 10 };
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
        waypoints
    })
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
