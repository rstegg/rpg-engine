use macroquad::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Direction {
    South = 0,
    SouthEast = 1,
    East = 2,
    NorthEast = 3,
    North = 4,
    NorthWest = 5,
    West = 6,
    SouthWest = 7,
}

impl Direction {
    pub fn from_vector(dir: Vec3) -> Self {
        // We use atan2(x, z) because in top-down 3D:
        // +Z is usually South, +X is East.
        // atan2(0, 1) = 0        => South
        // atan2(1, 1) = PI/4     => SouthEast
        // atan2(1, 0) = PI/2     => East
        // atan2(1, -1) = 3PI/4   => NorthEast
        // atan2(0, -1) = PI/-PI  => North
        // atan2(-1, -1) = -3PI/4 => NorthWest
        // atan2(-1, 0) = -PI/2   => West
        // atan2(-1, 1) = -PI/4   => SouthWest
        
        let mut angle = f32::atan2(dir.x, dir.z);
        if angle < 0.0 {
            angle += std::f32::consts::PI * 2.0;
        }

        let octant = (angle / (std::f32::consts::PI / 4.0)).round() as i32 % 8;
        
        match octant {
            0 => Direction::South,
            1 => Direction::SouthEast,
            2 => Direction::East,
            3 => Direction::NorthEast,
            4 => Direction::North,
            5 => Direction::NorthWest,
            6 => Direction::West,
            7 => Direction::SouthWest,
            _ => Direction::South,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnimationState {
    Idle,
    Walk,
    Sword,
    Bow,
    Staff,
    Punch,
    Hurt,
    Death,
    CarryIdle,
    CarryWalk,
    Jump,
}

pub struct SpriteSheetConfig {
    pub columns: u32,
    pub rows: u32,
}

pub struct AnimationManager {
    pub state: AnimationState,
    pub direction: Direction,
    pub current_frame: f32,
    pub config: SpriteSheetConfig,
}

impl AnimationManager {
    pub fn new(config: SpriteSheetConfig) -> Self {
        Self {
            state: AnimationState::Idle,
            direction: Direction::South,
            current_frame: 0.0,
            config,
        }
    }

    pub fn set_state(&mut self, new_state: AnimationState) {
        if self.state != new_state {
            self.state = new_state;
            self.current_frame = 0.0;
        }
    }

    pub fn set_direction(&mut self, dir: Vec3) {
        if dir.length() > 0.001 {
            self.direction = Direction::from_vector(dir.normalize());
        }
    }

    pub fn get_frame_indices(&self) -> Vec<u32> {
        match self.state {
            // Mapping 1-indexed cols to 0-indexed code
            AnimationState::Idle => vec![0, 1],
            AnimationState::Walk => vec![2, 3, 4],
            AnimationState::Sword => vec![5, 6, 7, 8],
            AnimationState::Bow => vec![9, 10, 11, 12],
            AnimationState::Staff => vec![13, 14, 15],
            AnimationState::Punch => vec![16, 17, 18],
            AnimationState::Hurt => vec![19, 20, 21],
            AnimationState::Death => vec![22, 23, 24],
            AnimationState::CarryIdle => vec![25],
            // Walk sequence [27, 26, 28, 26, 27] mapped to 0-index
            AnimationState::CarryWalk => vec![26, 25, 27, 25, 26],
            AnimationState::Jump => vec![28],
        }
    }

    pub fn update(&mut self, dt: f32, move_speed: f32) {
        let frames = self.get_frame_indices();
        let frame_count = frames.len() as f32;
        
        // Different animations have different base speeds
        let anim_fps = match self.state {
            AnimationState::Idle | AnimationState::CarryIdle => 2.0, // Slower idle
            AnimationState::Walk | AnimationState::CarryWalk => 4.0 + (move_speed * 1.5), // Scales with movement speed
            _ => 8.0, // Fixed faster speed for attacks/actions
        };
        
        self.current_frame += dt * anim_fps;
        
        if self.current_frame >= frame_count {
            match self.state {
                AnimationState::Death
                | AnimationState::Sword
                | AnimationState::Bow
                | AnimationState::Staff
                | AnimationState::Punch
                | AnimationState::Hurt => {
                    // Stop on the last frame
                    self.current_frame = frame_count - 0.01;
                }
                _ => {
                    // Loop other animations (Walk, Idle, Carry, etc.)
                    self.current_frame %= frame_count;
                }
            }
        }
    }

    pub fn get_source_rect(&self, tex_w: f32, tex_h: f32) -> Rect {
        let frames = self.get_frame_indices();
        let frame_idx = (self.current_frame as usize).min(frames.len() - 1);
        let col = frames[frame_idx];
        let row = self.direction as u32;

        let frame_w = tex_w / self.config.columns as f32;
        let frame_h = tex_h / self.config.rows as f32;

        Rect::new(
            col as f32 * frame_w,
            row as f32 * frame_h,
            frame_w,
            frame_h,
        )
    }
}


// 5. TECHNICAL RENDERING HELPER
// Renders the character as a billboard that faces the camera.
// Renders the character as a billboard that faces the camera.
pub fn draw_character_billboard(
    pos: Vec3,
    texture: &Texture2D,
    source_rect: Rect,
    camera_pos: Vec3,
) {
    draw_character_billboard_ex(pos, texture, source_rect, camera_pos, 2.0);
}

pub fn draw_character_billboard_ex(
    pos: Vec3,
    texture: &Texture2D,
    source_rect: Rect,
    camera_pos: Vec3,
    base_size: f32,
) {
    let size = vec2(base_size, base_size); 
    let half_w = size.x / 2.0;
    let half_h = size.y / 2.0;

    let billboard_rot = f32::atan2(camera_pos.x - pos.x, camera_pos.z - pos.z);
    let rot = macroquad::math::Mat4::from_rotation_y(billboard_rot);

    let center = pos + vec3(0.0, half_h, 0.0);

    let p1 = center + rot.transform_point3(vec3(-half_w, -half_h, 0.0));
    let p2 = center + rot.transform_point3(vec3( half_w, -half_h, 0.0));
    let p3 = center + rot.transform_point3(vec3( half_w,  half_h, 0.0));
    let p4 = center + rot.transform_point3(vec3(-half_w,  half_h, 0.0));

    let tex_w = texture.width();
    let tex_h = texture.height();

    let u_min = source_rect.x / tex_w;
    let v_min = source_rect.y / tex_h;
    let u_max = (source_rect.x + source_rect.w) / tex_w;
    let v_max = (source_rect.y + source_rect.h) / tex_h;

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
        texture: Some(texture.clone()),
    };
    
    draw_mesh(&mesh);
}
