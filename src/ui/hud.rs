use crate::Assets;
use crate::entities::player::{Hero, SpellId};
use macroquad::prelude::*;

pub fn draw_hud(hero: &Hero, assets: &Assets) {
    // Basic bottom HUD panel
    let screen_w = screen_width();
    let screen_h = screen_height();

    let hud_height = (screen_h * 0.15).clamp(100.0, 200.0);
    let hud_y = screen_h - hud_height;
    let icon_size = (hud_height * 0.5).clamp(40.0, 80.0);

    // Draw background
    draw_rectangle(
        0.0,
        hud_y,
        screen_w,
        hud_height,
        Color::new(0.15, 0.15, 0.15, 1.0),
    );
    draw_rectangle_lines(0.0, hud_y, screen_w, hud_height, 2.0, BLACK);

    // Draw Orbs
    let orb_radius = (hud_height * 0.45).clamp(45.0, 90.0);
    let padding = 20.0;
    
    // Health Orb (Bottom Left)
    let hp_percent = hero.stats.current_hp as f32 / hero.stats.max_hp as f32;
    draw_orb(
        orb_radius + padding,
        screen_h - orb_radius - padding,
        orb_radius,
        hp_percent,
        Color::new(0.8, 0.1, 0.1, 1.0),
        "LIFE",
        &format!("{}/{}", hero.stats.current_hp, hero.stats.max_hp),
    );

    // Mana Orb (Bottom Right)
    let mp_percent = hero.stats.current_mp as f32 / hero.stats.max_mp as f32;
    draw_orb(
        screen_w - orb_radius - padding,
        screen_h - orb_radius - padding,
        orb_radius,
        mp_percent,
        Color::new(0.1, 0.1, 0.8, 1.0),
        "MANA",
        &format!("{}/{}", hero.stats.current_mp, hero.stats.max_mp),
    );

    // Action Bar (Spells)
    let icons = [
        (&assets.icon_q, "Q", SpellId::Q),
        (&assets.icon_w, "W", SpellId::W),
        (&assets.icon_e, "E", SpellId::E),
        (&assets.icon_r, "R", SpellId::R),
    ];

    let action_bar_x = screen_w / 2.0 - (icons.len() as f32 * icon_size * 1.2) / 2.0;
    let action_bar_y = hud_y + (hud_height - icon_size) / 2.0;

    for (i, (texture, hotkey, spell_id)) in icons.iter().enumerate() {
        let slot_x = action_bar_x + (i as f32 * (icon_size * 1.2));

        // Draw icon
        draw_texture_ex(
            texture,
            slot_x,
            action_bar_y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(icon_size, icon_size)),
                ..Default::default()
            },
        );

        // Draw cooldown overlay
        let cd = hero.cooldowns.get(spell_id).copied().unwrap_or(0.0);
        let max_cd = spell_id.get_max_cooldown();
        if cd > 0.0 {
            let progress = cd / max_cd;
            draw_radial_cooldown(slot_x, action_bar_y, icon_size, progress);

            // Draw cooldown number at top right
            let cd_text = format!("{:.1}", cd);
            draw_text(&cd_text, slot_x + icon_size - 25.0, action_bar_y + 15.0, 20.0, RED);
        }

        // Draw border
        draw_rectangle_lines(slot_x, action_bar_y, icon_size, icon_size, 2.0, BLACK);

        // Draw hotkey label
        draw_text(*hotkey, slot_x + 5.0, action_bar_y + 15.0, 16.0, WHITE);
    }
}

