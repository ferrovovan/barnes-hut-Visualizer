use std::{
    f32::consts::{PI, TAU},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::{body::Body, gui_state::GuiState, quadtree::Node, scenario_config::SimulationConfig};

use quarkstrom::{egui, winit::event::VirtualKeyCode, winit_input_helper::WinitInputHelper};

use palette::{rgb::Rgba, Hsluv, IntoColor};
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    [
        lerp(a[0] as f32, b[0] as f32, t) as u8,
        lerp(a[1] as f32, b[1] as f32, t) as u8,
        lerp(a[2] as f32, b[2] as f32, t) as u8,
        255,
    ]
}

fn normalize_mass(mass: f32, min_mass: f32, max_mass: f32) -> f32 {
    let min_log = min_mass.max(0.0001).ln();
    let max_log = max_mass.max(0.0001).ln();
    let mass_log = mass.max(0.0001).ln();

    let x = (mass_log - min_log) / (max_log - min_log);

    let k = 10.0;

    1.0 / (1.0 + (-k * (x - 0.5)).exp())
}

fn blackbody_color(t: f32) -> [u8; 4] {
    let red = [255, 80, 0, 255];
    let orange = [255, 140, 0, 255];
    let yellow = [255, 220, 120, 255];
    let white = [255, 255, 255, 255];
    let blue = [160, 200, 255, 255];

    if t < 0.25 {
        lerp_color(red, orange, t / 0.25)
    } else if t < 0.5 {
        lerp_color(orange, yellow, (t - 0.25) / 0.25)
    } else if t < 0.75 {
        lerp_color(yellow, white, (t - 0.5) / 0.25)
    } else {
        lerp_color(white, blue, (t - 0.75) / 0.25)
    }
}

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rfd::FileDialog;
use ultraviolet::Vec2;

pub static PAUSED: Lazy<AtomicBool> = Lazy::new(|| true.into());
pub static UPDATE_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
pub static BODIES: Lazy<Mutex<Vec<Body>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static QUADTREE: Lazy<Mutex<Vec<Node>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static SPAWN: Lazy<Mutex<Vec<Body>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static RESET_BODIES: Lazy<Mutex<Option<(Vec<Body>, f32, f32, f32)>>> =
    Lazy::new(|| Mutex::new(None));
pub static DT: Lazy<Mutex<f32>> = Lazy::new(|| Mutex::new(0.05));

const PRESETS: &[(&str, &str)] = &[
    ("🌌 Оригинальный диск (100k тел)", ""),
    (
        "🪐 Солнечная система (полная)",
        "presets/11_solar_system_full.json",
    ),
    ("⭐ Двойная звезда", "presets/binary_star.json"),
    ("💥 Столкновение галактик", "presets/galaxy_collision.json"),
    (
        "🌀 Аккреционный диск чёрной дыры",
        "presets/black_hole_accretion.json",
    ),
    (
        "💍 Кольца Сатурна (улучшенные)",
        "presets/saturn_rings.json",
    ),
    ("🔵 Глобулярное скопление", "presets/globular_cluster.json"),
    (
        "☄ Солнечная система + комета",
        "presets/solar_system_comet.json",
    ),
    (
        "☄ Бродячая звезда (Разрушение системы)",
        "presets/rogue_star_encounter.json",
    ),
    ("📀 Галактика с перемычкой", "presets/barred_galaxy.json"),
    (
        "🌌 Расширяющаяся Вселенная",
        "presets/expanding_universe.json",
    ),
];

pub struct Renderer {
    pos: Vec2,
    scale: f32,
    settings_window_open: bool,
    show_bodies: bool,
    show_quadtree: bool,
    show_stats: bool,

    depth_range: (usize, usize),
    spawn_body: Option<Body>,
    angle: Option<f32>,
    total: Option<f32>,
    confirmed_bodies: Option<Body>,

    simulation_updates: u64,
    fps: f32,
    frame_counter: u32,
    last_fps_update: Instant,

    bodies: Vec<Body>,
    quadtree: Vec<Node>,
    gui_state: GuiState,
    dt: f32,
    preset_window_open: bool,
    pending_preset: Option<String>,
}

impl quarkstrom::Renderer for Renderer {
    fn new() -> Self {
        Self {
            pos: Vec2::zero(),
            scale: 3600.0,
            settings_window_open: true,
            show_bodies: true,
            show_quadtree: false,
            show_stats: true,

            depth_range: (0, 0),
            spawn_body: None,
            angle: None,
            total: None,
            confirmed_bodies: None,

            simulation_updates: 0,
            fps: 0.0,
            frame_counter: 0,
            last_fps_update: Instant::now(),

            bodies: Vec::new(),
            quadtree: Vec::new(),
            gui_state: GuiState::new(),
            dt: 0.05,
            preset_window_open: false,
            pending_preset: None,
        }
    }

