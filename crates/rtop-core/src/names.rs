//! Nombres legibles para sensores y procesadores.

/// Nombres de sensores más legibles.
pub fn pretty_sensor(label: &str) -> String {
    let l = label.trim();
    let map = [
        ("Tctl", "CPU (Tctl)"),
        ("Tdie", "CPU (Tdie)"),
        ("edge", "GPU integrada"),
        ("junction", "GPU (junction)"),
        ("Composite", "NVMe"),
        ("Sensor 1", "NVMe sensor 1"),
        ("Sensor 2", "NVMe sensor 2"),
        ("spd5118 temp1", "RAM"),
        ("acpitz temp1", "Placa (ACPI 1)"),
        ("acpitz temp2", "Placa (ACPI 2)"),
    ];
    for (k, v) in map {
        if l == k || l.ends_with(&format!(" {k}")) {
            return v.to_string();
        }
    }
    if l.starts_with("Package id") {
        return "CPU (Package)".into();
    }
    l.to_string()
}

/// "AMD Ryzen 7 7445HS w/ Radeon 740M Graphics" -> "Ryzen 7 7445HS"
/// "Intel(R) Core(TM) i7-10750H CPU @ 2.60GHz" -> "Core i7-10750H"
pub fn short_cpu_name(brand: &str) -> String {
    let mut s = brand.to_string();
    for cut in [" w/ ", " with ", " CPU @", " @ "] {
        if let Some(i) = s.find(cut) {
            s.truncate(i);
        }
    }
    for junk in ["(R)", "(TM)", "(tm)", "AMD ", "Intel ", " Processor", " CPU", "-Core", " Core Processor"] {
        s = s.replace(junk, "");
    }
    // "Ryzen 9 5900X 12" (cores al final) -> fuera
    let mut w: Vec<&str> = s.split_whitespace().collect();
    if w.len() > 3 && w.last().is_some_and(|t| t.len() <= 3 && t.chars().all(|c| c.is_ascii_digit())) {
        w.pop();
    }
    let s = w.join(" ");
    if s.is_empty() { brand.trim().to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_names() {
        assert_eq!(short_cpu_name("AMD Ryzen 7 7445HS w/ Radeon 740M Graphics"), "Ryzen 7 7445HS");
        assert_eq!(short_cpu_name("Intel(R) Core(TM) i7-10750H CPU @ 2.60GHz"), "Core i7-10750H");
        assert_eq!(short_cpu_name("AMD Ryzen 9 5900X 12-Core Processor"), "Ryzen 9 5900X");
    }
    #[test]
    fn sensors() {
        assert_eq!(pretty_sensor("Tctl"), "CPU (Tctl)");
        assert_eq!(pretty_sensor("nvme Composite"), "NVMe");
        assert_eq!(pretty_sensor("raro"), "raro");
    }
}