fn draw_orb(x: f32, y: f32, radius: f32, percent: f32, color: Color, label: &str, sublabel: &str) {
    // Outer border/glass
    draw_circle(x, y, radius + 4.0, BLACK);
    draw_circle(x, y, radius + 2.0, Color::new(0.2, 0.2, 0.2, 1.0));
    
    // Background (empty orb)
    draw_circle(x, y, radius, Color::new(0.05, 0.05, 0.05, 1.0));
    
    // Liquid fill
    if percent > 0.0 {
        let fill_height = radius * 2.0 * percent;
        let fill_top = y + radius - fill_height;
        
        let mut liquid_points = Vec::new();
        
        // Calculate intersection points with the fill_top line
        let dy = fill_top - y;
        if dy.abs() < radius {
            let dx = (radius * radius - dy * dy).sqrt();
            let x_left = x - dx;
            let x_right = x + dx;
            
            liquid_points.push(vec2(x_left, fill_top));
            
            // Add points along the bottom arc
            let detail = 32;
            for i in 0..=detail {
                let a = std::f32::consts::PI * 2.0 * (i as f32 / detail as f32);
                let px = x + a.cos() * radius;
                let py = y + a.sin() * radius;
                if py > fill_top {
                    liquid_points.push(vec2(px, py));
                }
            }
            
            liquid_points.push(vec2(x_right, fill_top));
        } else if dy <= -radius {
            // Full circle
            let detail = 32;
            for i in 0..=detail {
                let a = std::f32::consts::PI * 2.0 * (i as f32 / detail as f32);
                liquid_points.push(vec2(x + a.cos() * radius, y + a.sin() * radius));
            }
        }
        
        if liquid_points.len() >= 3 {
            // Sort points by angle to center for proper triangle fan? 
            // Actually, we can just use a triangle fan from the center if we order them correctly.
            // But a simple way is to draw a triangle fan from the first point if it's convex.
            for i in 1..liquid_points.len() - 1 {
                draw_triangle(liquid_points[0], liquid_points[i], liquid_points[i+1], color);
            }
        }
    }
    
    // Glass highlight
    draw_circle_lines(x, y, radius, 2.0, Color::new(1.0, 1.0, 1.0, 0.1));
    draw_circle(x - radius * 0.3, y - radius * 0.3, radius * 0.2, Color::new(1.0, 1.0, 1.0, 0.2));

    // Labels
    let label_size = 18.0;
    let label_w = measure_text(label, None, label_size as u16, 1.0).width;
    draw_text(label, x - label_w / 2.0, y - 5.0, label_size, WHITE);
    
    let sub_size = 14.0;
    let sub_w = measure_text(sublabel, None, sub_size as u16, 1.0).width;
    draw_text(sublabel, x - sub_w / 2.0, y + 15.0, sub_size, LIGHTGRAY);
}

pub fn draw_revive_progress(progress: f32) {
    if progress <= 0.0 { return; }
    
    let sw = screen_width();
    let sh = screen_height();
    let w = 300.0;
    let h = 30.0;
    let x = (sw - w) / 2.0;
    let y = sh / 2.0 + 100.0;
    
    draw_rectangle(x, y, w, h, Color::new(0.1, 0.1, 0.1, 0.8));
    draw_rectangle(x + 2.0, y + 2.0, (w - 4.0) * progress, h - 4.0, GOLD);
    draw_rectangle_lines(x, y, w, h, 2.0, BLACK);
    
    let text = "REVIVING...";
    let tw = measure_text(text, None, 20, 1.0).width;
    draw_text(text, (sw - tw) / 2.0, y - 10.0, 20.0, WHITE);
}

fn draw_radial_cooldown(x: f32, y: f32, size: f32, progress: f32) {
    if progress <= 0.0 {
        return;
    }

    let cx = x + size / 2.0;
    let cy = y + size / 2.0;
    let half_size = size / 2.0;

    let start_angle = -std::f32::consts::PI / 2.0; // 12 o'clock
    let end_angle = start_angle + progress * 2.0 * std::f32::consts::PI;

    let segments = 32;
    let angle_step = (2.0 * std::f32::consts::PI) / segments as f32;

    let get_pt = |a: f32| -> Vec2 {
        let r = if a.cos().abs() > 0.0001 && a.sin().abs() > 0.0001 {
            (half_size / a.cos().abs()).min(half_size / a.sin().abs())
        } else if a.cos().abs() <= 0.0001 {
            half_size / a.sin().abs()
        } else {
            half_size / a.cos().abs()
        };
        vec2(cx + a.cos() * r, cy + a.sin() * r)
    };

    let color = Color::new(0.0, 0.0, 0.0, 0.6);

    for i in 0..segments {
        let a1 = start_angle + (i as f32) * angle_step;
        let a2 = start_angle + ((i + 1) as f32) * angle_step;

        if a1 >= end_angle {
            break;
        }

        let a2_clamped = a2.min(end_angle);

        let p1 = get_pt(a1);
        let p2 = get_pt(a2_clamped);

        draw_triangle(vec2(cx, cy), p1, p2, color);
    }
}
