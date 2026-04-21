// ──────────────────────────────────────────────────────────────────────────────
// Infill — relleno rectilíneo con patrón serpentín
//
// Algoritmo scanline:
//   1. Rota el contorno −angulo para convertir las líneas diagonales en horizontales.
//   2. Intersecta líneas y=const con todos los bordes del contorno.
//   3. Empareja intersecciones y alterna la dirección (patrón serpentín).
//   4. Rota los segmentos de vuelta al espacio original.
//
// El patrón serpentín minimiza los viajes: la línea N termina en el mismo
// lado por donde empieza la línea N+1.
// ──────────────────────────────────────────────────────────────────────────────

use super::Contorno;

/// Genera segmentos de infill rectilíneo para un contorno cerrado.
///
/// `espaciado` — distancia entre líneas en mm.
/// `angulo_deg` — ángulo de las líneas en grados (0° = horizontal, 45° = diagonal).
///
/// Devuelve una lista de segmentos `[[inicio; 2]; [fin; 2]]` orientados en
/// patrón serpentín.
pub fn generar_infill(
    contorno: &Contorno,
    espaciado: f32,
    angulo_deg: f32,
) -> Vec<[[f32; 2]; 2]> {
    if contorno.len() < 3 || espaciado < 0.01 {
        return Vec::new();
    }

    let ang = angulo_deg.to_radians();
    let (ca, sa) = (ang.cos(), ang.sin());

    // Rotación hacia atrás: puntos del contorno → espacio alineado con X
    let rot: Vec<[f32; 2]> = contorno
        .iter()
        .map(|&[x, y]| [x * ca + y * sa, -x * sa + y * ca])
        .collect();

    let y_min = rot.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let y_max = rot.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    let n = rot.len();

    let mut segs: Vec<[[f32; 2]; 2]> = Vec::new();
    let mut linea = 0usize;
    let mut y = y_min + espaciado * 0.5;

    while y < y_max {
        let mut xs: Vec<f32> = Vec::new();

        // Intersectar y=y con cada arista del contorno (intervalo semi-abierto)
        for i in 0..n {
            let [x0, y0] = rot[i];
            let [x1, y1] = rot[(i + 1) % n];
            if (y0 < y && y <= y1) || (y1 < y && y <= y0) {
                let t = (y - y0) / (y1 - y0);
                xs.push(x0 + t * (x1 - x0));
            }
        }

        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Emparejar intersecciones: [x0,x1], [x2,x3], ... → segmentos de relleno
        let mut i = 0;
        while i + 1 < xs.len() {
            let xa = xs[i];
            let xb = xs[i + 1];
            if (xb - xa) > 0.01 {
                // Alternar dirección para el patrón serpentín
                let seg = if linea % 2 == 0 {
                    [[xa, y], [xb, y]]
                } else {
                    [[xb, y], [xa, y]]
                };
                segs.push(seg);
            }
            i += 2;
        }

        linea += 1;
        y += espaciado;
    }

    // Rotar los segmentos de vuelta al espacio original
    segs.iter_mut().for_each(|seg| {
        for p in seg.iter_mut() {
            let [x, ry] = *p;
            *p = [x * ca - ry * sa, x * sa + ry * ca];
        }
    });

    segs
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cuadrado_1x1() -> Contorno {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    #[test]
    fn contorno_vacio_devuelve_vacio() {
        assert!(generar_infill(&Vec::new(), 0.4, 0.0).is_empty());
    }

    #[test]
    fn espaciado_cero_devuelve_vacio() {
        assert!(generar_infill(&cuadrado_1x1(), 0.0, 0.0).is_empty());
    }

    #[test]
    fn cuadrado_produce_segmentos_horizontales() {
        let segs = generar_infill(&cuadrado_1x1(), 0.3, 0.0);
        assert!(!segs.is_empty(), "El cuadrado debe producir segmentos de infill");
    }

    #[test]
    fn cuadrado_produce_segmentos_diagonales() {
        let segs = generar_infill(&cuadrado_1x1(), 0.3, 45.0);
        assert!(!segs.is_empty(), "El infill a 45° debe producir segmentos");
    }

    #[test]
    fn segmentos_dentro_del_contorno() {
        let segs = generar_infill(&cuadrado_1x1(), 0.2, 0.0);
        for seg in &segs {
            for &[x, y] in seg {
                assert!(x >= -0.02 && x <= 1.02, "x={x:.3} fuera del cuadrado");
                assert!(y >= -0.02 && y <= 1.02, "y={y:.3} fuera del cuadrado");
            }
        }
    }

    #[test]
    fn mayor_espaciado_produce_menos_lineas() {
        let s1 = generar_infill(&cuadrado_1x1(), 0.2, 0.0);
        let s2 = generar_infill(&cuadrado_1x1(), 0.4, 0.0);
        assert!(s1.len() > s2.len(), "Mayor espaciado debe producir menos líneas");
    }

    #[test]
    fn patron_serpentin_alterna_direccion() {
        let segs = generar_infill(&cuadrado_1x1(), 0.25, 0.0);
        if segs.len() >= 2 {
            // Línea par: va de izq a der (x0 < x1)
            // Línea impar: va de der a izq (x0 > x1)
            let dir_0 = segs[0][1][0] - segs[0][0][0];
            let dir_1 = segs[1][1][0] - segs[1][0][0];
            assert!(dir_0 * dir_1 < 0.0, "Líneas consecutivas deben ir en sentidos opuestos");
        }
    }
}
