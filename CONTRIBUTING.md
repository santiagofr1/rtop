# Cómo contribuir a rtop

¡Gracias por meter mano! Cualquier aporte suma: código, issues, ideas, docs o temas de colores.

## Arrancar

```sh
git clone https://github.com/<tu-usuario>/rtop   # tu fork
cd rtop
cargo run -p rtop          # TUI
cargo run -p rtop-gui      # GUI
cargo test --workspace
```

## Antes de mandar un PR

- `cargo test --workspace` tiene que pasar.
- `cargo clippy --workspace` sin advertencias nuevas.
- Un PR por tema. Si es un cambio grande, conviene abrir un issue antes para charlarlo.
- Si tocás algo que lee del sistema (`rtop-core`), sumá un test con una muestra del archivo real (`/proc/...`, `/sys/...`). Así se puede probar sin tu hardware.

## Dónde está cada cosa

- `crates/rtop-core`: recolección y parseo de datos. Sin UI.
- `crates/rtop`: TUI (ratatui).
- `crates/rtop-gui`: GUI (egui). `cargo run -p rtop-gui --features devtools` habilita ganchos para probar la UI (ver `devtools.rs`).

## Reportar un bug

Contá tu distro, tu hardware (GPU, notebook o PC) y qué esperabas ver. Si es un problema de lectura de datos, pegá el contenido del archivo de `/proc` o `/sys` involucrado.

Al contribuir aceptás que tu aporte se publique bajo la licencia [MIT](LICENSE) del proyecto.