    fn input(&mut self, input: &WinitInputHelper, width: u16, height: u16) {
        self.settings_window_open ^= input.key_pressed(VirtualKeyCode::E);

        if input.key_pressed(VirtualKeyCode::Space) {
            let val = PAUSED.load(Ordering::Relaxed);
            PAUSED.store(!val, Ordering::Relaxed);
        }
        if input.key_pressed(VirtualKeyCode::F3) {
            self.show_stats = !self.show_stats;
        }
        if let Some((mx, my)) = input.mouse() {
            let steps = 5.0;
            let zoom = (-input.scroll_diff() / steps).exp2();
            let target =
                Vec2::new(mx * 2.0 - width as f32, height as f32 - my * 2.0) / height as f32;
            self.pos += target * self.scale * (1.0 - zoom);
            self.scale *= zoom;
            self.gui_state.camera_zoom = self.scale;
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

        if input.mouse_pressed(0) {
            let mouse_world = world_mouse();
            self.gui_state
                .handle_click([mouse_world.x, mouse_world.y], &self.bodies);
        }
    }

    fn render(&mut self, ctx: &mut quarkstrom::RenderContext) {
        {
            self.frame_counter += 1;
            let elapsed = self.last_fps_update.elapsed();

            if elapsed >= Duration::from_secs(1) {
                self.fps = self.frame_counter as f32 / elapsed.as_secs_f32();

                self.frame_counter = 0;
                self.last_fps_update = Instant::now();
            }
            let mut lock = UPDATE_LOCK.lock();
            if *lock {
                self.simulation_updates += 1;
                std::mem::swap(&mut self.bodies, &mut BODIES.lock());
                std::mem::swap(&mut self.quadtree, &mut QUADTREE.lock());
                if self.bodies.len() > self.gui_state.names.len() {
                    self.gui_state.names.resize(self.bodies.len(), None);
                }
            }
            if let Some(body) = self.confirmed_bodies.take() {
                self.bodies.push(body);
                self.gui_state.names.push(None);
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
                let min_mass = self
                    .bodies
                    .iter()
                    .map(|b| b.mass)
                    .fold(f32::INFINITY, f32::min);

                let max_mass = self
                    .bodies
                    .iter()
                    .map(|b| b.mass)
                    .fold(f32::NEG_INFINITY, f32::max);

                for i in 0..self.bodies.len() {
                    let body = &self.bodies[i];
                    let t = normalize_mass(body.mass, min_mass, max_mass);
                    let color = blackbody_color(t);
                    let radius = 1.0 + body.mass.cbrt() * 0.6;

                    ctx.draw_circle(body.pos, radius, color);
                }
            }

            if let Some(body) = &self.confirmed_bodies {
                let color = blackbody_color(1.0);
                ctx.draw_circle(body.pos, body.radius, color);
                ctx.draw_line(body.pos, body.pos + body.vel, [0xff; 4]);
            }

            if let Some(body) = &self.spawn_body {
                let color = blackbody_color(1.0);
                ctx.draw_circle(body.pos, body.radius, color);
                ctx.draw_line(body.pos, body.pos + body.vel, [0xff; 4]);
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

    fn gui(&mut self, ctx: &quarkstrom::egui::Context) {
        ctx.set_pixels_per_point(1.0);

        let mut settings_open = self.settings_window_open;
        let mut show_bodies = self.show_bodies;

        egui::Window::new("Barnes-Hut Launcher")
            .open(&mut settings_open)
            .show(ctx, |ui| {
                ui.checkbox(&mut show_bodies, "Show Bodies");
                let is_paused = PAUSED.load(Ordering::Relaxed);
                if ui
                    .button(if is_paused {
                        "▶ Запустить"
                    } else {
                        "⏸ Пауза"
                    })
                    .clicked()
                {
                    PAUSED.store(!is_paused, Ordering::Relaxed);
                }
                ui.separator();
                ui.add(egui::Slider::new(&mut self.dt, 0.01..=2.0).text("Δt (time step)"));
                if ui.button("Apply Δt").clicked() {
                    *DT.lock() = self.dt;
                }
                ui.separator();
                if ui.button("📋 Сценарии").clicked() {
                    self.preset_window_open = true;
                }
                if ui.button("📂 Загрузить свой пресет").clicked() {
                    if let Some(path) = FileDialog::new().add_filter("JSON", &["json"]).pick_file()
                    {
                        let path_str = path.to_string_lossy().to_string();
                        self.load_preset(&path_str);
                    }
                }
                ui.separator();
                if let Some(idx) = self.gui_state.selected_body_index {
                    if idx < self.bodies.len() && idx < self.gui_state.names.len() {
                        if let Some(name) = &self.gui_state.names[idx] {
                            let body = &self.bodies[idx];
                            ui.label(format!("Selected: {}", name));
                            ui.label(format!("Mass: {:.2}", body.mass));
                            ui.label(format!("Radius: {:.2}", body.radius));
                            ui.label(format!("Position: ({:.1}, {:.1})", body.pos.x, body.pos.y));
                            ui.label(format!("Velocity: ({:.1}, {:.1})", body.vel.x, body.vel.y));
                        } else {
                            ui.label("Selected object has no name");
                        }
                    } else {
                        ui.label("Selected index out of range");
                    }
                } else {
                    ui.label("Click on an object (LMB) to see info");
                }
            });

        self.settings_window_open = settings_open;
        self.show_bodies = show_bodies;

        // Окно выбора сценариев (без захвата open)
        if self.preset_window_open {
            let mut open = true;
            egui::Window::new("Выбор сценария")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.set_width(300.0);
                    for (name, path) in PRESETS {
                        if ui.button(*name).clicked() {
                            self.pending_preset = Some(path.to_string());
                        }
                    }
                });
            // Закрываем окно после обработки
            if !open {
                self.preset_window_open = false;
            }
        }

        // Отложенная загрузка
        if let Some(preset_path) = self.pending_preset.take() {
            if preset_path.is_empty() {
                self.load_uniform_disc();
            } else {
                self.load_preset(&preset_path);
            }
        }

        if self.show_stats {
            egui::Window::new("Statistics")
                .resizable(false)
                .collapsible(false)
                .default_pos([10.0, 10.0])
                .show(ctx, |ui| {
                    let body_count = self.bodies.len();

                    let total_mass: f32 = self.bodies.iter().map(|b| b.mass).sum();

                    let max_mass = self.bodies.iter().map(|b| b.mass).fold(0.0_f32, f32::max);

                    ui.label(format!("Bodies: {}", body_count));
                    ui.label(format!("FPS: {:.1}", self.fps));
                    ui.label(format!("Simulation updates: {}", self.simulation_updates));
                    ui.label(format!("Quadtree nodes: {}", self.quadtree.len()));
                    ui.label(format!("Scale: {:.2}", self.scale));
                    ui.label(format!("Total mass: {:.2}", total_mass));
                    ui.label(format!("Max mass: {:.2}", max_mass));

                    ui.separator();

                    ui.label(format!("Paused: {}", PAUSED.load(Ordering::Relaxed)));
                });
        }
    }
}

impl Renderer {
    fn load_preset(&mut self, path: &str) {
        println!("Попытка загрузки пресета: {}", path);
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<SimulationConfig>(&content) {
                let (new_bodies, names) = config.into_particles();
                self.apply_new_bodies(new_bodies, names);
                println!(">>> УСПЕХ: Пресет загружен!");
            } else {
                eprintln!("!!! ОШИБКА: Неверный формат JSON.");
            }
        } else {
            eprintln!("!!! ОШИБКА: Файл не найден: {}", path);
        }
    }

    fn load_uniform_disc(&mut self) {
        use crate::utils;
        let new_bodies = utils::uniform_disc(100000);
        let names = vec![None; new_bodies.len()];
        self.apply_new_bodies(new_bodies, names);
        println!(">>> Загружен оригинальный диск (100k тел)");
    }

    fn apply_new_bodies(&mut self, new_bodies: Vec<Body>, names: Vec<Option<String>>) {
        self.gui_state.names = names;
        self.gui_state.selected_body_index = None;

        if !new_bodies.is_empty() {
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for body in &new_bodies {
                min_x = min_x.min(body.pos.x);
                min_y = min_y.min(body.pos.y);
                max_x = max_x.max(body.pos.x);
                max_y = max_y.max(body.pos.y);
            }
            let width = (max_x - min_x).abs();
            let height = (max_y - min_y).abs();
            let needed_scale = (height.max(width) / 0.8) / 2.0;
            self.scale = needed_scale.max(100.0);
            self.pos = Vec2::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        } else {
            self.scale = 3600.0;
            self.pos = Vec2::zero();
        }

        let theta = 1.0;
        let epsilon = 1.0;
        let dt = self.dt;
        *RESET_BODIES.lock() = Some((new_bodies, theta, epsilon, dt));
        PAUSED.store(false, Ordering::SeqCst);
    }
}
