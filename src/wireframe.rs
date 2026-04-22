// ──────────────────────────────────────────────────────────────────────────────
// FRAMEBUFFER
// ──────────────────────────────────────────────────────────────────────────────

use std::collections::HashMap;

pub struct Framebuffer {
    pub char_w: usize,
    pub char_h: usize,
    pw: usize,
    ph: usize,
    depth:  Vec<f32>,
    brillo: Vec<f32>,   // <0=vacío  0..1=shade  2.0=silueta/wire
}

impl Framebuffer {
    pub fn new(char_w: usize, char_h: usize) -> Self {
        let (pw, ph) = (char_w, char_h * 2);
        let n = pw * ph;
        Self { char_w, char_h, pw, ph,
               depth:  vec![f32::MAX; n],
               brillo: vec![-1.0;    n] }
    }

    fn clear(&mut self) {
        self.depth.fill(f32::MAX);
        self.brillo.fill(-1.0);
    }

    #[inline]
    fn set_shade(&mut self, x: i32, y: i32, z: f32, b: f32) {
        if x < 0 || y < 0 { return; }
        let (xu, yu) = (x as usize, y as usize);
        if xu >= self.pw || yu >= self.ph { return; }
        let idx = yu * self.pw + xu;
        if z < self.depth[idx] { self.depth[idx] = z; self.brillo[idx] = b; }
    }

