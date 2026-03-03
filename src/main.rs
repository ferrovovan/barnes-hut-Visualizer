use std::sync::atomic::Ordering;

mod body;
mod quadtree;
mod renderer;
mod simulation;
mod utils;
mod scenario_config;

use renderer::Renderer;
use simulation::Simulation;

fn main() {
    let config = quarkstrom::Config {
        window_mode: quarkstrom::WindowMode::Windowed(900, 900),
    };

    let mut simulation = Simulation::new();
    simulation.bodies.clear();

    renderer::PAUSED.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        loop {
            // Если GUI передал нам новый пресет
            if let Some((new_bodies, _theta, _epsilon, dt)) = renderer::RESET_BODIES.lock().take() {
                simulation.bodies = new_bodies;
                simulation.dt = dt;
                
                // Вызываем вашу функцию отрисовки для кадра №0 (Она поднимет UPDATE_LOCK!)
                render(&mut simulation);
            }

            if renderer::PAUSED.load(Ordering::Relaxed) {
                std::thread::yield_now();
            } else {
                if !simulation.bodies.is_empty() {
                    simulation.step();
                    render(&mut simulation);
                }
            }
        }
    });

    quarkstrom::run::<Renderer>(config);
}

// ВАША ОРИГИНАЛЬНАЯ ФУНКЦИЯ: Идеально синхронизирует физику и видеокарту
fn render(simulation: &mut Simulation) {
    let mut lock = renderer::UPDATE_LOCK.lock();
    for body in renderer::SPAWN.lock().drain(..) {
        simulation.bodies.push(body);
    }
    {
        let mut bodies_lock = renderer::BODIES.lock();
        bodies_lock.clear();
        bodies_lock.extend_from_slice(&simulation.bodies);
    }
    {
        let mut quadtree_lock = renderer::QUADTREE.lock();
        quadtree_lock.clear();
        quadtree_lock.extend_from_slice(&simulation.quadtree.nodes);
    }
    *lock = true; // ВОТ ОН, ФИКС ЧЕРНОГО ЭКРАНА! Говорим рендереру: "Данные готовы, рисуй!"
}