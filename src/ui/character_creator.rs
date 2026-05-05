use crate::core::animation::*;
use crate::entities::character::*;
use macroquad::prelude::*;

/// The character creator UI state.
pub struct CharacterCreator {
    pub appearance: CharacterAppearance,
    pub catalog: LayerCatalog,
    pub selected_category: LayerCategory,
    pub selected_indices: [Option<usize>; 10], // Index into each category's options (None = "none" selected)
    pub preview_textures: Vec<Texture2D>,
    pub preview_anim: AnimationManager,
    pub confirmed: bool,
    pub needs_reload: bool,
    pub scroll_offset: usize, // Scroll position for the options panel
}

impl CharacterCreator {
    pub fn new(catalog: LayerCatalog) -> Self {
        let config = SpriteSheetConfig {
            columns: 29,
            rows: 8,
        };
        // Default: first skin selected, everything else None
        let mut selected_indices = [None; 10];
        selected_indices[0] = Some(0); // First skin

        Self {
            appearance: CharacterAppearance::default_human(),
            catalog,
            selected_category: LayerCategory::Skin,
            selected_indices,
            preview_textures: Vec::new(),
            preview_anim: AnimationManager::new(config),
            confirmed: false,
            needs_reload: true,
            scroll_offset: 0,
        }
    }

    /// Get the options list for the currently selected category.
    fn get_options_for_category(&self, cat: LayerCategory) -> &[LayerOption] {
        match cat {
            LayerCategory::Skin => &self.catalog.skins,
            LayerCategory::Shoes => &self.catalog.shoes,
            LayerCategory::Clothes => &self.catalog.clothes,
            LayerCategory::Gloves => &self.catalog.gloves,
            LayerCategory::Hairstyle => &self.catalog.hairstyles,
            LayerCategory::FacialHair => &self.catalog.facial_hair,
            LayerCategory::EyeColor => &self.catalog.eye_colors,
            LayerCategory::Eyelashes => &self.catalog.eyelashes,
            LayerCategory::Headgear => &self.catalog.headgears,
            LayerCategory::Addon => &self.catalog.addons,
        }
    }

    fn category_index(cat: LayerCategory) -> usize {
        match cat {
            LayerCategory::Skin => 0,
            LayerCategory::Shoes => 1,
            LayerCategory::Clothes => 2,
            LayerCategory::Gloves => 3,
            LayerCategory::Hairstyle => 4,
            LayerCategory::FacialHair => 5,
            LayerCategory::EyeColor => 6,
            LayerCategory::Eyelashes => 7,
            LayerCategory::Headgear => 8,
            LayerCategory::Addon => 9,
        }
    }

    /// Apply the selected indices back to the CharacterAppearance struct.
    fn sync_appearance(&mut self) {
        let get_name = |cat: LayerCategory, idx: Option<usize>| -> Option<String> {
            idx.and_then(|i| {
                let opts = match cat {
                    LayerCategory::Skin => &self.catalog.skins,
                    LayerCategory::Shoes => &self.catalog.shoes,
                    LayerCategory::Clothes => &self.catalog.clothes,
                    LayerCategory::Gloves => &self.catalog.gloves,
                    LayerCategory::Hairstyle => &self.catalog.hairstyles,
                    LayerCategory::FacialHair => &self.catalog.facial_hair,
                    LayerCategory::EyeColor => &self.catalog.eye_colors,
                    LayerCategory::Eyelashes => &self.catalog.eyelashes,
                    LayerCategory::Headgear => &self.catalog.headgears,
                    LayerCategory::Addon => &self.catalog.addons,
                };
                opts.get(i).map(|o| o.name.clone())
            })
        };

        self.appearance.skin = get_name(LayerCategory::Skin, self.selected_indices[0])
            .unwrap_or_else(|| "Human1".to_string());
        self.appearance.shoes = get_name(LayerCategory::Shoes, self.selected_indices[1]);
        self.appearance.clothes = get_name(LayerCategory::Clothes, self.selected_indices[2]);
        self.appearance.gloves = get_name(LayerCategory::Gloves, self.selected_indices[3]);
        self.appearance.hairstyle = get_name(LayerCategory::Hairstyle, self.selected_indices[4]);
        self.appearance.facial_hair = get_name(LayerCategory::FacialHair, self.selected_indices[5]);
        self.appearance.eye_color = get_name(LayerCategory::EyeColor, self.selected_indices[6]);
        self.appearance.eyelashes = get_name(LayerCategory::Eyelashes, self.selected_indices[7]);
        self.appearance.headgear = get_name(LayerCategory::Headgear, self.selected_indices[8]);
        self.appearance.addon = get_name(LayerCategory::Addon, self.selected_indices[9]);
    }

