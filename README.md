# rtop

**Un monitor del sistema para Linux, escrito en Rust. En la terminal o en una ventana, como prefieras.**

[![deb](https://github.com/santiagofr1/rtop/actions/workflows/deb.yml/badge.svg)](https://github.com/santiagofr1/rtop/actions/workflows/deb.yml)
[![Licencia: MIT](https://img.shields.io/badge/licencia-MIT-blue.svg)](LICENSE)
[![PRs bienvenidos](https://img.shields.io/badge/PRs-bienvenidos-brightgreen.svg)](CONTRIBUTING.md)

rtop te muestra qué está haciendo tu máquina: CPU por núcleo, memoria, GPUs (NVIDIA, AMD e Intel), red con conexiones por proceso, discos, temperaturas, batería y la lista de procesos, con historial y gráficos. Viene en dos sabores que comparten el mismo motor:

- **`rtop`**: la TUI, para la terminal o por SSH. Rápida, con teclas estilo htop/vim y temas.
- **`rtop-gui`**: una app de escritorio hecha con egui, con páginas de detalle y gráficos.

## Qué tiene

- **CPU**: uso total y por núcleo, frecuencias e historial.
- **Memoria**: RAM y swap con desglose (caché, buffers, etc.).
- **GPU**: uso, VRAM y temperatura. NVIDIA vía `nvidia-smi`, AMD e Intel vía DRM.
- **Red**: tráfico por interfaz y qué conexiones abrió cada proceso.
- **Disco**: espacio, lectura y escritura.
- **Sensores**: todas las temperaturas que expone el sistema, sensor por sensor.
- **Batería**: estado, consumo actual y una estimación de cuánto consume cada proceso.
- **Procesos**: tabla ordenable con filtro y detalle. También podés matarlos (SIGTERM o SIGKILL).
- **Temas**: `default`, `nord`, `gruvbox`, `dracula`, `tokyo` y `mono`, más colores propios.

## Instalación

### Debian / Ubuntu y derivadas: repo apt (recomendado)

Agregás el repo una vez y después rtop se actualiza con el resto del sistema:

```sh
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://santiagofr1.github.io/apt/key.asc | sudo gpg --dearmor -o /etc/apt/keyrings/santiagofr1.gpg
echo "deb [signed-by=/etc/apt/keyrings/santiagofr1.gpg] https://santiagofr1.github.io/apt stable main" | sudo tee /etc/apt/sources.list.d/santiagofr1.list
sudo apt update
sudo apt install rtop
```

Para actualizar: `sudo apt update && sudo apt upgrade`. El repo está firmado con GPG; el código que lo arma está en [santiagofr1/apt](https://github.com/santiagofr1/apt).

### Debian / Ubuntu: .deb suelto

Cada push a `master` compila y publica un `.deb` nuevo en [Releases](https://github.com/santiagofr1/rtop/releases):

```sh
wget https://github.com/santiagofr1/rtop/releases/latest/download/rtop_amd64.deb
sudo apt install ./rtop_amd64.deb
```

Trae los dos binarios y un acceso directo a la GUI en el menú de aplicaciones.

### Desde el código

Necesitás [Rust](https://rustup.rs) estable:

```sh
git clone https://github.com/santiagofr1/rtop
cd rtop
cargo build --release
./target/release/rtop        # TUI
./target/release/rtop-gui    # GUI
```

Para compilar la GUI en Debian/Ubuntu pueden hacer falta estos paquetes:
`libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev`.

## Uso

```sh
rtop          # en la terminal
rtop --gui    # abre la ventana (igual que rtop-gui)
```

Teclas de la TUI:

| Tecla | Acción |
|---|---|
| `↑↓` / `j k`, `PgUp PgDn`, `g G` | moverse |
| `/` o `F3` | filtrar procesos |
| `c m n i t` | ordenar por CPU, memoria, nombre, PID o tiempo |
| `s` / `Tab` / `F6` | cambiar el orden · `r` invertirlo |
| `x` / `F9` / `Supr` | matar el proceso (`9` para SIGKILL) |
| `p` / `Espacio` | pausar |
| `l` | mostrar la línea de comando completa |
| `v` | mostrar u ocultar los gráficos |
| `T` | cambiar de tema |
| `+ -` | ajustar el refresco |
| `?` / `F1` | ayuda |
| `q` / `F10` | salir |

## Configuración

`~/.config/rtop/config.toml` es la misma para la TUI y la GUI. Los cambios que hacés desde la app se guardan solos.

```toml
theme = "nord"          # default | nord | gruvbox | dracula | tokyo | mono
refresh_ms = 1000
show_graphs = true
show_cmd = false

[colors]                # opcional: cambia colores del tema elegido
accent = "#ff8800"
```

## Cómo está armado

```
crates/
├── rtop-core   # el motor: recolecta y parsea datos del sistema (sin UI, con tests)
├── rtop        # TUI con ratatui
└── rtop-gui    # GUI con egui/eframe
```

Todo lo que lee del sistema vive en `rtop-core`. Si querés sumar una métrica nueva, empezás ahí y las dos interfaces la pueden usar.

## Es tuyo también: forkealo, rompelo, mejoralo

rtop es **software libre** bajo licencia MIT. Lo podés usar, copiar, modificar y redistribuir como quieras, incluso en proyectos comerciales.

El software libre funciona porque la gente se anima a meter mano. No hace falta ser experto en Rust ni pedir permiso:

- **Forkealo** y hacé tu propia versión: otro layout, otra paleta, métricas que a vos te sirvan. Si sale algo lindo, contalo.
- **Mandá un PR**, por chico que sea. Arreglar un typo, sumar un tema de colores o mejorar un parser cuenta igual.
- **Abrí un issue** si algo no anda en tu hardware. Con tantas GPUs, sensores y baterías distintas, tu reporte es lo que hace que funcione para todos.
- **Proponé ideas**: soporte para otras distros, empaquetado para Arch/Fedora, Flatpak, nuevas páginas, exportar datos…

Algunas buenas primeras contribuciones:

- [ ] Temas de colores nuevos (`crates/rtop-core/src/config.rs`)
- [ ] Ícono propio para la app
- [ ] Paquetes para otras distros (AUR, RPM, Flatpak)
- [ ] Más tests para los parsers de `rtop-core`
- [ ] Traducción al inglés

La guía para arrancar está en [CONTRIBUTING.md](CONTRIBUTING.md).

## Licencia

[MIT](LICENSE). Hacé lo que quieras con el código, solo mantené el aviso de copyright.
