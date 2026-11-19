//! Red: tráfico total, cada interfaz y los procesos con conexiones.

use super::prelude::*;

impl App {
    pub(crate) fn page_net(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let (real, virt): (Vec<_>, Vec<_>) = s.nets.iter().partition(|n| !n.is_virtual());
        let real: Vec<_> = real.into_iter().filter(|n| n.total_rx + n.total_tx > 0 || !n.ips.is_empty()).collect();

        stat_row(
            ui,
            th,
            "net_stats",
            &[
                Stat::new("Bajada", rate_fmt(s.rx_rate), c(th.ok)),
                Stat::new("Subida", rate_fmt(s.tx_rate), c(th.warn)),
                Stat::new("Conexiones", real.len().to_string(), c(th.text))
                    .detail(real.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join(", ")),
            ],
        );

        let rate = |h: f32| {
            let mut o = self.opts(h, None, rate_fmt);
            o.floor = 4096.0;
            o.binary = true;
            o
        };
        surface(ui, th, |ui| {
            section(ui, th, "Tráfico", None);
            chart(
                ui,
                th,
                &rate(190.0),
                &[Series::new("bajada", tail_f64(&m.hist.rx, WINDOW), Grad::Mono(th.ok))],
                Some(&Series::new("subida", tail_f64(&m.hist.tx, WINDOW), Grad::Mono(th.warn))),
            );
            legend(ui, th, &[("bajada (arriba)".into(), c(th.ok)), ("subida (abajo)".into(), c(th.warn))]);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Interfaces", None);
            for (i, n) in real.iter().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                let kind = if n.name.starts_with("wl") {
                    "Wi-Fi"
                } else if n.name.starts_with("en") || n.name.starts_with("eth") {
                    "cable"
                } else if n.name.starts_with("tailscale") {
                    "VPN de Tailscale"
                } else if n.name.starts_with("tun") || n.name.starts_with("wg") {
                    "VPN"
                } else {
                    ""
                };
                ui.horizontal(|ui| {
                    ui.label(strong(&n.name, c(th.text), 15.0));
                    ui.label(meta(th, kind));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(num(format!("subida {}", rate_fmt(n.tx_rate)), c(th.warn)));
                        ui.label(num(format!("bajada {}", rate_fmt(n.rx_rate)), c(th.ok)));
                    });
                });
                ui.add_space(4.0);
                if let Some((rx, tx)) = m.hist.nets.get(&n.name) {
                    chart(
                        ui,
                        th,
                        &rate(130.0),
                        &[Series::new("bajada", tail_f64(rx, WINDOW), Grad::Mono(th.ok))],
                        Some(&Series::new("subida", tail_f64(tx, WINDOW), Grad::Mono(th.warn))),
                    );
                }
                ui.add_space(8.0);
                let ips: Vec<std::net::IpAddr> = n.ips.iter().filter_map(|s| s.split('/').next()?.parse().ok()).collect();
                let users = rtop_core::netproc::by_process(&self.net.conns, &ips);
                let mut rows = vec![("IP", if n.ips.is_empty() { "sin IP".into() } else { n.ips.join("\n") })];
                if n.mac != "00:00:00:00:00:00" && !n.mac.is_empty() {
                    rows.push(("MAC", n.mac.clone()));
                }
                rows.push(("Bajado desde el arranque", human(n.total_rx)));
                rows.push(("Subido desde el arranque", human(n.total_tx)));
                kv(ui, th, &format!("net_{}", n.name), &rows);
                ui.add_space(8.0);
                ui.label(body(th, "Procesos con conexiones activas"));
                if users.is_empty() {
                    ui.label(meta(th, "Sin conexiones activas en esta interfaz."));
                } else {
                    let rows = users
                        .iter()
                        .take(8)
                        .map(|(pid, name, k, dst)| (*pid, name.to_string(), vec![Col::Text(k.to_string(), c(th.text)), Col::Note(dst.join(", "))]))
                        .collect();
                    self.proc_table(ui, &format!("netp_{}", n.name), &[("Conexiones", w::SMALL), ("Destinos", 380.0)], rows);
                }
            }
            ui.add_space(8.0);
            ui.label(meta(
                th,
                "El tráfico por proceso requiere permisos de administrador, por eso se listan las conexiones activas de cada uno. No incluye procesos del sistema (root).",
            ));
            if !virt.is_empty() {
                ui.separator();
                egui::CollapsingHeader::new(meta(th, format!("Interfaces virtuales (docker, puentes): {}", virt.len())))
                    .id_salt("virt")
                    .show(ui, |ui| {
                        egui::Grid::new("virt_g").num_columns(4).spacing([18.0, 4.0]).show(ui, |ui| {
                            for n in virt {
                                ui.label(body(th, &n.name));
                                ui.label(meta(th, n.ips.first().cloned().unwrap_or("sin IP".into())));
                                ui.label(meta(th, format!("bajado {}", human(n.total_rx))));
                                ui.label(meta(th, format!("subido {}", human(n.total_tx))));
                                ui.end_row();
                            }
                        });
                    });
            }
        });
    }
}