    /// Draw the full character creator UI. Returns true when the user confirms.
    pub fn update_and_draw(&mut self, dt: f32) -> bool {
        let sw = screen_width();
        let sh = screen_height();

        // Dark overlay
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.08, 0.08, 0.12, 1.0));

        // Title
        let title = "CHARACTER CREATOR";
        let title_size = 36.0;
        let title_w = measure_text(title, None, title_size as u16, 1.0).width;
        draw_text(title, (sw - title_w) / 2.0, 45.0, title_size, WHITE);

        // Layout — proportional columns so it scales with window size
        let margin = 20.0;
        let usable_w = sw - margin * 2.0;
        let panel_left_x = margin;
        let panel_left_w = (usable_w * 0.18).max(160.0); // ~18% for categories
        let preview_x = panel_left_x + panel_left_w + 15.0;
        let preview_w = (usable_w * 0.28).max(200.0); // ~28% for preview
        let options_x = preview_x + preview_w + 15.0;
        let options_w = sw - options_x - margin; // Remainder for options
        let top_y = 70.0;

        // ─── LEFT PANEL: Category Buttons ───
        draw_rectangle(
            panel_left_x - 5.0,
            top_y - 5.0,
            panel_left_w + 10.0,
            sh - top_y - 80.0,
            Color::new(0.12, 0.12, 0.18, 1.0),
        );
        draw_text(
            "LAYERS",
            panel_left_x + 10.0,
            top_y + 20.0,
            22.0,
            Color::new(0.6, 0.6, 0.8, 1.0),
        );

        let (mx, my) = mouse_position();
        let clicked = is_mouse_button_pressed(MouseButton::Left);

        for (i, cat) in LayerCategory::ALL.iter().enumerate() {
            let btn_y = top_y + 35.0 + i as f32 * 38.0;
            let btn_h = 32.0;
            let is_selected = self.selected_category == *cat;
            let is_hovered = mx >= panel_left_x
                && mx <= panel_left_x + panel_left_w
                && my >= btn_y
                && my <= btn_y + btn_h;

            let bg = if is_selected {
                Color::new(0.3, 0.3, 0.6, 1.0)
            } else if is_hovered {
                Color::new(0.2, 0.2, 0.35, 1.0)
            } else {
                Color::new(0.15, 0.15, 0.22, 1.0)
            };

            draw_rectangle(panel_left_x, btn_y, panel_left_w, btn_h, bg);
            draw_rectangle_lines(
                panel_left_x,
                btn_y,
                panel_left_w,
                btn_h,
                1.0,
                Color::new(0.3, 0.3, 0.4, 1.0),
            );

            // Show a dot if something is selected for this category
            let cat_idx = Self::category_index(*cat);
            let has_selection = if cat_idx == 0 {
                true
            } else {
                self.selected_indices[cat_idx].is_some()
            };
            if has_selection {
                draw_circle(panel_left_x + 12.0, btn_y + btn_h / 2.0, 4.0, GREEN);
            }

            draw_text(cat.label(), panel_left_x + 22.0, btn_y + 22.0, 18.0, WHITE);

            if is_hovered && clicked {
                self.selected_category = *cat;
                self.scroll_offset = 0; // Reset scroll when switching categories
            }
        }

        // ─── CENTER: Character Preview ───
        draw_rectangle(
            preview_x - 5.0,
            top_y - 5.0,
            preview_w + 10.0,
            preview_w + 10.0 + 40.0,
            Color::new(0.12, 0.12, 0.18, 1.0),
        );
        draw_text(
            "PREVIEW",
            preview_x + 10.0,
            top_y + 20.0,
            22.0,
            Color::new(0.6, 0.6, 0.8, 1.0),
        );

        // Preview is South/Idle by default
        self.preview_anim.state = AnimationState::Idle;
        self.preview_anim.direction = Direction::South;
        self.preview_anim.update(dt, 1.0);

        // Draw each layer stacked
        let preview_size = (preview_w - 60.0).min(sh * 0.3); // Scale with panel, cap at 30% screen height
        let preview_draw_x = preview_x + (preview_w - preview_size) / 2.0;
        let preview_draw_y = top_y + 35.0;

        for tex in &self.preview_textures {
            let src = self.preview_anim.get_source_rect(tex.width(), tex.height());
            draw_texture_ex(
                tex,
                preview_draw_x,
                preview_draw_y,
                WHITE,
                DrawTextureParams {
                    source: Some(src),
                    dest_size: Some(vec2(preview_size, preview_size)),
                    ..Default::default()
                },
            );
        }

        // ─── RIGHT PANEL: Options Grid ───
        let cat_idx = Self::category_index(self.selected_category);
        let options = self
            .get_options_for_category(self.selected_category)
            .to_vec();

        draw_rectangle(
            options_x - 5.0,
            top_y - 5.0,
            options_w + 10.0,
            sh - top_y - 80.0,
            Color::new(0.12, 0.12, 0.18, 1.0),
        );
        draw_text(
            &format!(
                "{} OPTIONS ({})",
                self.selected_category.label().to_uppercase(),
                options.len()
            ),
            options_x + 10.0,
            top_y + 20.0,
            22.0,
            Color::new(0.6, 0.6, 0.8, 1.0),
        );

        // "None" button for optional categories (not Skin)
        let mut opt_y = top_y + 35.0;
        if cat_idx != 0 {
            let btn_w = options_w - 20.0;
            let btn_h = 28.0;
            let is_none = self.selected_indices[cat_idx].is_none();
            let hovered = mx >= options_x + 10.0
                && mx <= options_x + 10.0 + btn_w
                && my >= opt_y
                && my <= opt_y + btn_h;
            let bg = if is_none {
                Color::new(0.4, 0.2, 0.2, 1.0)
            } else if hovered {
                Color::new(0.25, 0.2, 0.2, 1.0)
            } else {
                Color::new(0.18, 0.15, 0.15, 1.0)
            };

            draw_rectangle(options_x + 10.0, opt_y, btn_w, btn_h, bg);
            draw_rectangle_lines(
                options_x + 10.0,
                opt_y,
                btn_w,
                btn_h,
                1.0,
                Color::new(0.4, 0.3, 0.3, 1.0),
            );
            draw_text("None", options_x + 20.0, opt_y + 20.0, 16.0, WHITE);

            if hovered && clicked && !is_none {
                self.selected_indices[cat_idx] = None;
                self.sync_appearance();
                self.needs_reload = true;
            }

            opt_y += btn_h + 5.0;
        }

        // Option buttons with scrolling
        let btn_h = 28.0;
        let btn_w = options_w - 20.0;
        let max_visible = ((sh - opt_y - 100.0) / (btn_h + 4.0)) as usize;
        let total_options = options.len();

        // Handle mouse wheel scrolling when hovering over the options panel
        let panel_bottom = sh - 80.0;
        let mouse_in_options = mx >= options_x - 5.0
            && mx <= options_x + options_w + 5.0
            && my >= top_y
            && my <= panel_bottom;
        if mouse_in_options {
            let (_, wheel_y) = mouse_wheel();
            if wheel_y < 0.0 && self.scroll_offset + max_visible < total_options {
                self.scroll_offset += 1;
            } else if wheel_y > 0.0 && self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
        }

        // Clamp scroll offset
        let max_scroll = if total_options > max_visible {
            total_options - max_visible
        } else {
            0
        };
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }

        // Draw scroll indicator
        if total_options > max_visible {
            let scrollbar_x = options_x + options_w - 5.0;
            let scrollbar_h = panel_bottom - opt_y - 10.0;
            let thumb_h = (max_visible as f32 / total_options as f32) * scrollbar_h;
            let thumb_y =
                opt_y + (self.scroll_offset as f32 / max_scroll as f32) * (scrollbar_h - thumb_h);

            draw_rectangle(
                scrollbar_x,
                opt_y,
                4.0,
                scrollbar_h,
                Color::new(0.1, 0.1, 0.15, 1.0),
            );
            draw_rectangle(
                scrollbar_x,
                thumb_y,
                4.0,
                thumb_h,
                Color::new(0.4, 0.4, 0.6, 1.0),
            );
        }

        for vis_idx in 0..max_visible {
            let i = vis_idx + self.scroll_offset;
            if i >= total_options {
                break;
            }
            let opt = &options[i];

            let by = opt_y + vis_idx as f32 * (btn_h + 4.0);
            let is_selected = self.selected_indices[cat_idx] == Some(i);
            let hovered = mx >= options_x + 10.0
                && mx <= options_x + 10.0 + btn_w
                && my >= by
                && my <= by + btn_h;

            let bg = if is_selected {
                Color::new(0.2, 0.4, 0.2, 1.0)
            } else if hovered {
                Color::new(0.22, 0.22, 0.32, 1.0)
            } else {
                Color::new(0.16, 0.16, 0.22, 1.0)
            };

            draw_rectangle(options_x + 10.0, by, btn_w, btn_h, bg);
            draw_rectangle_lines(
                options_x + 10.0,
                by,
                btn_w,
                btn_h,
                1.0,
                Color::new(0.3, 0.3, 0.4, 1.0),
            );

            if is_selected {
                draw_circle(options_x + 22.0, by + btn_h / 2.0, 4.0, GREEN);
            }

            draw_text(&opt.name, options_x + 32.0, by + 20.0, 16.0, WHITE);

            if hovered && clicked && !is_selected {
                self.selected_indices[cat_idx] = Some(i);
                self.sync_appearance();
                self.needs_reload = true;
            }
        }

        // ─── BOTTOM BUTTONS: Randomize / Reset / Confirm ───
        let btn_w = 160.0;
        let btn_h = 45.0;
        let btn_gap = 20.0;
        let total_w = btn_w * 3.0 + btn_gap * 2.0;
        let start_x = (sw - total_w) / 2.0;
        let btn_y = sh - 65.0;

        // Helper to draw a button and return whether it was clicked
        let draw_button = |x: f32,
                           label: &str,
                           base_color: Color,
                           border_color: Color,
                           mx: f32,
                           my: f32,
                           clicked: bool|
         -> bool {
            let hovered = mx >= x && mx <= x + btn_w && my >= btn_y && my <= btn_y + btn_h;
            let bg = if hovered {
                Color::new(
                    base_color.r + 0.1,
                    base_color.g + 0.1,
                    base_color.b + 0.1,
                    1.0,
                )
            } else {
                base_color
            };
            draw_rectangle(x, btn_y, btn_w, btn_h, bg);
            draw_rectangle_lines(x, btn_y, btn_w, btn_h, 2.0, border_color);
            let tw = measure_text(label, None, 22, 1.0).width;
            draw_text(label, x + (btn_w - tw) / 2.0, btn_y + 30.0, 22.0, WHITE);
            hovered && clicked
        };

        // RANDOMIZE
        let randomize_clicked = draw_button(
            start_x,
            "RANDOMIZE",
            Color::new(0.35, 0.25, 0.55, 1.0),
            Color::new(0.5, 0.4, 0.7, 1.0),
            mx,
            my,
            clicked,
        );

        // RESET
        let reset_clicked = draw_button(
            start_x + btn_w + btn_gap,
            "RESET",
            Color::new(0.5, 0.2, 0.15, 1.0),
            Color::new(0.7, 0.3, 0.3, 1.0),
            mx,
            my,
            clicked,
        );

        // CONFIRM
        let confirm_clicked = draw_button(
            start_x + (btn_w + btn_gap) * 2.0,
            "CONFIRM",
            Color::new(0.15, 0.45, 0.15, 1.0),
            Color::new(0.3, 0.7, 0.3, 1.0),
            mx,
            my,
            clicked,
        );

        if randomize_clicked {
            self.randomize();
            self.sync_appearance();
            self.needs_reload = true;
        }

        if reset_clicked {
            self.reset();
            self.sync_appearance();
            self.needs_reload = true;
        }

        if confirm_clicked {
            let _ = self.appearance.save_to_file("character.json");
            self.confirmed = true;
            return true;
        }

        false
    }

    fn randomize(&mut self) {
        let rand_opt = |count: usize| -> Option<usize> {
            if count == 0 {
                None
            } else {
                Some(macroquad::rand::gen_range(0, count as i32) as usize)
            }
        };

        // Skin is always required
        self.selected_indices[0] = rand_opt(self.catalog.skins.len());

        // Optional layers: ~70% chance of having something, 30% None
        let maybe = |count: usize| -> Option<usize> {
            if count == 0 {
                return None;
            }
            if macroquad::rand::gen_range(0, 100) < 30 {
                None
            } else {
                rand_opt(count)
            }
        };

        self.selected_indices[1] = maybe(self.catalog.shoes.len());
        self.selected_indices[2] = maybe(self.catalog.clothes.len());
        self.selected_indices[3] = maybe(self.catalog.gloves.len());
        self.selected_indices[4] = maybe(self.catalog.hairstyles.len());
        self.selected_indices[5] = maybe(self.catalog.facial_hair.len());
        self.selected_indices[6] = maybe(self.catalog.eye_colors.len());
        self.selected_indices[7] = maybe(self.catalog.eyelashes.len());
        self.selected_indices[8] = maybe(self.catalog.headgears.len());
        self.selected_indices[9] = maybe(self.catalog.addons.len());

        self.scroll_offset = 0;
    }

    fn reset(&mut self) {
        self.selected_indices = [None; 10];
        self.selected_indices[0] = Some(0); // Default first skin
        self.scroll_offset = 0;
    }
}
