mod archivo;
mod config;
mod wireframe;

use std::{io, path::{Path, PathBuf}, fs, time::Duration};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};

// ──────────────────────────────────────────────────────────────────────────────
// DOMINIO
// ──────────────────────────────────────────────────────────────────────────────

use config::Cotizacion;

#[derive(Clone, Debug)]
struct Campo {
    label: &'static str,
    unidad: &'static str,
    hint: &'static str,
    default: f64,
    valor: String,
}

impl Campo {
    fn new(label: &'static str, unidad: &'static str, hint: &'static str, default: f64) -> Self {
        Self { label, unidad, hint, default, valor: String::new() }
    }

    fn parsed(&self) -> f64 {
        let s = self.valor.trim().replace(',', ".");
        if s.is_empty() { self.default } else { s.parse().unwrap_or(self.default) }
    }

    fn display(&self) -> String {
        if self.valor.is_empty() {
            fmt_num(self.default)
        } else {
            self.valor.clone()
        }
    }
}

fn fmt_num(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.0}", v)
    } else if v >= 1_000.0 {
        format!("{:.0}", v)
    } else if v == v.floor() {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

fn fmt_pesos(v: f64) -> String {
    format!("$ {:>10.0}", v)
}

// Índices de campos
const F_PRECIO_KG: usize = 0;
const F_GRAMOS: usize = 1;
const F_DESPERDICIO: usize = 2;
const F_KWH: usize = 3;
const F_WATTS: usize = 4;
const F_HORAS: usize = 5;
const F_COSTO_IMP: usize = 6;
const F_VIDA: usize = 7;
const F_MANT: usize = 8;
const F_MI_HORA: usize = 9;
const F_MIS_HORAS: usize = 10;
const F_POST_PROC: usize = 11;
const F_ENVIO: usize = 12;
const F_MARGEN: usize = 13;
const F_DOLAR: usize = 14;
const NUM_CAMPOS: usize = 15;

#[derive(Default)]
struct Calc {
    gramos_reales: f64,
    costo_mat: f64,
    kwh: f64,
    costo_elec: f64,
    costo_dep: f64,
    costo_mant: f64,
    costo_mo: f64,
    costo_post: f64,
    costo_envio: f64,
    costo_base: f64,
    ganancia: f64,
    precio: f64,
    precio_usd: f64,
    redondeo_50: f64,
    redondeo_100: f64,
}

fn calcular(campos: &[Campo; NUM_CAMPOS]) -> Calc {
    let precio_kg   = campos[F_PRECIO_KG].parsed();
    let gramos      = campos[F_GRAMOS].parsed();
    let desperdicio = campos[F_DESPERDICIO].parsed();
    let kwh_precio  = campos[F_KWH].parsed();
    let watts       = campos[F_WATTS].parsed();
    let horas       = campos[F_HORAS].parsed();
    let costo_imp   = campos[F_COSTO_IMP].parsed();
    let vida        = campos[F_VIDA].parsed();
    let mant_hora   = campos[F_MANT].parsed();
    let mi_hora     = campos[F_MI_HORA].parsed();
    let mis_horas   = campos[F_MIS_HORAS].parsed();
    let post_proc   = campos[F_POST_PROC].parsed();
    let envio       = campos[F_ENVIO].parsed();
    let margen      = campos[F_MARGEN].parsed();
    let dolar       = campos[F_DOLAR].parsed();

    let gramos_reales = gramos * (1.0 + desperdicio / 100.0);
    let costo_mat     = (gramos_reales / 1000.0) * precio_kg;
    let kwh           = (watts / 1000.0) * horas;
    let costo_elec    = kwh * kwh_precio;
    let dep_hora      = if vida > 0.0 { costo_imp / vida } else { 0.0 };
    let costo_dep     = dep_hora * horas;
    let costo_mant    = mant_hora * horas;
    let costo_mo      = mi_hora * mis_horas;
    let costo_post    = post_proc;
    let costo_envio   = envio;
    let costo_base    = costo_mat + costo_elec + costo_dep + costo_mant + costo_mo + costo_post + costo_envio;
    let ganancia      = costo_base * margen / 100.0;
    let precio        = costo_base + ganancia;
    let precio_usd    = if dolar > 0.0 { precio / dolar } else { 0.0 };
    let redondeo_50   = (precio / 50.0).ceil() * 50.0;
    let redondeo_100  = (precio / 100.0).ceil() * 100.0;

    Calc { gramos_reales, costo_mat, kwh, costo_elec, costo_dep, costo_mant, costo_mo, costo_post, costo_envio, costo_base, ganancia, precio, precio_usd, redondeo_50, redondeo_100 }
}

// ──────────────────────────────────────────────────────────────────────────────
// APP STATE
// ──────────────────────────────────────────────────────────────────────────────

enum PantallaActiva { Principal, Historial, Ayuda, Archivos, Vista3D }

// ──────────────────────────────────────────────────────────────────────────────
// EXPLORADOR DE ARCHIVOS
// ──────────────────────────────────────────────────────────────────────────────

struct EntradaDir {
    nombre:    String,
    es_dir:    bool,
    tamaño_kb: u64,
    ext:       String,
}

impl EntradaDir {
    fn relevante(&self) -> bool {
        matches!(self.ext.as_str(), "stl" | "gcode" | "gco" | "g")
    }
}

struct NavArchivos {
    directorio: PathBuf,
    entradas:   Vec<EntradaDir>,
    seleccion:  usize,
    scroll:     usize,
    filtro:     String,
    error:      Option<String>,
}

impl NavArchivos {
    fn nuevo(desde: PathBuf) -> Self {
        let mut nav = Self {
            directorio: desde,
            entradas:   Vec::new(),
            seleccion:  0,
            scroll:     0,
            filtro:     String::new(),
            error:      None,
        };
        nav.cargar();
        nav
    }

    fn cargar(&mut self) {
        self.entradas.clear();
        self.seleccion = 0;
        self.scroll    = 0;

        if self.directorio.parent().is_some() {
            self.entradas.push(EntradaDir {
                nombre: "..".into(), es_dir: true, tamaño_kb: 0, ext: String::new(),
            });
        }

        let mut dirs: Vec<EntradaDir>  = Vec::new();
        let mut files: Vec<EntradaDir> = Vec::new();

        if let Ok(rd) = fs::read_dir(&self.directorio) {
            for entry in rd.flatten() {
                let nombre = entry.file_name().to_string_lossy().into_owned();
                if nombre.starts_with('.') { continue; }
                let es_dir    = entry.file_type().map_or(false, |t| t.is_dir());
                let tamaño_kb = entry.metadata().map_or(0, |m| m.len() / 1024);
                let ext = Path::new(&nombre)
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                let e = EntradaDir { nombre, es_dir, tamaño_kb, ext };
                if es_dir { dirs.push(e) } else { files.push(e) }
            }
        }

        dirs.sort_by(|a,b| a.nombre.cmp(&b.nombre));
        files.sort_by(|a,b| a.nombre.cmp(&b.nombre));
        self.entradas.extend(dirs);
        self.entradas.extend(files);
    }

    fn filtradas(&self) -> Vec<usize> {
        let f = self.filtro.to_lowercase();
        self.entradas.iter().enumerate()
            .filter(|(_,e)| f.is_empty() || e.nombre.to_lowercase().contains(&f))
            .map(|(i,_)| i)
            .collect()
    }

    fn mover(&mut self, delta: i32) {
        let ids = self.filtradas();
        if ids.is_empty() { return; }
        let pos = ids.iter().position(|&i| i == self.seleccion).unwrap_or(0) as i32;
        let nuevo = (pos + delta).clamp(0, ids.len() as i32 - 1) as usize;
        self.seleccion = ids[nuevo];
    }

    // Devuelve la ruta si se seleccionó un archivo (no directorio)
    fn confirmar(&mut self) -> Option<PathBuf> {
        let ids = self.filtradas();
        // seleccion puede no estar en filtradas si se filtró — usar el primero
        let idx = if ids.contains(&self.seleccion) {
            self.seleccion
        } else {
            *ids.first()?
        };
        let e = &self.entradas[idx];
        if e.es_dir {
            let nuevo = if e.nombre == ".." {
                self.directorio.parent()?.to_path_buf()
            } else {
                self.directorio.join(&e.nombre)
            };
            self.directorio = nuevo;
            self.filtro.clear();
            self.cargar();
            None
        } else {
            Some(self.directorio.join(&e.nombre))
        }
    }

    fn subir(&mut self) {
        if let Some(padre) = self.directorio.parent() {
            self.directorio = padre.to_path_buf();
            self.filtro.clear();
            self.cargar();
        }
    }
}

struct App {
    campos: [Campo; NUM_CAMPOS],
    foco: usize,
    pantalla: PantallaActiva,
    historial: Vec<Cotizacion>,
    guardado_msg: Option<u8>,
    // archivos
    info_stl:   Option<archivo::InfoSTL>,
    info_gcode: Option<archivo::InfoGcode>,
    tris_norm:  Vec<[[f32; 3]; 3]>,
    // 3D view
    rot_x:        f32,
    rot_y:        f32,
    rot_z:        f32,
    zoom_3d:      f32,
    perfil_idx:       usize,
    auto_rotar:       bool,
    normals_suaves:   Vec<[[f32;3];3]>,
    paleta_3d:        u8,
    modo_wire:        u8,   // 0=sólido  1=wire  2=sólido+wire
    confirmar_salida: bool,
    // explorador de archivos
    nav: NavArchivos,
}

impl App {
    fn new() -> Self {
        let cfg       = config::cargar_config();
        let historial = config::cargar_historial();
        let perfil_idx = cfg.perfil_idx.min(config::PERFILES.len() - 1);

        let mut app = Self {
            campos: [
                Campo::new("Precio PLA ($/kg)",       "$", "Precio del filamento en ARS. Revisalo semanalmente.", 12000.0),
                Campo::new("Peso de la pieza (g)",     "g", "Gramos del objeto terminado (sin soportes).", 50.0),
                Campo::new("% Desperdicio / fallas",   "%", "Soportes, purgas, re-impresiones. Mínimo 15%.", 15.0),
                Campo::new("Precio kWh (ARS)",         "$", "Tarifa eléctrica residencial ARG (tu valor actual).", 111.0),
                Campo::new("Watts de la impresora",    "W", "Adventurer 5M: pico 350W, promedio real imprimiendo ~220W.", 220.0),
                Campo::new("Horas de impresión",       "h", "Tiempo total incluyendo calentamiento.", 2.0),
                Campo::new("Costo de la impresora",    "$", "Flashforge Adventurer 5M — precio de reposición actual.", 585_900.0),
                Campo::new("Vida útil estimada (hs)",  "h", "Adventurer 5M: estructura robusta, estimá 2500–3500h hasta revisión mayor.", 3000.0),
                Campo::new("Mant./insumos por hora",   "$", "Lubricantes, correas, boquillas, etc.", 300.0),
                Campo::new("Tu hora de trabajo",       "$", "Smín ARG ≈ $4.500/h. Recomendado: $5.000–$10.000.", 5000.0),
                Campo::new("Tus horas en la pieza",    "h", "Preparación, post-proceso, control de calidad.", 0.5),
                Campo::new("Post-procesado ($)",       "$", "Costo fijo: lijado, pintura, armado, soporte.", 0.0),
                Campo::new("Envío / packaging ($)",    "$", "Costo fijo de empaque y envío.", 0.0),
                Campo::new("Margen de ganancia (%)",   "%", "Recomendado ≥ 50% por inflación estructural.", 50.0),
                Campo::new("Dólar blue (referencia)",  "$", "Para ver el precio en USD. Usalo para cotizar en divisas.", 1200.0),
            ],
            foco: 0,
            pantalla: PantallaActiva::Principal,
            historial,
            guardado_msg: None,
            info_stl: None,
            info_gcode: None,
            tris_norm: Vec::new(),
            normals_suaves: Vec::new(),
            perfil_idx,
            rot_x: -0.4,
            rot_y:  0.6,
            rot_z:  0.0,
            zoom_3d: 0.0,
            auto_rotar: false,
            paleta_3d: 0,
            modo_wire: 0,
            confirmar_salida: false,
            nav: NavArchivos::nuevo(
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
            ),
        };

        // Restaurar valores guardados de los campos
        for (i, v) in cfg.valores.iter().enumerate() {
            if i < NUM_CAMPOS { app.campos[i].valor = v.clone(); }
        }

        app
    }

    fn aplicar_perfil(&mut self) {
        let p = &config::PERFILES[self.perfil_idx];
        if p.watts     > 0.0 { self.campos[F_WATTS].valor     = format!("{}", p.watts     as u32); }
        if p.costo_ars > 0.0 { self.campos[F_COSTO_IMP].valor = format!("{}", p.costo_ars as u32); }
        if p.vida_hs   > 0.0 { self.campos[F_VIDA].valor      = format!("{}", p.vida_hs   as u32); }
        if p.mant_hr   > 0.0 { self.campos[F_MANT].valor      = format!("{}", p.mant_hr   as u32); }
    }

    fn guardar_estado(&self) {
        config::guardar_config(&config::AppConfig {
            valores:    self.campos.iter().map(|c| c.valor.clone()).collect(),
            perfil_idx: self.perfil_idx,
        });
    }

    fn guardar_cotizacion(&mut self) {
        let calc = calcular(&self.campos);
        let num = self.historial.len() + 1;
        let gramos = self.campos[F_GRAMOS].parsed();
        self.historial.push(Cotizacion {
            label: format!("#{num}  {:.0}g · {:.0}h", gramos, self.campos[F_HORAS].parsed()),
            precio: calc.precio,
            precio_usd: calc.precio_usd,
            costo_base: calc.costo_base,
        });
        self.guardado_msg = Some(30);
        config::guardar_historial(&self.historial);
    }

    fn handle_key(&mut self, key: event::KeyEvent) {
        match &self.pantalla {
            // ── Explorador de archivos ───────────────────────────────────
            PantallaActiva::Archivos => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => self.pantalla = PantallaActiva::Principal,
                    KeyCode::Up   | KeyCode::BackTab => self.nav.mover(-1),
                    KeyCode::Down | KeyCode::Tab     => self.nav.mover(1),
                    KeyCode::Left => self.nav.subir(),
                    KeyCode::Right | KeyCode::Enter => {
                        if let Some(ruta) = self.nav.confirmar() {
                            self.abrir_archivo(ruta);
                        }
                    }
                    KeyCode::Backspace => {
                        if self.nav.filtro.is_empty() {
                            self.nav.subir();
                        } else {
                            self.nav.filtro.pop();
                        }
                    }
                    KeyCode::Char(c) => {
                        self.nav.filtro.push(c);
                        // Seleccionar primer resultado del filtro
                        let ids = self.nav.filtradas();
                        if let Some(&i) = ids.first() {
                            self.nav.seleccion = i;
                        }
                    }
                    _ => {}
                }
                return;
            }
            // ── Popup: Vista 3D ──────────────────────────────────────────
            PantallaActiva::Vista3D => {
                match key.code {
                    KeyCode::Left  => { self.auto_rotar = false; self.rot_y -= 0.12; }
                    KeyCode::Right => { self.auto_rotar = false; self.rot_y += 0.12; }
                    KeyCode::Up    => { self.auto_rotar = false; self.rot_x -= 0.12; }
                    KeyCode::Down  => { self.auto_rotar = false; self.rot_x += 0.12; }
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        self.auto_rotar = false;
                        if self.zoom_3d <= 0.0 {
                            let sz = (self.zoom_3d.abs() as usize).max(10);
                            self.zoom_3d = sz as f32 * 0.70 * 1.15;
                        } else { self.zoom_3d *= 1.15; }
                    }
                    KeyCode::Char('-') => {
                        self.auto_rotar = false;
                        if self.zoom_3d <= 0.0 {
                            let sz = (self.zoom_3d.abs() as usize).max(10);
                            self.zoom_3d = sz as f32 * 0.70 / 1.15;
                        } else { self.zoom_3d /= 1.15; }
                    }
                    KeyCode::Char('j') => { self.auto_rotar = false; self.rot_z -= 0.12; }
                    KeyCode::Char('k') => { self.auto_rotar = false; self.rot_z += 0.12; }
                    KeyCode::Char('c') => {
                        self.paleta_3d = (self.paleta_3d + 1)
                            % wireframe::PALETAS_NOMBRE.len() as u8;
                    }
                    KeyCode::Char('w') => {
                        self.modo_wire = (self.modo_wire + 1) % 3;
                    }
                    KeyCode::Char('r') => {
                        self.rot_x = -0.4; self.rot_y = 0.6; self.rot_z = 0.0;
                        self.zoom_3d = 0.0; self.auto_rotar = true;
                    }
                    _ => self.pantalla = PantallaActiva::Principal,
                }
                return;
            }
            // ── Popups simples ───────────────────────────────────────────
            PantallaActiva::Historial | PantallaActiva::Ayuda => {
                self.pantalla = PantallaActiva::Principal;
                return;
            }
            PantallaActiva::Principal => {}
        }

        if let Some(ref mut t) = self.guardado_msg {
            *t = t.saturating_sub(1);
            if *t == 0 { self.guardado_msg = None; }
        }

        match (key.code, key.modifiers) {
            (KeyCode::Tab, _) | (KeyCode::Down, _) => {
                self.foco = (self.foco + 1) % NUM_CAMPOS;
            }
            (KeyCode::BackTab, _) | (KeyCode::Up, _) => {
                self.foco = (self.foco + NUM_CAMPOS - 1) % NUM_CAMPOS;
            }
            (KeyCode::Char('p'), _) => {
                self.perfil_idx = (self.perfil_idx + 1) % config::PERFILES.len();
                self.aplicar_perfil();
            }
            (KeyCode::Char('s'), _) => self.guardar_cotizacion(),
            (KeyCode::Char('h'), _) => self.pantalla = PantallaActiva::Historial,
            (KeyCode::Char('?'), _) => self.pantalla = PantallaActiva::Ayuda,
            (KeyCode::Char('a'), _) => {
                self.nav.filtro.clear();
                self.nav.error = None;
                self.pantalla = PantallaActiva::Archivos;
            }
            (KeyCode::Char('v'), _) if !self.tris_norm.is_empty() => {
                self.pantalla = PantallaActiva::Vista3D;
            }
            (KeyCode::Char('r'), _) => {
                self.campos[self.foco].valor.clear();
            }
            (KeyCode::Char(c), _) if c.is_ascii_digit() || c == '.' || c == ',' => {
                let f = &mut self.campos[self.foco];
                if (c == '.' || c == ',') && (f.valor.contains('.') || f.valor.contains(',')) {
                    return;
                }
                f.valor.push(c);
            }
            (KeyCode::Backspace, _) => {
                self.campos[self.foco].valor.pop();
            }
            _ => {}
        }
    }

    fn abrir_archivo(&mut self, ruta: PathBuf) {
        let ruta_str = ruta.to_string_lossy().to_string();
        let ext = ruta.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        match ext.as_str() {
            "stl" => match archivo::cargar_stl(&ruta_str) {
                Ok(info) => {
                    self.campos[F_GRAMOS].valor = format!("{:.1}", info.gramos_pla);
                    self.tris_norm = wireframe::normalizar(&info.tris);
                    self.normals_suaves = wireframe::calcular_normales_suaves(&self.tris_norm);
                    self.rot_x = -0.4; self.rot_y = 0.6; self.rot_z = 0.0;
                    self.zoom_3d = 0.0; self.auto_rotar = true;
                    self.info_stl = Some(info);
                    self.nav.error = None;
                    self.pantalla = PantallaActiva::Principal;
                }
                Err(e) => self.nav.error = Some(e),
            },
            "gcode" | "gco" | "g" => match archivo::cargar_gcode(&ruta_str) {
                Ok(info) => {
                    if info.tiempo_min > 0.0 {
                        self.campos[F_HORAS].valor = format!("{:.2}", info.tiempo_min / 60.0);
                    }
                    if info.filamento_g > 0.0 {
                        self.campos[F_GRAMOS].valor = format!("{:.1}", info.filamento_g);
                    }
                    self.info_gcode = Some(info);
                    self.nav.error = None;
                    self.pantalla = PantallaActiva::Principal;
                }
                Err(e) => self.nav.error = Some(e),
            },
            _ => {
                self.nav.error = Some(
                    format!("Formato «.{ext}» no soportado  —  usá .stl o .gcode")
                );
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// PALETA  —  celeste y blanco con el oro del Sol de Mayo
// ──────────────────────────────────────────────────────────────────────────────

const CELESTE:    Color = Color::LightBlue;   // bandera argentina
const ORO:        Color = Color::Yellow;       // Sol de Mayo
const BLANCO:     Color = Color::White;
const GRIS:       Color = Color::DarkGray;
const AZUL:       Color = Color::Blue;
const VERDE_USD:  Color = Color::Green;        // para el dólar, obvio

fn st(fg: Color) -> Style { Style::default().fg(fg) }
fn st_bold(fg: Color) -> Style { Style::default().fg(fg).add_modifier(Modifier::BOLD) }
fn st_italic(fg: Color) -> Style { Style::default().fg(fg).add_modifier(Modifier::ITALIC) }

// ──────────────────────────────────────────────────────────────────────────────
// RENDER
// ──────────────────────────────────────────────────────────────────────────────

fn barra(ratio: f64, ancho: usize, color: Color) -> Line<'static> {
    let lleno = ((ratio.clamp(0.0, 1.0) * ancho as f64) as usize).min(ancho);
    let vacio = ancho - lleno;
    Line::from(vec![
        Span::styled("█".repeat(lleno), st(color)),
        Span::styled("░".repeat(vacio), st(GRIS)),
    ])
}

fn render(f: &mut Frame, app: &App) {
    let calc = calcular(&app.campos);
    let size = f.area();

    // Layout: titulo | cuerpo | archivo | status
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(size);

    render_titulo(f, main[0]);

    let cuerpo = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(main[1]);

    render_campos(f, app, cuerpo[0]);
    render_desglose(f, app, &calc, cuerpo[1]);
    render_panel_archivo(f, app, main[2]);
    render_statusbar(f, app, main[3]);

    match &app.pantalla {
        PantallaActiva::Historial => render_popup_historial(f, app, size),
        PantallaActiva::Ayuda     => render_popup_ayuda(f, size),
        PantallaActiva::Archivos  => render_popup_archivos(f, app, size),
        PantallaActiva::Vista3D   => render_popup_3d(f, app, size),
        PantallaActiva::Principal => {}
    }

    // Modal de confirmación de salida (siempre encima de todo)
    if app.confirmar_salida {
        render_confirmar_salida(f, size);
    }
}

fn render_confirmar_salida(f: &mut Frame, area: Rect) {
    let popup = centered_rect(40, 35, area);
    f.render_widget(Clear, popup);

    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st_bold(ORO))
        .title(Span::styled(" ☀  Salir ", st_bold(ORO)));

    let inner = bloque.inner(popup);
    f.render_widget(bloque, popup);

    let texto = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("¿Salir del programa?", st_bold(BLANCO)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Presioná  Q  para confirmar", st(CELESTE)),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Cualquier otra tecla cancela", st(GRIS)),
        ]),
    ];
    f.render_widget(Paragraph::new(texto), inner);
}

