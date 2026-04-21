// ──────────────────────────────────────────────────────────────────────────────
// G-code generator
//
// Itera sobre las capas y sus shells de perímetro (exterior primero, interiores
// después) y genera G-code compatible con Marlin / Klipper.
//
// Convenciones:
//   G21 — mm,  G90 — absoluto,  M82 — extrusión absoluta
//   Velocidades en mm/min (F = vel_mm_s × 60)
// ──────────────────────────────────────────────────────────────────────────────

use super::{calc_extrusion_mm, ResultadoSlicing, SlicerConfig};

/// Genera el G-code completo para el resultado de slicing dado.
pub fn generar_gcode(resultado: &ResultadoSlicing, config: &SlicerConfig) -> String {
    let puntos_totales: usize = resultado.capas.iter()
        .flat_map(|c| c.perimetros.iter())
        .flat_map(|isla| isla.iter())
        .map(|shell| shell.len())
        .sum();

    let mut out = String::with_capacity(puntos_totales * 80 + 2048);

    escribir_header(&mut out, resultado, config);
    escribir_start_gcode(&mut out, config);
    escribir_capas(&mut out, resultado, config);
    escribir_end_gcode(&mut out);

    out
}

// ── Secciones ─────────────────────────────────────────────────────────────────

fn escribir_header(out: &mut String, r: &ResultadoSlicing, cfg: &SlicerConfig) {
    let h = r.tiempo_min as u32 / 60;
    let m = r.tiempo_min as u32 % 60;

    let radio   = cfg.diametro_filamento as f64 / 2.0;
    let vol_cm3 = r.filamento_mm * std::f64::consts::PI * radio * radio / 1000.0;
    let gramos  = vol_cm3 * 1.24; // referencia PLA

    out.push_str("; Generado por CondorLab Slicer\n");
    out.push_str(&format!("; Archivo           : {}\n", cfg.nombre_archivo));
    out.push_str(&format!("; Capas             : {}\n", r.capas.len()));
    out.push_str(&format!("; Altura de capa    : {:.3} mm\n", cfg.altura_capa));
    out.push_str(&format!("; Perímetros/isla   : {}\n", cfg.n_perimetros));
    out.push_str(&format!("; Boquilla          : {:.2} mm\n", cfg.diametro_boquilla));
    out.push_str(&format!("; Filamento         : {:.2} mm diámetro\n", cfg.diametro_filamento));
    out.push_str(&format!("; Temp hotend       : {}°C\n", cfg.temp_hotend));
    out.push_str(&format!("; Temp cama         : {}°C\n", cfg.temp_cama));
    out.push_str(&format!("; Vel. impresión    : {:.0} mm/s\n", cfg.velocidad_impresion));
    out.push_str(&format!("; Vel. viaje        : {:.0} mm/s\n", cfg.velocidad_viaje));
    out.push_str(&format!("; Tiempo estimado   : {}h {:02}m\n", h, m));
    out.push_str(&format!("; Filamento (est.)  : {:.1} mm  /  {:.2} g (ref. PLA)\n",
        r.filamento_mm, gramos));
    out.push('\n');
}

fn escribir_start_gcode(out: &mut String, cfg: &SlicerConfig) {
    out.push_str("; === INICIO ===\n");
    out.push_str("G21        ; unidades en mm\n");
    out.push_str("G90        ; posicionamiento absoluto\n");
    out.push_str("M82        ; extrusión absoluta\n");
    out.push_str(&format!("M140 S{}   ; temperatura cama (no bloquea)\n", cfg.temp_cama));
    out.push_str(&format!("M104 S{}  ; temperatura hotend (no bloquea)\n", cfg.temp_hotend));
    out.push_str(&format!("M190 S{}   ; esperar cama\n", cfg.temp_cama));
    out.push_str(&format!("M109 S{}  ; esperar hotend\n", cfg.temp_hotend));
    out.push_str("G28        ; home todos los ejes\n");
    out.push_str("G92 E0     ; reset extrusor\n");
    out.push_str("; --- Línea de purga ---\n");
    out.push_str("G1 Z0.30 F3000\n");
    out.push_str("G1 X10.0 Y20.0 F5000\n");
    out.push_str("G1 X10.0 Y190.0 E14.0 F1500\n");
    out.push_str("G92 E0\n\n");
}

fn escribir_capas(out: &mut String, resultado: &ResultadoSlicing, cfg: &SlicerConfig) {
    let vel_print = cfg.velocidad_impresion * 60.0; // mm/s → mm/min
    let vel_viaje = cfg.velocidad_viaje     * 60.0;
    let num_capas = resultado.capas.len();

    let mut e:   f64           = 0.0;   // extrusión acumulada (modo absoluto M82)
    let mut pos: Option<[f32; 2]> = None; // posición actual XY

    for (n_capa, capa) in resultado.capas.iter().enumerate() {
        out.push_str(&format!("\n; --- Capa {}/{} · z={:.3} mm ---\n",
            n_capa + 1, num_capas, capa.z));

        out.push_str(&format!("G0 Z{:.3} F3000\n", capa.z));

        // Iterar islas y sus shells: exterior primero, interiores después
        for isla in &capa.perimetros {
            for shell in isla {
                if shell.len() < 2 {
                    continue;
                }

                let primer_pt = shell[0];

                // Viaje al inicio del shell
                if pos.map_or(true, |p| distancia_2d(p, primer_pt) > 0.5) {
                    out.push_str(&format!(
                        "G0 X{:.3} Y{:.3} F{:.0}\n",
                        primer_pt[0], primer_pt[1], vel_viaje,
                    ));
                }

                // Trazar el shell cerrando el loop
                let n_pts = shell.len();
                for i in 1..=n_pts {
                    let desde = shell[(i - 1) % n_pts];
                    let hasta = shell[i % n_pts];
                    let d = distancia_2d(desde, hasta);
                    if d < 0.01 { continue; }

                    e += calc_extrusion_mm(d, cfg) as f64;
                    out.push_str(&format!(
                        "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                        hasta[0], hasta[1], e, vel_print,
                    ));
                }
                pos = Some(shell[n_pts - 1]);
            }
        }
    }
}

fn escribir_end_gcode(out: &mut String) {
    out.push_str("\n; === FIN ===\n");
    out.push_str("G91          ; relativo\n");
    out.push_str("G1 E-2 F2700 ; retracción\n");
    out.push_str("G1 Z10 F3000 ; levantar boquilla\n");
    out.push_str("G90          ; volver a absoluto\n");
    out.push_str("G28 X Y      ; home XY\n");
    out.push_str("M104 S0      ; apagar hotend\n");
    out.push_str("M140 S0      ; apagar cama\n");
    out.push_str("M84          ; apagar motores\n");
}

// ── Helpers ────────────────────────────────────────────────────────────────────

#[inline]
fn distancia_2d(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}
