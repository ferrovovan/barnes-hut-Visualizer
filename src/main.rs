use std::sync::atomic::Ordering;

mod body;
mod gui_state;
mod language;
mod quadtree;
mod renderer;
mod scenario_config;
mod simulation;
mod utils;
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
            //// Если GUI передал нам новый пресет
            if let Some((new_bodies, _theta, _epsilon, dt)) = renderer::RESET_BODIES.lock().take() {
                simulation.bodies = new_bodies;
                simulation.dt = dt;

                render(&mut simulation); // поднимет UPDATE_LOCK
            }

            if renderer::PAUSED.load(Ordering::Relaxed) {
                //// Минимальная нагрузка ЦП, но "разогревается".
                std::thread::sleep(std::time::Duration::from_millis(50));
                //// Лишняя нагрузка, но мнгновенный запуск.
                // std::thread::yield_now();
            } else {
                if !simulation.bodies.is_empty() {
                    simulation.is_cleaning = renderer::CLEANING.load(Ordering::Relaxed); // not optimized
                    simulation.step();
                    render(&mut simulation);
                }
            }
        }
    });

    quarkstrom::run::<Renderer>(config);
}

fn render(simulation: &mut Simulation) {
    let mut lock = renderer::UPDATE_LOCK.lock();
    for body in renderer::SPAWN.lock().drain(..) {
        simulation.bodies.push(body);
    }
    {
        let mut lock = renderer::BODIES.lock();
        lock.clear();
        lock.extend_from_slice(&simulation.bodies);
    }
    {
        let mut lock = renderer::QUADTREE.lock();
        lock.clear();
        lock.extend_from_slice(&simulation.quadtree.nodes);
    }
    *lock = true;
}
