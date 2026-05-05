use crate::Assets;
use crate::entities::player::Hero;
use macroquad::prelude::*;

pub fn draw_hud(hero: &Hero, assets: &Assets) {
    // Basic bottom HUD panel
    let screen_w = screen_width();
    let screen_h = screen_height();

    let hud_height = 150.0;
    let hud_y = screen_h - hud_height;

    // Draw background
    draw_rectangle(
        0.0,
        hud_y,
        screen_w,
        hud_height,
        Color::new(0.15, 0.15, 0.15, 1.0),
    );
    draw_rectangle_lines(0.0, hud_y, screen_w, hud_height, 2.0, BLACK);

    // Draw Stats
    let stats_x = 20.0;
    let stats_y = hud_y + 30.0;
    draw_text("HERO STATS", stats_x, stats_y, 20.0, WHITE);
    draw_text(
        &format!("HP: {}/{}", hero.stats.current_hp, hero.stats.max_hp),
        stats_x,
        stats_y + 25.0,
        20.0,
        RED,
    );
    draw_text(
        &format!("MP: {}/{}", hero.stats.current_mp, hero.stats.max_mp),
        stats_x,
        stats_y + 50.0,
        20.0,
        BLUE,
    );
    draw_text(
        &format!(
            "Str: {}  Agi: {}  Int: {}",
            hero.stats.strength, hero.stats.agility, hero.stats.intelligence
        ),
        stats_x,
        stats_y + 75.0,
        20.0,
        WHITE,
    );

    // Action Bar (Spells)
    let action_bar_x = screen_w / 2.0 - 150.0;
    let action_bar_y = hud_y + 30.0;

    let icons = [
        (&assets.icon_q, "Q"),
        (&assets.icon_w, "W"),
        (&assets.icon_e, "E"),
        (&assets.icon_r, "R"),
    ];

    for (i, (texture, hotkey)) in icons.iter().enumerate() {
        let slot_x = action_bar_x + (i as f32 * 60.0);

        // Draw icon
        draw_texture_ex(
            texture,
            slot_x,
            action_bar_y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(50.0, 50.0)),
                ..Default::default()
            },
        );

        // Draw border
        draw_rectangle_lines(slot_x, action_bar_y, 50.0, 50.0, 2.0, BLACK);

        // Draw hotkey label
        draw_text(*hotkey, slot_x + 5.0, action_bar_y + 15.0, 16.0, WHITE);
    }
}
