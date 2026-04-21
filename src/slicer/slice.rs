// ──────────────────────────────────────────────────────────────────────────────
// Slicing — intersección de triángulos con planos horizontales
// ──────────────────────────────────────────────────────────────────────────────

/// Un segmento 2D: dos puntos [x, y].
pub type Segmento = [[f32; 2]; 2];

/// Intersecta todos los triángulos del mesh con el plano `z = z_capa`.
/// Devuelve los segmentos 2D resultantes (sin ordenar).
pub fn cortar_capa(tris: &[[[f32; 3]; 3]], z_capa: f32) -> Vec<Segmento> {
    tris.iter()
        .filter_map(|tri| intersectar_triangulo(tri, z_capa))
        .collect()
}

/// Intersecta un triángulo con el plano z = `z`.
///
/// Retorna `None` si el triángulo no cruza el plano (todos los vértices
/// están de un mismo lado, o el triángulo es coplanar).
/// Retorna `Some([P0, P1])` con los dos puntos de intersección.
pub(crate) fn intersectar_triangulo(tri: &[[f32; 3]; 3], z: f32) -> Option<Segmento> {
    let [v0, v1, v2] = tri;

    // Distancia con signo de cada vértice al plano
    let d0 = v0[2] - z;
    let d1 = v1[2] - z;
    let d2 = v2[2] - z;

    // Sin intersección si todos están del mismo lado (o todos en el plano)
    let arriba = (d0 > 0.0) as u8 + (d1 > 0.0) as u8 + (d2 > 0.0) as u8;
    let abajo  = (d0 < 0.0) as u8 + (d1 < 0.0) as u8 + (d2 < 0.0) as u8;
    if arriba == 0 || abajo == 0 {
        return None;
    }

    // Buscar puntos de intersección en las 3 aristas
    let aristas = [
        (v0, v1, d0, d1),
        (v1, v2, d1, d2),
        (v2, v0, d2, d0),
    ];

    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(2);

    for (va, vb, da, db) in &aristas {
        // La arista cruza solo si los signos son estrictamente distintos
        if (da < &0.0) != (db < &0.0) {
            // t: parámetro de interpolación tal que z_a + t*(z_b - z_a) = z
            let t = da / (da - db);
            pts.push([
                va[0] + t * (vb[0] - va[0]),
                va[1] + t * (vb[1] - va[1]),
            ]);
        }
    }

    // Solo emitimos segmentos con exactamente 2 puntos
    if pts.len() == 2 { Some([pts[0], pts[1]]) } else { None }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn approx_eq(a: f32, b: f32) -> bool { (a - b).abs() < EPS }
    fn pt_approx(p: [f32; 2], x: f32, y: f32) -> bool {
        approx_eq(p[0], x) && approx_eq(p[1], y)
    }

    // ── intersectar_triangulo ─────────────────────────────────────────────────

    #[test]
    fn triangulo_cruza_plano_produce_segmento() {
        // v0 debajo, v1 y v2 arriba
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];
        assert!(intersectar_triangulo(&tri, 0.5).is_some());
    }

    #[test]
    fn triangulo_sobre_plano_no_intersecta() {
        let tri = [[0.0, 0.0, 1.0], [1.0, 0.0, 2.0], [0.0, 1.0, 3.0]];
        assert!(intersectar_triangulo(&tri, 0.5).is_none());
    }

    #[test]
    fn triangulo_bajo_plano_no_intersecta() {
        let tri = [[0.0, 0.0, -1.0], [1.0, 0.0, -0.5], [0.0, 1.0, -0.1]];
        assert!(intersectar_triangulo(&tri, 0.5).is_none());
    }

    #[test]
    fn triangulo_coplanar_no_intersecta() {
        // Todos en z=0.5 → ni arriba ni abajo → None
        let tri = [[0.0, 0.0, 0.5], [1.0, 0.0, 0.5], [0.0, 1.0, 0.5]];
        assert!(intersectar_triangulo(&tri, 0.5).is_none());
    }

    #[test]
    fn puntos_de_interseccion_son_correctos() {
        // Triángulo: v0=(0,0,0), v1=(2,0,2), v2=(0,2,2)
        // A z=1.0 los cortes de arista son:
        //   arista v0-v1: t=0.5 → (1.0, 0.0)
        //   arista v0-v2: t=0.5 → (0.0, 1.0)
        let tri = [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 2.0], [0.0, 2.0, 2.0]];
        let seg = intersectar_triangulo(&tri, 1.0).expect("Debe intersectar");
        let pts = [seg[0], seg[1]];
        assert!(
            pts.iter().any(|&p| pt_approx(p, 1.0, 0.0)),
            "Falta punto (1.0, 0.0): {:?}", pts,
        );
        assert!(
            pts.iter().any(|&p| pt_approx(p, 0.0, 1.0)),
            "Falta punto (0.0, 1.0): {:?}", pts,
        );
    }

    #[test]
    fn vertice_en_plano_con_los_otros_dos_debajo_no_intersecta() {
        // v0 en el plano, v1 y v2 debajo → arriba=0 → None
        let tri = [[0.5_f32, 0.0, 0.5], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // d0=0, d1=-0.5, d2=-0.5 → arriba=0 → None
        assert!(intersectar_triangulo(&tri, 0.5).is_none());
    }

    #[test]
    fn vertice_en_plano_con_otros_dos_en_lados_opuestos_produce_segmento() {
        // v2=(0.5,0.5,0.5) en el plano; v0 debajo, v1 arriba → 2 pts → Some
        let tri = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 1.0], [0.5, 0.5, 0.5]];
        assert!(intersectar_triangulo(&tri, 0.5).is_some());
    }

    // ── cortar_capa ───────────────────────────────────────────────────────────

    #[test]
    fn mesh_vacio_produce_cero_segmentos() {
        assert!(cortar_capa(&[], 0.5).is_empty());
    }

    #[test]
    fn un_triangulo_que_cruza_produce_un_segmento() {
        let tri = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];
        assert_eq!(cortar_capa(&[tri], 0.5).len(), 1);
    }

    #[test]
    fn triangulos_que_no_cruzan_producen_cero_segmentos() {
        let tris = [
            [[0.0_f32, 0.0, 1.0], [1.0, 0.0, 2.0], [0.0, 1.0, 3.0]], // encima
            [[0.0_f32, 0.0, -1.0], [1.0, 0.0, -0.5], [0.0, 1.0, -0.1]], // debajo
        ];
        assert_eq!(cortar_capa(&tris, 0.5).len(), 0);
    }
}
