//! Config en `~/.config/rtop/config.toml`, compartida por `rtop` y `rtop-gui`.
//!
//! ```toml
//! theme = "nord"          # default | nord | gruvbox | dracula | tokyo | mono
//! refresh_ms = 1000
//! show_graphs = true
//! show_cmd = false
//!
//! [colors]                # opcional: pisa colores del tema elegido
//! accent = "#ff8800"
//! ```

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Rgb,
    pub panel: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub accent: Rgb,
    pub ok: Rgb,
    pub warn: Rgb,
    pub hot: Rgb,
    pub sel: Rgb,
    pub mem: Rgb,
}

impl Theme {
    /// verde / amarillo / rojo según porcentaje
    pub fn heat(&self, p: f32) -> Rgb {
        if p < 50.0 {
            self.ok
        } else if p < 80.0 {
            self.warn
        } else {
            self.hot
        }
    }
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "default",
        bg: Rgb(22, 24, 30),
        panel: Rgb(30, 33, 42),
        text: Rgb(225, 228, 235),
        dim: Rgb(110, 115, 130),
        accent: Rgb(130, 170, 255),
        ok: Rgb(120, 220, 140),
        warn: Rgb(240, 200, 90),
        hot: Rgb(240, 100, 100),
        sel: Rgb(45, 55, 80),
        mem: Rgb(200, 140, 255),
    },
    Theme {
        name: "nord",
        bg: Rgb(46, 52, 64),
        panel: Rgb(59, 66, 82),
        text: Rgb(236, 239, 244),
        dim: Rgb(118, 130, 150),
        accent: Rgb(136, 192, 208),
        ok: Rgb(163, 190, 140),
        warn: Rgb(235, 203, 139),
        hot: Rgb(191, 97, 106),
        sel: Rgb(67, 76, 94),
        mem: Rgb(180, 142, 173),
    },
    Theme {
        name: "gruvbox",
        bg: Rgb(40, 40, 40),
        panel: Rgb(50, 48, 47),
        text: Rgb(235, 219, 178),
        dim: Rgb(146, 131, 116),
        accent: Rgb(250, 189, 47),
        ok: Rgb(184, 187, 38),
        warn: Rgb(254, 128, 25),
        hot: Rgb(251, 73, 52),
        sel: Rgb(80, 73, 69),
        mem: Rgb(211, 134, 155),
    },
    Theme {
        name: "dracula",
        bg: Rgb(40, 42, 54),
        panel: Rgb(52, 55, 70),
        text: Rgb(248, 248, 242),
        dim: Rgb(98, 114, 164),
        accent: Rgb(189, 147, 249),
        ok: Rgb(80, 250, 123),
        warn: Rgb(241, 250, 140),
        hot: Rgb(255, 85, 85),
        sel: Rgb(68, 71, 90),
        mem: Rgb(255, 121, 198),
    },
    Theme {
        name: "tokyo",
        bg: Rgb(26, 27, 38),
        panel: Rgb(36, 40, 59),
        text: Rgb(192, 202, 245),
        dim: Rgb(86, 95, 137),
        accent: Rgb(122, 162, 247),
        ok: Rgb(158, 206, 106),
        warn: Rgb(224, 175, 104),
        hot: Rgb(247, 118, 142),
        sel: Rgb(41, 46, 66),
        mem: Rgb(187, 154, 247),
    },
    Theme {
        name: "mono",
        bg: Rgb(18, 18, 18),
        panel: Rgb(28, 28, 28),
        text: Rgb(230, 230, 230),
        dim: Rgb(120, 120, 120),
        accent: Rgb(255, 255, 255),
        ok: Rgb(170, 170, 170),
        warn: Rgb(210, 210, 210),
        hot: Rgb(255, 255, 255),
        sel: Rgb(60, 60, 60),
        mem: Rgb(200, 200, 200),
    },
];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ColorOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub refresh_ms: u64,
    pub show_graphs: bool,
    pub show_cmd: bool,
    /// CPU de cada proceso como % de un núcleo (estilo htop) en vez de % del total
    pub cpu_per_core: bool,
    /// columnas visibles en la tabla de procesos (GUI)
    pub columns: Vec<String>,
    pub colors: ColorOverrides,
}

pub const DEFAULT_COLUMNS: [&str; 9] = ["pid", "name", "user", "cpu", "mem", "gpu", "vram", "disk", "time"];

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: "default".into(),
            refresh_ms: 1000,
            show_graphs: true,
            show_cmd: false,
            cpu_per_core: false,
            columns: DEFAULT_COLUMNS.iter().map(|s| s.to_string()).collect(),
            colors: ColorOverrides::default(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("rtop").join("config.toml"))
    }

    /// Carga la config; si no existe o está rota, usa la default.
    pub fn load() -> Config {
        Self::path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path().ok_or_else(|| std::io::Error::other("sin HOME"))?;
        if let Some(d) = p.parent() {
            fs::create_dir_all(d)?;
        }
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(p, s)
    }

    pub fn refresh_ms(&self) -> u64 {
        self.refresh_ms.clamp(200, 10_000)
    }

    /// Tema elegido con los colores pisados aplicados.
    pub fn theme(&self) -> Theme {
        let mut t = THEMES
            .iter()
            .find(|t| t.name == self.theme)
            .unwrap_or(&THEMES[0])
            .clone();
        let c = &self.colors;
        for (slot, ov) in [
            (&mut t.bg, &c.bg),
            (&mut t.panel, &c.panel),
            (&mut t.text, &c.text),
            (&mut t.dim, &c.dim),
            (&mut t.accent, &c.accent),
            (&mut t.ok, &c.ok),
            (&mut t.warn, &c.warn),
            (&mut t.hot, &c.hot),
            (&mut t.sel, &c.sel),
            (&mut t.mem, &c.mem),
        ] {
            if let Some(rgb) = ov.as_deref().and_then(Rgb::parse) {
                *slot = rgb;
            }
        }
        t
    }

    pub fn next_theme(&mut self) {
        let i = THEMES.iter().position(|t| t.name == self.theme).unwrap_or(0);
        self.theme = THEMES[(i + 1) % THEMES.len()].name.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut c = Config::default();
        c.colors.accent = Some("#ff8800".into());
        let s = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
        assert_eq!(back.theme().accent, Rgb(255, 136, 0));
    }

    #[test]
    fn partial_config() {
        let c: Config = toml::from_str("theme = \"nord\"").unwrap();
        assert_eq!(c.refresh_ms, 1000);
        assert_eq!(c.theme().name, "nord");
    }
}