fn render_titulo(f: &mut Frame, area: Rect) {
    // Franja superior celeste, texto blanco, acento dorado
    let w = area.width as usize;
    let franja = "▄".repeat(w);

    let lineas = vec![
        Line::from(Span::styled(franja, st(CELESTE))),
        Line::from(vec![
            Span::styled("  ☀  ", st_bold(ORO)),
            Span::styled("CALCULADORA PLA 3D", st_bold(BLANCO)),
            Span::styled("   —   República Argentina", st(CELESTE)),
            Span::styled("   v0.2", st(GRIS)),
        ]),
        Line::from(vec![
            Span::styled("       Flashforge Adventurer 5M", st_italic(GRIS)),
        ]),
    ];

    let bloque = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(st(CELESTE));

    f.render_widget(Paragraph::new(lineas).block(bloque), area);
}

fn render_statusbar(f: &mut Frame, app: &App, area: Rect) {
    let perfil_nombre = config::PERFILES[app.perfil_idx].nombre;
    let msg = if app.guardado_msg.is_some() {
        Line::from(Span::styled("  ✔  Cotización guardada en el historial", st_bold(ORO)))
    } else {
        let v_hint = if app.tris_norm.is_empty() {
            Span::styled("v ", st(GRIS))
        } else {
            Span::styled("v ", st_bold(ORO))
        };
        Line::from(vec![
            Span::styled(" Tab/↑↓ ", st_bold(ORO)), Span::styled("nav  ",      st(BLANCO)),
            Span::styled("0-9/. ",   st_bold(ORO)), Span::styled("editar  ",   st(BLANCO)),
            Span::styled("r ",       st_bold(ORO)), Span::styled("reset  ",    st(BLANCO)),
            Span::styled("p ",       st_bold(ORO)), Span::styled(format!("perfil ({perfil_nombre})  "), st(BLANCO)),
            Span::styled("a ",       st_bold(ORO)), Span::styled("explorar  ", st(BLANCO)),
            v_hint,                                  Span::styled("vista 3D  ", st(BLANCO)),
            Span::styled("s ",       st_bold(ORO)), Span::styled("guardar  ",  st(BLANCO)),
            Span::styled("h ",       st_bold(ORO)), Span::styled("historial  ",st(BLANCO)),
            Span::styled("? ",       st_bold(ORO)), Span::styled("ayuda  ",    st(BLANCO)),
            Span::styled("q ",       st_bold(ORO)), Span::styled("salir",      st(BLANCO)),
        ])
    };
    f.render_widget(
        Paragraph::new(msg).style(Style::default().bg(AZUL)),
        area,
    );
}

