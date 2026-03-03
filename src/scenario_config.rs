use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use crate::body::Body;
use ultraviolet::Vec2;

// ----- Вспомогательные структуры для JSON -----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vec2Def {
    pub x: f32,
    pub y: f32,
}

impl From<Vec2Def> for Vec2 {
    fn from(v: Vec2Def) -> Self {
        Vec2::new(v.x, v.y)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyDef {
    pub name: Option<String>,
    pub position: Vec2Def,
    pub velocity: Vec2Def,
    pub mass: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "gen_type")]
pub enum GeneratorDef {
    Ring {
        center: Vec2Def,
        central_mass: f32,
        count: usize,
        distance_range: [f32; 2],
        density_range: [f32; 2],
        radius_range: [f32; 2],
        base_velocity: Vec2Def,
    },
    // При желании можно добавить другие типы (Spiral, Disc и т.д.)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub name: String,
    #[serde(default)]
    pub kind: serde_json::Value,
    pub star_count: usize,
    pub satellite_count: usize,
    pub star_mass: f32,
    pub satellite_mass_range: [f32; 2],
    pub disk_radius: f32,
    pub seed: u64,
    pub theta: f32,
    pub epsilon: f32,
    pub dt: f32,
    #[serde(default)]
    pub bodies: Vec<BodyDef>,
    #[serde(default)]
    pub generators: Vec<GeneratorDef>,
}

impl SimulationConfig {
    pub fn into_particles(&self) -> (Vec<Body>, Vec<Option<String>>) {
        let mut bodies = Vec::new();
        let mut names = Vec::new();

        // ----- 1. Явно заданные тела -----
        for body_def in &self.bodies {
            bodies.push(Body {
                pos: body_def.position.clone().into(),
                vel: body_def.velocity.clone().into(),
                acc: Vec2::zero(),
                mass: body_def.mass,
                radius: body_def.radius,
            });
            names.push(body_def.name.clone());
        }

        // ----- 2. Генераторы (кольца, диски) -----
        let mut seed = self.seed;
        let mut rng = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 32) as u32 as f32 / u32::MAX as f32
        };

        for gen in &self.generators {
            match gen {
                GeneratorDef::Ring {
                    center,
                    central_mass,
                    count,
                    distance_range,
                    density_range,
                    radius_range,
                    base_velocity,
                } => {
                    let center_pos = Vec2::from(center.clone());
                    let base_vel = Vec2::from(base_velocity.clone());
                    for _ in 0..*count {
                        let r = distance_range[0] + rng() * (distance_range[1] - distance_range[0]);
                        let angle = rng() * 2.0 * PI;
                        let dx = r * angle.cos();
                        let dy = r * angle.sin();
                        let pos = center_pos + Vec2::new(dx, dy);

                        // Круговая орбитальная скорость относительно центрального тела
                        let orbital_speed = (1.0 * central_mass / r).sqrt();
                        let vel_dir = Vec2::new(-angle.sin(), angle.cos());
                        let vel = base_vel + vel_dir * orbital_speed;

                        let mass = density_range[0] + rng() * (density_range[1] - density_range[0]);
                        let radius = radius_range[0] + rng() * (radius_range[1] - radius_range[0]);

                        bodies.push(Body::new(pos, vel, mass, radius));
                        names.push(None);
                    }
                }
            }
        }

        // ----- 3. Если тела всё ещё не добавлены, используем старую логику на основе kind -----
        if bodies.is_empty() {
            let mut seed = self.seed;
            let mut next_random = move |min: f32, max: f32| -> f32 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let val = (seed >> 32) as u32 as f32 / u32::MAX as f32;
                min + val * (max - min)
            };

            // Центральные звёзды
            if self.star_count > 0 {
                for i in 0..self.star_count {
                    let name = if self.star_count == 1 {
                        "Central Star".to_string()
                    } else {
                        format!("Star {}", i + 1)
                    };
                    let offset_x = if self.star_count > 1 { next_random(-20.0, 20.0) } else { 0.0 };
                    let offset_y = if self.star_count > 1 { next_random(-20.0, 20.0) } else { 0.0 };
                    bodies.push(Body {
                        pos: [offset_x, offset_y].into(),
                        vel: [0.0, 0.0].into(),
                        acc: [0.0, 0.0].into(),
                        mass: self.star_mass,
                        radius: (self.star_mass / 10000.0).max(5.0),
                    });
                    names.push(Some(name));
                }
            }

            let kind_str = self.kind.as_str().unwrap_or("");
            if kind_str == "SolarSystemFull" || kind_str == "SolarSystem" {
                // ... ваш старый код для Солнечной системы ...
                let planets = vec![
                    ("Mercury", 100.0, 0.4, 0.5),
                    ("Venus", 250.0, 0.9, 0.8),
                    ("Earth", 400.0, 1.0, 1.0),
                    ("Mars", 600.0, 0.1, 0.7),
                    ("Jupiter", 1000.0, 317.0, 4.0),
                    ("Saturn", 1500.0, 95.0, 3.5),
                    ("Uranus", 2200.0, 14.0, 2.5),
                    ("Neptune", 2900.0, 17.0, 2.4),
                ];
                for (p_name, dist, p_mass, p_rad) in planets {
                    let theta = next_random(0.0, 2.0 * PI);
                    let px = dist * theta.cos();
                    let py = dist * theta.sin();
                    let orbital_speed = (1.0 * self.star_mass / dist).sqrt();
                    let vx = -orbital_speed * theta.sin();
                    let vy = orbital_speed * theta.cos();
                    bodies.push(Body {
                        pos: [px, py].into(),
                        vel: [vx, vy].into(),
                        acc: [0.0, 0.0].into(),
                        mass: p_mass,
                        radius: p_rad,
                    });
                    names.push(Some(p_name.to_string()));
                }
            } else {
                // Галактики и диски (UniformDisc и т.д.)
                for _ in 0..self.satellite_count {
                    let r_body = next_random(self.satellite_mass_range[0], self.satellite_mass_range[1]);
                    let distance = next_random(10.0, self.disk_radius);
                    let mut theta = next_random(0.0, 2.0 * PI);
                    // Спиральные рукава (опционально)
                    if let Some(obj) = self.kind.as_object() {
                        if obj.contains_key("SpiralGalaxy") {
                            let arms = obj.get("SpiralGalaxy").and_then(|v| v.as_u64()).unwrap_or(3) as f32;
                            let arm_index = (next_random(0.0, arms).floor()) * (2.0 * PI / arms);
                            theta = arm_index + (distance * 0.005) + next_random(-0.1, 0.1);
                        }
                    }
                    let px = distance * theta.cos();
                    let py = distance * theta.sin();
                    let orbital_speed = (1.0 * self.star_mass / distance).sqrt();
                    let vx = -orbital_speed * theta.sin();
                    let vy = orbital_speed * theta.cos();
                    bodies.push(Body {
                        pos: [px, py].into(),
                        vel: [vx, vy].into(),
                        acc: [0.0, 0.0].into(),
                        mass: r_body,
                        radius: (r_body * 2.0).max(0.5),
                    });
                    names.push(None);
                }
            }
        }

        (bodies, names)
    }
}