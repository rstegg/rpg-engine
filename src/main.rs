mod animation;
mod player;
mod camera;
mod input;
mod ui;
mod effects;

use macroquad::prelude::*;
use animation::*;
use player::*;
use camera::*;
use input::*;
use effects::*;

pub struct Assets {
    pub hero: Texture2D,
    pub icon_q: Texture2D,
    pub icon_w: Texture2D,
    pub icon_e: Texture2D,
    pub icon_r: Texture2D,
    pub spell_q: Texture2D,
    pub spell_w: Texture2D,
    pub spell_e: Texture2D,
    pub spell_r: Texture2D,
    pub dummy: Texture2D,
    pub target_mouse: Texture2D,
}

async fn load_or_fallback(path: &str, color: Color) -> Texture2D {
    load_texture(path).await.unwrap_or_else(|_| {
        let mut bytes: Vec<u8> = Vec::with_capacity(64 * 64 * 4);
        for _ in 0..(64*64) {
            bytes.push((color.r * 255.0) as u8);
            bytes.push((color.g * 255.0) as u8);
            bytes.push((color.b * 255.0) as u8);
            bytes.push((color.a * 255.0) as u8);
        }
        let tex = Texture2D::from_rgba8(64, 64, &bytes);
        tex.set_filter(FilterMode::Nearest);
        tex
    })
}

#[macroquad::main("2.5D Indie RPG")]
async fn main() {
    let assets = Assets {
        hero: load_or_fallback("hero.png", WHITE).await,
        icon_q: load_or_fallback("arrow-rain-icon.png", BLUE).await,
        icon_w: load_or_fallback("power-strike-icon.png", RED).await,
        icon_e: load_or_fallback("fire-claw-icon.png", ORANGE).await,
        icon_r: load_or_fallback("dark-void-icon.png", PURPLE).await,
        spell_q: load_or_fallback("arrow-projectile.png", BLUE).await,
        spell_w: load_or_fallback("power-strike-spell.png", RED).await,
        spell_e: load_or_fallback("fire-claw-spell.png", ORANGE).await,
        spell_r: load_or_fallback("dark-void-spell.png", PURPLE).await,
        dummy: load_or_fallback("dummy.png", GRAY).await,
        target_mouse: load_or_fallback("target-mouse.png", RED).await,
    };

    let config = SpriteSheetConfig {
        columns: 29,
        rows: 8,
    };

    let mut hero = Hero {
        pos: vec3(0.0, 0.0, 0.0),
        target_pos: vec3(0.0, 0.0, 0.0),
        texture: assets.hero.clone(),
        stats: Stats::new(10, 15, 10),
        anim: AnimationManager::new(config),
        targeting_state: TargetingState::None,
        casting_timer: 0.0,
    };

    let mut game_camera = GameCamera::new(hero.pos);
    let mut effect_manager = EffectManager::new();

    // Spawn some test dummies
    let mut dummies = vec![
        vec3(5.0, 0.0, 5.0),
        vec3(-5.0, 0.0, 3.0),
        vec3(2.0, 0.0, -6.0),
    ];

    loop {
        clear_background(DARKGRAY);

        // Cap delta_time to prevent massive stutters from skipping animation frames
        let delta_time = get_frame_time().min(0.05);

        // 1. Update Camera
        game_camera.update(hero.pos);

        // 2. Handle Input
        handle_input(&mut hero, &game_camera, &mut effect_manager, &dummies, &assets);

        // 3. Update Game State
        if hero.casting_timer > 0.0 {
            hero.casting_timer -= delta_time;
        } else {
            let speed = hero.stats.get_movement_speed();
            let to_target = hero.target_pos - hero.pos;
            if to_target.length() > 0.1 {
                hero.pos += to_target.normalize() * speed * delta_time;
                hero.anim.set_state(AnimationState::Walk);
                hero.anim.set_direction(to_target);
            } else {
                hero.anim.set_state(AnimationState::Idle);
            }
        }

        hero.anim.update(delta_time, hero.stats.get_movement_speed());
        effect_manager.update(delta_time);

        // 4. Render 3D World
        set_camera(&game_camera.camera);
        draw_grid(20, 1.0, BLACK, GRAY);
        
        // Draw target movement marker
        if (hero.target_pos - hero.pos).length() > 0.1 {
            draw_cube_wires(hero.target_pos, vec3(0.5, 0.1, 0.5), GREEN);
        }

        // Draw Dummies
        for d_pos in &dummies {
            let rot = f32::atan2(game_camera.camera.position.x - d_pos.x, game_camera.camera.position.z - d_pos.z);
            let dummy_rect = Rect::new(0.0, 0.0, assets.dummy.width(), assets.dummy.height());
            draw_character_billboard(*d_pos, &assets.dummy, dummy_rect, game_camera.camera.position);
            // Draw health bar placeholder
            draw_cube(*d_pos + vec3(0.0, 2.0, 0.0), vec3(1.0, 0.1, 0.1), None, RED);
        }

        // Draw Hero
        let source_rect = hero.anim.get_source_rect(assets.hero.width(), assets.hero.height());
        draw_character_billboard(hero.pos, &assets.hero, source_rect, game_camera.camera.position);

        // Draw Effects
        effect_manager.draw(game_camera.camera.position);

        // Draw Targeting Indicators
        match hero.targeting_state {
            TargetingState::Aoe(_, radius) => {
                if let Some(intersection) = game_camera.get_mouse_ray_intersection() {
                    // Draw a green wireframe circle
                    let segments = 32;
                    for i in 0..segments {
                        let angle1 = (i as f32 / segments as f32) * std::f32::consts::PI * 2.0;
                        let angle2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::PI * 2.0;
                        let p1 = intersection + vec3(angle1.cos() * radius, 0.1, angle1.sin() * radius);
                        let p2 = intersection + vec3(angle2.cos() * radius, 0.1, angle2.sin() * radius);
                        draw_line_3d(p1, p2, GREEN);
                    }
                }
            }
            TargetingState::UnitTarget(_) => {
                if let Some(intersection) = game_camera.get_mouse_ray_intersection() {
                    draw_cube_wires(intersection, vec3(2.5, 0.1, 2.5), RED);
                }
            }
            TargetingState::None => {}
        }

        set_default_camera();

        // 5. Render UI
        ui::hud::draw_hud(&hero, &assets);

        // Custom Mouse Cursor
        if let TargetingState::UnitTarget(_) = hero.targeting_state {
            show_mouse(false);
            let (mx, my) = mouse_position();
            let tex = &assets.target_mouse;
            draw_texture_ex(
                tex,
                mx - 16.0, // Center a 32x32 icon on the exact mouse coordinate
                my - 16.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(32.0, 32.0)), // Force a 32x32 size in case the png is larger
                    ..Default::default()
                },
            );
        } else {
            show_mouse(true);
        }

        next_frame().await
    }
}