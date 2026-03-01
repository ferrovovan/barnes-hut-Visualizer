use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use crate::body::Body; // Импортируем оригинальную структуру Body

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyDef {
    pub name: Option<String>,
    pub position: Vector2,
    pub velocity: Vector2,
    pub mass: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeneratorType {
    Disk,
    Sphere,
    Ring,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorDef {
    pub gen_type: GeneratorType,
    pub center: Vector2,
    pub central_mass: Option<f32>,
    pub count: usize,
    pub distance_range: [f32; 2],
    pub density_range: [f32; 2],
    pub radius_range: [f32; 2],
    pub base_velocity: Vector2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub name: String,
    #[serde(default)]
    pub bodies: Vec<BodyDef>,
    #[serde(default)]
    pub generators: Vec<GeneratorDef>,
}

impl SimulationConfig {
    /// Метод теперь называется into_particles, как и просит ваш main.rs
    pub fn into_particles(&self) -> Vec<Body> {
        let mut bodies = Vec::new();

        // Вспомогательная функция генерации случайного числа в диапазоне [min..max]
        // без привязки к конкретной версии структуры ThreadRng
        let gen_range = |min: f32, max: f32| -> f32 {
            let val: f32 = rand::random();
            min + val * (max - min)
        };

        // 1. Импортируем жестко заданные тела (планеты, звезды)
        for b in &self.bodies {
            // Используем .into() для конвертации [f32; 2] в ultraviolet::Vec2
            bodies.push(Body {
                pos: [b.position.x, b.position.y].into(),
                vel: [b.velocity.x, b.velocity.y].into(),
                acc: [0.0, 0.0].into(),
                mass: b.mass,
                radius: b.radius,
            });
        }

        // 2. Генерируем массовые скопления частиц (кольца, диски)
        for gen in &self.generators {
            for _ in 0..gen.count {
                let r_body = gen_range(gen.radius_range[0], gen.radius_range[1]);
                let density = gen_range(gen.density_range[0], gen.density_range[1]);
                let mass = density * PI * r_body.powi(2);

                let min_dist = match gen.gen_type {
                    GeneratorType::Ring => gen.distance_range[0],
                    _ => 0.0,
                };
                
                let distance = gen_range(min_dist, gen.distance_range[1]);
                let theta = gen_range(0.0, 2.0 * PI);
                
                let px = gen.center.x + distance * theta.cos();
                let py = gen.center.y + distance * theta.sin();

                let mut vx = gen.base_velocity.x;
                let mut vy = gen.base_velocity.y;

                match gen.gen_type {
                    GeneratorType::Ring | GeneratorType::Disk => {
                        if let Some(c_mass) = gen.central_mass {
                            let g_constant = 1.0; 
                            let orbital_speed = (g_constant * c_mass / distance).sqrt();
                            
                            vx -= orbital_speed * theta.sin();
                            vy += orbital_speed * theta.cos();
                        }
                    }
                    GeneratorType::Sphere => {}
                }

                // Переводим координаты в ультрафиолетовые векторы ultraviolet::Vec2 через .into()
                bodies.push(Body {
                    pos: [px, py].into(),
                    vel: [vx, vy].into(),
                    acc: [0.0, 0.0].into(),
                    mass,
                    radius: r_body,
                });
            }
        }

        bodies
    }
}