// Grupos de campos para el panel izquierdo
const GRUPOS: &[(&str, &[usize])] = &[
    ("MATERIAL",      &[F_PRECIO_KG, F_GRAMOS, F_DESPERDICIO]),
    ("ELECTRICIDAD",  &[F_KWH, F_WATTS, F_HORAS]),
    ("IMPRESORA",     &[F_COSTO_IMP, F_VIDA]),
    ("OPERACION",     &[F_MANT, F_MI_HORA, F_MIS_HORAS]),
    ("EXTRAS",        &[F_POST_PROC, F_ENVIO]),
    ("PRECIO",        &[F_MARGEN, F_DOLAR]),
];

fn render_campos(f: &mut Frame, app: &App, area: Rect) {
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(CELESTE))
        .title(Span::styled(" ☀  PARÁMETROS ", st_bold(CELESTE)));

    let inner = bloque.inner(area);
    f.render_widget(bloque, area);

    let mut lineas: Vec<Line> = Vec::new();

    for (nombre_grupo, indices) in GRUPOS {
        lineas.push(Line::from(vec![
            Span::styled(" ── ", st(GRIS)),
            Span::styled(*nombre_grupo, st_bold(ORO)),
            Span::styled(" ", st(GRIS)),
        ]));

        for &idx in *indices {
            let campo = &app.campos[idx];
            let activo = app.foco == idx;

            let (prefix_sp, label_sp, val_sp, unit_sp) = if activo {
                (
                    Span::styled(" ▶ ", st_bold(ORO)),
                    Span::styled(format!("{:<26}", campo.label), st_bold(BLANCO)),
                    Span::styled(
                        format!("{}▌", campo.display()),
                        Style::default().fg(AZUL).bg(CELESTE).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {}", campo.unidad), st(CELESTE)),
                )
            } else {
                (
                    Span::styled("   ", st(GRIS)),
                    Span::styled(format!("{:<26}", campo.label), st(BLANCO)),
                    Span::styled(campo.display(), st(CELESTE)),
                    Span::styled(format!(" {}", campo.unidad), st(GRIS)),
                )
            };

            lineas.push(Line::from(vec![prefix_sp, label_sp, val_sp, unit_sp]));
        }
    }

    lineas.push(Line::from(""));
    lineas.push(Line::from(Span::styled(
        format!(" ℹ  {}", app.campos[app.foco].hint),
        st_italic(GRIS),
    )));

    f.render_widget(Paragraph::new(lineas), inner);
}

