//! Batería y fuentes de energía según `/sys/class/power_supply`, más el perfil
//! de energía de `/sys/firmware/acpi/platform_profile`.
//!
//! Algunas baterías informan energía (µWh, µW) y otras carga (µAh, µA); acá
//! todo se pasa a Wh y W usando el voltaje.

use std::{fs, path::Path};

const PS: &str = "/sys/class/power_supply";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BatStatus {
    Charging,
    Discharging,
    Full,
    /// enchufada pero sin cargar (umbral de carga, o llena)
    NotCharging,
    Unknown,
}

impl BatStatus {
    fn parse(s: &str) -> Self {
        match s.trim() {
            "Charging" => Self::Charging,
            "Discharging" => Self::Discharging,
            "Full" => Self::Full,
            "Not charging" => Self::NotCharging,
            _ => Self::Unknown,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "Cargando",
            Self::Discharging => "Descargando",
            Self::Full => "Llena",
            Self::NotCharging => "Conectada, sin cargar",
            Self::Unknown => "Desconocido",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Battery {
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub technology: String,
    pub status: BatStatus,
    /// % informado por el sistema
    pub capacity: f32,
    /// "Normal", "Low", "Critical"...
    pub level: String,
    /// Wh
    pub energy_now: Option<f32>,
    pub energy_full: Option<f32>,
    pub energy_design: Option<f32>,
    /// el sistema avisa al bajar de esto (Wh)
    pub alarm: Option<f32>,
    /// W, siempre positivo: entrando o saliendo según `status`
    pub power: Option<f32>,
    /// V
    pub voltage: Option<f32>,
    pub voltage_design: Option<f32>,
    pub cycles: Option<u32>,
    /// umbrales de carga, si el firmware los expone (%)
    pub start_threshold: Option<u32>,
    pub end_threshold: Option<u32>,
}

impl Battery {
    /// capacidad que le queda respecto de la de fábrica (%)
    pub fn health(&self) -> Option<f32> {
        match (self.energy_full, self.energy_design) {
            (Some(f), Some(d)) if d > 0.0 => Some(f * 100.0 / d),
            _ => None,
        }
    }

    /// horas hasta vaciarse (descargando) o hasta llenarse (cargando)
    pub fn hours_left(&self) -> Option<f32> {
        let p = self.power.filter(|p| *p > 0.05)?;
        let now = self.energy_now?;
        match self.status {
            BatStatus::Discharging => Some(now / p),
            BatStatus::Charging => {
                // con umbral de carga, se llena hasta ahí
                let full = self.energy_full? * self.end_threshold.map_or(1.0, |t| t as f32 / 100.0);
                Some((full - now).max(0.0) / p)
            }
            _ => None,
        }
    }
}

/// Cargador, USB-C, etc.
#[derive(Clone, Debug)]
pub struct Adapter {
    pub name: String,
    /// "Mains", "USB"...
    pub kind: String,
    pub online: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Power {
    pub batteries: Vec<Battery>,
    pub adapters: Vec<Adapter>,
    /// perfil actual y los disponibles ("balanced", "low-power"...)
    pub profile: Option<String>,
    pub profiles: Vec<String>,
}

impl Power {
    pub fn read() -> Power {
        let mut p = Power::default();
        if let Ok(rd) = fs::read_dir(PS) {
            let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for d in dirs {
                let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                match get(&d, "type").as_deref() {
                    // "scope=Device" son mouses, joysticks...: no son del equipo
                    Some("Battery") if get(&d, "scope").as_deref() != Some("Device") => {
                        if get(&d, "present").as_deref() != Some("0") {
                            p.batteries.push(read_battery(&d, name));
                        }
                    }
                    Some(kind @ ("Mains" | "USB" | "USB_C" | "USB_PD")) => {
                        p.adapters.push(Adapter { name, kind: kind.into(), online: get(&d, "online").as_deref() == Some("1") })
                    }
                    _ => {}
                }
            }
        }
        let fw = Path::new("/sys/firmware/acpi");
        p.profile = get(fw, "platform_profile");
        p.profiles = get(fw, "platform_profile_choices").map(|s| s.split_whitespace().map(String::from).collect()).unwrap_or_default();
        p
    }

    /// enchufado a algo (None si no hay cargadores que informen)
    pub fn on_ac(&self) -> Option<bool> {
        (!self.adapters.is_empty()).then(|| self.adapters.iter().any(|a| a.online))
    }

    /// todas las baterías como una sola (la mayoría de las notebooks tiene una)
    pub fn total(&self) -> Option<Battery> {
        let first = self.batteries.first()?.clone();
        if self.batteries.len() == 1 {
            return Some(first);
        }
        let sum = |f: fn(&Battery) -> Option<f32>| self.batteries.iter().map(f).sum::<Option<f32>>();
        let (now, full) = (sum(|b| b.energy_now), sum(|b| b.energy_full));
        let status = if self.batteries.iter().any(|b| b.status == BatStatus::Charging) {
            BatStatus::Charging
        } else if self.batteries.iter().any(|b| b.status == BatStatus::Discharging) {
            BatStatus::Discharging
        } else {
            first.status
        };
        let capacity = match (now, full) {
            (Some(n), Some(f)) if f > 0.0 => n * 100.0 / f,
            _ => self.batteries.iter().map(|b| b.capacity).sum::<f32>() / self.batteries.len() as f32,
        };
        Some(Battery {
            name: "todas".into(),
            status,
            capacity,
            energy_now: now,
            energy_full: full,
            energy_design: sum(|b| b.energy_design),
            alarm: sum(|b| b.alarm),
            power: sum(|b| b.power),
            ..first
        })
    }
}

fn get(dir: &Path, f: &str) -> Option<String> {
    fs::read_to_string(dir.join(f)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn num(dir: &Path, f: &str) -> Option<f64> {
    get(dir, f)?.parse().ok()
}

fn read_battery(d: &Path, name: String) -> Battery {
    let micro = |f: &str| num(d, f).map(|v| v / 1e6);
    let voltage = micro("voltage_now");
    let voltage_design = micro("voltage_min_design");
    // carga en Ah -> energía en Wh (con el voltaje de diseño, que es el que usa el fabricante)
    let v_ref = voltage_design.or(voltage);
    let energy = |e: &str, c: &str| micro(e).or_else(|| Some(micro(c)? * v_ref?)).map(|v| v as f32);
    let power = micro("power_now").or_else(|| Some(micro("current_now")? * voltage?)).map(|w| w.abs() as f32);
    let energy_now = energy("energy_now", "charge_now");
    let energy_full = energy("energy_full", "charge_full");
    let capacity = num(d, "capacity").map(|c| c as f32).or_else(|| match (energy_now, energy_full) {
        (Some(n), Some(f)) if f > 0.0 => Some(n * 100.0 / f),
        _ => None,
    });
    Battery {
        name,
        manufacturer: get(d, "manufacturer").unwrap_or_default(),
        model: get(d, "model_name").unwrap_or_default(),
        technology: get(d, "technology").unwrap_or_default(),
        status: get(d, "status").map(|s| BatStatus::parse(&s)).unwrap_or(BatStatus::Unknown),
        capacity: capacity.unwrap_or(0.0).clamp(0.0, 100.0),
        level: get(d, "capacity_level").unwrap_or_default(),
        energy_now,
        energy_full,
        energy_design: energy("energy_full_design", "charge_full_design"),
        // `alarm` está en las mismas unidades que energy_now o charge_now
        alarm: micro("alarm").filter(|v| *v > 0.0).map(|v| if num(d, "energy_now").is_some() { v } else { v * v_ref.unwrap_or(0.0) } as f32),
        power,
        voltage: voltage.map(|v| v as f32),
        voltage_design: voltage_design.map(|v| v as f32),
        cycles: num(d, "cycle_count").filter(|c| *c > 0.0).map(|c| c as u32),
        start_threshold: num(d, "charge_control_start_threshold").map(|v| v as u32),
        end_threshold: num(d, "charge_control_end_threshold").map(|v| v as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bat(status: BatStatus, now: f32, full: f32, power: f32) -> Battery {
        Battery {
            name: "BAT0".into(),
            manufacturer: String::new(),
            model: String::new(),
            technology: String::new(),
            status,
            capacity: now * 100.0 / full,
            level: String::new(),
            energy_now: Some(now),
            energy_full: Some(full),
            energy_design: Some(64.0),
            alarm: None,
            power: Some(power),
            voltage: None,
            voltage_design: None,
            cycles: None,
            start_threshold: None,
            end_threshold: None,
        }
    }

    #[test]
    fn time_left() {
        assert_eq!(bat(BatStatus::Discharging, 30.0, 60.0, 10.0).hours_left(), Some(3.0));
        assert_eq!(bat(BatStatus::Charging, 30.0, 60.0, 15.0).hours_left(), Some(2.0));
        let mut b = bat(BatStatus::Charging, 30.0, 60.0, 15.0);
        b.end_threshold = Some(80);
        assert_eq!(b.hours_left(), Some(1.2));
        assert_eq!(bat(BatStatus::Full, 60.0, 60.0, 0.0).hours_left(), None);
        assert_eq!(bat(BatStatus::Discharging, 30.0, 60.0, 0.0).hours_left(), None);
    }

    #[test]
    fn health() {
        assert_eq!(bat(BatStatus::Full, 48.0, 48.0, 0.0).health(), Some(75.0));
    }
}
