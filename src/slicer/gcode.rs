// ──────────────────────────────────────────────────────────────────────────────
// G-code generator
//
// Itera sobre las capas, sus shells de perímetro y los segmentos de infill,
// y genera G-code compatible con Marlin / Klipper.
//
// Convenciones:
//   G21 — mm,  G90 — absoluto,  M82 — extrusión absoluta
//   Velocidades en mm/min (F = vel_mm_s × 60)
//
// Funcionalidades:
//   - Primera capa al 50% de velocidad para mejor adhesión
//   - Ventilador (M106 S255) desde la segunda capa
//   - Retracción / unretracción en viajes largos
//   - Infill después de los perímetros de cada capa
// ──────────────────────────────────────────────────────────────────────────────

use super::{calc_extrusion_mm, ResultadoSlicing, SlicerConfig};

/// Genera el G-code completo para el resultado de slicing dado.
pub fn generar_gcode(resultado: &ResultadoSlicing, config: &SlicerConfig) -> String {
    let pts_perim: usize = resultado.capas.iter()
        .flat_map(|c| c.perimetros.iter())
        .flat_map(|isla| isla.iter())
        .map(|shell| shell.len())
        .sum();
    let pts_infill: usize = resultado.capas.iter()
        .flat_map(|c| c.infill.iter())
        .map(|isla| isla.len())
        .sum();

    let mut out = String::with_capacity((pts_perim + pts_infill * 2) * 80 + 2048);

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
    let gramos  = vol_cm3 * cfg.densidad_material;

    out.push_str("; Generado por CondorLab Slicer\n");
    out.push_str(&format!("; Archivo           : {}\n", cfg.nombre_archivo));
    out.push_str(&format!("; Capas             : {}\n", r.capas.len()));
    out.push_str(&format!("; Altura de capa    : {:.3} mm\n", cfg.altura_capa));
    out.push_str(&format!("; Perímetros/isla   : {}\n", cfg.n_perimetros));
    out.push_str(&format!("; Infill            : {:.0}%\n", cfg.infill_densidad));
    out.push_str(&format!("; Boquilla          : {:.2} mm\n", cfg.diametro_boquilla));
    out.push_str(&format!("; Filamento         : {:.2} mm diámetro\n", cfg.diametro_filamento));
    out.push_str(&format!("; Temp hotend       : {}°C\n", cfg.temp_hotend));
    out.push_str(&format!("; Temp cama         : {}°C\n", cfg.temp_cama));
    out.push_str(&format!("; Vel. impresión    : {:.0} mm/s  (1ª capa {:.0} mm/s)\n",
        cfg.velocidad_impresion, cfg.velocidad_impresion * 0.5));
    out.push_str(&format!("; Vel. viaje        : {:.0} mm/s\n", cfg.velocidad_viaje));
    out.push_str(&format!("; Retracción        : {:.2} mm\n", cfg.retraccion_mm));
    out.push_str(&format!("; Tiempo estimado   : {}h {:02}m\n", h, m));
    out.push_str(&format!("; Filamento (est.)  : {:.1} mm  /  {:.2} g\n",
        r.filamento_mm, gramos));
    out.push('\n');
}

fn escribir_start_gcode(out: &mut String, cfg: &SlicerConfig) {
    out.push_str("; === INICIO ===\n");
    out.push_str("G21        ; unidades en mm\n");
    out.push_str("G90        ; posicionamiento absoluto\n");
    out.push_str("M82        ; extrusión absoluta\n");
    out.push_str("M107       ; ventilador apagado (primera capa sin cooling)\n");
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
    let vel_viaje  = cfg.velocidad_viaje as f64 * 60.0;
    let num_capas  = resultado.capas.len();
    let retrae     = cfg.retraccion_mm > 0.0;

    let mut e:          f64              = 0.0;
    let mut pos:        Option<[f32; 2]> = None;
    let mut retractado: bool             = false;

    for (n_capa, capa) in resultado.capas.iter().enumerate() {
        // Primera capa al 50% de velocidad para mejor adhesión a la cama
        let vel_print = if n_capa == 0 {
            cfg.velocidad_impresion * 0.5 * 60.0
        } else {
            cfg.velocidad_impresion * 60.0
        } as f64;

        out.push_str(&format!("\n; --- Capa {}/{} · z={:.3} mm ---\n",
            n_capa + 1, num_capas, capa.z));

        // Encender ventilador a partir de la segunda capa
        if n_capa == 1 {
            out.push_str("M106 S255  ; ventilador al 100%\n");
        }

        out.push_str(&format!("G0 Z{:.3} F3000\n", capa.z));

        // ── Perímetros (exterior → interiores) ──────────────────────────────
        for isla in &capa.perimetros {
            for shell in isla {
                if shell.len() < 2 { continue; }
                let primer_pt = shell[0];

                viajar(out, &mut e, &mut pos, &mut retractado,
                       primer_pt, capa.z, vel_viaje, cfg, retrae);

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

        // ── Infill ───────────────────────────────────────────────────────────
        for isla_inf in &capa.infill {
            for seg in isla_inf {
                let d = distancia_2d(seg[0], seg[1]);
                if d < 0.01 { continue; }

                viajar(out, &mut e, &mut pos, &mut retractado,
                       seg[0], capa.z, vel_viaje, cfg, retrae);

                e += calc_extrusion_mm(d, cfg) as f64;
                out.push_str(&format!(
                    "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                    seg[1][0], seg[1][1], e, vel_print,
                ));
                pos = Some(seg[1]);
            }
        }
    }
}

/// Emite un viaje (con retracción, z-hop y unretracción si corresponde).
///
/// Solo retrae si la distancia supera el diámetro de la boquilla para evitar
/// micro-retracciones inútiles en movimientos cortos entre shells adyacentes.
/// El z-hop solo se aplica cuando hay retracción (viajes largos).
fn viajar(
    out:         &mut String,
    e:           &mut f64,
    pos:         &mut Option<[f32; 2]>,
    retractado:  &mut bool,
    destino:     [f32; 2],
    z_actual:    f32,
    vel_viaje:   f64,
    cfg:         &SlicerConfig,
    retrae:      bool,
) {
    let dist = pos.map_or(f32::MAX, |p| distancia_2d(p, destino));
    if dist <= 0.5 {
        return; // ya estamos cerca, sin viaje
    }

    let hace_hop = retrae && cfg.z_hop_mm > 0.0 && dist > cfg.diametro_boquilla;

    if retrae && !*retractado && dist > cfg.diametro_boquilla {
        *e -= cfg.retraccion_mm as f64;
        out.push_str(&format!("G1 E{:.5} F2700  ; retracción\n", e));
        *retractado = true;
    }

    if hace_hop && *retractado {
        out.push_str(&format!("G0 Z{:.3} F3000  ; z-hop\n",
            z_actual + cfg.z_hop_mm));
    }

    out.push_str(&format!("G0 X{:.3} Y{:.3} F{:.0}\n", destino[0], destino[1], vel_viaje));

    if hace_hop && *retractado {
        out.push_str(&format!("G0 Z{:.3} F3000  ; bajar\n", z_actual));
    }

    if *retractado {
        *e += cfg.retraccion_mm as f64;
        out.push_str(&format!("G1 E{:.5} F2700  ; unretracción\n", e));
        *retractado = false;
    }
}

fn escribir_end_gcode(out: &mut String) {
    out.push_str("\n; === FIN ===\n");
    out.push_str("M107         ; apagar ventilador\n");
    out.push_str("G91          ; relativo\n");
    out.push_str("G1 E-2 F2700 ; retracción final\n");
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