fn render_desglose(f: &mut Frame, app: &App, calc: &Calc, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(10)])
        .split(area);

    render_panel_desglose(f, calc, chunks[0]);
    render_panel_precio(f, app, calc, chunks[1]);
}

fn render_panel_desglose(f: &mut Frame, calc: &Calc, area: Rect) {
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(CELESTE))
        .title(Span::styled(" ☀  DESGLOSE DE COSTOS ", st_bold(CELESTE)));

    let inner = bloque.inner(area);
    f.render_widget(bloque, area);

    let base = calc.costo_base;
    let bar_ancho = (inner.width as usize).saturating_sub(38).max(8).min(20);

    // Todos los colores dentro del espectro celeste-blanco (paleta cohesiva)
    let items: Vec<(String, f64, Color)> = vec![
        (format!("Mat. ({:.1}g)", calc.gramos_reales), calc.costo_mat,  CELESTE),
        (format!("Elec. ({:.2}kWh)", calc.kwh),        calc.costo_elec, Color::Cyan),
        ("Depreciacion".into(),   calc.costo_dep,  AZUL),
        ("Mantenimiento".into(),  calc.costo_mant, Color::LightCyan),
        ("Mano de obra".into(),   calc.costo_mo,   BLANCO),
        ("Post-procesado".into(), calc.costo_post, Color::Gray),
        ("Envio".into(),          calc.costo_envio,CELESTE),
    ];

    let mut lineas: Vec<Line> = vec![Line::from("")];

    for (label, costo, color) in &items {
        if *costo == 0.0 { continue; }
        let pct   = if base > 0.0 { costo / base * 100.0 } else { 0.0 };
        let ratio = if base > 0.0 { costo / base } else { 0.0 };

        lineas.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{:<15}", label.as_str()), st(BLANCO)),
            Span::styled(fmt_pesos(*costo), st_bold(*color)),
            Span::styled(format!(" {:>4.0}%", pct), st(GRIS)),
        ]));

        let mut barra_spans = vec![Span::raw("  ")];
        barra_spans.extend(barra(ratio, bar_ancho, *color).spans);
        lineas.push(Line::from(barra_spans));
        lineas.push(Line::from(""));
    }

    lineas.push(Line::from(Span::styled(
        format!("  {}", "─".repeat((inner.width as usize).saturating_sub(4))),
        st(GRIS),
    )));
    lineas.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<15}", "COSTO BASE"), st_bold(BLANCO)),
        Span::styled(fmt_pesos(calc.costo_base), st_bold(BLANCO)),
    ]));

    let margen = 100.0 * calc.ganancia / calc.costo_base.max(1.0);
    lineas.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<15}", format!("+ Margen {:.0}%", margen)), st(GRIS)),
        Span::styled(fmt_pesos(calc.ganancia), st(GRIS)),
    ]));

    f.render_widget(Paragraph::new(lineas), inner);
}

