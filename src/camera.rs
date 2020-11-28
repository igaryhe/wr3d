use glam::{Vec3, Mat4};

pub struct Camera {
    pub eye: Vec3,
    pub center: Vec3,
    pub up: Vec3,
    pub aspect_ratio: f32,
    pub fov: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.center, self.up);
        let proj = Mat4::perspective_rh(self.fov, self.aspect_ratio, self.z_near, self.z_far);
        proj * view
    }
}
