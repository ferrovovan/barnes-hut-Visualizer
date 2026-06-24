use crate::{
    body::Body,
    quadtree::{Node, Quad, Quadtree},
    renderer::DT,
    utils,
};

use broccoli::aabb::Rect;
use ultraviolet::Vec2;

//// Epsilon block
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU32, Ordering};
pub static EPSILON_BITS: Lazy<AtomicU32> = Lazy::new(|| AtomicU32::new(0.005_f32.to_bits()));

pub fn get_epsilon() -> f32 {
    f32::from_bits(EPSILON_BITS.load(Ordering::Relaxed))
}

pub fn set_epsilon(val: f32) {
    EPSILON_BITS.store(val.to_bits(), Ordering::Relaxed);
}

//// Simulation block
pub struct Simulation {
    pub dt: f32,
    pub frame: usize,
    pub bodies: Vec<Body>,
    pub quadtree: Quadtree,

    pub is_cleaning: bool,
}

impl Simulation {
    pub fn new() -> Self {
        let dt = 0.05;
        let n = 100000;
        let theta = 1.0;
        let epsilon = 1.0;

        let bodies: Vec<Body> = utils::uniform_disc(n);
        let quadtree = Quadtree::new(theta, epsilon);

        Self {
            dt,
            frame: 0,
            bodies,
            quadtree,
            is_cleaning: false,
        }
    }

    pub fn step(&mut self) {
        // Синхронизация dt с глобальным значением из GUI
        if let Some(dt) = DT.try_lock() {
            self.dt = *dt;
        }
        if self.is_cleaning {
            self.cleaning();
        }
        self.iterate();
        self.collide();
        self.attract();
        self.frame += 1;
    }

    pub fn cleaning(&mut self) {
        let nodes = &self.quadtree.nodes;
        let root = Quadtree::ROOT;
        if root >= nodes.len() || nodes[root].children == 0 {
            return;
        }

        // Рекурсивный сбор крайних узлов в главном квадрате
        let collector = ExtremeCollector::new(nodes, root, get_epsilon());
        let mut extremes = collector.into_extremes();
        println!("-->  Узлов для очистки: {}", extremes.len());
        if extremes.is_empty() {
            return;
        } else {
            extremes.sort_unstable();
            extremes.dedup();
        }

        // Сохраняем Quad для удаления тел
        let to_remove: Vec<(usize, Quad)> = extremes
            .iter()
            .map(|&i| (i, self.quadtree.nodes[i].quad))
            .collect();

        // Удаляем тела, попавшие в крайние «мусорные» квадранты
        self.bodies.retain(|body| {
            !to_remove.iter().any(|(_, quad)| {
                let half = quad.size / 2.0;
                let min = quad.center - Vec2::new(half, half);
                let max = quad.center + Vec2::new(half, half);
                body.pos.x >= min.x
                    && body.pos.x <= max.x
                    && body.pos.y >= min.y
                    && body.pos.y <= max.y
            })
        });

        // println!("-->  Узлов до очистки: {}", self.bodies.len());
        // Удаляем помеченные узлы из дерева (в обратном порядке индексов)
        for idx in extremes.into_iter().rev() {
            self.quadtree.nodes.remove(idx);
            // Поддерживаем синхронность parents
            if idx < self.quadtree.parents.len() {
                self.quadtree.parents.remove(idx);
            }
        }
        // println!("--> Узлов после очистки: {}", self.bodies.len());
    }

    pub fn attract(&mut self) {
        let quad = Quad::new_containing(&self.bodies);
        self.quadtree.clear(quad);

        for body in &self.bodies {
            self.quadtree.insert(body.pos, body.mass);
        }

        self.quadtree.propagate();

        for body in &mut self.bodies {
            body.acc = self.quadtree.acc(body.pos);
        }
    }

    pub fn iterate(&mut self) {
        for body in &mut self.bodies {
            body.update(self.dt);
        }
    }

    pub fn collide(&mut self) {
        let mut rects = self
            .bodies
            .iter()
            .enumerate()
            .map(|(index, body)| {
                let pos = body.pos;
                let radius = body.radius;
                let min = pos - Vec2::one() * radius;
                let max = pos + Vec2::one() * radius;
                (Rect::new(min.x, max.x, min.y, max.y), index)
            })
            .collect::<Vec<_>>();

        let mut broccoli = broccoli::Tree::new(&mut rects);

        broccoli.find_colliding_pairs(|i, j| {
            let i = *i.unpack_inner();
            let j = *j.unpack_inner();
            self.resolve(i, j);
        });
    }

