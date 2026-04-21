// ──────────────────────────────────────────────────────────────────────────────
// Perimeter — shells de perímetro mediante offset de polígono (Clipper2)
//
// Para n_perimetros=2 y un cuadrado:
//   shell[0] = contorno exterior original
//   shell[1] = contorno desplazado -1×ancho hacia adentro
//
// Usa clipper2 para el offsetting. La función `inflate` con delta negativo
// encoge el polígono (offset hacia adentro = perimetros interiores).
// ──────────────────────────────────────────────────────────────────────────────

use clipper2::{inflate, simplify, EndType, JoinType, Paths};
use super::Contorno;

/// Genera `n` shells de perímetro para un contorno dado.
///
/// `shells[0]` = contorno exterior (copia del original).
/// `shells[1]` = primer shell interior (offset de -1×ancho_linea).
/// `shells[n-1]` = shell más interior.
///
/// Si `n = 0` devuelve un Vec vacío.
/// Si el contorno es demasiado pequeño para encogerse, los shells interiores
/// no se agregan (clipper2 los descarta automáticamente).
pub fn generar_shells(contorno: &Contorno, n: u32, ancho_linea: f32) -> Vec<Contorno> {
    if n == 0 || contorno.len() < 3 {
        return Vec::new();
    }

    let mut shells: Vec<Contorno> = Vec::with_capacity(n as usize);
    shells.push(contorno.clone()); // shell[0] = exterior

    if n == 1 {
        return shells;
    }

    // Pre-convertir el contorno a formato clipper2 (Vec de tuplas f64)
    let pts_f64: Vec<(f64, f64)> = contorno
        .iter()
        .map(|p| (p[0] as f64, p[1] as f64))
        .collect();

    for i in 1..n {
        let delta = -(i as f64 * ancho_linea as f64);

        // Convertir a Paths (un solo camino cerrado)
        let paths: Paths = pts_f64.clone().into();

        // Offset hacia adentro
        let resultado = inflate(paths, delta, JoinType::Miter, EndType::Polygon, 2.0);
        // Simplificar para eliminar puntos extra que introduce inflate
        let resultado = simplify(resultado, 0.005, false);

        // Convertir de vuelta a nuestros Contornos
        let coords: Vec<Vec<(f64, f64)>> = resultado.into();
        for pts in coords {
            if pts.len() >= 3 {
                let shell: Contorno = pts
                    .iter()
                    .map(|&(x, y)| [x as f32, y as f32])
                    .collect();
                shells.push(shell);
            }
        }
    }

    shells
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Cuadrado de lado 2 centrado en el origen.
    fn cuadrado_2x2() -> Contorno {
        vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
    }

    #[test]
    fn cero_perimetros_devuelve_vacio() {
        let shells = generar_shells(&cuadrado_2x2(), 0, 0.4);
        assert!(shells.is_empty());
    }

    #[test]
    fn un_perimetro_devuelve_solo_exterior() {
        let contorno = cuadrado_2x2();
        let shells = generar_shells(&contorno, 1, 0.4);
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].len(), contorno.len(), "El exterior debe ser idéntico al original");
    }

    #[test]
    fn dos_perimetros_produce_dos_shells() {
        let shells = generar_shells(&cuadrado_2x2(), 2, 0.4);
        assert_eq!(shells.len(), 2, "Deben generarse 2 shells");
    }

    #[test]
    fn shell_interior_es_mas_pequeno() {
        let shells = generar_shells(&cuadrado_2x2(), 2, 0.4);
        assert_eq!(shells.len(), 2);

        // El bounding box del shell interior debe ser más pequeño
        let bbox = |shell: &Contorno| -> (f32, f32, f32, f32) {
            let min_x = shell.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
            let max_x = shell.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
            let min_y = shell.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
            let max_y = shell.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            (min_x, max_x, min_y, max_y)
        };

        let (ox0, ox1, oy0, oy1) = bbox(&shells[0]);
        let (ix0, ix1, iy0, iy1) = bbox(&shells[1]);

        assert!(ix0 > ox0, "Shell interior debe tener min_x mayor");
        assert!(ix1 < ox1, "Shell interior debe tener max_x menor");
        assert!(iy0 > oy0, "Shell interior debe tener min_y mayor");
        assert!(iy1 < oy1, "Shell interior debe tener max_y menor");
    }

    #[test]
    fn contorno_muy_pequeno_no_genera_shell_interior() {
        // Cuadrado de 0.2mm de lado: no cabe un shell de 0.4mm de ancho
        let pequeño: Contorno = vec![
            [0.0, 0.0], [0.2, 0.0], [0.2, 0.2], [0.0, 0.2],
        ];
        let shells = generar_shells(&pequeño, 2, 0.4);
        // El shell[0] siempre existe; shell[1] no debe generarse si no cabe
        assert_eq!(shells.len(), 1,
            "Un contorno muy pequeño no debe generar shell interior");
    }

    #[test]
    fn tres_perimetros_cada_uno_mas_pequeno() {
        let shells = generar_shells(&cuadrado_2x2(), 3, 0.3);
        assert_eq!(shells.len(), 3);

        // Verificar que cada shell es estrictamente más pequeño que el anterior
        for i in 1..shells.len() {
            let area_prev = area_aproximada(&shells[i - 1]);
            let area_curr = area_aproximada(&shells[i]);
            assert!(area_curr < area_prev,
                "Shell {} debe ser más pequeño que shell {}", i, i - 1);
        }
    }

    /// Área aproximada de un polígono (fórmula del trapecio / shoelace).
    fn area_aproximada(contorno: &Contorno) -> f32 {
        let n = contorno.len();
        let mut area = 0.0_f32;
        for i in 0..n {
            let j = (i + 1) % n;
            area += contorno[i][0] * contorno[j][1];
            area -= contorno[j][0] * contorno[i][1];
        }
        (area / 2.0).abs()
    }
}
