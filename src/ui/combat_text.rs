use macroquad::prelude::*;

pub struct CombatText {
    pub pos: Vec3,
    pub text: String,
    pub color: Color,
    pub timer: f32,
    pub max_time: f32,
    pub velocity: Vec3,
}

pub struct CombatTextManager {
    pub active_texts: Vec<CombatText>,
}

impl CombatTextManager {
    pub fn new() -> Self {
        Self {
            active_texts: Vec::new(),
        }
    }

    pub fn spawn(&mut self, pos: Vec3, amount: i32, is_crit: bool, color: Color) {
        let mut spawn_pos = pos;
        spawn_pos.y += 2.0; // Start above head
        spawn_pos.x += macroquad::rand::gen_range(-0.5, 0.5);
        spawn_pos.z += macroquad::rand::gen_range(-0.5, 0.5);

        let velocity = vec3(0.0, 1.5, 0.0);
        let max_time = if is_crit { 1.5 } else { 1.0 };
        
        let prefix = if is_crit { "!" } else { "" };
        let text = format!("{}{}{}", prefix, amount, prefix);

        self.active_texts.push(CombatText {
            pos: spawn_pos,
            text,
            color,
            timer: max_time,
            max_time,
            velocity,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for t in &mut self.active_texts {
            t.pos += t.velocity * dt;
            t.timer -= dt;
        }
        self.active_texts.retain(|t| t.timer > 0.0);
    }

    pub fn draw(&self, camera: &crate::core::camera::GameCamera) {
        // Floating text needs to be drawn in screen space based on 3D position
        // macroquad's draw_text is 2D
        for t in &self.active_texts {
            // Project 3D point to 2D screen
            let matrix = camera.camera.matrix();
            let screen_pos = matrix.project_point3(t.pos);
            
            // If behind camera, skip
            if screen_pos.z < 0.0 || screen_pos.z > 1.0 {
                continue;
            }

            let x = (screen_pos.x + 1.0) / 2.0 * screen_width();
            let y = (1.0 - screen_pos.y) / 2.0 * screen_height();

            let alpha = (t.timer / t.max_time).clamp(0.0, 1.0);
            let mut c = t.color;
            c.a = alpha;

            let font_size = 24.0;
            
            // Text shadow
            draw_text(&t.text, x + 2.0, y + 2.0, font_size, Color::new(0.0, 0.0, 0.0, alpha));
            // Actual text
            draw_text(&t.text, x, y, font_size, c);
        }
    }
}