fn render_panel_precio(f: &mut Frame, _app: &App, calc: &Calc, area: Rect) {
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(ORO))
        .title(Span::styled(" ☀  PRECIO FINAL ", st_bold(ORO)));

    let inner = bloque.inner(area);
    f.render_widget(bloque, area);

    let lineas = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("PRECIO SUGERIDO   ", st_bold(BLANCO)),
            Span::styled(format!("$ {:.0}", calc.precio), st_bold(ORO)),
            Span::styled(format!("   ~  USD {:.2}", calc.precio_usd), st_bold(VERDE_USD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ▸ ", st(ORO)),
            Span::styled(format!("{:<22}", "Redondeado a $50 más:"),  st(GRIS)),
            Span::styled(format!("$ {:.0}", calc.redondeo_50),  st(CELESTE)),
        ]),
        Line::from(vec![
            Span::styled("  ▸ ", st(ORO)),
            Span::styled(format!("{:<22}", "Redondeado a $100 más:"), st(GRIS)),
            Span::styled(format!("$ {:.0}", calc.redondeo_100), st(CELESTE)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Revisá el precio del PLA semanalmente — inflación ARG",
            st_italic(GRIS),
        )),
    ];

    f.render_widget(Paragraph::new(lineas), inner);
}

fn render_popup_historial(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(70, 60, area);
    f.render_widget(Clear, popup);

    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(CELESTE))
        .title(Span::styled(
            " ☀  HISTORIAL DE COTIZACIONES  —  cualquier tecla para cerrar ",
            st_bold(CELESTE),
        ));

    let inner = bloque.inner(popup);
    f.render_widget(bloque, popup);

    let mut lineas: Vec<Line> = vec![Line::from("")];

    if app.historial.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  Sin cotizaciones guardadas. Presioná [s] para guardar la actual.",
            st_italic(GRIS),
        )));
    } else {
        lineas.push(Line::from(vec![
            Span::styled(format!("  {:<20}", "Descripción"), st_bold(CELESTE)),
            Span::styled(format!("{:>14}", "Costo base"),    st_bold(CELESTE)),
            Span::styled(format!("{:>14}", "Precio ARS"),    st_bold(ORO)),
            Span::styled(format!("{:>10}", "USD"),           st_bold(VERDE_USD)),
        ]));
        lineas.push(Line::from(Span::styled(
            format!("  {}", "─".repeat(56)), st(GRIS),
        )));
        for cot in &app.historial {
            lineas.push(Line::from(vec![
                Span::styled(format!("  {:<20}", cot.label),      st(BLANCO)),
                Span::styled(format!("{:>14.0}", cot.costo_base), st(GRIS)),
                Span::styled(format!("{:>14.0}", cot.precio),      st_bold(ORO)),
                Span::styled(format!("{:>10.2}", cot.precio_usd),  st(VERDE_USD)),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lineas), inner);
}

fn render_popup_ayuda(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 72, area);
    f.render_widget(Clear, popup);

    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(CELESTE))
        .title(Span::styled(
            " ☀  AYUDA  —  cualquier tecla para cerrar ",
            st_bold(CELESTE),
        ));

    let inner = bloque.inner(popup);
    f.render_widget(bloque, popup);

    let key = |k: &'static str| Span::styled(k, st_bold(ORO));
    let txt = |t: &'static str| Span::styled(t, st(BLANCO));
    let sec = |t: &'static str| Span::styled(t, st_bold(CELESTE));
    let tip = |t: &'static str| Span::styled(t, st_italic(GRIS));

    let lineas = vec![
        Line::from(""),
        Line::from(sec("  NAVEGACIÓN")),
        Line::from(vec![key("  Tab / ↓       "), txt("Campo siguiente")]),
        Line::from(vec![key("  Shift+Tab / ↑ "), txt("Campo anterior")]),
        Line::from(""),
        Line::from(sec("  EDICIÓN")),
        Line::from(vec![key("  0-9 / .       "), txt("Escribir valor numérico")]),
        Line::from(vec![key("  Backspace     "), txt("Borrar último dígito")]),
        Line::from(vec![key("  r             "), txt("Resetear campo al default")]),
        Line::from(""),
        Line::from(sec("  FUNCIONES")),
        Line::from(vec![key("  s             "), txt("Guardar cotización en historial")]),
        Line::from(vec![key("  h             "), txt("Ver historial de cotizaciones")]),
        Line::from(vec![key("  ?             "), txt("Esta ayuda")]),
        Line::from(vec![key("  q / Esc       "), txt("Salir")]),
        Line::from(""),
        Line::from(sec("  CONSEJOS — ARGENTINA")),
        Line::from(tip("  • Revisá el precio del PLA cada semana.")),
        Line::from(tip("  • Cotizá en USD para encargos de > 1 semana.")),
        Line::from(tip("  • Campo Dólar blue es solo referencia.")),
        Line::from(tip("  • Adventurer 5M: consumo real ~220W promedio.")),
        Line::from(tip("  • Margen ≥ 50% para cubrir inflación ARG.")),
        Line::from(tip("  • Actualizá costo impresora al precio de hoy,")),
        Line::from(tip("    no al precio de compra original.")),
    ];

    f.render_widget(Paragraph::new(lineas), inner);
}

