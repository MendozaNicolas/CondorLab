// ──────────────────────────────────────────────────────────────────────────────
// CondorLab Slicer — módulo principal
//
// Pipeline:
//   STL (triángulos)
//     └─ slice::cortar_capa()           → segmentos 2D por capa
//         └─ contour::construir()       → contornos exteriores (polígonos)
//             └─ perimeter::generar()   → shells de perímetro por isla
//                 └─ gcode::generar()   → G-code listo para imprimir
// ──────────────────────────────────────────────────────────────────────────────

mod contour;
mod slice;
pub mod gcode;
pub mod perimeter;

pub use gcode::generar_gcode;

// ── Configuración ──────────────────────────────────────────────────────────────

/// Parámetros que controlan el slicing y la generación de G-code.
#[derive(Debug, Clone)]
pub struct SlicerConfig {
    /// Altura de capa en mm (ej: 0.20)
    pub altura_capa: f32,
    /// Diámetro de la boquilla en mm (ej: 0.40)
    pub diametro_boquilla: f32,
    /// Diámetro del filamento en mm (1.75 o 2.85)
    pub diametro_filamento: f32,
    /// Número de shells de perímetro (1 = solo exterior, 2 = exterior + 1 interior, …)
    pub n_perimetros: u32,
    /// Velocidad de impresión en mm/s
    pub velocidad_impresion: f32,
    /// Velocidad de viaje (sin extrusión) en mm/s
    pub velocidad_viaje: f32,
    /// Temperatura del hotend en °C
    pub temp_hotend: u32,
    /// Temperatura de la cama en °C
    pub temp_cama: u32,
    /// Nombre base para el comentario en el G-code
    pub nombre_archivo: String,
}

impl Default for SlicerConfig {
    fn default() -> Self {
        Self {
            altura_capa:         0.20,
            diametro_boquilla:   0.40,
            diametro_filamento:  1.75,
            n_perimetros:        2,
            velocidad_impresion: 50.0,
            velocidad_viaje:     150.0,
            temp_hotend:         200,
            temp_cama:           60,
            nombre_archivo:      "output".to_string(),
        }
    }
}

// ── Tipos de datos ─────────────────────────────────────────────────────────────

/// Lista de puntos XY que forman un polígono cerrado.
pub type Contorno = Vec<[f32; 2]>;

/// Una capa horizontal del modelo.
///
/// `perimetros[i]` = shells de la isla i, de exterior a interior.
/// `perimetros[i][0]` = contorno exterior de la isla.
/// `perimetros[i][1]` = primer shell interior, etc.
#[derive(Debug, Clone)]
pub struct Capa {
    /// Altura Z en mm.
    pub z: f32,
    /// Shells de perímetro agrupados por isla.
    pub perimetros: Vec<Vec<Contorno>>,
}

/// Resultado completo del proceso de slicing.
#[derive(Debug, Clone)]
pub struct ResultadoSlicing {
    pub capas: Vec<Capa>,
    /// Longitud total de filamento estimada en mm.
    pub filamento_mm: f64,
    /// Tiempo de impresión estimado en minutos.
    pub tiempo_min: f64,
}

// ── API pública ────────────────────────────────────────────────────────────────

/// Slicea un mesh STL y devuelve las capas con sus shells de perímetro.
pub fn slicear(tris: &[[[f32; 3]; 3]], config: &SlicerConfig) -> ResultadoSlicing {
    if tris.is_empty() {
        return ResultadoSlicing { capas: Vec::new(), filamento_mm: 0.0, tiempo_min: 0.0 };
    }

    let z_min = tris.iter().flat_map(|t| t.iter()).map(|v| v[2]).fold(f32::MAX, f32::min);
    let z_max = tris.iter().flat_map(|t| t.iter()).map(|v| v[2]).fold(f32::MIN, f32::max);

    let h         = config.altura_capa;
    let num_capas = ((z_max - z_min) / h).ceil() as usize;

    let mut capas        = Vec::with_capacity(num_capas);
    let mut filamento_mm = 0.0f64;
    let mut tiempo_s     = 0.0f64;

    for i in 0..num_capas {
        let z = (z_min + (i as f32 + 1.0) * h).min(z_max);

        let segmentos = slice::cortar_capa(tris, z);
        let contornos = contour::construir_contornos(segmentos);

        if contornos.is_empty() {
            continue;
        }

        // Generar shells de perímetro para cada isla
        let perimetros: Vec<Vec<Contorno>> = contornos
            .iter()
            .map(|c| perimeter::generar_shells(c, config.n_perimetros, config.diametro_boquilla))
            .collect();

        // Estimar filamento y tiempo usando solo el shell exterior de cada isla
        for isla in &perimetros {
            if let Some(exterior) = isla.first() {
                let perim = perimetro_2d(exterior);
                // Multiplicar por número de shells como aproximación del total
                let factor = isla.len() as f64;
                filamento_mm += calc_extrusion_mm(perim as f32, config) as f64 * factor;
                tiempo_s     += (perim / config.velocidad_impresion as f64) * factor;
            }
        }
        // Overhead por cambio de capa
        tiempo_s += 2.0;

        capas.push(Capa { z, perimetros });
    }

    ResultadoSlicing {
        capas,
        filamento_mm,
        tiempo_min: tiempo_s / 60.0,
    }
}

// ── Helpers internos ──────────────────────────────────────────────────────────

/// Longitud de filamento para extrudir una distancia dada.
pub(crate) fn calc_extrusion_mm(distancia: f32, cfg: &SlicerConfig) -> f32 {
    let vol   = distancia * cfg.diametro_boquilla * cfg.altura_capa;
    let radio = cfg.diametro_filamento / 2.0;
    vol / (std::f32::consts::PI * radio * radio)
}

