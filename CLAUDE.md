# CondorLab — Contexto para Claude

## Qué es este proyecto

TUI en Rust para calcular costos de impresión 3D (mercado argentino) y un slicer básico en desarrollo. La app usa Ratatui + Crossterm. Los datos persisten en `~/.config/condorlab/`.

## Cómo compilar y testear

```bash
cargo build
cargo test
cargo test slicer   # solo tests del slicer
cargo run
```

## Estructura de archivos clave

```
src/
├── main.rs          — Estado (App), render (Ratatui), handle_key, abrir_archivo
├── config.rs        — PERFILES[], MATERIALES[], AppConfig, Cotizacion
├── archivo.rs       — cargar_stl() → InfoSTL,  cargar_gcode() → InfoGcode
├── wireframe.rs     — Framebuffer, renderizar(), fb_a_lineas(), calcular_normales_suaves()
└── slicer/
    ├── mod.rs       — SlicerConfig, Capa { z, perimetros }, ResultadoSlicing, slicear()
    ├── slice.rs     — cortar_capa(), intersectar_triangulo() [pub(crate)]
    ├── contour.rs   — construir_contornos()  ← snapping a 0.001mm
    ├── perimeter.rs — generar_shells()  ← usa clipper2::inflate()
    └── gcode.rs     — generar_gcode()
```

## Convenciones del código

- **Idioma:** todo en español (nombres de variables, comentarios, UI, mensajes de error)
- **Estilo:** sin unwrap en producción excepto donde el error es imposible; usar `unwrap_or` / `map_or`
- **Tests:** `#[cfg(test)]` al final de cada módulo, nombres descriptivos en español
- **Sin abstracciones prematuras:** si algo se usa una sola vez, no crear helper

## Estado actual del slicer (v0.2)

**Implementado:**
- `slice.rs` — intersección plano/triángulo → segmentos 2D
- `contour.rs` — une segmentos en loops cerrados (HashMap + snapping)
- `perimeter.rs` — shells de perímetro con Clipper2 (offset inward)
- `gcode.rs` — genera G-code con start/end, purga, extrusión absoluta acumulada
- 28 tests pasando

**NO implementado todavía:**
- Infill (relleno) — la próxima feature grande
- Soportes automáticos
- Primera capa especial (más lenta, más pegada)
- Retracción / unretracción entre viajes
- Interfaz TUI para configurar y disparar el slicer (solo existe como módulo Rust)
- Detección de islas flotantes / overhangs

## Próximos pasos recomendados

1. **Infill rectilinear** — líneas de relleno dentro del contorno interior. Requiere intersectar líneas paralelas con el contorno más interno de cada isla.
2. **Integración TUI** — nueva pantalla `PantallaActiva::Slicer` con `SlicerConfig` editable y tecla para ejecutar + guardar `.gcode`.
3. **Primera capa** — velocidad reducida (50% de velocidad_impresion) y altura ligeramente mayor para adhesión.

## Bugs conocidos / limitaciones

- El plano z=z_max no produce contornos (los triángulos del techo quedan con d=0, que no se cuenta como "encima"). La última capa se pierde si el modelo tiene altura múltiplo exacto de layer_height. Workaround: usar alturas no exactas o agregar epsilon en `cortar_capa`.
- El slicer solo genera perímetros, sin infill. El G-code resultante es un esqueleto hueco.
- No hay retracción entre viajes: posibles hilos (stringing).

## Tipos importantes

```rust
// config.rs
struct PerfilMaterial { nombre, densidad: f64, precio_ref_kg, temp_hotend, temp_cama, nota }
struct PerfilImpresora { nombre, watts, costo_ars, vida_hs, mant_hr }
const MATERIALES: &[PerfilMaterial]  // 6 materiales
const PERFILES:   &[PerfilImpresora] // 6 impresoras + personalizada

// archivo.rs
struct InfoSTL { nombre, triangulos, volumen_cm3, gramos, dim_mm, tris }
  // gramos = volumen_cm3 * densidad_material (NO hardcodeado a PLA)

// slicer/mod.rs
struct SlicerConfig { altura_capa, diametro_boquilla, diametro_filamento,
                      n_perimetros, velocidad_impresion, velocidad_viaje,
                      temp_hotend, temp_cama, nombre_archivo }
struct Capa { z: f32, perimetros: Vec<Vec<Contorno>> }
  // perimetros[isla][shell]:  [0] = exterior, [1..] = interiores
type Contorno = Vec<[f32; 2]>
```

## Cómo agregar un material o impresora

Editar `src/config.rs` directamente. Son arrays estáticos (`const`). El índice en `MATERIALES` se guarda en `AppConfig.material_idx`.

## Dependencias importantes

- `clipper2 = "0.5"` — offsetting de polígonos. API: `inflate(paths: impl Into<Paths>, delta: f64, JoinType, EndType, miter_limit) -> Paths`. Conversiones: `Vec<(f64,f64)>` ↔ `Paths` via `.into()`.
- `stl_io = "0.7"` — leer STL binario y ASCII
- `ratatui = "0.29"` — widgets: `Paragraph`, `Block`, `Layout`, `Span`, `Line`