// ──────────────────────────────────────────────────────────────────────────────
// PANEL ARCHIVO (barra inferior fija)
// ──────────────────────────────────────────────────────────────────────────────

fn render_panel_archivo(f: &mut Frame, app: &App, area: Rect) {
    let tiene = app.info_stl.is_some() || app.info_gcode.is_some();
    let bcolor = if tiene { CELESTE } else { GRIS };

    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(bcolor))
        .title(Span::styled(
            " ☀  ARCHIVO  —  [a] abrir STL/G-code  [v] vista 3D ",
            if tiene { st_bold(CELESTE) } else { st(GRIS) },
        ));

    let inner = bloque.inner(area);
    f.render_widget(bloque, area);

    let mut lineas: Vec<Line> = Vec::new();

    if !tiene {
        lineas.push(Line::from(Span::styled(
            "  Sin archivos cargados. Presioná [a] para abrir un STL o G-code.",
            st_italic(GRIS),
        )));
    }

    if let Some(stl) = &app.info_stl {
        lineas.push(Line::from(vec![
            Span::styled("  STL   ", st_bold(ORO)),
            Span::styled(stl.nombre.clone(), st(BLANCO)),
            Span::styled(format!("   {:.2} cm³", stl.volumen_cm3), st(CELESTE)),
            Span::styled(format!("   {:.1} g", stl.gramos_pla), st_bold(ORO)),
            Span::styled(
                format!("   {}×{}×{} mm",
                    stl.dim_mm[0] as u32, stl.dim_mm[1] as u32, stl.dim_mm[2] as u32),
                st(GRIS),
            ),
            Span::styled(format!("   {} tris", stl.triangulos), st(GRIS)),
        ]));
    }

    if let Some(gc) = &app.info_gcode {
        let h = (gc.tiempo_min / 60.0) as u32;
        let m = (gc.tiempo_min % 60.0) as u32;
        let mut spans = vec![
            Span::styled("  GCODE ", st_bold(ORO)),
            Span::styled(gc.nombre.clone(), st(BLANCO)),
            Span::styled(format!("   {}h {:02}min", h, m), st_bold(ORO)),
        ];
        if gc.filamento_g > 0.0 {
            spans.push(Span::styled(format!("   {:.1} g", gc.filamento_g), st(CELESTE)));
        }
        if gc.capas > 0 {
            spans.push(Span::styled(format!("   {} capas", gc.capas), st(GRIS)));
        }
        if gc.temp_hotend > 0 {
            spans.push(Span::styled(format!("   {}°C/{}", gc.temp_hotend, gc.temp_cama), st(GRIS)));
        }
        if !gc.slicer.is_empty() && gc.slicer != "Desconocido" {
            spans.push(Span::styled(format!("   {}", gc.slicer), st_italic(GRIS)));
        }
        lineas.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lineas), inner);
}

