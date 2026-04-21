use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

// ──────────────────────────────────────────────────────────────────────────────
// COTIZACIÓN  (serializable para historial persistente)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Cotizacion {
    pub label:      String,
    pub precio:     f64,
    pub precio_usd: f64,
    pub costo_base: f64,
    #[serde(default)]
    pub gramos: f64,
    #[serde(default)]
    pub horas:  f64,
}

// ──────────────────────────────────────────────────────────────────────────────
// PERFILES DE IMPRESORA
// ──────────────────────────────────────────────────────────────────────────────

pub struct PerfilImpresora {
    pub nombre:    &'static str,
    pub watts:     f64,
    pub costo_ars: f64,
    pub vida_hs:   f64,
    pub mant_hr:   f64,
}

pub const PERFILES: &[PerfilImpresora] = &[
    PerfilImpresora { nombre: "Flashforge Adventurer 5M", watts: 220.0, costo_ars: 585_900.0, vida_hs: 3000.0, mant_hr: 300.0 },
    PerfilImpresora { nombre: "Creality Ender 3 V2",      watts: 180.0, costo_ars: 180_000.0, vida_hs: 2000.0, mant_hr: 200.0 },
    PerfilImpresora { nombre: "Prusa MK4",                watts: 200.0, costo_ars: 850_000.0, vida_hs: 5000.0, mant_hr: 250.0 },
    PerfilImpresora { nombre: "Bambu Lab A1 Mini",        watts: 170.0, costo_ars: 450_000.0, vida_hs: 3000.0, mant_hr: 250.0 },
    PerfilImpresora { nombre: "Creality K1",              watts: 350.0, costo_ars: 320_000.0, vida_hs: 2500.0, mant_hr: 220.0 },
    PerfilImpresora { nombre: "Personalizada",            watts:   0.0, costo_ars:       0.0, vida_hs:    0.0, mant_hr:   0.0 },
];

// ──────────────────────────────────────────────────────────────────────────────
// TIPOS DE FILAMENTO
// ──────────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct PerfilMaterial {
    pub nombre:        &'static str,
    pub densidad:      f64,     // g/cm³
    pub precio_ref_kg: f64,     // precio de referencia ARS
    pub temp_hotend:   u16,
    pub temp_cama:     u16,
    pub nota:          &'static str,
}

pub const MATERIALES: &[PerfilMaterial] = &[
    PerfilMaterial { nombre: "PLA",     densidad: 1.24, precio_ref_kg: 12_000.0, temp_hotend: 200, temp_cama:  60, nota: "Fácil de imprimir, biodegradable. Ideal para piezas estéticas." },
    PerfilMaterial { nombre: "PETG",    densidad: 1.27, precio_ref_kg: 15_000.0, temp_hotend: 235, temp_cama:  80, nota: "Resistente a humedad y golpes. Buena relación precio/durabilidad." },
    PerfilMaterial { nombre: "ABS",     densidad: 1.04, precio_ref_kg: 13_000.0, temp_hotend: 245, temp_cama: 100, nota: "Resistente al calor. Requiere recinto cerrado y buena ventilación." },
    PerfilMaterial { nombre: "TPU 95A", densidad: 1.20, precio_ref_kg: 22_000.0, temp_hotend: 230, temp_cama:  45, nota: "Flexible y elástico. Impresión lenta, retracción mínima." },
    PerfilMaterial { nombre: "ASA",     densidad: 1.07, precio_ref_kg: 18_000.0, temp_hotend: 250, temp_cama: 100, nota: "Como ABS pero resistente a UV. Ideal para piezas exteriores." },
    PerfilMaterial { nombre: "PETG-CF", densidad: 1.30, precio_ref_kg: 28_000.0, temp_hotend: 250, temp_cama:  85, nota: "Alta rigidez y resistencia. Desgasta boquillas de bronce." },
];

// ──────────────────────────────────────────────────────────────────────────────
// CONFIG PERSISTENTE
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub valores:     Vec<String>,
    pub perfil_idx:  usize,
    #[serde(default)]
    pub material_idx: usize,
    #[serde(default)]
    pub recientes:   Vec<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// RUTAS
// ──────────────────────────────────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".config")
        .join("condorlab")
}

fn config_path()    -> PathBuf { config_dir().join("config.json")    }
fn historial_path() -> PathBuf { config_dir().join("historial.json") }

fn ensure_dir() { let _ = fs::create_dir_all(config_dir()); }

// ──────────────────────────────────────────────────────────────────────────────
// API PÚBLICA
// ──────────────────────────────────────────────────────────────────────────────

pub fn guardar_config(cfg: &AppConfig) {
    ensure_dir();
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(config_path(), json);
    }
}

pub fn cargar_config() -> AppConfig {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn guardar_historial(historial: &[Cotizacion]) {
    ensure_dir();
    if let Ok(json) = serde_json::to_string_pretty(historial) {
        let _ = fs::write(historial_path(), json);
    }
}

pub fn cargar_historial() -> Vec<Cotizacion> {
    fs::read_to_string(historial_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
