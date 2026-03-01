use std::{
    f32::consts::{PI, TAU},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    body::Body,
    quadtree::{Node, Quadtree},
    scenario_config::SimulationConfig,
};

use quarkstrom::{egui, winit::event::VirtualKeyCode, winit_input_helper::WinitInputHelper};
use ultraviolet::Vec2;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

pub static PAUSED: Lazy<AtomicBool> = Lazy::new(|| true.into()); // Со старта на паузе
pub static UPDATE_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

pub static BODIES: Lazy<Mutex<Vec<Body>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static QUADTREE: Lazy<Mutex<Vec<Node>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static SPAWN: Lazy<Mutex<Vec<Body>>> = Lazy::new(|| Mutex::new(Vec::new()));

// Канал для передачи тел из JSON в поток физики
pub static RESET_BODIES: Lazy<Mutex<Option<(Vec<Body>, f32, f32, f32)>>> = Lazy::new(|| Mutex::new(None));

pub struct Renderer {
    pos: Vec2,
    scale: f32,
    settings_window_open: bool,
    show_bodies: bool,
    show_quadtree: bool,
    depth_range: (usize, usize),
    spawn_body: Option<Body>,
    angle: Option<f32>,
    total: Option<f32>,
    confirmed_bodies: Option<Body>,
    bodies: Vec<Body>,
    quadtree: Vec<Node>,
}

impl quarkstrom::Renderer for Renderer {
    fn new() -> Self {
        Self {
            pos: Vec2::zero(),
            scale: 3600.0, // Оригинальный масштаб вашей камеры
            settings_window_open: true,
            show_bodies: true,
            show_quadtree: false,
            depth_range: (0, 0),
            spawn_body: None,
            angle: None,
            total: None,
            confirmed_bodies: None,
            bodies: Vec::new(),
            quadtree: Vec::new(),
        }
    }

    fn input(&mut self, input: &WinitInputHelper, width: u16, height: u16) {
        self.settings_window_open ^= input.key_pressed(VirtualKeyCode::E);

        if input.key_pressed(VirtualKeyCode::Space) {
            let val = PAUSED.load(Ordering::Relaxed);
            PAUSED.store(!val, Ordering::Relaxed)
        }

        if let Some((mx, my)) = input.mouse() {
            let steps = 5.0;
            let zoom = (-input.scroll_diff() / steps).exp2();
            let target = Vec2::new(mx * 2.0 - width as f32, height as f32 - my * 2.0) / height as f32;
            self.pos += target * self.scale * (1.0 - zoom);
            self.scale *= zoom;
        }

        if input.mouse_held(2) {
            let (mdx, mdy) = input.mouse_diff();
            self.pos.x -= mdx / height as f32 * self.scale * 2.0;
            self.pos.y += mdy / height as f32 * self.scale * 2.0;
        }

        let world_mouse = || -> Vec2 {
            let (mx, my) = input.mouse().unwrap_or_default();
            let mut mouse = Vec2::new(mx, my);
            mouse *= 2.0 / height as f32;
            mouse.y -= 1.0;
            mouse.y *= -1.0;
            mouse.x -= width as f32 / height as f32;
            mouse * self.scale + self.pos
        };

        if input.mouse_pressed(1) {
            let mouse = world_mouse();
            self.spawn_body = Some(Body::new(mouse, Vec2::zero(), 1.0, 1.0));
            self.angle = None;
            self.total = Some(0.0);
        } else if input.mouse_held(1) {
            if let Some(body) = &mut self.spawn_body {
                let mouse = world_mouse();
                if let Some(angle) = self.angle {
                    let d = mouse - body.pos;
                    let angle2 = d.y.atan2(d.x);
                    let a = angle2 - angle;
                    let a = (a + PI).rem_euclid(TAU) - PI;
                    let total = self.total.unwrap() - a;
                    body.mass = (total / TAU).exp2();
                    self.angle = Some(angle2);
                    self.total = Some(total);
                } else {
                    let d = mouse - body.pos;
                    let angle = d.y.atan2(d.x);
                    self.angle = Some(angle);
                }
                body.radius = body.mass.cbrt();
                body.vel = mouse - body.pos;
            }
        } else if input.mouse_released(1) {
            self.confirmed_bodies = self.spawn_body.take();
        }
    }