// ──────────────────────────────────────────────────────────────────────────────
// POPUP: EXPLORADOR DE ARCHIVOS
// ──────────────────────────────────────────────────────────────────────────────

fn render_popup_archivos(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(72, 78, area);
    f.render_widget(Clear, popup);

    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(CELESTE))
        .title(Span::styled(
            " ☀  EXPLORADOR  —  Enter/→ abrir  ← subir  Escribí para filtrar  Esc cerrar ",
            st_bold(CELESTE),
        ));

    let inner = bloque.inner(popup);
    f.render_widget(bloque, popup);

    // Layout interno: ruta | separador | lista | hint/error
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // ruta actual
            Constraint::Length(1),  // separador
            Constraint::Min(0),     // lista de archivos
            Constraint::Length(1),  // hint / filtro
        ])
        .split(inner);

    // ── Ruta actual ──────────────────────────────────────────────────────────
    let ruta_str = app.nav.directorio.to_string_lossy().to_string();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" / ", st_bold(ORO)),
            Span::styled(ruta_str, st_bold(BLANCO)),
        ])),
        layout[0],
    );

    // ── Separador ────────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(layout[1].width as usize),
            st(GRIS),
        )),
        layout[1],
    );

    // ── Lista de archivos ────────────────────────────────────────────────────
    let nav = &app.nav;
    let ids = nav.filtradas();
    let visible = layout[2].height as usize;

    // Ajustar scroll para mantener la selección visible
    let pos_en_filtradas = ids.iter().position(|&i| i == nav.seleccion).unwrap_or(0);
    let scroll = if pos_en_filtradas < nav.scroll {
        pos_en_filtradas
    } else if pos_en_filtradas >= nav.scroll + visible {
        pos_en_filtradas + 1 - visible
    } else {
        nav.scroll
    };

    let mut lineas: Vec<Line> = Vec::new();

    if ids.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  Sin resultados para el filtro.",
            st_italic(GRIS),
        )));
    }

    for &idx in ids.iter().skip(scroll).take(visible) {
        let e = &nav.entradas[idx];
        let activo = idx == nav.seleccion;

        let (color_nombre, color_meta) = if activo {
            (Style::default().fg(AZUL).bg(CELESTE).add_modifier(Modifier::BOLD),
             Style::default().fg(AZUL).bg(CELESTE))
        } else if e.es_dir {
            (st_bold(BLANCO), st(GRIS))
        } else if e.relevante() {
            (st_bold(ORO), st(GRIS))
        } else {
            (st(GRIS), st(GRIS))
        };

        let prefijo = if activo { " ▶ " } else { "   " };
        let tipo = if e.es_dir {
            "DIR  "
        } else {
            match e.ext.as_str() {
                "stl"                  => "STL  ",
                "gcode" | "gco" | "g"  => "GCODE",
                _                      => "     ",
            }
        };
        let tamaño = if e.es_dir || e.tamaño_kb == 0 {
            String::from("          ")
        } else if e.tamaño_kb >= 1024 {
            format!("{:>6.1} MB  ", e.tamaño_kb as f32 / 1024.0)
        } else {
            format!("{:>6} KB  ", e.tamaño_kb)
        };

        let max_nombre = (layout[2].width as usize).saturating_sub(22);
        let nombre_pad = format!("{:<width$}", e.nombre, width = max_nombre);

        lineas.push(Line::from(vec![
            Span::styled(prefijo, if activo { st_bold(ORO) } else { st(GRIS) }),
            Span::styled(tipo, if e.relevante() { st_bold(ORO) } else { st(GRIS) }),
            Span::styled(" ", st(GRIS)),
            Span::styled(nombre_pad, color_nombre),
            Span::styled(tamaño, color_meta),
        ]));
    }

    f.render_widget(Paragraph::new(lineas), layout[2]);

    // ── Hint / filtro / error ─────────────────────────────────────────────────
    let hint = if let Some(err) = &nav.error {
        Line::from(Span::styled(format!(" ⚠  {}", err), st_bold(Color::LightRed)))
    } else if !nav.filtro.is_empty() {
        Line::from(vec![
            Span::styled(" Filtro: ", st_bold(ORO)),
            Span::styled(
                format!("{}▌", nav.filtro),
                Style::default().fg(AZUL).bg(CELESTE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} resultados)", ids.len()),
                st(GRIS),
            ),
        ])
    } else {
        Line::from(Span::styled(
            "  Escribí para filtrar  ·  STL y G-code resaltados en dorado",
            st_italic(GRIS),
        ))
    };
    f.render_widget(Paragraph::new(vec![hint]), layout[3]);
}

