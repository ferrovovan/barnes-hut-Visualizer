use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Контейнер для всех переводов (плоский словарь ключ → значение)
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct Translations {
    map: HashMap<String, String>,
}

impl Translations {
    /// Загружает переводы из YAML-строки (например, встроенной через `include_str!`)
    pub fn from_yaml(yaml_str: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml_str)
    }

    /// Загружает переводы из внешнего файла (например, `translations/ru.yaml`)
    pub fn from_file(path: &str) -> Result<Self, anyhow::Error> {
        let content = fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    /// Получить перевод по ключу, если ключ отсутствует — возвращает сам ключ
    pub fn get(&self, key: &str) -> String {
        self.map.get(key).cloned().unwrap_or_else(|| key.to_owned())
    }
}

/// Глобальный объект текущих переводов
pub static TRANSLATIONS: Lazy<Mutex<Translations>> = Lazy::new(|| {
    // По умолчанию встроен русский перевод (можно переключить позже)
    let default_ru = include_str!("../translations/russian.yaml");
    let translations =
        Translations::from_yaml(default_ru).expect("Ошибка парсинга встроенного russian.yaml");
    Mutex::new(translations)
});

/// Удобная функция для получения строки в текущей локали
pub fn t(key: &str) -> String {
    TRANSLATIONS.lock().get(key)
}

/// Смена языка (перезагружает переводы из файла)
pub fn set_language(lang_code: &str) -> Result<(), anyhow::Error> {
    let path = format!("translations/{}.yaml", lang_code);
    let new_translations = Translations::from_file(&path)?;
    *TRANSLATIONS.lock() = new_translations;
    Ok(())
}
