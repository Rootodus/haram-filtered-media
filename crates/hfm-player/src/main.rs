#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]
#![allow(unused)] // this is temporary 

mod app;
mod audio_output;
mod config;
mod gst_source;
mod gui;
mod pipeline_manager;
mod pts_offset;
mod renderer;

use app::App;
use winit::event_loop::EventLoop;

#[cfg(feature = "dhat")]
use dhat::{Alloc, Profiler};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: Alloc = Alloc;

#[cfg(not(feature = "dhat"))]
use mimalloc::MiMalloc;

#[cfg(not(feature = "dhat"))]
#[global_allocator]
static ALLOC: MiMalloc = MiMalloc;

#[cfg(target_os = "windows")]
fn add_lib_to_dll_search_path() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW;

    // Get the directory of the executable.
    let exe_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Warning: Could not get executable path: {}", e);
            return;
        }
    };

    // Build the path to the 'lib' subdirectory.
    let lib_path = exe_path
        .parent()
        .expect("Executable has no parent directory")
        .join("lib");

    if !lib_path.exists() {
        // If 'lib' doesn't exist, we may be in development; no need to add it.
        return;
    }

    // Convert the path to a null-terminated UTF-16 string.
    let path_str = match lib_path.to_str() {
        Some(s) => s,
        None => {
            eprintln!("Warning: lib path is not valid UTF-8");
            return;
        }
    };
    let wide: Vec<u16> = OsStr::new(path_str).encode_wide().chain(Some(0)).collect();

    unsafe {
        let result = SetDllDirectoryW(wide.as_ptr());
        if result == 0 {
            eprintln!("Warning: Failed to set DLL directory to '{}'", path_str);
        } else {
            println!("Set DLL directory to '{}'", path_str);
        }
    }
}

fn main() {
    #[cfg(feature = "dhat")]
    let _dhat = Profiler::new_heap();

    #[cfg(target_os = "windows")]
    add_lib_to_dll_search_path();

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
