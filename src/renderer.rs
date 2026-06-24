use crate::language::t; // translation utility
use std::{
    f32::consts::{PI, TAU},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::simulation::{get_epsilon, set_epsilon}; // cleaning addition
use crate::{body::Body, gui_state::GuiState, quadtree::Node, scenario_config::SimulationConfig};
use quarkstrom::{egui, winit::event::VirtualKeyCode, winit_input_helper::WinitInputHelper};

// ---- Color block -----
use palette::{rgb::Rgba, Hsluv, IntoColor};
// fn lerp(a: f32, b: f32, t: f32) -> f32 {
//     a + (b - a) * t
// }

// fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
//     [
//         lerp(a[0] as f32, b[0] as f32, t) as u8,
//         lerp(a[1] as f32, b[1] as f32, t) as u8,
//         lerp(a[2] as f32, b[2] as f32, t) as u8,
//         255,
//     ]
// }

// fn relative_mass(mass: f32) -> f32 {
//     mass.max(1.0).log10() - 8.5
// }

fn body_color(mass: f32) -> [u8; 4] {
    match mass {
        m if m < 2.0 => [220, 220, 220, 255],
        m if m < 20.0 => [120, 120, 120, 255],
        m if m < 100.0 => [0, 120, 255, 255],
        m if m < 1000.0 => [140, 220, 255, 255],
        m if m < 10000.0 => [255, 190, 120, 255],
        m if m < 100000.0 => [255, 220, 0, 255],
        _ => [255, 120, 0, 255],
    }
}

//

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
pub static CLEANING: Lazy<AtomicBool> = Lazy::new(|| false.into());

const PRESETS: &[(&str, &str)] = &[
    ("🌌 Оригинальный диск (100k тел)", ""),
    (
        "🪐 Солнечная система (полная)",
        "../presets/11_solar_system_full.json",
    ),
    ("⭐ Двойная звезда", "../presets/binary_star.json"),
    (
        "💥 Столкновение галактик",
        "../presets/galaxy_collision.json",
    ),
    (
        "🌀 Аккреционный диск чёрной дыры",
        "../presets/black_hole_accretion.json",
    ),
    (
        "💍 Кольца Сатурна (улучшенные)",
        "../presets/saturn_rings.json",
    ),
    (
        "🔵 Глобулярное скопление",
        "../presets/globular_cluster.json",
    ),
    (
        "☄ Солнечная система + комета",
        "../presets/solar_system_comet.json",
    ),
    (
        "☄ Бродячая звезда (Разрушение системы)",
        "../presets/rogue_star_encounter.json",
    ),
    ("📀 Галактика с перемычкой", "../presets/barred_galaxy.json"),
    (
        "🌌 Расширяющаяся Вселенная",
        "../presets/expanding_universe.json",
    ),
];

pub struct Renderer {
    pos: Vec2,
    scale: f32,

    // GUI variables
    settings_window_open: bool,
    show_bodies: bool,
    show_quadtree: bool,
    show_stats: bool,
    show_help: bool,
    show_change_language: bool,
    show_cleaning: bool,

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

            // GUI variables
            settings_window_open: true,
            show_bodies: true,
            show_quadtree: false,
            show_stats: false,
            show_help: false,
            show_change_language: false,
            show_cleaning: false,

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

        // ────── Отображение тел ──────
        if !self.bodies.is_empty() {
            if self.show_bodies {
                for i in 0..self.bodies.len() {
                    let body = &self.bodies[i];
                    let color = body_color(body.mass);
                    let radius = body.radius;

                    ctx.draw_circle(body.pos, radius, color);
                }
            }

            // Создаваемые тела
            if let Some(body) = &self.confirmed_bodies {
                let color = body_color(body.mass);
                ctx.draw_circle(body.pos, body.radius, color);
                ctx.draw_line(body.pos, body.pos + body.vel, [0xff; 4]);
            }

            if let Some(body) = &self.spawn_body {
                let color = body_color(body.mass);
                ctx.draw_circle(body.pos, body.radius, color);
                ctx.draw_line(body.pos, body.pos + body.vel, [0xff; 4]);
            }
        }

        // ────── Отображение квадродерева ──────
        if self.show_quadtree && !self.quadtree.is_empty() {
            let mut depth_range = self.depth_range;

            // Автоматически вычисляем диапазон глубин листьев, если он ещё не задан
            if depth_range.0 >= depth_range.1 {
                let mut stack = Vec::new();
                stack.push((0usize, 0)); // корень всегда с индексом 0

                let mut min_depth = usize::MAX;
                let mut max_depth = 0;

                while let Some((node_idx, depth)) = stack.pop() {
                    let node = &self.quadtree[node_idx];

                    if node.is_leaf() {
                        min_depth = min_depth.min(depth);
                        max_depth = max_depth.max(depth);
                    } else {
                        for i in 0..4 {
                            stack.push((node.children + i, depth + 1));
                        }
                    }
                }

                depth_range = (min_depth, max_depth);
            }

            let (min_depth, max_depth) = depth_range;
            let mut stack = Vec::new();
            stack.push((0usize, 0));

            while let Some((node_idx, depth)) = stack.pop() {
                let node = &self.quadtree[node_idx];

                if node.is_branch() && depth < max_depth {
                    for i in 0..4 {
                        stack.push((node.children + i, depth + 1));
                    }
                } else if depth >= min_depth {
                    let quad = node.quad;
                    let half = Vec2::new(0.5, 0.5) * quad.size;
                    let min = quad.center - half;
                    let max = quad.center + half;

                    let t = ((depth - min_depth + !node.is_empty() as usize) as f32)
                        / (max_depth - min_depth + 1) as f32;

                    let start_h = -100.0;
                    let end_h = 80.0;
                    let h = start_h + (end_h - start_h) * t;
                    let s = 100.0;
                    let l = t * 100.0;

                    let c = Hsluv::new(h, s, l);
                    let rgba: Rgba = c.into_color();
                    let color = rgba.into_format().into();

                    ctx.draw_rect(min, max, color);
                }
            }
        }
    }

    fn gui(&mut self, ctx: &quarkstrom::egui::Context) {
        ctx.set_pixels_per_point(1.0);

        // ──────  Launcher window  ──────
        let mut settings_open = self.settings_window_open;
        egui::Window::new(t("window_title"))
            .open(&mut settings_open)
            .show(ctx, |ui| {
                ui.checkbox(&mut self.show_bodies, t("show_bodies"));
                ui.checkbox(&mut self.show_quadtree, t("show_quadtree"));

                if self.show_quadtree {
                    let range = &mut self.depth_range;
                    ui.horizontal(|ui| {
                        ui.label(t("quadtree_depth_range_label"));
                        ui.add(egui::DragValue::new(&mut range.0).speed(0.05));
                        ui.label(t("quadtree_depth_range_to"));
                        ui.add(egui::DragValue::new(&mut range.1).speed(0.05));
                    });
                }

                if ui
                    .checkbox(&mut self.show_cleaning, t("show_cleaning"))
                    .changed()
                {
                    CLEANING.store(self.show_cleaning, std::sync::atomic::Ordering::Relaxed);
                }
                if self.show_cleaning {
                    let range = (0.00001_f64, 1.5_f64);
                    let mut eps = get_epsilon() as f64;
                    let response = ui.add(
                        egui::Slider::new(&mut eps, range.0..=range.1)
                            .logarithmic(true)
                            .fixed_decimals(5)
                            .text(t("cleaning_value_label")),
                    );
                    if response.changed() {
                        set_epsilon(eps as f32);
                    }
                }

                ui.checkbox(&mut self.show_stats, t("show_statistics"));
                ui.checkbox(&mut self.show_change_language, t("set_language"));

                let is_paused = PAUSED.load(Ordering::Relaxed);
                if ui
                    .button(if is_paused {
                        t("button_start")
                    } else {
                        t("button_pause")
                    })
                    .clicked()
                {
                    PAUSED.store(!is_paused, Ordering::Relaxed);
                }

                ui.separator();

                ui.add(egui::Slider::new(&mut self.dt, 0.01..=2.0).text(t("dt_label")));
                if ui.button(t("apply_dt")).clicked() {
                    *DT.lock() = self.dt;
                }

                ui.separator();

                if ui.button(t("button_presets")).clicked() {
                    self.preset_window_open = true;
                }
                if ui.button(t("button_load_preset")).clicked() {
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
                            ui.label(format!("{} {}", t("selected_prefix"), name));
                            ui.label(format!("{} {:.2}", t("mass_label"), body.mass));
                            ui.label(format!("{} {:.2}", t("radius_label"), body.radius));
                            ui.label(format!(
                                "{} ({:.1}, {:.1})",
                                t("position_label"),
                                body.pos.x,
                                body.pos.y
                            ));
                            ui.label(format!(
                                "{} ({:.1}, {:.1})",
                                t("velocity_label"),
                                body.vel.x,
                                body.vel.y
                            ));
                        } else {
                            ui.label(t("no_name"));
                        }
                    } else {
                        ui.label(t("out_of_range"));
                    }
                } else {
                    ui.label(t("click_info"));
                }
            });
        self.settings_window_open = settings_open;

        // ──────  Окно выбора сценариев (без захвата open)  ──────
        if self.preset_window_open {
            let mut open = true;
            egui::Window::new(t("scenario_window_title"))
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

        // ──────  Отложенная загрузка сценария  ──────
        if let Some(preset_path) = self.pending_preset.take() {
            if preset_path.is_empty() {
                self.load_uniform_disc();
            } else {
                self.load_preset(&preset_path);
            }
        }

        // ──────  Кнопка для высвечивания подсказок  ──────
        egui::Area::new("help_button")
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0)) // отступ от краёв
            .interactable(true)
            .movable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                if ui.button(t("button_help")).clicked() {
                    self.show_help = !self.show_help;
                }
            });

        // ──────  Окошко статистики  ──────
        if self.show_stats {
            let screen_right_x = ctx.screen_rect().right();
            egui::Window::new(t("stats_window_title"))
                .resizable(false)
                .collapsible(false)
                .fixed_pos(egui::pos2(screen_right_x / 2.0, 10.0))
                .show(ctx, |ui| {
                    ui.set_width(300.0);
                    let total_mass: f32 = self.bodies.iter().map(|b| b.mass).sum();
                    let max_mass = self.bodies.iter().map(|b| b.mass).fold(0.0_f32, f32::max);

                    ui.label(format!("{} {}", t("stats_bodies"), self.bodies.len()));
                    ui.label(format!("{} {:.1}", t("stats_fps"), self.fps));
                    ui.label(format!(
                        "{} {}",
                        t("stats_updates"),
                        self.simulation_updates
                    ));
                    ui.label(format!(
                        "{} {}",
                        t("stats_quadtree_nodes"),
                        self.quadtree.len()
                    ));
                    ui.label(format!("{} {:.2}", t("stats_scale"), self.scale));
                    ui.label(format!("{} {:.2}", t("stats_total_mass"), total_mass));
                    ui.label(format!("{} {:.2}", t("stats_max_mass"), max_mass));
                    ui.separator();
                    ui.label(format!(
                        "{} {}",
                        t("stats_paused"),
                        PAUSED.load(Ordering::Relaxed)
                    ));
                });
        }

        // ──────  Окошко выбора языка  ──────
        if self.show_change_language {
            egui::Window::new("Select language")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .open(&mut self.show_change_language)
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        if ui.button("🇪🇸 Español").clicked() {
                            let _ = crate::language::set_language("spanish");
                        }
                        if ui.button("🇬🇧 English").clicked() {
                            let _ = crate::language::set_language("english");
                        }
                        if ui.button("🇫🇷 Français").clicked() {
                            let _ = crate::language::set_language("french");
                        }
                        if ui.button("🇮🇹 Italiano").clicked() {
                            let _ = crate::language::set_language("italian");
                        }
                        if ui.button("🇩🇪 Deutsch").clicked() {
                            let _ = crate::language::set_language("german");
                        }
                        if ui.button("🇷🇺 Русский").clicked() {
                            let _ = crate::language::set_language("russian");
                        }
                        if ui.button("🇨🇳 中文").clicked() {
                            let _ = crate::language::set_language("chinese");
                        }
                        if ui.button("🇯🇵 日本語").clicked() {
                            let _ = crate::language::set_language("japanese");
                        }
                        if ui.button("🇰🇷 한국어").clicked() {
                            let _ = crate::language::set_language("korean");
                        }
                    });
                });
        }

        // ──────  Окошко с подсказками по управлению  ──────
        if self.show_help {
            let help_width = ctx.screen_rect().width() * 2.0 / 3.0;
            let help_height = ctx.screen_rect().height() * 1.5 / 3.0;
            egui::Window::new(t("help_window_title"))
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .fixed_size(egui::vec2(help_width, help_height))
                .collapsible(false)
                .resizable(false)
                .open(&mut self.show_help)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(t("help_text"));
                    });
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
