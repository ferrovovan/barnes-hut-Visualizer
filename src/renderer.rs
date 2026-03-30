use std::{
    f32::consts::{PI, TAU},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::{
    body::Body,
    quadtree::{Node, Quadtree},
};

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

fn normalize_mass(
    mass: f32,
    min_mass: f32,
    max_mass: f32,
) -> f32 {

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
use ultraviolet::Vec2;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

pub static PAUSED: Lazy<AtomicBool> = Lazy::new(|| false.into());
pub static UPDATE_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

pub static BODIES: Lazy<Mutex<Vec<Body>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static QUADTREE: Lazy<Mutex<Vec<Node>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub static SPAWN: Lazy<Mutex<Vec<Body>>> = Lazy::new(|| Mutex::new(Vec::new()));

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
    fps: f32,
    frame_counter: u32,
    last_fps_update: Instant,
    bodies: Vec<Body>,
    quadtree: Vec<Node>,
}

impl quarkstrom::Renderer for Renderer {
    fn new() -> Self {
        Self {
            pos: Vec2::zero(),
            scale: 3600.0,

            settings_window_open: false,

            show_bodies: true,
            show_quadtree: false,
            show_stats: true,
            depth_range: (0, 0),

            spawn_body: None,
            angle: None,
            total: None,

            confirmed_bodies: None,
            fps: 0.0,
            frame_counter: 0,
            last_fps_update: Instant::now(),
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
        if input.key_pressed(VirtualKeyCode::F3) {
            self.show_stats = !self.show_stats;
        }
        if let Some((mx, my)) = input.mouse() {
            // Scroll steps to double/halve the scale
            let steps = 5.0;

            // Modify input
            let zoom = (-input.scroll_diff() / steps).exp2();

            // Screen space -> view space
            let target =
                Vec2::new(mx * 2.0 - width as f32, height as f32 - my * 2.0) / height as f32;

            // Move view position based on target
            self.pos += target * self.scale * (1.0 - zoom);

            // Zoom
            self.scale *= zoom;
        }

        // Grab
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
            self.frame_counter += 1;
            let elapsed = self.last_fps_update.elapsed();

            if elapsed >= Duration::from_secs(1) {
                self.fps = self.frame_counter as f32 / elapsed.as_secs_f32();

                self.frame_counter = 0;
                self.last_fps_update = Instant::now();
            }
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
                    let t = normalize_mass(
                        body.mass,
                        min_mass,
                        max_mass,
                    );
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

        if self.show_quadtree && !self.quadtree.is_empty() {
            let mut depth_range = self.depth_range;
            if depth_range.0 >= depth_range.1 {
                let mut stack = Vec::new();
                stack.push((Quadtree::ROOT, 0));

                let mut min_depth = usize::MAX;
                let mut max_depth = 0;
                while let Some((node, depth)) = stack.pop() {
                    let node = &self.quadtree[node];

                    if node.is_leaf() {
                        if depth < min_depth {
                            min_depth = depth;
                        }
                        if depth > max_depth {
                            max_depth = depth;
                        }
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
            stack.push((Quadtree::ROOT, 0));
            while let Some((node, depth)) = stack.pop() {
                let node = &self.quadtree[node];

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
        egui::Window::new("")
            .open(&mut self.settings_window_open)
            .show(ctx, |ui| {
                ui.checkbox(&mut self.show_bodies, "Show Bodies");
                ui.checkbox(&mut self.show_quadtree, "Show Quadtree");
                if self.show_quadtree {
                    let range = &mut self.depth_range;
                    ui.horizontal(|ui| {
                        ui.label("Depth Range:");
                        ui.add(egui::DragValue::new(&mut range.0).speed(0.05));
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut range.1).speed(0.05));
                    });
                }
            });
        if self.show_stats {
            egui::Window::new("Statistics")
            .resizable(false)
            .collapsible(false)
            .default_pos([10.0, 10.0])
            .show(ctx, |ui| {

            let body_count = self.bodies.len();

            let total_mass: f32 =
                self.bodies.iter().map(|b| b.mass).sum();

            let max_mass =
                self.bodies
                    .iter()
                    .map(|b| b.mass)
                    .fold(0.0_f32, f32::max);

            ui.label(format!("Bodies: {}", body_count));
            ui.label(format!("FPS: {:.1}", self.fps));
            ui.label(format!("Quadtree nodes: {}", self.quadtree.len()));
            ui.label(format!("Scale: {:.2}", self.scale));
            ui.label(format!("Total mass: {:.2}", total_mass));
            ui.label(format!("Max mass: {:.2}", max_mass));

            ui.separator();

            ui.label(format!(
                "Paused: {}",
                PAUSED.load(Ordering::Relaxed)
            ));
            });
        }
    }
}