    // Bresenham — dibuja aristas para el modo wireframe
    fn set_linea(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (mut x, mut y) = (x0, y0);
        let dx =  (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1i32 } else { -1 };
        let sy = if y0 < y1 { 1i32 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x >= 0 && y >= 0 && (x as usize) < self.pw && (y as usize) < self.ph {
                self.brillo[y as usize * self.pw + x as usize] = 2.0;
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// GEOMETRÍA
// ──────────────────────────────────────────────────────────────────────────────

#[inline] fn rot_y(v: [f32;3], a: f32) -> [f32;3] {
    let (c,s) = (a.cos(), a.sin());
    [v[0]*c + v[2]*s,  v[1],  -v[0]*s + v[2]*c]
}
#[inline] fn rot_x(v: [f32;3], a: f32) -> [f32;3] {
    let (c,s) = (a.cos(), a.sin());
    [v[0],  v[1]*c - v[2]*s,  v[1]*s + v[2]*c]
}
#[inline] fn rot_z(v: [f32;3], a: f32) -> [f32;3] {
    let (c,s) = (a.cos(), a.sin());
    [v[0]*c - v[1]*s,  v[0]*s + v[1]*c,  v[2]]
}

pub fn normalizar(tris: &[[[f32;3];3]]) -> Vec<[[f32;3];3]> {
    if tris.is_empty() { return Vec::new(); }
    let mut mn = [f32::MAX;3]; let mut mx = [f32::MIN;3];
    for tri in tris { for v in tri { for i in 0..3 {
        if v[i] < mn[i] { mn[i] = v[i]; }
        if v[i] > mx[i] { mx[i] = v[i]; }
    }}}
    let c  = [(mn[0]+mx[0])/2.0, (mn[1]+mx[1])/2.0, (mn[2]+mx[2])/2.0];
    let sc = [(mx[0]-mn[0]),(mx[1]-mn[1]),(mx[2]-mn[2])]
        .iter().cloned().fold(0.0f32, f32::max) / 2.0;
    let sc = if sc == 0.0 { 1.0 } else { sc };
    tris.iter().map(|tri| std::array::from_fn(|i|
        [(tri[i][0]-c[0])/sc, (tri[i][1]-c[1])/sc, (tri[i][2]-c[2])/sc]
    )).collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// NORMALES SUAVES  (Gouraud — precalculado en espacio modelo)
// ──────────────────────────────────────────────────────────────────────────────

fn vkey(v: [f32;3]) -> (i32,i32,i32) {
    ((v[0]*2000.0).round() as i32,
     (v[1]*2000.0).round() as i32,
     (v[2]*2000.0).round() as i32)
}

pub fn calcular_normales_suaves(tris: &[[[f32;3];3]]) -> Vec<[[f32;3];3]> {
    if tris.is_empty() { return Vec::new(); }

    let face_normals: Vec<[f32;3]> = tris.iter().map(|p| {
        let a = [p[1][0]-p[0][0], p[1][1]-p[0][1], p[1][2]-p[0][2]];
        let b = [p[2][0]-p[0][0], p[2][1]-p[0][1], p[2][2]-p[0][2]];
        let nx = a[1]*b[2] - a[2]*b[1];
        let ny = a[2]*b[0] - a[0]*b[2];
        let nz = a[0]*b[1] - a[1]*b[0];
        let len = (nx*nx+ny*ny+nz*nz).sqrt().max(1e-8);
        [nx/len, ny/len, nz/len]
    }).collect();

    let mut acum: HashMap<(i32,i32,i32), [f32;3]> = HashMap::new();
    for (i, tri) in tris.iter().enumerate() {
        let n = face_normals[i];
        for v in tri {
            let e = acum.entry(vkey(*v)).or_insert([0.0;3]);
            e[0] += n[0]; e[1] += n[1]; e[2] += n[2];
        }
    }
    for n in acum.values_mut() {
        let len = (n[0]*n[0]+n[1]*n[1]+n[2]*n[2]).sqrt().max(1e-8);
        n[0] /= len; n[1] /= len; n[2] /= len;
    }

    tris.iter().enumerate().map(|(i, tri)| {
        let fn_ = face_normals[i];
        std::array::from_fn(|j| *acum.get(&vkey(tri[j])).unwrap_or(&fn_))
    }).collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// ILUMINACIÓN  (Blinn-Phong)
// ──────────────────────────────────────────────────────────────────────────────

const K_KEY:  [f32;3] = [ 0.524,  0.688, -0.502];
const K_FILL: [f32;3] = [-0.600, -0.354, -0.612];
const K_OBS:  [f32;3] = [ 0.0,    0.0,  -1.0  ];

fn shade(n: [f32;3]) -> f32 {
    let dot = |a: [f32;3], b: [f32;3]| a[0]*b[0] + a[1]*b[1] + a[2]*b[2];
    let d_key  = dot(n, K_KEY).max(0.0);
    let d_fill = dot(n, K_FILL).max(0.0);
    let hx = K_KEY[0] + K_OBS[0];
    let hy = K_KEY[1] + K_OBS[1];
    let hz = K_KEY[2] + K_OBS[2];
    let hlen = (hx*hx + hy*hy + hz*hz).sqrt().max(1e-8);
    let spec = dot(n, [hx/hlen, hy/hlen, hz/hlen]).max(0.0).powi(48) * 0.45;
    (0.08 + d_key*0.62 + d_fill*0.18 + spec).clamp(0.0, 1.0)
}

// ──────────────────────────────────────────────────────────────────────────────
// PROYECCIÓN PERSPECTIVA
// ──────────────────────────────────────────────────────────────────────────────

const CAM_Z: f32 = 3.5;

fn proyectar(v: &[f32;3], zoom: f32, cx: f32, cy: f32) -> (f32, f32, f32) {
    let z_cam = v[2] + CAM_Z;
    let f = if z_cam > 0.05 { CAM_Z / z_cam } else { CAM_Z / 0.05 };
    (v[0] * zoom * f + cx, -v[1] * zoom * f + cy, v[2])
}

// ──────────────────────────────────────────────────────────────────────────────
// RENDER PRINCIPAL
//
// modo_wire:  0 = sólido   1 = solo aristas   2 = sólido + aristas
// ──────────────────────────────────────────────────────────────────────────────

pub fn renderizar(
    fb:        &mut Framebuffer,
    tris:      &[[[f32;3];3]],
    normals:   &[[[f32;3];3]],
    rx: f32, ry: f32, rz: f32,
    zoom: f32,
    modo_wire: u8,
) {
    fb.clear();
    renderizar_impl(fb, tris, normals, rx, ry, rz, zoom, modo_wire);
}

/// Como `renderizar` pero sin limpiar el framebuffer — permite capas sobre un render previo.
pub fn renderizar_sobre(
    fb:        &mut Framebuffer,
    tris:      &[[[f32;3];3]],
    normals:   &[[[f32;3];3]],
    rx: f32, ry: f32, rz: f32,
    zoom: f32,
    modo_wire: u8,
) {
    renderizar_impl(fb, tris, normals, rx, ry, rz, zoom, modo_wire);
}

fn renderizar_impl(
    fb:        &mut Framebuffer,
    tris:      &[[[f32;3];3]],
    normals:   &[[[f32;3];3]],
    rx: f32, ry: f32, rz: f32,
    zoom: f32,
    modo_wire: u8,
) {
    let zoom = if zoom <= 0.0 {
        fb.char_w.min(fb.char_h) as f32 * 0.70
    } else { zoom };

    let cx = fb.pw as f32 / 2.0;
    let cy = fb.ph as f32 / 2.0;
    let use_smooth = normals.len() == tris.len();

    // ── Precalcular triángulos rotados + normales de cara ─────────────────────
    struct TriData {
        p:   [[f32;3];3],
        nf:  [f32;3],        // normal de cara (en espacio vista)
        vis: bool,           // pasa back-face cull
    }

    let tri_data: Vec<TriData> = tris.iter().map(|tri| {
        let p: [[f32;3];3] = std::array::from_fn(|j| rot_x(rot_y(rot_z(tri[j], rz), ry), rx));
        let a = [p[1][0]-p[0][0], p[1][1]-p[0][1], p[1][2]-p[0][2]];
        let b = [p[2][0]-p[0][0], p[2][1]-p[0][1], p[2][2]-p[0][2]];
        let nx = a[1]*b[2] - a[2]*b[1];
        let ny = a[2]*b[0] - a[0]*b[2];
        let nz = a[0]*b[1] - a[1]*b[0];
        let nlen = (nx*nx+ny*ny+nz*nz).sqrt();
        let (nf, vis) = if nlen < 1e-8 {
            ([0.0;3], false)
        } else {
            ([nx/nlen, ny/nlen, nz/nlen], nz <= 0.0)
        };
        TriData { p, nf, vis }
    }).collect();

    // ── Paso 1: rasterizar caras (sólido) ────────────────────────────────────
    if modo_wire != 1 {
        for (i, td) in tri_data.iter().enumerate() {
            if !td.vis { continue; }

            let bv: [f32;3] = if use_smooth {
                std::array::from_fn(|j| shade(rot_x(rot_y(rot_z(normals[i][j], rz), ry), rx)))
            } else {
                [shade(td.nf); 3]
            };

            let s0 = proyectar(&td.p[0], zoom, cx, cy);
            let s1 = proyectar(&td.p[1], zoom, cx, cy);
            let s2 = proyectar(&td.p[2], zoom, cx, cy);
            rasterizar_tri(fb, s0, s1, s2, bv);
        }

        // Contorno/silueta — dos pasadas: primero detectar, luego marcar.
        // Evita clonar el buffer y garantiza que la detección usa los valores originales.
        let mut edge_pixels: Vec<usize> = Vec::new();
        for y in 0..fb.ph {
            for x in 0..fb.pw {
                let idx = y * fb.pw + x;
                if fb.brillo[idx] < 0.0 { continue; }
                let vecino_fondo =
                    (x == 0         || fb.brillo[idx - 1]     < 0.0) ||
                    (x == fb.pw - 1 || fb.brillo[idx + 1]     < 0.0) ||
                    (y == 0         || fb.brillo[idx - fb.pw] < 0.0) ||
                    (y == fb.ph - 1 || fb.brillo[idx + fb.pw] < 0.0);
                if vecino_fondo { edge_pixels.push(idx); }
            }
        }
        for idx in edge_pixels {
            fb.brillo[idx] = 2.0;
        }
    }

    // ── Paso 2: aristas wireframe ─────────────────────────────────────────────
    if modo_wire >= 1 {
        for td in &tri_data {
            if !td.vis { continue; }
            let s: [(i32,i32);3] = std::array::from_fn(|j| {
                let (sx, sy, _) = proyectar(&td.p[j], zoom, cx, cy);
                (sx as i32, sy as i32)
            });
            fb.set_linea(s[0].0, s[0].1, s[1].0, s[1].1);
            fb.set_linea(s[1].0, s[1].1, s[2].0, s[2].1);
            fb.set_linea(s[2].0, s[2].1, s[0].0, s[0].1);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// RASTERIZADOR  (coordenadas baricéntricas + z-buffer + Gouraud)
// ──────────────────────────────────────────────────────────────────────────────

#[inline]
fn edge_fn(ax:f32,ay:f32,bx:f32,by:f32,px:f32,py:f32)->f32 {
    (px-ax)*(by-ay)-(py-ay)*(bx-ax)
}

fn rasterizar_tri(
    fb: &mut Framebuffer,
    p0:(f32,f32,f32), p1:(f32,f32,f32), p2:(f32,f32,f32),
    bv: [f32;3],
) {
    let (x0,y0,z0)=p0; let (x1,y1,z1)=p1; let (x2,y2,z2)=p2;
    let xmin = x0.min(x1).min(x2).max(0.0) as i32;
    let xmax = x0.max(x1).max(x2).min(fb.pw as f32 - 1.0) as i32;
    let ymin = y0.min(y1).min(y2).max(0.0) as i32;
    let ymax = y0.max(y1).max(y2).min(fb.ph as f32 - 1.0) as i32;
    if xmin > xmax || ymin > ymax { return; }
    let area = edge_fn(x0,y0, x1,y1, x2,y2);
    if area.abs() < 0.5 { return; }
    let inv = 1.0 / area;
    for y in ymin..=ymax {
        for x in xmin..=xmax {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge_fn(x1,y1, x2,y2, px,py);
            let w1 = edge_fn(x2,y2, x0,y0, px,py);
            let w2 = edge_fn(x0,y0, x1,y1, px,py);
            if (w0>=0.0)==(area>=0.0) && (w1>=0.0)==(area>=0.0) && (w2>=0.0)==(area>=0.0) {
                let (u,v,w) = (w0*inv, w1*inv, w2*inv);
                fb.set_shade(x, y, u*z0+v*z1+w*z2, u*bv[0]+v*bv[1]+w*bv[2]);
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// PALETAS DE COLOR
//
// Cada paleta: 7 paradas (t, r, g, b).  t=0 = sombra profunda, t=1 = especular.
// brillo >= 2.0 siempre → blanco (silueta / aristas wireframe).
// ──────────────────────────────────────────────────────────────────────────────

pub const PALETAS_NOMBRE: &[&str] = &[
    "Celeste Arg.", "Rojo filamento", "Verde filamento", "Plata", "Dorado Sol ☀",
];

const PALETAS: &[&[(f32, u8, u8, u8)]] = &[
    // 0 — Celeste argentino
    &[(0.00, 10, 15, 55),(0.15, 25, 65,155),(0.36, 70,145,210),
      (0.58,118,182,232),(0.78,178,215,243),(0.90,222,236,248),(1.00,255,255,255)],
    // 1 — Rojo filamento
    &[(0.00, 35,  5,  5),(0.15,120, 20, 15),(0.36,200, 50, 20),
      (0.58,230,100, 30),(0.78,240,165, 80),(0.90,250,220,160),(1.00,255,255,255)],
    // 2 — Verde filamento
    &[(0.00,  5, 20,  5),(0.15, 15, 80, 20),(0.36, 30,160, 50),
      (0.58, 80,200, 80),(0.78,160,230,140),(0.90,210,245,200),(1.00,255,255,255)],
    // 3 — Plata / gris
    &[(0.00, 15, 15, 20),(0.15, 50, 55, 65),(0.36,100,105,115),
      (0.58,155,160,170),(0.78,200,205,215),(0.90,230,232,238),(1.00,255,255,255)],
    // 4 — Dorado Sol de Mayo
    &[(0.00, 30, 20,  0),(0.15,100, 60,  5),(0.36,180,120, 10),
      (0.58,215,165, 40),(0.78,235,200,100),(0.90,248,230,170),(1.00,255,255,245)],
];

fn brillo_a_color(b: f32, paleta: u8) -> ratatui::style::Color {
    use ratatui::style::Color;
    if b >= 2.0 { return Color::Rgb(255, 255, 255); }

    let stops = PALETAS[(paleta as usize).min(PALETAS.len() - 1)];
    let t = b.clamp(0.0, 1.0);

    let mut lo = stops[0];
    let mut hi = *stops.last().unwrap();
    for w in stops.windows(2) {
        if t >= w[0].0 && t < w[1].0 { lo = w[0]; hi = w[1]; break; }
    }

    let range = hi.0 - lo.0;
    let f = if range < 1e-6 { 1.0 } else { ((t - lo.0) / range).clamp(0.0, 1.0) };
    let lerp = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * f) as u8 };
    Color::Rgb(lerp(lo.1, hi.1), lerp(lo.2, hi.2), lerp(lo.3, hi.3))
}

// ──────────────────────────────────────────────────────────────────────────────
// CONVERSIÓN A RATATUI  (half-block 2× resolución vertical)
// ──────────────────────────────────────────────────────────────────────────────

use ratatui::{style::{Color, Style}, text::{Line, Span}};

fn half_block(b_top: f32, b_bot: f32, paleta: u8) -> (char, Color, Color) {
    let (hit_t, hit_b) = (b_top >= 0.0, b_bot >= 0.0);
    match (hit_t, hit_b) {
        (false, false) => (' ',  Color::Reset, Color::Reset),
        (true,  false) => ('▀',  brillo_a_color(b_top, paleta), Color::Reset),
        (false, true)  => ('▄',  brillo_a_color(b_bot, paleta), Color::Reset),
        (true,  true)  => ('▀',  brillo_a_color(b_top, paleta), brillo_a_color(b_bot, paleta)),
    }
}

pub fn fb_a_lineas(fb: &Framebuffer, paleta: u8) -> Vec<Line<'static>> {
    (0..fb.char_h).map(|y_char| {
        let y_top = y_char * 2;
        let y_bot = y_top + 1;

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut cur = (' ', Color::Reset, Color::Reset);
        let mut buf = String::new();

        for x in 0..fb.char_w {
            let b_top = fb.brillo[y_top * fb.pw + x];
            let b_bot = if y_bot < fb.ph { fb.brillo[y_bot * fb.pw + x] } else { -1.0 };
            let cell = half_block(b_top, b_bot, paleta);

            if cell == cur {
                buf.push(cell.0);
            } else {
                if !buf.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut buf),
                        Style::default().fg(cur.1).bg(cur.2),
                    ));
                }
                cur = cell;
                buf.push(cell.0);
            }
        }
        if !buf.is_empty() {
            spans.push(Span::styled(buf, Style::default().fg(cur.1).bg(cur.2)));
        }
        Line::from(spans)
    }).collect()
}
