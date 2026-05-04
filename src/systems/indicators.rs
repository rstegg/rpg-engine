use macroquad::prelude::*;

const MOVE_MARKER_LIFETIME: f32 = 1.15;
const MOVE_ORANGE: Color = Color::new(0.95, 0.28, 0.02, 1.0);
const MOVE_ORANGE_GLOW: Color = Color::new(1.0, 0.45, 0.05, 0.55);
const TARGET_ORANGE: Color = Color::new(0.95, 0.25, 0.0, 1.0);
const TARGET_SHADOW: Color = Color::new(0.18, 0.06, 0.0, 1.0);

struct MoveMarker {
    pos: Vec3,
    age: f32,
}

pub struct IndicatorManager {
    move_markers: Vec<MoveMarker>,
}

impl IndicatorManager {
    pub fn new() -> Self {
        Self {
            move_markers: Vec::new(),
        }
    }

    pub fn spawn_move_marker(&mut self, pos: Vec3) {
        self.move_markers.push(MoveMarker { pos, age: 0.0 });
    }

    pub fn update(&mut self, dt: f32) {
        for marker in &mut self.move_markers {
            marker.age += dt;
        }
        self.move_markers
            .retain(|marker| marker.age < MOVE_MARKER_LIFETIME);
    }

    pub fn draw(&self) {
        for marker in &self.move_markers {
            draw_move_marker(marker);
        }
    }
}

pub fn draw_aoe_target(center: Vec3, radius: f32, time: f32) {
    let rotation = time * 0.65;
    let y = 0.075;

    draw_fat_ring(center, radius, y, 96, 0.045, TARGET_SHADOW);
    draw_fat_ring(center, radius, y + 0.015, 96, 0.03, TARGET_ORANGE);
    draw_fat_ring(center, radius * 0.62, y + 0.02, 72, 0.02, MOVE_ORANGE_GLOW);

    for i in 0..4 {
        let angle = rotation + i as f32 * std::f32::consts::FRAC_PI_2;
        let dir = vec3(angle.cos(), 0.0, angle.sin());
        let side = vec3(-dir.z, 0.0, dir.x);
        let outer = center + dir * radius;
        let inner = center + dir * (radius * 0.72);
        draw_line_3d(
            inner + vec3(0.0, y + 0.035, 0.0),
            outer + vec3(0.0, y + 0.035, 0.0),
            TARGET_ORANGE,
        );
        draw_line_3d(
            outer - dir * 0.26 + side * 0.18 + vec3(0.0, y + 0.035, 0.0),
            outer + vec3(0.0, y + 0.035, 0.0),
            TARGET_ORANGE,
        );
        draw_line_3d(
            outer - dir * 0.26 - side * 0.18 + vec3(0.0, y + 0.035, 0.0),
            outer + vec3(0.0, y + 0.035, 0.0),
            TARGET_ORANGE,
        );
    }

    for i in 0..4 {
        let angle = rotation * -1.35 + i as f32 * std::f32::consts::FRAC_PI_2;
        let p1 = center + vec3(angle.cos(), 0.0, angle.sin()) * radius * 0.18;
        let p2 = center + vec3(angle.cos(), 0.0, angle.sin()) * radius * 0.46;
        draw_line_3d(
            p1 + vec3(0.0, y + 0.05, 0.0),
            p2 + vec3(0.0, y + 0.05, 0.0),
            MOVE_ORANGE_GLOW,
        );
    }
}

fn draw_move_marker(marker: &MoveMarker) {
    let t = marker.age / MOVE_MARKER_LIFETIME;
    let alpha = (1.0 - t).clamp(0.0, 1.0);
    let pulse = (marker.age * 11.0).sin() * 0.5 + 0.5;
    let drop = 1.25 - t * 0.85 + pulse * 0.12;
    let y = 0.08;
    let base_color = Color::new(MOVE_ORANGE.r, MOVE_ORANGE.g, MOVE_ORANGE.b, alpha);
    let glow_color = Color::new(
        MOVE_ORANGE_GLOW.r,
        MOVE_ORANGE_GLOW.g,
        MOVE_ORANGE_GLOW.b,
        alpha * 0.7,
    );

    draw_fat_ring(marker.pos, 0.38 + t * 0.28, y, 48, 0.025, glow_color);
    draw_fat_ring(
        marker.pos,
        0.18 + t * 0.12,
        y + 0.015,
        40,
        0.018,
        base_color,
    );

    let tip = marker.pos + vec3(0.0, y + 0.08, 0.0);
    let tail = marker.pos + vec3(0.0, drop, 0.0);
    draw_line_3d(tail, tip, base_color);
    draw_line_3d(tail + vec3(0.03, 0.0, 0.03), tip, base_color);
    draw_line_3d(tail + vec3(-0.03, 0.0, -0.03), tip, base_color);

    let head_y = y + 0.36;
    let head = marker.pos + vec3(0.0, head_y, 0.0);
    for angle in [
        0.0,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        std::f32::consts::PI * 1.5,
    ] {
        let outward = vec3(angle.cos(), 0.0, angle.sin());
        draw_line_3d(
            head + outward * 0.34 + vec3(0.0, 0.28, 0.0),
            tip,
            base_color,
        );
    }
}

fn draw_fat_ring(center: Vec3, radius: f32, y: f32, segments: usize, thickness: f32, color: Color) {
    for offset in [-thickness, 0.0, thickness] {
        let r = (radius + offset).max(0.02);
        for i in 0..segments {
            let a1 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let a2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
            let p1 = center + vec3(a1.cos() * r, y, a1.sin() * r);
            let p2 = center + vec3(a2.cos() * r, y, a2.sin() * r);
            draw_line_3d(p1, p2, color);
        }
    }
}