    fn resolve(&mut self, i: usize, j: usize) {
        let b1 = &self.bodies[i];
        let b2 = &self.bodies[j];

        let p1 = b1.pos;
        let p2 = b2.pos;

        let r1 = b1.radius;
        let r2 = b2.radius;

        let d = p2 - p1;
        let r = r1 + r2;

        if d.mag_sq() > r * r {
            return;
        }

        let v1 = b1.vel;
        let v2 = b2.vel;

        let v = v2 - v1;

        let d_dot_v = d.dot(v);

        let m1 = b1.mass;
        let m2 = b2.mass;

        let weight1 = m2 / (m1 + m2);
        let weight2 = m1 / (m1 + m2);

        if d_dot_v >= 0.0 && d != Vec2::zero() {
            let tmp = d * (r / d.mag() - 1.0);
            self.bodies[i].pos -= weight1 * tmp;
            self.bodies[j].pos += weight2 * tmp;
            return;
        }

        let v_sq = v.mag_sq();
        let d_sq = d.mag_sq();
        let r_sq = r * r;

        let t = (d_dot_v + (d_dot_v * d_dot_v - v_sq * (d_sq - r_sq)).max(0.0).sqrt()) / v_sq;

        self.bodies[i].pos -= v1 * t;
        self.bodies[j].pos -= v2 * t;

        let p1 = self.bodies[i].pos;
        let p2 = self.bodies[j].pos;
        let d = p2 - p1;
        let d_dot_v = d.dot(v);
        let d_sq = d.mag_sq();

        let tmp = d * (1.5 * d_dot_v / d_sq);
        let v1 = v1 + tmp * weight1;
        let v2 = v2 - tmp * weight2;

        self.bodies[i].vel = v1;
        self.bodies[j].vel = v2;
        self.bodies[i].pos += v1 * t;
        self.bodies[j].pos += v2 * t;
    }
}

// Класс, выполняющий сбор крайних узлов при создании
struct ExtremeCollector<'a> {
    nodes: &'a [Node],
    eps: f32,
    extremes: Vec<usize>,
}

impl<'a> ExtremeCollector<'a> {
    fn new(nodes: &'a [Node], root: usize, eps: f32) -> Self {
        let mut collector = Self {
            nodes,
            eps,
            extremes: Vec::new(),
        };
        if root < nodes.len() && nodes[root].children != 0 {
            collector.collect(root);
        }
        collector
    }

    fn calc_cond(&mut self, node: &Node) -> bool {
        let node_s: f32 = node.quad.size; // "s" stands for side or square

        //// Lenght variant. Because space in pixels is huge
        let koeff: f32 = node.mass / (node_s);
        //// Square variant. It's mean.
        // let koeff: f32 = node.mass / (node_s * node_s);

        return (koeff < self.eps);
        // return (koeff * koeff < eps);
    }

    /// Запуск сбора для каждого из 4-х непосредственных потомков корня
    fn collect(&mut self, root: usize) {
        let root_node = &self.nodes[root];
        let root_center = root_node.quad.center;
        let mut child_idx = root_node.children;
        while child_idx != 0 {
            let child = &self.nodes[child_idx];
            let is_right = child.quad.center.x > root_center.x;
            let is_top = child.quad.center.y > root_center.y;

            self.collect_lvl1(child_idx, (is_right, is_top));
            child_idx = child.next;
        }
    }
    /// Рекурсивный сбор всех крайних узлов в заданном направлении
    fn collect_lvl1(
        &mut self,
        node_idx: usize,
        direction: (bool, bool), // (is_right, is_top)
    ) {
        let node = &self.nodes[node_idx];
        if self.calc_cond(node) {
            self.extremes.push(node_idx);
        }

        let mut child_idx = node.children;

        while child_idx != 0 {
            let child = &self.nodes[child_idx];

            let right_cond = child.quad.center.x > node.quad.center.x;
            let top_cond = child.quad.center.y > node.quad.center.y;

            if right_cond == direction.0 && top_cond == direction.1 {
                self.collect_lvl1(child_idx, direction);
                break;
            } else if right_cond == direction.0 || top_cond == direction.1 {
                let side = if right_cond == direction.0 {
                    (direction.0, true) // (положительное_направление, ось_X)
                } else {
                    (direction.1, false) // ось_Y
                };
                self.collect_lvl2(child_idx, side);
                break;
            }
            child_idx = child.next;
        }
    }

    fn collect_lvl2(
        &mut self,
        node_idx: usize,
        side: (bool, bool), // (positive, is_x_axis)
    ) {
        let node = &self.nodes[node_idx];
        if self.calc_cond(node) {
            // println!("Коэфф: {}, индекс {}", koeff, node_idx);
            self.extremes.push(node_idx);
        }
        let (positive, is_x) = side;
        let mut child_idx = node.children;

        while child_idx != 0 {
            let child = &self.nodes[child_idx];

            let cond = if is_x {
                child.quad.center.x > node.quad.center.x // right_cond
            } else {
                child.quad.center.y > node.quad.center.y // top_cond
            };
            if cond == positive {
                self.collect_lvl2(child_idx, side);
            }
            child_idx = child.next;
        }
    }

    fn into_extremes(self) -> Vec<usize> {
        self.extremes
    }
}