// ──────────────────────────────────────────────────────────────────────────────
// POPUP: VISTA 3D ILUMINADA
// ──────────────────────────────────────────────────────────────────────────────

fn render_popup_3d(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(84, 84, area);
    f.render_widget(Clear, popup);

    let titulo = if let Some(stl) = &app.info_stl {
        format!(
            " ☀  {}  ·  {:.1}g  ·  {:.2}cm³  ─  ←→↑↓ XY  j/k Z  +/- zoom  c color  w wire  r reset ",
            stl.nombre, stl.gramos_pla, stl.volumen_cm3
        )
    } else {
        " ☀  VISTA 3D  ─  ←→↑↓ XY  j/k Z  +/- zoom  c color  w wire  r reset ".to_string()
    };

    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_style(st(CELESTE))
        .title(Span::styled(titulo, st_bold(CELESTE)));

    let inner = bloque.inner(popup);
    f.render_widget(bloque, popup);

    if app.tris_norm.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  Sin modelo STL cargado.", st_italic(GRIS))),
            inner,
        );
        return;
    }

    // Dividir: área 3D | 1 línea de info inferior
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let view_area = layout[0];
    let info_area = layout[1];

    // ── Render 3D ────────────────────────────────────────────────────────────
    let w = view_area.width  as usize;
    let h = view_area.height as usize;

    let mut fb = wireframe::Framebuffer::new(w, h);
    wireframe::renderizar(
        &mut fb, &app.tris_norm, &app.normals_suaves,
        app.rot_x, app.rot_y, app.rot_z,
        app.zoom_3d, app.modo_wire,
    );
    f.render_widget(Paragraph::new(wireframe::fb_a_lineas(&fb, app.paleta_3d)), view_area);

    // ── Barra de info inferior ────────────────────────────────────────────────
    let modo_str = ["Sólido", "Wireframe", "Sólido+Wire"][app.modo_wire as usize];
    let paleta_str = wireframe::PALETAS_NOMBRE[app.paleta_3d as usize];

    let info_line = if let Some(stl) = &app.info_stl {
        Line::from(vec![
            Span::styled("  W ", st_bold(CELESTE)),
            Span::styled(format!("{:>7}  ", stl.dim_mm[0]), st(BLANCO)),
            Span::styled("D ", st_bold(CELESTE)),
            Span::styled(format!("{:>7}  ", stl.dim_mm[1]), st(BLANCO)),
            Span::styled("H ", st_bold(CELESTE)),
            Span::styled(format!("{:.1}mm  ", stl.dim_mm[2]), st(BLANCO)),
            Span::styled("│ ", st(GRIS)),
            Span::styled(paleta_str, st_bold(ORO)),
            Span::styled("  │  ", st(GRIS)),
            Span::styled(modo_str, st(CELESTE)),
        ])
    } else {
        Line::from(vec![
            Span::styled(paleta_str, st_bold(ORO)),
            Span::styled("  │  ", st(GRIS)),
            Span::styled(modo_str, st(CELESTE)),
        ])
    };
    f.render_widget(Paragraph::new(vec![info_line]), info_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ──────────────────────────────────────────────────────────────────────────────
// MAIN
// ──────────────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        // Auto-rotación: ~20 fps mientras el visor esté abierto y activo
        if matches!(app.pantalla, PantallaActiva::Vista3D) && app.auto_rotar {
            app.rot_y += 0.030;
            app.rot_x += 0.011;
        }

        terminal.draw(|f| render(f, &app))?;

        // Con animación usamos poll con timeout; sin animación bloqueamos
        let animando = matches!(app.pantalla, PantallaActiva::Vista3D) && app.auto_rotar;
        if !animando || event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        app.guardar_estado(); break;
                    }
                    (KeyCode::Char('q'), _) | (KeyCode::Esc, _)
                        if matches!(app.pantalla, PantallaActiva::Principal) =>
                    {
                        if app.confirmar_salida { app.guardar_estado(); break; }
                        app.confirmar_salida = true;
                    }
                    _ => {
                        app.confirmar_salida = false;
                        app.handle_key(key);
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
