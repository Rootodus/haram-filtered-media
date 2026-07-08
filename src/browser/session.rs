use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::{Browser, BrowserConfig, Handler, Page};
use std::error::Error;
use sysinfo::System;

pub struct BrowserSession {
    pub browser: Browser,
    pub page: Page,
}

/// Native helper function to surgically find and kill dangling automation profiles
fn kill_automation_browsers() {
    // 1. Initialize all tracking metrics using sysinfo 0.39.x native constructor
    let mut sys = System::new_all();
    sys.refresh_all();

    for (pid, process) in sys.processes() {
        // 2. Convert name dynamically via to_string_lossy to handle OsStr types safely
        let name = process.name().to_string_lossy().to_lowercase();

        if name.contains("chrome") || name.contains("chromium") {
            // 3. Map OsStr arguments vector array cleanly into a standard cohesive String row
            let cmdline_args: Vec<String> = process
                .cmd()
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();

            let cmdline = cmdline_args.join(" ");

            // 4. Match the custom string marker token to avoid closing your user profile tabs
            if cmdline.contains("chrome_profile_") {
                println!("[Sysinfo] Force killing orphan browser PID: {}", pid);
                let _ = process.kill(); // Instantly drops the process via native OS kernels
            }
        }
    }
}

impl BrowserSession {
    /// Launches a headless Chrome browser and returns the session along with its event loop handler.
    pub async fn launch() -> Result<(Self, Handler), Box<dyn Error>> {
        // 1. Native instant cleanup hook on startup
        kill_automation_browsers();

        // 2. Clean up directory files safely now that locks are broken
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("chrome_profile_") {
                        let _ = std::fs::remove_dir_all(entry.path());
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

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let profile_dir = std::env::temp_dir().join(format!("chrome_profile_{}", timestamp));
        std::fs::create_dir_all(&profile_dir)?;

        let config = BrowserConfig::builder()
            .chrome_executable(&chrome_path)
            .user_data_dir(&profile_dir)
            .no_sandbox()
            .arg("--disable-gpu")
            .arg("--no-startup-window")
            .arg("--disable-breakpad")
            .arg("--disable-dev-shm-usage")
            .arg("--ignore-certificate-errors")
            .arg("--no-process-singleton-dialog")
            .build()?;

        let (browser, handler) = Browser::launch(config).await?;
        let page = browser.new_page("about:blank").await?;

        println!("Chrome successfully initialized.");
        Ok((Self { browser, page }, handler))
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

    /// Triggers a clean closing sequence via the WebSocket protocol channel layout
    pub async fn close(mut self) -> Result<(), Box<dyn Error>> {
        let _ = self.browser.close().await;

        // 2. Force termination instantly on close down
        kill_automation_browsers();

        // 3. Clear profile files instantly
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("chrome_profile_") {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
        Ok(())
    }
}
