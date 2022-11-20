use std::f32::consts::PI;

use rand::Rng;

use crate::vec2::Vec2;
use crate::vertex::Vertex;

#[derive(Clone)]
pub struct Boid {
    pub location: Vec2,
    pub vel: Vec2,
}

const SIZE: f32 = 0.01 / 4.0;
const VERTEX_COUNT: u32 = 8;

impl Boid {
    pub fn new_random() -> Boid {
        let mut rng = rand::thread_rng();

        Boid {
            location: Vec2::new(0.0, 0.0),
            vel: Vec2::new(rng.gen::<f32>() * 2.0 - 1.0, rng.gen::<f32>() * 2.0 - 1.0),
        }
    }

    pub fn update(&mut self) {
        if self.location.x < -0.8 {
            self.add_vel(&mut Vec2::new(1.0, 0.0), ((-self.location.x - 0.8) / 0.2).powi(3));
        }

        if self.location.x > 0.8 {
            self.add_vel(&mut Vec2::new(-1.0, 0.0), ((self.location.x - 0.8) / 0.2).powi(3));
        }

        if self.location.y < -0.8 {
            self.add_vel(&mut Vec2::new(0.0, 1.0), ((-self.location.y - 0.8) / 0.2).powi(3));
        }

        if self.location.y > 0.8 {
            self.add_vel(&mut Vec2::new(0.0, -1.0), ((self.location.y - 0.8) / 0.2).powi(3));
        }

        self.vel.mul(0.005);

        self.location.add(&self.vel);

        self.vel.normalize();

        let mut rng = rand::thread_rng();
        self.add_vel(&mut Vec2::new(rng.gen::<f32>() * 2.0 - 1.0, rng.gen::<f32>() * 2.0 - 1.0), 0.2);
    }

    pub fn add_vel(&mut self, vel: &mut Vec2, factor: f32) {
        vel.mul(factor);

        self.vel.add(vel);
        self.vel.normalize();
    }

    pub fn create_buffer(&self, vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, index: u32) {
        for i in 0..(VERTEX_COUNT) {
            let angle = ((PI * 2.0) / VERTEX_COUNT as f32) * i as f32;

            vertices.push(Vertex {
                position: [self.location.x + angle.cos() * SIZE, self.location.y + angle.sin() * SIZE, 0.0],
                color: [0.5, 0.0, 0.5],
            });
        }

        for i in 0..(VERTEX_COUNT - 2) {
            indices.push(index * VERTEX_COUNT);
            indices.push(index * VERTEX_COUNT + i + 1);
            indices.push(index * VERTEX_COUNT + i + 2);
        }
    }
}