/// Perímetro de un contorno cerrado 2D.
fn perimetro_2d(contorno: &[[f32; 2]]) -> f64 {
    let n = contorno.len();
    (0..n).map(|i| {
        let a = contorno[i];
        let b = contorno[(i + 1) % n];
        let dx = (b[0] - a[0]) as f64;
        let dy = (b[1] - a[1]) as f64;
        (dx * dx + dy * dy).sqrt()
    }).sum()
}

// ── Tests de integración ───────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Cubo unitario con base en z=0 y techo en z=1, centrado en XY.
    /// 12 triángulos (6 caras × 2 triángulos).
    pub fn cubo_unitario() -> Vec<[[f32; 3]; 3]> {
        vec![
            // Cara inferior (z=0)
            [[-0.5, -0.5, 0.0], [ 0.5, -0.5, 0.0], [ 0.5,  0.5, 0.0]],
            [[-0.5, -0.5, 0.0], [ 0.5,  0.5, 0.0], [-0.5,  0.5, 0.0]],
            // Cara superior (z=1)
            [[-0.5, -0.5, 1.0], [ 0.5,  0.5, 1.0], [ 0.5, -0.5, 1.0]],
            [[-0.5, -0.5, 1.0], [-0.5,  0.5, 1.0], [ 0.5,  0.5, 1.0]],
            // Cara frontal (y=-0.5)
            [[-0.5, -0.5, 0.0], [ 0.5, -0.5, 0.0], [ 0.5, -0.5, 1.0]],
            [[-0.5, -0.5, 0.0], [ 0.5, -0.5, 1.0], [-0.5, -0.5, 1.0]],
            // Cara trasera (y=0.5)
            [[-0.5,  0.5, 0.0], [ 0.5,  0.5, 1.0], [ 0.5,  0.5, 0.0]],
            [[-0.5,  0.5, 0.0], [-0.5,  0.5, 1.0], [ 0.5,  0.5, 1.0]],
            // Cara izquierda (x=-0.5)
            [[-0.5, -0.5, 0.0], [-0.5,  0.5, 0.0], [-0.5,  0.5, 1.0]],
            [[-0.5, -0.5, 0.0], [-0.5,  0.5, 1.0], [-0.5, -0.5, 1.0]],
            // Cara derecha (x=0.5)
            [[ 0.5, -0.5, 0.0], [ 0.5,  0.5, 1.0], [ 0.5,  0.5, 0.0]],
            [[ 0.5, -0.5, 0.0], [ 0.5, -0.5, 1.0], [ 0.5,  0.5, 1.0]],
        ]
    }

    #[test]
    fn mesh_vacio_devuelve_resultado_vacio() {
        let config = SlicerConfig::default();
        let r = slicear(&[], &config);
        assert!(r.capas.is_empty());
        assert_eq!(r.filamento_mm, 0.0);
        assert_eq!(r.tiempo_min, 0.0);
    }

    #[test]
    fn cubo_numero_de_capas_correcto() {
        // Cubo 1mm alto, capa 0.2mm → 5 planos (z=0.2..1.0),
        // pero z=1.0 no produce contornos (plano tangente al techo) → 4 capas.
        let config = SlicerConfig { altura_capa: 0.2, n_perimetros: 1, ..SlicerConfig::default() };
        let r = slicear(&cubo_unitario(), &config);
        assert_eq!(r.capas.len(), 4, "Se esperaban 4 capas, se obtuvieron {}", r.capas.len());
    }

    #[test]
    fn cubo_cada_capa_tiene_una_isla() {
        let config = SlicerConfig { n_perimetros: 1, ..SlicerConfig::default() };
        let r = slicear(&cubo_unitario(), &config);
        for (i, capa) in r.capas.iter().enumerate() {
            assert_eq!(capa.perimetros.len(), 1,
                "Capa {}: se esperaba 1 isla, hay {}", i, capa.perimetros.len());
        }
    }

    #[test]
    fn cubo_exterior_tiene_puntos_en_borde() {
        // El shell exterior de cada capa debe tener todos sus puntos
        // con |x| ≈ 0.5 o |y| ≈ 0.5 (en el borde del cubo).
        let config = SlicerConfig { n_perimetros: 1, ..SlicerConfig::default() };
        let r = slicear(&cubo_unitario(), &config);

        for (i, capa) in r.capas.iter().enumerate() {
            let exterior = &capa.perimetros[0][0];
            assert!(exterior.len() >= 4,
                "Capa {}: el contorno exterior debe tener al menos 4 puntos", i);

            for pt in exterior {
                let en_borde = (pt[0].abs() - 0.5).abs() < 0.01
                            || (pt[1].abs() - 0.5).abs() < 0.01;
                assert!(en_borde,
                    "Capa {}: punto {:?} no está en el borde del cubo", i, pt);
            }
        }
    }

    #[test]
    fn cubo_estimaciones_positivas() {
        let config = SlicerConfig::default();
        let r = slicear(&cubo_unitario(), &config);
        assert!(r.filamento_mm > 0.0, "El filamento estimado debe ser > 0");
        assert!(r.tiempo_min   > 0.0, "El tiempo estimado debe ser > 0");
    }

    #[test]
    fn alturas_de_capa_crecientes() {
        let config = SlicerConfig { n_perimetros: 1, ..SlicerConfig::default() };
        let r = slicear(&cubo_unitario(), &config);
        let zs: Vec<f32> = r.capas.iter().map(|c| c.z).collect();
        for w in zs.windows(2) {
            assert!(w[1] > w[0], "Las alturas de capa deben ser estrictamente crecientes");
        }
    }
}
