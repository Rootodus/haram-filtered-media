use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::{Browser, BrowserConfig, Handler, Page};
use std::error::Error;
use tempfile::{Builder, TempDir};

pub struct BrowserSession {
    pub browser: Browser,
    pub page: Page,
    pub profile_dir: TempDir,
}

/// Fast native Windows kernel function to find the PID matching our custom profile directory.
#[cfg(target_os = "windows")]
fn find_chrome_pid_by_profile(profile_str: &str) -> Option<u32> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

    eprintln!(
        "[TRACE 1] find_chrome_pid_by_profile called with profile marker: '{}'",
        profile_str
    );

    unsafe {
        eprintln!("[TRACE 2] Requesting CreateToolhelp32Snapshot from Windows Kernel...");
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            eprintln!("[TRACE 3] Snapshot failed. Invalid handle.");
            return None;
        }
        eprintln!("[TRACE 4] Snapshot handle obtained successfully.");

        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        eprintln!("[TRACE 5] Querying Process32First...");
        if Process32First(snapshot, &mut entry) != 0 {
            let mut loop_counter = 0;
            loop {
                loop_counter += 1;
                let name_bytes: Vec<u16> = entry
                    .szExeFile
                    .iter()
                    .map(|&c| c as u16)
                    .take_while(|&c| c != 0)
                    .collect();
                let process_name = OsString::from_wide(&name_bytes)
                    .to_string_lossy()
                    .to_lowercase();

                if process_name.contains("chrome.exe") {
                    let pid = entry.th32ProcessID;
                    eprintln!(
                        "[TRACE 6][Loop {}] Found chrome.exe matching PID: {}. Inspecting arguments...",
                        loop_counter, pid
                    );

                    if let Some(cmd) = get_process_command_line(pid) {
                        eprintln!(
                            "[TRACE 7][Loop {}] PID {} command line extracted successfully.",
                            loop_counter, pid
                        );
                        if cmd.contains(profile_str) {
                            eprintln!(
                                "[TRACE 8] Match found! PID: {}. Closing snapshot handle...",
                                pid
                            );
                            CloseHandle(snapshot);
                            return Some(pid);
                        }
                    } else {
                        eprintln!(
                            "[TRACE 9][Loop {}] PID {} returned no command line (Skipped/Protected).",
                            loop_counter, pid
                        );
                    }
                }

                if Process32Next(snapshot, &mut entry) == 0 {
                    eprintln!(
                        "[TRACE 10] Process32Next returned 0. End of process tree snapshot reached."
                    );
                    break;
                }
            }
        }
        eprintln!("[TRACE 11] Closing snapshot handle...");
        CloseHandle(snapshot);
    }
    eprintln!("[TRACE 12] find_chrome_pid_by_profile finished. No target PID matched.");
    None
}
/// Helper function to extract a process's raw command line arguments using NT APIs
#[cfg(target_os = "windows")]
fn get_process_command_line(pid: u32) -> Option<String> {
    use ntapi::ntpsapi::{NtQueryInformationProcess, ProcessCommandLineInformation};
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::{PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return None;
        }

        let mut size: u32 = 0;

        eprintln!(
            "    [NTAPI DBG 1][PID {}] Calling NtQueryInformationProcess for buffer size...",
            pid
        );
        let mut status = NtQueryInformationProcess(
            handle,
            ProcessCommandLineInformation,
            std::ptr::null_mut(),
            0,
            &mut size,
        );
        eprintln!(
            "    [NTAPI DBG 2][PID {}] Size call finished. Required buffer size: {} bytes, NTSTATUS: {}",
            pid, size, status
        );

        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            eprintln!(
                "    [NTAPI DBG 3][PID {}] Allocating memory vector array. Calling NtQueryInformationProcess with buffer...",
                pid
            );
            status = NtQueryInformationProcess(
                handle,
                ProcessCommandLineInformation,
                buf.as_mut_ptr() as *mut _,
                size,
                &mut size,
            );
            eprintln!(
                "    [NTAPI DBG 4][PID {}] Payload call finished. NTSTATUS: {}",
                pid, status
            );

            if status >= 0 {
                let unicode_str =
                    buf.as_ptr() as *const ntapi::winapi::shared::ntdef::UNICODE_STRING;
                let len = (*unicode_str).Length as usize / 2;
                let buffer_ptr = (*unicode_str).Buffer;
                if !buffer_ptr.is_null() {
                    let slice = std::slice::from_raw_parts(buffer_ptr, len);
                    CloseHandle(handle);
                    return Some(String::from_utf16_lossy(slice));
                }
            }
        }

        CloseHandle(handle);
    }
    None
}

