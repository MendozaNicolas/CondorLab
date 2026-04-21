// ──────────────────────────────────────────────────────────────────────────────
// Contour builder — une segmentos sueltos en polígonos cerrados
//
// Después de slicear, tenemos N segmentos sin ordenar. Esta etapa los conecta
// en cadenas cerradas (contornos). El algoritmo usa un mapa de adyacencia para
// seguir el "hilo" de segmento en segmento hasta cerrar cada loop.
//
// Tolerancia de snapping: 1 µm (0.001 mm). Los vértices de triángulos
// adyacentes deberían coincidir exactamente, pero por aritmética flotante
// pueden diferir en ulps. El snapping a grilla los fusiona.
// ──────────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use super::slice::Segmento;
use super::Contorno;

/// Resolución del grid de snapping en mm.
const SNAP_MM: f32 = 0.001;

// ── API ────────────────────────────────────────────────────────────────────────

/// Toma una lista de segmentos sin ordenar y devuelve contornos cerrados.
///
/// Algoritmo O(n) con mapa de adyacencia:
/// 1. Indexar cada extremo de cada segmento por su posición snapeada.
/// 2. Desde cada segmento no usado, seguir la cadena de extremo en extremo.
/// 3. Cuando no hay más segmentos disponibles, cerrar el contorno.
pub fn construir_contornos(segmentos: Vec<Segmento>) -> Vec<Contorno> {
    if segmentos.is_empty() {
        return Vec::new();
    }

    let n = segmentos.len();
    let mut usados = vec![false; n];

    // mapa: punto_snapeado → lista de (índice_seg, extremo: 0 o 1)
    let mut mapa: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::with_capacity(n * 2);
    for (i, seg) in segmentos.iter().enumerate() {
        for (j, pt) in seg.iter().enumerate() {
            mapa.entry(snap(*pt)).or_default().push((i, j));
        }
    }

    let mut contornos: Vec<Contorno> = Vec::new();

    for inicio in 0..n {
        if usados[inicio] {
            continue;
        }
        usados[inicio] = true;

        // Punto de partida: extremo 0 del primer segmento
        let mut contorno: Contorno = vec![segmentos[inicio][0]];
        // Extremo actual desde el que extendemos la cadena
        let mut extremo_actual = segmentos[inicio][1];

        loop {
            // Agregar el extremo actual al contorno
            contorno.push(extremo_actual);

            // Buscar el próximo segmento no usado que comparta este extremo
            let key = snap(extremo_actual);
            let siguiente = mapa
                .get(&key)
                .and_then(|lista| lista.iter().find(|&&(idx, _)| !usados[idx]).copied());

            match siguiente {
                Some((idx, extremo_entrada)) => {
                    usados[idx] = true;
                    // Atravesar el segmento hacia el extremo opuesto
                    extremo_actual = segmentos[idx][1 - extremo_entrada];
                }
                None => {
                    // Sin continuación: el loop terminó (cerrado o abierto)
                    break;
                }
            }
        }

        // Eliminar el último punto si duplica el primero (contorno cerrado)
        if let (Some(&primero), Some(&ultimo)) = (contorno.first(), contorno.last()) {
            if snap(primero) == snap(ultimo) {
                contorno.pop();
            }
        }

        // Solo guardar contornos con al menos 3 puntos
        if contorno.len() >= 3 {
            contornos.push(contorno);
        }
    }

    contornos
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Convierte un punto 2D a coordenadas enteras en la grilla de snapping.
#[inline]
fn snap(p: [f32; 2]) -> (i64, i64) {
    (
        (p[0] / SNAP_MM).round() as i64,
        (p[1] / SNAP_MM).round() as i64,
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(x0: f32, y0: f32, x1: f32, y1: f32) -> Segmento {
        [[x0, y0], [x1, y1]]
    }

    #[test]
    fn sin_segmentos_devuelve_vacio() {
        assert!(construir_contornos(vec![]).is_empty());
    }

    #[test]
    fn tres_segmentos_forman_triangulo_cerrado() {
        // Triángulo (0,0)→(1,0)→(0.5,1)→(0,0)
        let segs = vec![
            seg(0.0, 0.0, 1.0, 0.0),
            seg(1.0, 0.0, 0.5, 1.0),
            seg(0.5, 1.0, 0.0, 0.0),
        ];
        let cs = construir_contornos(segs);
        assert_eq!(cs.len(), 1, "Debe haber 1 contorno");
        assert_eq!(cs[0].len(), 3, "El triángulo debe tener 3 puntos");
    }

    #[test]
    fn cuatro_segmentos_forman_cuadrado_cerrado() {
        let segs = vec![
            seg(0.0, 0.0, 1.0, 0.0),
            seg(1.0, 0.0, 1.0, 1.0),
            seg(1.0, 1.0, 0.0, 1.0),
            seg(0.0, 1.0, 0.0, 0.0),
        ];
        let cs = construir_contornos(segs);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].len(), 4);
    }

    #[test]
    fn dos_triangulos_separados_forman_dos_contornos() {
        let segs = vec![
            // Triángulo A
            seg(0.0, 0.0, 1.0, 0.0),
            seg(1.0, 0.0, 0.5, 1.0),
            seg(0.5, 1.0, 0.0, 0.0),
            // Triángulo B (lejos)
            seg(10.0, 10.0, 11.0, 10.0),
            seg(11.0, 10.0, 10.5, 11.0),
            seg(10.5, 11.0, 10.0, 10.0),
        ];
        let cs = construir_contornos(segs);
        assert_eq!(cs.len(), 2, "Deben haber 2 contornos separados");
    }

    #[test]
    fn segmentos_invertidos_forman_contorno_correcto() {
        // Mismo cuadrado pero con algunos segmentos con orientación inversa
        let segs = vec![
            seg(0.0, 0.0, 1.0, 0.0),
            seg(1.0, 1.0, 1.0, 0.0), // invertido
            seg(1.0, 1.0, 0.0, 1.0),
            seg(0.0, 0.0, 0.0, 1.0), // invertido
        ];
        let cs = construir_contornos(segs);
        assert_eq!(cs.len(), 1, "Debe haber 1 contorno aunque los segmentos estén invertidos");
        assert_eq!(cs[0].len(), 4);
    }

    #[test]
    fn contorno_no_duplica_punto_de_cierre() {
        // El primer y último punto del contorno NO deben ser iguales
        let segs = vec![
            seg(0.0, 0.0, 1.0, 0.0),
            seg(1.0, 0.0, 0.5, 1.0),
            seg(0.5, 1.0, 0.0, 0.0),
        ];
        let cs = construir_contornos(segs);
        let c = &cs[0];
        assert_ne!(
            snap(*c.first().unwrap()),
            snap(*c.last().unwrap()),
            "El primer y último punto no deben ser duplicados"
        );
    }
}