    fn render(&mut self, ctx: &mut quarkstrom::RenderContext) {
        {
            let mut lock = UPDATE_LOCK.lock();
            if *lock {
                std::mem::swap(&mut self.bodies, &mut BODIES.lock());
                std::mem::swap(&mut self.quadtree, &mut QUADTREE.lock());
            }
            if let Some(body) = self.confirmed_bodies.take() {
                self.bodies.push(body);
                SPAWN.lock().push(body);
            }
            *lock = false;
        }

        ctx.clear_circles();
        ctx.clear_lines();
        ctx.clear_rects();
        ctx.set_view_pos(self.pos);
        ctx.set_view_scale(self.scale);

        if !self.bodies.is_empty() {
            if self.show_bodies {
                for i in 0..self.bodies.len() {
                    // Используем ваш оригинальный белый цвет [0xff; 4]
                    ctx.draw_circle(self.bodies[i].pos, self.bodies[i].radius, [0xff; 4]);
                }
            }

            if let Some(body) = &self.confirmed_bodies {
                ctx.draw_circle(body.pos, body.radius, [0xff; 4]);
                ctx.draw_line(body.pos, body.pos + body.vel, [0xff; 4]);
            }

            if let Some(body) = &self.spawn_body {
                ctx.draw_circle(body.pos, body.radius, [0xff; 4]);
                ctx.draw_line(body.pos, body.pos + body.vel, [0xff; 4]);
            }
        }
    }

    fn gui(&mut self, ctx: &quarkstrom::egui::Context) {
        ctx.set_pixels_per_point(1.0);

        // 1. Копируем флаги, чтобы не держать ссылку на self внутри замыкания
        let mut settings_open = self.settings_window_open;
        let mut show_bodies = self.show_bodies;

        egui::Window::new("Barnes-Hut Launcher")
            .open(&mut settings_open)
            .show(ctx, |ui| {
                ui.checkbox(&mut show_bodies, "Show Bodies");
                
                let is_paused = PAUSED.load(Ordering::Relaxed);
                if ui.button(if is_paused { "▶ Запустить" } else { "⏸ Пауза" }).clicked() {
                    PAUSED.store(!is_paused, Ordering::Relaxed);
                }

                ui.separator();

                // 2. Логика кнопок: вызываем метод напрямую, 
                // так как мы больше не держим &mut self внутри .open()
                if ui.button("🪐 Загрузить Солнечную Систему").clicked() {
                    // Используем временную переменную для вызова метода
                    self.load_preset("presets/11_solar_system_full.json");
                }
                
                if ui.button("☄ Загрузить Тестовый пресет (1k)").clicked() {
                    self.load_preset("presets/12_test_fast.json");
                }
            });

        // 3. Синхронизируем изменения обратно в self
        self.settings_window_open = settings_open;
        self.show_bodies = show_bodies;
    
    }
}

impl Renderer {
    // Вспомогательный метод загрузки
    fn load_preset(&mut self, path: &str) {
        println!("Попытка загрузки пресета: {}", path);
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<SimulationConfig>(&content) {
                let (new_bodies, _names) = config.into_particles();
                *RESET_BODIES.lock() = Some((new_bodies, config.theta, config.epsilon, config.dt));
                
                // Сбрасываем позицию камеры для нового пресета
                self.pos = Vec2::zero();
                self.scale = 3600.0;
                
                println!(">>> УСПЕХ: Пресет загружен!");
            } else {
                println!("!!! ОШИБКА: Неверный формат JSON.");
            }
        } else {
            println!("!!! ОШИБКА: Файл не найден: {}", path);
        }
    }
}