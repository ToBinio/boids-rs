#[derive(Debug, Clone)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Vec2 {
        Vec2 {
            x,
            y,
        }
    }

    pub fn from_angle(angle: f32) -> Vec2 {
        Vec2 {
            x: angle.cos(),
            y: angle.sin(),
        }
    }

    pub fn length(&self) -> f32 {
        (self.x.powi(2) + self.y.powi(2)).sqrt()
    }

    pub fn normalize(&mut self) {
        let mut length = self.length();

        if length == 0.0 {
            length = 1.0;
        }

        self.x /= length;
        self.y /= length;
    }

    pub fn add(&mut self, other: &Vec2) {
        self.x += other.x;
        self.y += other.y;
    }

    pub fn sub(&mut self, other: &Vec2) {
        self.x -= other.x;
        self.y -= other.y;
    }

    pub fn mul(&mut self, factor: f32) {
        self.x *= factor;
        self.y *= factor;
    }

    pub fn div(&mut self, factor: f32) {
        self.x /= factor;
        self.y /= factor;
    }

    pub fn angle(&mut self) -> f32 {
        self.y.atan2(self.x)
    }
}