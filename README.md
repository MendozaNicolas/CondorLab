# 🦅 CondorLab

**Calculadora de impresión 3D y slicer para el mercado argentino.**

Aplicación TUI (Terminal User Interface) escrita en Rust que permite calcular costos de impresión 3D y slicear modelos STL directamente desde la terminal.

---

## Características

### Calculadora de costos
- 15 parámetros organizados en 6 grupos: material, electricidad, impresora, operación, extras y precio
- Desglose visual de costos con barras de progreso
- Tres sugerencias de precio: exacto, redondeado a $50 y a $100
- Conversión automática a dólares (referencia blue)
- Historial de cotizaciones persistente con estadísticas

### Archivos
- Carga de modelos **STL**: calcula volumen, peso, dimensiones y cantidad de triángulos
- Carga de archivos **G-code**: extrae tiempo de impresión, filamento, capas, temperaturas y slicer origen
- Soporte para PrusaSlicer, OrcaSlicer, Cura, FlashPrint y SuperSlicer
- Explorador de archivos integrado con filtro en tiempo real
- Lista de archivos recientes (últimos 10)

### Vista 3D
- Render wireframe ASCII en tiempo real con iluminación Blinn-Phong
- Shading Gouraud con normales suaves por vértice
- 5 paletas de color (Celeste Arg., Rojo, Verde, Plata, Dorado Sol)
- 3 modos de render: sólido, wireframe, sólido + wireframe
- Rotación manual (flechas, j/k) y auto-rotación
- Zoom con `+` / `-`

### Perfiles preconfigurados
- **5 impresoras**: Flashforge Adventurer 5M, Creality Ender 3 V2, Prusa MK4, Bambu Lab A1 Mini, Creality K1 + Personalizada
- **6 materiales**: PLA, PETG, ABS, TPU 95A, ASA, PETG-CF (con densidad, precios y temperaturas)

### Slicer (en desarrollo)
- Corte de mesh STL en capas horizontales (intersección plano/triángulo)
- Construcción de contornos cerrados desde segmentos sin ordenar
- Shells de perímetro mediante offset de polígonos con [Clipper2](https://github.com/AngusJohnson/Clipper2)
- Generación de G-code con extrusión calculada, start/end G-code y purga
- Estimación de filamento y tiempo de impresión
- 28 tests unitarios e integración

---

## Instalación

### Requisitos
- Rust 1.85+ (edition 2024)
- Terminal con soporte de color TrueColor (recomendado)

### Compilar y ejecutar

```bash
git clone https://github.com/tu-usuario/CondorLab
cd CondorLab
cargo run --release
```

---

## Uso

| Tecla | Acción |
|-------|--------|
| `Tab` / `↑↓` | Navegar campos |
| `0-9` `.` | Ingresar valor |
| `r` | Resetear campo al default |
| `p` | Ciclar perfil de impresora |
| `m` | Ciclar material (actualiza precio y peso) |
| `a` | Abrir explorador de archivos |
| `f` | Archivos recientes |
| `v` | Vista 3D (requiere STL cargado) |
| `s` | Guardar cotización en historial |
| `h` | Ver historial |
| `?` | Ayuda |
| `q` / `Esc` | Salir |

### En la vista 3D
| Tecla | Acción |
|-------|--------|
| `←→↑↓` | Rotar en XY |
| `j` / `k` | Rotar en Z |
| `+` / `-` | Zoom |
| `c` | Cambiar paleta de color |
| `w` | Cambiar modo de render |
| `r` | Resetear y activar auto-rotación |
| Cualquier otra | Volver a la calculadora |

---

## Estructura del proyecto

```
src/
├── main.rs          — UI principal (Ratatui), estado de la app, manejo de eventos
├── config.rs        — Perfiles de impresora/material, persistencia JSON
├── archivo.rs       — Parsers STL y G-code
├── wireframe.rs     — Motor de render 3D ASCII (Blinn-Phong, Gouraud)
└── slicer/
    ├── mod.rs       — API pública: SlicerConfig, Capa, ResultadoSlicing, slicear()
    ├── slice.rs     — Intersección triángulo/plano → segmentos 2D
    ├── contour.rs   — Loop builder: segmentos → polígonos cerrados
    ├── perimeter.rs — Shells de perímetro con Clipper2
    └── gcode.rs     — Generación de G-code
```

### Configuración persistente

Los datos se guardan en `~/.config/condorlab/`:
- `config.json` — valores de campos, perfil, material y archivos recientes
- `historial.json` — cotizaciones guardadas

---

## Stack tecnológico

| Crate | Versión | Uso |
|-------|---------|-----|
| `ratatui` | 0.29 | Framework TUI |
| `crossterm` | 0.28 | Control de terminal |
| `stl_io` | 0.7 | Parser STL |
| `serde` / `serde_json` | 1 | Serialización JSON |
| `clipper2` | 0.5 | Offsetting de polígonos (slicer) |

---

## Tests

```bash
cargo test              # todos los tests
cargo test slicer       # solo tests del slicer
```

28 tests cubren: intersección plano/triángulo, construcción de contornos, offsetting de perímetros, integración completa con mesh de cubo unitario.
