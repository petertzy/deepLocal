use deeplocal_core::{GpuInfo, HardwareProfile};
use sysinfo::System;

pub fn detect_hardware() -> HardwareProfile {
    let mut system = System::new_all();
    system.refresh_all();

    let cpu_brand = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    HardwareProfile {
        os: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
        arch: std::env::consts::ARCH.to_string(),
        cpu_brand,
        cpu_cores: num_cpus::get(),
        total_ram_bytes: system.total_memory(),
        available_ram_bytes: system.available_memory(),
        gpu: vec![GpuInfo {
            name: "GPU detection pending".to_string(),
            vendor: None,
            total_vram_bytes: None,
        }],
    }
}
