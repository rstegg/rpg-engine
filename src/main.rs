mod animation;
use animation::*;
use macroquad::prelude::*;

struct Hero {
    pos: Vec3,
    target_pos: Vec3,
    texture: Texture2D,
    stats: Stats,
    anim: AnimationManager,
}

#[macroquad::main("2.5D Indie RPG")]
async fn main() {
    let hero_texture = load_texture("hero.png").await.unwrap_or_else(|_| {
        Texture2D::from_rgba8(8, 8, &[255; 256])
    });
    hero_texture.set_filter(FilterMode::Nearest);

    let config = SpriteSheetConfig {
        columns: 29, // max column index is 28 (0-indexed), so at least 29 columns
        rows: 8,
    };

    let mut hero = Hero {
        pos: vec3(0.0, 0.0, 0.0),
        target_pos: vec3(0.0, 0.0, 0.0),
        texture: hero_texture,
        stats: Stats::new(10, 15, 10),
        anim: AnimationManager::new(config),
    };

    loop {
        clear_background(DARKGRAY);

        let camera_pos = vec3(hero.pos.x, 15.0, hero.pos.z + 12.0);
        let camera = Camera3D {
            position: camera_pos,
            target: hero.pos,
            up: vec3(0.0, 1.0, 0.0),
            ..Default::default()
        };

        if is_mouse_button_pressed(MouseButton::Right) {
            let (mouse_x, mouse_y) = mouse_position();
            let ndc_x = (mouse_x / screen_width()) * 2.0 - 1.0;
            let ndc_y = 1.0 - (mouse_y / screen_height()) * 2.0;

            let inv_vp = camera.matrix().inverse();
            let ray_origin = inv_vp.project_point3(vec3(ndc_x, ndc_y, -1.0));
            let far_pt = inv_vp.project_point3(vec3(ndc_x, ndc_y, 1.0));
            let ray_direction = (far_pt - ray_origin).normalize();
            
            if ray_direction.y != 0.0 {
                let t = -ray_origin.y / ray_direction.y;
                if t > 0.0 {
                    let intersect_point = ray_origin + ray_direction * t;
                    hero.target_pos = vec3(intersect_point.x, 0.0, intersect_point.z);
                }
            }
        }

        let speed = hero.stats.get_movement_speed();
        let delta_time = get_frame_time();
        
        let to_target = hero.target_pos - hero.pos;
        if to_target.length() > 0.1 {
            hero.pos += to_target.normalize() * speed * delta_time;
            hero.anim.set_state(AnimationState::Walk);
            hero.anim.set_direction(to_target);
        } else {
            hero.anim.set_state(AnimationState::Idle);
        }

        hero.anim.update(delta_time, speed);

        set_camera(&camera);
        draw_grid(20, 1.0, BLACK, GRAY);
        draw_cube_wires(hero.target_pos, vec3(0.5, 0.1, 0.5), GREEN);

        let source_rect = hero.anim.get_source_rect(hero.texture.width(), hero.texture.height());
        draw_character_billboard(hero.pos, &hero.texture, source_rect, camera.position);

        set_default_camera();

        draw_text(&format!("Speed (Agility {}): {:.2}", hero.stats.agility, speed), 10.0, 20.0, 20.0, WHITE);
        draw_text("Right Click to Move", 10.0, 40.0, 20.0, GREEN);

        next_frame().await
    }
}