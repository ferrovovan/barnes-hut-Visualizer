use crate::body::Body;

pub struct GuiState {
    pub names: Vec<Option<String>>,       // Массив имен, синхронизированный с симуляцией по индексам
    pub selected_body_index: Option<usize>, // Индекс объекта, на который кликнули
    pub camera_zoom: f32,                  // Текущий уровень зума камеры
}

impl GuiState {
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            selected_body_index: None,
            camera_zoom: 1.0,
        }
    }

    /// Проверяет, кликнул ли пользователь на какой-то космический объект
    pub fn handle_click(&mut self, mouse_world_pos: [f32; 2], bodies: &[Body]) {
        self.selected_body_index = None;
        
        for (i, body) in bodies.iter().enumerate() {
            if self.names[i].is_some() { // Проверяем только именованные объекты
                let dx = body.pos.x - mouse_world_pos[0];
                let dy = body.pos.y - mouse_world_pos[1];
                let distance = (dx * dx + dy * dy).sqrt();

                // Если кликнули внутри радиуса планеты (с учетом минимальной зоны клика в 10 пикселей)
                let click_radius = body.radius.max(10.0 / self.camera_zoom);
                if distance <= click_radius {
                    self.selected_body_index = Some(i);
                    break;
                }
            }
        }
    }

    /// Возвращает список имен и их экранных координат для отрисовки поверх симуляции
    pub fn get_visible_labels(&self, bodies: &[Body], mouse_world_pos: [f32; 2]) -> Vec<(String, [f32; 2], f32)> {
        let mut labels = Vec::new();

        for (i, body) in bodies.iter().enumerate() {
            if let Some(name) = &self.names[i] {
                let dx = body.pos.x - mouse_world_pos[0];
                let dy = body.pos.y - mouse_world_pos[1];
                let distance_to_mouse = (dx * dx + dy * dy).sqrt();

                // Условия показа текста:
                let is_hovered = distance_to_mouse <= body.radius.max(12.0 / self.camera_zoom);
                let is_selected = Some(i) == self.selected_body_index;
                let is_close_enough = self.camera_zoom > 3.5; // Текст виден автоматически при сильном зуме

                if is_hovered || is_selected || is_close_enough {
                    // Определяем размер шрифта динамически
                    let size = if is_selected {
                        22.0 // Выбранный объект — самый крупный текст
                    } else if is_hovered {
                        16.0 // При наведении чуть меньше
                    } else {
                        12.0 // При обычном зуме — аккуратный мелкий шрифт
                    };

                    labels.push((name.clone(), [body.pos.x, body.pos.y], size));
                }
            }
        }

        labels
    }
}