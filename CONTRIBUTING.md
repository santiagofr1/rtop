# Contributing to rtop

Thanks for diving in! Every contribution helps: code, issues, ideas, docs, or color themes.

## Getting started

```sh
git clone https://github.com/<your-user>/rtop   # your fork
cd rtop
cargo run -p rtop          # TUI
cargo run -p rtop-gui      # GUI
cargo test --workspace
```

## Before sending a PR

- `cargo test --workspace` must pass.
- `cargo clippy --workspace` with no new warnings.
- One PR per topic. For a big change, open an issue first to discuss it.
- If you touch anything that reads from the system (`rtop-core`), add a test with a sample of the real file
  (`/proc/...`, `/sys/...`). That way it can be tested without your hardware.

## Where things live

- `crates/rtop-core`: data collection and parsing. No UI.
- `crates/rtop`: TUI (ratatui).
- `crates/rtop-gui`: GUI (egui). `cargo run -p rtop-gui --features devtools` enables hooks for testing the UI
  (see `devtools.rs`).

## Reporting a bug

Tell us your distro, your hardware (GPU, laptop or desktop), and what you expected to see. If it's a problem
reading data, paste the contents of the relevant `/proc` or `/sys` file.

By contributing, you agree that your contribution is published under the project's [MIT](LICENSE) license.
