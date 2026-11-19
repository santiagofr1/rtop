//! Qué procesos tienen conexiones abiertas y por qué interfaz salen.
//!
//! Sin root no se puede medir cuántos bytes manda cada proceso (eso necesita
//! capturar paquetes, como nethogs). Lo que sí se puede: leer las conexiones
//! de `/proc/net/{tcp,udp}{,6}`, ver a qué proceso pertenece cada socket
//! (por el inode en `/proc/<pid>/fd`) y deducir la interfaz por la IP local.

use std::{collections::HashMap, fs, net::IpAddr};

#[derive(Clone, Debug)]
pub struct Conn {
    pub pid: u32,
    pub name: String,
    pub local: IpAddr,
    pub remote: IpAddr,
    pub remote_port: u16,
    pub tcp: bool,
}

/// "0100007F:0035" -> (127.0.0.1, 53). Las palabras de 32 bits vienen en el
/// orden de bytes de la máquina.
fn parse_addr(s: &str) -> Option<(IpAddr, u16)> {
    let (ip, port) = s.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let word = |h: &str| u32::from_str_radix(h, 16).ok().map(|v| v.to_ne_bytes());
    let ip = match ip.len() {
        8 => IpAddr::from(word(ip)?),
        32 => {
            let mut b = [0u8; 16];
            for i in 0..4 {
                b[i * 4..i * 4 + 4].copy_from_slice(&word(&ip[i * 8..i * 8 + 8])?);
            }
            let v6 = std::net::Ipv6Addr::from(b);
            match v6.to_ipv4_mapped() {
                Some(v4) => IpAddr::V4(v4),
                None => IpAddr::V6(v6),
            }
        }
        _ => return None,
    };
    Some((ip, port))
}

/// Conexiones establecidas (TCP) o conectadas (UDP) de los procesos que se
/// pueden leer (los tuyos; los de root no sin permisos).
pub fn scan() -> Vec<Conn> {
    // inode -> (local, remoto, puerto, tcp)
    let mut socks: HashMap<u64, (IpAddr, IpAddr, u16, bool)> = HashMap::new();
    for (file, tcp) in [("tcp", true), ("tcp6", true), ("udp", false), ("udp6", false)] {
        let Ok(text) = fs::read_to_string(format!("/proc/net/{file}")) else { continue };
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "01" {
                continue; // 01 = ESTABLISHED (en UDP: conectado)
            }
            let (Some((l, _)), Some((r, rp))) = (parse_addr(f[1]), parse_addr(f[2])) else { continue };
            if l.is_loopback() || r.is_unspecified() {
                continue;
            }
            if let Ok(inode) = f[9].parse::<u64>() {
                if inode != 0 {
                    socks.insert(inode, (l, r, rp, tcp));
                }
            }
        }
    }
    if socks.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    let Ok(dir) = fs::read_dir("/proc") else { return out };
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(fds) = fs::read_dir(e.path().join("fd")) else { continue };
        let mut name = None;
        for fd in fds.flatten() {
            let Ok(link) = fs::read_link(fd.path()) else { continue };
            let Some(inode) = link.to_str().and_then(|s| s.strip_prefix("socket:[")).and_then(|s| s.strip_suffix(']')) else {
                continue;
            };
            let Some(&(local, remote, remote_port, tcp)) = inode.parse::<u64>().ok().and_then(|i| socks.get(&i)) else { continue };
            let name = name
                .get_or_insert_with(|| fs::read_to_string(e.path().join("comm")).map(|s| s.trim().to_string()).unwrap_or_default())
                .clone();
            out.push(Conn { pid, name, local, remote, remote_port, tcp });
        }
    }
    out
}

/// Conexiones agrupadas por proceso para las IPs de una interfaz:
/// (pid, nombre, cantidad, algunos destinos).
pub fn by_process<'a>(conns: &'a [Conn], ips: &[IpAddr]) -> Vec<(u32, &'a str, usize, Vec<String>)> {
    let mut m: HashMap<u32, (&str, usize, Vec<String>)> = HashMap::new();
    for c in conns.iter().filter(|c| ips.contains(&c.local)) {
        let e = m.entry(c.pid).or_insert((&c.name, 0, vec![]));
        e.1 += 1;
        let dst = match c.remote {
            IpAddr::V6(v6) => format!("[{v6}]:{}", c.remote_port),
            v4 => format!("{v4}:{}", c.remote_port),
        };
        if e.2.len() < 3 && !e.2.contains(&dst) {
            e.2.push(dst);
        }
    }
    let mut v: Vec<_> = m.into_iter().map(|(pid, (n, k, d))| (pid, n, k, d)).collect();
    v.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(b.1)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v4_and_v6() {
        assert_eq!(parse_addr("0100007F:0035"), Some(("127.0.0.1".parse().unwrap(), 53)));
        assert_eq!(parse_addr("0000000000000000FFFF00000A01A8C0:01BB"), Some(("192.168.1.10".parse().unwrap(), 443)));
    }
}
