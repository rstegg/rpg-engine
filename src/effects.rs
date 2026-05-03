use macroquad::prelude::*;
use crate::player::SpellId;

pub struct Particle {
    pub pos: Vec3,
    pub velocity: Vec3,
    pub texture: Texture2D,
    pub timer: f32,
    pub rotation: f32, // For 45 degree tilt
    
    pub columns: u32,
    pub current_frame: f32,
    pub fps: f32,
    pub looping: bool,
}

pub struct EffectManager {
    pub particles: Vec<Particle>,
}

impl EffectManager {
    pub fn new() -> Self {
        Self { particles: Vec::new() }
    }

    pub fn spawn_arrow_rain(&mut self, target_pos: Vec3, texture: Texture2D) {
        // Spawn 6-8 arrows high up, falling diagonally
        let count = macroquad::rand::gen_range(6, 9);
        for _ in 0..count {
            let height = macroquad::rand::gen_range(5.0, 10.0);
            let fall_time = height / 15.0; // Time it takes to hit the ground at Y-velocity 15
            let spawn_x = -15.0 * fall_time; // Offset X so it lands near the target
            
            let offset_x = macroquad::rand::gen_range(-1.5, 1.5);
            let offset_z = macroquad::rand::gen_range(-1.5, 1.5);
            
            self.particles.push(Particle {
                pos: target_pos + vec3(spawn_x + offset_x, height, offset_z),
                velocity: vec3(15.0, -15.0, 0.0), // falling diagonally
                texture: texture.clone(),
                timer: 1.0, // Lives for up to 1 second
                rotation: std::f32::consts::PI / 4.0 + std::f32::consts::PI, // Flipped 180 degrees
                columns: 6,
                current_frame: 0.0,
                fps: 15.0, // Arrow loop speed
                looping: true,
            });
        }
    }

    pub fn spawn_single_hit(&mut self, target_pos: Vec3, texture: Texture2D, spell: SpellId) {
        // Retrieve spritesheet configuration based on the spell
        let (columns, fps) = match spell {
            SpellId::W => (5, 8.0),   // Updated to 5 columns per user's spritesheet
            SpellId::E => (10, 15.0), // Fire claw
            SpellId::R => (18, 20.0), // Dark void
            _ => (1, 1.0),
        };

        // Spawn a static hit effect at the target position
        self.particles.push(Particle {
            pos: target_pos + vec3(0.0, 1.5, 0.0), // Raised slightly to 1.5 to guarantee it doesn't clip into the floor
            velocity: vec3(0.0, 0.0, 0.0),
            texture: texture.clone(),
            timer: 99.0, // Lifespan is controlled by the animation length
            rotation: 0.0,
            columns,
            current_frame: 0.0,
            fps,
            looping: false,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.pos += p.velocity * dt;
            p.timer -= dt;
            p.current_frame += p.fps * dt;
        }
        self.particles.retain(|p| {
            if p.looping {
                p.timer > 0.0 && p.pos.y > -0.5
            } else {
                p.current_frame < p.columns as f32
            }
        });
    }

    pub fn draw(&self, camera_pos: Vec3) {
        for p in &self.particles {
            // Calculate aspect ratio so the frame isn't squashed or cut off
            let tex_w = p.texture.width();
            let tex_h = p.texture.height();
            let frame_w_pixels = tex_w / p.columns as f32;
            let aspect_ratio = if tex_h > 0.0 { frame_w_pixels / tex_h } else { 1.0 };
            
            let base_height = 3.0; // Slightly larger for better visibility
            let size = vec2(base_height * aspect_ratio, base_height);
            let half_w = size.x / 2.0;
            let half_h = size.y / 2.0;

            // Billboard rotation towards camera
            let billboard_rot = f32::atan2(camera_pos.x - p.pos.x, camera_pos.z - p.pos.z);
            let rot_y = macroquad::math::Mat4::from_rotation_y(billboard_rot);
            // Apply Z/X rotation if needed (like 45 deg tilt for arrows)
            let rot_z = macroquad::math::Mat4::from_rotation_z(p.rotation);
            let rot = rot_y * rot_z;

            let center = p.pos;
            let p1 = center + rot.transform_point3(vec3(-half_w, -half_h, 0.0));
            let p2 = center + rot.transform_point3(vec3( half_w, -half_h, 0.0));
            let p3 = center + rot.transform_point3(vec3( half_w,  half_h, 0.0));
            let p4 = center + rot.transform_point3(vec3(-half_w,  half_h, 0.0));

            // UV Mapping for spritesheet
            let frame_idx = if p.looping {
                (p.current_frame as u32) % p.columns
            } else {
                (p.current_frame as u32).min(p.columns - 1)
            };

            let u_width = 1.0 / p.columns as f32;
            let u_min = frame_idx as f32 * u_width;
            let u_max = u_min + u_width;
            let v_min = 0.0;
            let v_max = 1.0;

            let vertices = vec![
                macroquad::models::Vertex::new2(p1, vec2(u_min, v_max), WHITE),
                macroquad::models::Vertex::new2(p2, vec2(u_max, v_max), WHITE),
                macroquad::models::Vertex::new2(p3, vec2(u_max, v_min), WHITE),
                macroquad::models::Vertex::new2(p4, vec2(u_min, v_min), WHITE),
            ];
            let indices = vec![0, 1, 2, 0, 2, 3];

            let mesh = macroquad::models::Mesh {
                vertices,
                indices,
                texture: Some(p.texture.clone()),
            };
            
            draw_mesh(&mesh);

            // DEBUG: Draw a faint wireframe box around the billboard so the user can see exactly what the engine considers the "full frame"
            if !p.looping {
                draw_line_3d(p1, p2, Color::new(1.0, 0.0, 0.0, 0.5));
                draw_line_3d(p2, p3, Color::new(1.0, 0.0, 0.0, 0.5));
                draw_line_3d(p3, p4, Color::new(1.0, 0.0, 0.0, 0.5));
                draw_line_3d(p4, p1, Color::new(1.0, 0.0, 0.0, 0.5));
            }
        }
    }
}
