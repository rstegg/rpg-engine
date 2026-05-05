use macroquad::prelude::*;

pub struct GameCamera {
    pub camera: Camera3D,
}

impl GameCamera {
    pub fn new(target: Vec3) -> Self {
        let position = vec3(target.x, 15.0, target.z + 12.0);
        Self {
            camera: Camera3D {
                position,
                target,
                up: vec3(0.0, 1.0, 0.0),
                ..Default::default()
            },
        }
    }

    pub fn update(&mut self, target: Vec3) {
        // Lock camera to the hero (WC3 allows panning, which we can add later)
        self.camera.position = vec3(target.x, 15.0, target.z + 12.0);
        self.camera.target = target;
    }

    pub fn get_mouse_ray_intersection(&self) -> Option<Vec3> {
        let (mouse_x, mouse_y) = mouse_position();
        let ndc_x = (mouse_x / screen_width()) * 2.0 - 1.0;
        let ndc_y = 1.0 - (mouse_y / screen_height()) * 2.0;

        let inv_vp = self.camera.matrix().inverse();
        let ray_origin = inv_vp.project_point3(vec3(ndc_x, ndc_y, -1.0));
        let far_pt = inv_vp.project_point3(vec3(ndc_x, ndc_y, 1.0));
        let ray_direction = (far_pt - ray_origin).normalize();

        if ray_direction.y != 0.0 {
            let t = -ray_origin.y / ray_direction.y;
            if t > 0.0 {
                let intersect_point = ray_origin + ray_direction * t;
                return Some(vec3(intersect_point.x, 0.0, intersect_point.z));
            }
        }
        None
    }
}
