fn main() {
    let mut m = rtop_core::Monitor::new();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    m.refresh();
    let s = &m.stats;
    println!("cpu={} freq={}MHz total={:.1}", s.cpu_model, s.cpu_freq, s.cpu_total);
    println!("disk r={:.0}/s w={:.0}/s used={}/{}", s.disk_read_rate, s.disk_write_rate, s.disk_used, s.disk_total);
    for g in &s.gpus { println!("{:?}", g); }
}
