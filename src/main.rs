use std::sync::atomic::Ordering;
use std::fs;

mod body;
mod quadtree;
mod renderer;
mod simulation;
mod utils;
mod scenario_config;

use scenario_config::SimulationConfig;
use renderer::Renderer;
use simulation::Simulation;

fn main() {
    // 1. ЗАГРУЖАЕМ КОНФИГУРАЦИЮ (Пока напрямую из файла для тестов)
    let json_data = fs::read_to_string("presets/solar_system_full.json")
        .expect("Не удалось найти файл конфигурации!");
        
    let config: SimulationConfig = serde_json::from_str(&json_data)
        .expect("Ошибка парсинга JSON!");

    // 2. КОНФИГ ОКНА
    let window_config = quarkstrom::Config {
        window_mode: quarkstrom::WindowMode::Windowed(900, 900),
    };

    // 3. ИНИЦИАЛИЗАЦИЯ СИМУЛЯЦИИ
    let mut simulation = Simulation::new();
    
    // ВАЖНЫЙ МОМЕНТ: Скорее всего, Simulation::new() внутри себя 
    // по умолчанию генерирует 1 миллион частиц (старый хардкод).
    // Мы очищаем этот старый массив и вставляем наши объекты из JSON!
    simulation.bodies.clear();
    simulation.bodies = config.into_particles(); // Заполняем новыми телами

    println!("Запущена симуляция: {}", config.name);
    println!("Всего тел на экране: {}", simulation.bodies.len());

    // 4. ЗАПУСК ПОТОКА ФИЗИКИ
    std::thread::spawn(move || {
        loop {
            if renderer::PAUSED.load(Ordering::Relaxed) {
                std::thread::yield_now();
            } else {
                simulation.step();
            }
            render(&mut simulation);
        }
    });

    // 5. ЗАПУСК ОКНА
    quarkstrom::run::<Renderer>(window_config);
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
    *lock |= true;
}