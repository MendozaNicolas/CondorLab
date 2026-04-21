use std::{fs, path::Path};

// ──────────────────────────────────────────────────────────────────────────────
// ESTRUCTURAS
// ──────────────────────────────────────────────────────────────────────────────

pub struct InfoSTL {
    pub nombre: String,
    pub triangulos: usize,
    pub volumen_cm3: f64,
    pub gramos_pla: f64,           // con densidad PLA 1.24 g/cm³
    pub dim_mm: [f32; 3],          // [ancho, profundidad, alto]
    pub tris: Vec<[[f32; 3]; 3]>,  // triángulos crudos para wireframe
}

#[allow(dead_code)]
pub struct InfoGcode {
    pub nombre: String,
    pub slicer: String,
    pub tiempo_min: f64,
    pub filamento_g: f64,
    pub filamento_mm: f64,
    pub capas: usize,
    pub temp_hotend: u32,
    pub temp_cama: u32,
    pub altura_capa: f64,
    pub relleno_pct: f64,
}

// ──────────────────────────────────────────────────────────────────────────────
// STL
// ──────────────────────────────────────────────────────────────────────────────

// Volumen signed del tetraedro (O, v0, v1, v2) — teorema de divergencia
fn vol_tet(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> f64 {
    let (x0, y0, z0) = (v0[0] as f64, v0[1] as f64, v0[2] as f64);
    let (x1, y1, z1) = (v1[0] as f64, v1[1] as f64, v1[2] as f64);
    let (x2, y2, z2) = (v2[0] as f64, v2[1] as f64, v2[2] as f64);
    (x0 * (y1 * z2 - y2 * z1)
   - y0 * (x1 * z2 - x2 * z1)
   + z0 * (x1 * y2 - x2 * y1)) / 6.0
}

pub fn cargar_stl(ruta: &str) -> Result<InfoSTL, String> {
    let mut f = std::fs::File::open(ruta)
        .map_err(|e| format!("No se pudo abrir: {e}"))?;

    let mesh = stl_io::read_stl(&mut f)
        .map_err(|e| format!("STL inválido: {e}"))?;

    let nombre = Path::new(ruta)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let mut vol = 0.0f64;
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut tris = Vec::with_capacity(mesh.faces.len());

    for face in &mesh.faces {
        let v0: [f32; 3] = mesh.vertices[face.vertices[0]].into();
        let v1: [f32; 3] = mesh.vertices[face.vertices[1]].into();
        let v2: [f32; 3] = mesh.vertices[face.vertices[2]].into();

        vol += vol_tet(v0, v1, v2);
        tris.push([v0, v1, v2]);

        for v in &[v0, v1, v2] {
            for i in 0..3 {
                if v[i] < min[i] { min[i] = v[i]; }
                if v[i] > max[i] { max[i] = v[i]; }
            }
        }
    }

    let volumen_cm3 = vol.abs() / 1000.0; // mm³ → cm³
    let gramos_pla  = volumen_cm3 * 1.24;  // densidad PLA estándar
    let dim_mm      = [max[0]-min[0], max[1]-min[1], max[2]-min[2]];

    Ok(InfoSTL {
        nombre,
        triangulos: mesh.faces.len(),
        volumen_cm3,
        gramos_pla,
        dim_mm,
        tris,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// G-CODE
// ──────────────────────────────────────────────────────────────────────────────

pub fn cargar_gcode(ruta: &str) -> Result<InfoGcode, String> {
    let contenido = fs::read_to_string(ruta)
        .map_err(|e| format!("No se pudo leer: {e}"))?;

    let nombre = Path::new(ruta)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let mut slicer        = String::from("Desconocido");
    let mut tiempo_min    = 0.0f64;
    let mut filamento_g   = 0.0f64;
    let mut filamento_mm  = 0.0f64;
    let mut capas         = 0usize;
    let mut temp_hotend   = 0u32;
    let mut temp_cama     = 0u32;
    let mut altura_capa   = 0.0f64;
    let mut relleno_pct   = 0.0f64;

    // Sólo leemos el header (primeras 300 líneas) para ser rápidos
    for line in contenido.lines().take(300) {
        let l = line.trim();

        // ── Detección de slicer ──────────────────────────────────────────
        if l.contains("FlashPrint") || l.contains("Flashforge") || l.contains("flashprint") {
            slicer = "FlashPrint".into();
        } else if l.contains("PrusaSlicer") || l.contains("prusaslicer") {
            slicer = "PrusaSlicer".into();
        } else if l.contains("Cura") || l.contains("cura") {
            slicer = "Cura".into();
        } else if l.contains("SuperSlicer") {
            slicer = "SuperSlicer".into();
        } else if l.contains("Orca") || l.contains("OrcaSlicer") {
            slicer = "OrcaSlicer".into();
        }

        // ── Tiempo ──────────────────────────────────────────────────────
        if (l.contains("Estimated printing time") || l.contains("estimated printing time"))
            && tiempo_min == 0.0
        {
            tiempo_min = parse_tiempo(l);
        }
        if l.contains("Print time:") && tiempo_min == 0.0 {
            tiempo_min = parse_tiempo(l);
        }

        // ── Filamento ────────────────────────────────────────────────────
        // FlashPrint
        if let Some(s) = istrip(l, ";   Filament weight:")
            .or_else(|| istrip(l, "; Filament weight:"))
        {
            if filamento_g == 0.0 { filamento_g = parse_f(s); }
        }
        if let Some(s) = istrip(l, ";   Filament length:")
            .or_else(|| istrip(l, "; Filament length:"))
        {
            if filamento_mm == 0.0 { filamento_mm = parse_f(s); }
        }
        // PrusaSlicer / SuperSlicer / OrcaSlicer
        if let Some(s) = istrip(l, "; filament used [g] =")
            .or_else(|| istrip(l, "; filament_used_g ="))
        {
            if filamento_g == 0.0 { filamento_g = parse_f(s); }
        }
        if let Some(s) = istrip(l, "; filament used [mm] =") {
            if filamento_mm == 0.0 { filamento_mm = parse_f(s); }
        }
        // Cura: ";Filament used: 1.23m"
        if let Some(s) = istrip(l, ";Filament used:") {
            if filamento_mm == 0.0 {
                let m = parse_f(s); // metros
                filamento_mm = m * 1000.0;
            }
        }

        // ── Capas ────────────────────────────────────────────────────────
        if let Some(s) = istrip(l, ";   Layer count:")
            .or_else(|| istrip(l, "; layer_count ="))
            .or_else(|| istrip(l, ";Layer count:"))
            .or_else(|| istrip(l, "; total layer number:"))
        {
            if capas == 0 { capas = parse_f(s) as usize; }
        }

        // ── Altura de capa ───────────────────────────────────────────────
        if let Some(s) = istrip(l, ";   Layer height:")
            .or_else(|| istrip(l, "; layer_height ="))
            .or_else(|| istrip(l, ";Layer height:"))
        {
            if altura_capa == 0.0 { altura_capa = parse_f(s); }
        }

        // ── Relleno ──────────────────────────────────────────────────────
        if let Some(s) = istrip(l, ";   Infill:")
            .or_else(|| istrip(l, "; fill_density ="))
            .or_else(|| istrip(l, ";Infill:"))
        {
            if relleno_pct == 0.0 { relleno_pct = parse_f(s); }
        }

        // ── Temperaturas desde comandos M104/M109/M140/M190 ──────────────
        if (l.starts_with("M104 S") || l.starts_with("M109 S")) && temp_hotend == 0 {
            temp_hotend = parse_f(&l[6..]) as u32;
        }
        if (l.starts_with("M140 S") || l.starts_with("M190 S")) && temp_cama == 0 {
            temp_cama = parse_f(&l[6..]) as u32;
        }
    }

    Ok(InfoGcode {
        nombre,
        slicer,
        tiempo_min,
        filamento_g,
        filamento_mm,
        capas,
        temp_hotend,
        temp_cama,
        altura_capa,
        relleno_pct,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// HELPERS DE PARSEO
// ──────────────────────────────────────────────────────────────────────────────

// Strip case-insensitive prefix
fn istrip<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.to_lowercase().starts_with(&prefix.to_lowercase()) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

// Parsea el primer número flotante de una cadena
fn parse_f(s: &str) -> f64 {
    s.trim()
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
        .find(|t| !t.is_empty())
        .unwrap_or("0")
        .replace(',', ".")
        .parse()
        .unwrap_or(0.0)
}

// Parsea tiempo a minutos desde formatos: "1h 23m 45s", "1 hours 23 minutes",
// "1:23:45", "5400" (segundos), "90" (minutos con contexto)
pub fn parse_tiempo(linea: &str) -> f64 {
    let l = linea.to_lowercase();
    let mut min = 0.0f64;
    let mut encontro = false;

    // Formato "X hours Y minutes" / "X hour Y minute"
    if l.contains("hour") {
        if let Some(pos) = l.find("hour") {
            if let Some(h) = num_antes(&l, pos) {
                min += h * 60.0;
                encontro = true;
            }
        }
        if let Some(pos) = l.find("minute") {
            if let Some(m) = num_antes(&l, pos) {
                min += m;
            }
        }
        if encontro { return min; }
    }

    // Formato "Xh Ym Zs"
    if l.contains('h') {
        if let Some(pos) = l.find('h') {
            // Verificar que lo que sigue no sea otra letra (evitar "height" etc.)
            let next = l.chars().nth(pos + 1);
            if next.map_or(true, |c| !c.is_alphabetic()) {
                if let Some(h) = num_antes(&l, pos) {
                    min += h * 60.0;
                    encontro = true;
                }
            }
        }
        // Buscar 'm' que sea minutos (no "mm")
        if let Some(pos) = l.rfind('m') {
            let next = l.chars().nth(pos + 1);
            if next.map_or(true, |c| c == ' ' || c == '\n' || !c.is_alphabetic()) {
                if let Some(m) = num_antes(&l, pos) {
                    min += m;
                }
            }
        }
        if encontro { return min; }
    }

    // Formato "H:MM:SS" o "H:MM"
    let partes: Vec<&str> = linea.split(':').collect();
    if partes.len() >= 3 {
        let h: f64 = partes[partes.len()-3].split_whitespace().last()
            .unwrap_or("0").parse().unwrap_or(0.0);
        let m: f64 = partes[partes.len()-2].trim().parse().unwrap_or(0.0);
        if h > 0.0 || m > 0.0 {
            return h * 60.0 + m;
        }
    }

    // Fallback: número suelto interpretado como segundos si > 300
    let nums: Vec<f64> = l.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    if let Some(&n) = nums.first() {
        if n > 300.0 { return n / 60.0; } // probablemente segundos
    }

    0.0
}

fn num_antes(s: &str, pos: usize) -> Option<f64> {
    s[..pos]
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter(|t| !t.is_empty())
        .last()
        .and_then(|n| n.parse().ok())
}
