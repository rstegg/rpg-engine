use rpg_engine::world::environment::{load_glb_template_sync, instantiate_gate};
use macroquad::prelude::*;

fn main() {
    let t = load_glb_template_sync("assets/world_models/fence_gate.glb").unwrap();
    let closed = instantiate_gate(&t, vec3(0.0,0.0,0.0), 0.0, 2.0, 0.0);
    let open = instantiate_gate(&t, vec3(0.0,0.0,0.0), 0.0, 2.0, 1.0);

    let mut min_closed = vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max_closed = vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for m in &closed { for v in &m.vertices { min_closed = min_closed.min(v.position); max_closed = max_closed.max(v.position); } }

    let mut min_open = vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max_open = vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for m in &open { for v in &m.vertices { min_open = min_open.min(v.position); max_open = max_open.max(v.position); } }

    println!("closed bbox {:?} {:?}", min_closed, max_closed);
    println!("open bbox   {:?} {:?}", min_open, max_open);

    let closed_sample = closed.iter().flat_map(|m| m.vertices.iter()).take(8).map(|v| v.position).collect::<Vec<_>>();
    let open_sample = open.iter().flat_map(|m| m.vertices.iter()).take(8).map(|v| v.position).collect::<Vec<_>>();
    println!("closed sample {:?}", closed_sample);
    println!("open sample   {:?}", open_sample);
}
