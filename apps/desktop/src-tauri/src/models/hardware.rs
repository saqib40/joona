//! What this PC can run: memory, CPU and graphics cards.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub cpu: String,
    pub cores: u32,
    pub threads: u32,
    pub ram_mb: u64,
    pub available_ram_mb: u64,
    /// whisper.cpp is several times slower without AVX2.
    pub avx2: bool,
    pub gpus: Vec<Gpu>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub name: String,
    pub vram_mb: u64,
    /// A graphics card with its own memory, rather than graphics built into the CPU.
    pub dedicated: bool,
}

impl Hardware {
    /// The best graphics card for running a model, if any.
    pub fn best_gpu(&self) -> Option<&Gpu> {
        self.gpus
            .iter()
            .filter(|g| g.dedicated)
            .max_by_key(|g| g.vram_mb)
    }
}

pub fn detect() -> Hardware {
    let (cpu, ram_mb, available_ram_mb, threads) = match sysinfo::System::new() {
        Ok(mut sys) => {
            sys.refresh_memory();
            sys.refresh_cpu_all();
            let cpu = sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_owned())
                .unwrap_or_default();
            (
                cpu,
                sys.total_memory() >> 20,
                sys.available_memory() >> 20,
                sys.cpus().len() as u32,
            )
        }
        Err(e) => {
            tracing::warn!("couldn't read the system's memory and CPU: {e}");
            (String::new(), 0, 0, 0)
        }
    };
    let threads = if threads > 0 {
        threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
    };
    let cores = sysinfo::System::physical_core_count().map_or(threads, |n| n as u32);
    Hardware {
        cpu,
        cores,
        threads,
        ram_mb,
        available_ram_mb,
        avx2: has_avx2(),
        gpus: gpus(),
    }
}

fn has_avx2() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

#[cfg(windows)]
fn gpus() -> Vec<Gpu> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
    };

    /// Integrated graphics report a small carve-out as their own memory.
    const DEDICATED_MB: u64 = 1024;

    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for i in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let len = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        let vram_mb = desc.DedicatedVideoMemory as u64 >> 20;
        found.push(Gpu {
            name: String::from_utf16_lossy(&desc.Description[..len])
                .trim()
                .to_owned(),
            vram_mb,
            dedicated: vram_mb >= DEDICATED_MB,
        });
    }
    found
}

#[cfg(target_os = "macos")]
fn gpus() -> Vec<Gpu> {
    #[link(name = "Metal", kind = "framework")]
    unsafe extern "C" {
        fn MTLCreateSystemDefaultDevice() -> *mut std::ffi::c_void;
    }
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    let device_ptr = unsafe { MTLCreateSystemDefaultDevice() };
    if device_ptr.is_null() {
        return Vec::new();
    }
    let device: &AnyObject = unsafe { &*(device_ptr as *const AnyObject) };

    let name_retained: Option<Retained<NSString>> = unsafe { msg_send![device, name] };
    let name = name_retained
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Apple Metal GPU".into());

    let max_vram: u64 = unsafe { msg_send![device, recommendedMaxWorkingSetSize] };
    let has_unified: bool = unsafe { msg_send![device, hasUnifiedMemory] };

    let vram_mb = max_vram >> 20;

    vec![Gpu {
        name,
        vram_mb,
        dedicated: !has_unified,
    }]
}

#[cfg(not(any(windows, target_os = "macos")))]
fn gpus() -> Vec<Gpu> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_something() {
        let hw = detect();
        assert!(hw.threads >= 1 && hw.cores >= 1);
        assert!(hw.ram_mb > 0);
    }
}
