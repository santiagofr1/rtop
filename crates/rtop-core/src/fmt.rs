/// 1536 -> "1.5K"
pub fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b}B")
    } else if v >= 100.0 {
        format!("{v:.0}{}", U[i])
    } else {
        format!("{v:.1}{}", U[i])
    }
}

/// segundos -> "01:02:03" o "3d04h"
pub fn hms(s: u64) -> String {
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h >= 24 {
        format!("{}d{:02}h", h / 24, h % 24)
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}
