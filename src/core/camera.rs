use macroquad::prelude::*;

pub struct GameCamera {
    pub camera: Camera3D,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl GameCamera {
    pub fn new(target: Vec3) -> Self {
        let mut camera = Self {
            camera: Camera3D {
                position: Vec3::ZERO,
                target,
                up: vec3(0.0, 1.0, 0.0),
                ..Default::default()
            },
            yaw: 0.0,
            pitch: (15.0_f32).atan2(12.0),
            distance: (15.0_f32 * 15.0 + 12.0_f32 * 12.0).sqrt(),
        };
        camera.update(target);
        camera
    }

    pub fn update(&mut self, target: Vec3) {
        self.camera.target = target;
        self.rebuild_position();
    }

    pub fn orbit(&mut self, delta_x: f32, delta_y: f32, target: Vec3) {
        const ORBIT_SENSITIVITY: f32 = 0.01;
        self.yaw -= delta_x * ORBIT_SENSITIVITY;
        self.pitch = (self.pitch + delta_y * ORBIT_SENSITIVITY).clamp(-1.25, 1.45);
        self.camera.target = target;
        self.rebuild_position();
    }

    pub fn reset_view(&mut self, target: Vec3) {
        self.yaw = 0.0;
        self.pitch = (15.0_f32).atan2(12.0);
        self.distance = (15.0_f32 * 15.0 + 12.0_f32 * 12.0).sqrt();
        self.camera.target = target;
        self.rebuild_position();
    }

    fn rebuild_position(&mut self) {
        let horizontal = self.distance * self.pitch.cos();
        let offset = vec3(
            horizontal * self.yaw.sin(),
            self.distance * self.pitch.sin(),
            horizontal * self.yaw.cos(),
        );
        self.camera.position = self.camera.target + offset;
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
