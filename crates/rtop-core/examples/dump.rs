fn main() {
    let mut m = rtop_core::Monitor::new();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    m.refresh();
    let s = &m.stats;
    println!("{} | {} | {}", s.host, s.os, s.kernel);
    println!("cpu={} freq={}MHz total={:.1} temp={:?}", s.cpu_model, s.cpu_freq, s.cpu_total, s.cpu_temp);
    println!("mem used={} cache={} avail={}", s.meminfo.used(), s.meminfo.cache(), s.meminfo.available);
    for t in &s.temps { println!("temp {} {:.1}", t.label, t.value); }
    for n in s.nets.iter().take(3) { println!("net {} rx={:.0} ips={:?}", n.name, n.rx_rate, n.ips); }
    for d in &s.devs { println!("dev {} {:?} r={:.0} w={:.0}", d.name, d.model, d.read_rate, d.write_rate); }
    for g in &s.gpus {
        println!("gpu {} [{}] busy={:?} clk={:?} pstate={:?} procs={}", g.name, g.id, g.busy, g.clock_mhz, g.pstate, g.procs.len());
        for p in g.procs.iter().take(4) { println!("   {} {} {:?} {}", p.pid, p.name, p.usage, p.vram); }
    }
}