impl BrowserSession {
    /// Launches a headless Chrome browser and returns the raw instances safely.
    pub async fn launch() -> Result<(Browser, Handler, TempDir), Box<dyn Error>> {
        eprintln!("[LAUNCH TRACK 1] BrowserSession::launch() started.");

        // Startup clean: clears out old profiles from previous hard crashes safely
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("chrome_profile_") {
                        let _ = remove_dir_all::remove_dir_all(entry.path());
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        const DEFAULT_CHROME: &str = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        #[cfg(target_os = "macos")]
        const DEFAULT_CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        const DEFAULT_CHROME: &str = "/usr/bin/google-chrome";

        let chrome_path =
            std::env::var("CHROME_PATH").unwrap_or_else(|_| DEFAULT_CHROME.to_string());

        // Generate an isolated, completely random directory prefixed for our app
        let temp_dir = Builder::new()
            .prefix("chrome_profile_")
            .tempdir_in(std::env::temp_dir())?;

        let config = BrowserConfig::builder()
            .chrome_executable(&chrome_path)
            .user_data_dir(temp_dir.path())
            .no_sandbox()
            .arg("--headless=new") // Enforce modern headless rendering engine
            .arg("--no-startup-window") // Bypasses initial default Win32 process canvas allocation
            .arg("--disable-gpu") // Prevents Chrome from allocating visible desktop swapchains
            .arg("--disable-software-rasterizer")
            .arg("--window-position=-10000,-10000") // Teleports accidental windows completely off-screen
            .arg("--disable-breakpad")
            .arg("--disable-dev-shm-usage")
            .arg("--ignore-certificate-errors")
            .arg("--no-process-singleton-dialog")
            .arg("--incognito")
            .arg("--single-process")
            .arg("--no-zygote")
            .build()?;

        eprintln!("[LAUNCH TRACK 6] Invoking async Browser::launch(config)...");
        let (browser, handler) = Browser::launch(config).await?;
        eprintln!("[LAUNCH TRACK 7] Core processes successfully initialized.");

        Ok((browser, handler, temp_dir))
    }

    pub async fn navigate(&self, url: &str) -> Result<(), Box<dyn Error>> {
        self.page.goto(url).await?;
        self.page.wait_for_navigation().await?;
        Ok(())
    }

    pub async fn set_viewport(&mut self, width: u32, height: u32) -> Result<(), Box<dyn Error>> {
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(width as i64)
            .height(height as i64)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()?;
        self.page.execute(params).await?;
        Ok(())
    }

    // session.rs (Line 242 onwards)
    pub fn close_sync(self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Get the path string out of our TempDir object safely
        let target_path = self.profile_dir.path().to_path_buf();
        let dir_string = target_path.to_string_lossy().to_string();

        // 2. Fire and forget the close command to the websocket
        let mut browser = self.browser;
        tokio::spawn(async move {
            let _ = browser.close().await;
        });

        // 3. CRATE-DRIVEN PROCESS TREE CLOSURE
        #[cfg(target_os = "windows")]
        {
            if let Some(pid) = find_chrome_pid_by_profile(&dir_string) {
                println!(
                    "[Cleanup] Found Chrome parent PID: {}. Invoking kill_tree...",
                    pid
                );
                let _ = kill_tree::blocking::kill_tree(pid);
            }
        }

        // 4. Sleep briefly to give the OS kernel time to tear down handles
        std::thread::sleep(std::time::Duration::from_millis(250));

        // 5. Explicitly consume the TempDir to trigger deletion with an automatic retry block
        let mut retries = 5;
        let deletion_result = self.profile_dir.close();

        while deletion_result.is_err() && retries > 0 {
            retries -= 1;
            println!(
                "[Cleanup] Path locked by OS. Retrying folder elimination ({} attempts left)...",
                retries
            );
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Attempt a manual fallback deletion if the tempfile closer is stubborn
            if target_path.exists() {
                if remove_dir_all::remove_dir_all(&target_path).is_ok() {
                    println!(
                        "[Cleanup] Profile folder successfully erased via fallback backoff engine."
                    );
                    return Ok(());
                }
            }
        }

        if deletion_result.is_ok() {
            println!("[Cleanup] Profile folder successfully erased via tempfile engine.");
        } else {
            eprintln!(
                "[Cleanup Error] Could not auto-delete folder structure: {:?}",
                deletion_result.err()
            );
        }

        Ok(())
    }
}
