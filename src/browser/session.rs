use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::{Browser, BrowserConfig, Handler, Page};
use std::error::Error;
use tempfile::{Builder, TempDir};

pub struct BrowserSession {
    pub browser: Browser,
    pub page: Page,
    pub profile_dir: TempDir,
}

impl BrowserSession {
    /// Launches a headless Chrome browser and returns the raw instances safely.
    pub async fn launch() -> Result<(Browser, Handler, TempDir), Box<dyn Error>> {
        eprintln!("[LAUNCH TRACK 1] BrowserSession::launch() started.");

        // Prioritize an explicit runtime path override via CHROME_PATH environment variable
        let mut final_path = std::env::var("CHROME_PATH").ok();

        // DYNAMIC LOOKUP: If no CHROME_PATH is set, scan the native OS system environment $PATH variable
        if final_path.is_none() {
            let binary_name = if cfg!(target_os = "windows") {
                "chrome-headless-shell.exe"
            } else {
                "chrome-headless-shell"
            };

            // Check if the shell executable is globally available on the system PATH
            if let Ok(paths) = std::env::var("PATH") {
                let split_char = if cfg!(target_os = "windows") {
                    ';'
                } else {
                    ':'
                };
                for path in paths.split(split_char) {
                    let full_test_path = std::path::Path::new(path).join(binary_name);
                    if full_test_path.exists() {
                        final_path = Some(full_test_path.to_string_lossy().into_owned());
                        break;
                    }
                }
            }
        }

        // FALLBACK GUESS: If it's not found anywhere, locate its parent container directory dynamically
        let chrome_path = final_path.unwrap_or_else(|| {
            let (base_dir, binary_name) = if cfg!(target_os = "windows") {
                let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
                    format!(
                        r"C:\Users\{}\AppData\Local",
                        std::env::var("USERNAME").unwrap_or_default()
                    )
                });
                (
                    std::path::PathBuf::from(local_app_data).join("Programs"),
                    "chrome-headless-shell.exe",
                )
            } else if cfg!(target_os = "macos") {
                let home = std::env::var("HOME").unwrap_or_default();
                (
                    std::path::PathBuf::from(home).join("Applications"),
                    "chrome-headless-shell",
                )
            } else {
                let home = std::env::var("HOME").unwrap_or_default();
                (
                    std::path::PathBuf::from(home).join(".local").join("share"),
                    "chrome-headless-shell",
                )
            };

            // SCANNING STEP: Look for directories like "chrome-headless-shell-win64" dynamically
            let mut resolved_binary_path = base_dir.join("chrome-headless-shell").join(binary_name);

            if !resolved_binary_path.exists() {
                if let Ok(entries) = std::fs::read_dir(&base_dir) {
                    for entry in entries.flatten() {
                        if let Some(folder_name) = entry.file_name().to_str() {
                            // Detects suffixed naming conventions automatically (e.g. -win64, -mac-x64, etc.)
                            if folder_name.starts_with("chrome-headless-shell") {
                                let test_path = entry.path().join(binary_name);
                                if test_path.exists() {
                                    resolved_binary_path = test_path;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            resolved_binary_path.to_string_lossy().into_owned()
        });

        // ENFORCE HARD EXISTENCE VERIFICATION
        let binary_path = std::path::Path::new(&chrome_path);
        if !binary_path.exists() {
            // Pick a clean, context-appropriate instruction banner string based on compile targets
            let setup_example = if cfg!(target_os = "windows") {
                r#"%LOCALAPPDATA%\Programs\chrome-headless-shell-win64\"#
            } else if cfg!(target_os = "macos") {
                r#"~/Applications/chrome-headless-shell-mac-x64/"#
            } else {
                r#"~/.local/share/chrome-headless-shell-linux64/"#
            };

            eprintln!(
                "\n=========================================================================="
            );
            eprintln!(
                "[FATAL LAUNCH ERROR] Core Dependency Missing: Headless Chrome Engine Not Found!"
            );
            eprintln!("Expected Binary Location: \"{}\"", chrome_path);
            eprintln!("==========================================================================");
            eprintln!("To resolve this issue, follow these installation steps:");
            eprintln!(
                "  1. Download the official, stable 'chrome-headless-shell' framework package from:"
            );
            eprintln!("     https://googlechromelabs.github.io/chrome-for-testing/");
            eprintln!(
                "  2. Unzip and drop the folder directly into your user local applications folder:"
            );
            eprintln!("     Path destination: \"{}\"", setup_example);
            eprintln!(
                "  3. Alternatively, place the shell anywhere and explicitly specify its coordinate by setting"
            );
            eprintln!(
                "     the environment variable: $env:CHROME_PATH=\"C:\\your\\custom\\path\\chrome-headless-shell.exe\""
            );
            eprintln!(
                "==========================================================================\n"
            );

            std::process::exit(1);
        }

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

        // Generate an isolated, completely random directory prefixed for our app
        let temp_dir = Builder::new()
            .prefix("chrome_profile_")
            .tempdir_in(std::env::temp_dir())?;

        let config = BrowserConfig::builder()
            .chrome_executable(&chrome_path)
            .no_sandbox()
            .arg("--headless=new")
            .arg("--disable-extensions")
            .arg("--use-gl=swiftshader")
            .arg("--disable-gpu")
            .arg("--no-startup-window")
            .arg("--disable-software-rasterizer")
            .arg("--disable-backgrounding-occluded-windows")
            .arg("--disable-breakpad")
            .arg("--disable-dev-shm-usage")
            .arg("--ignore-certificate-errors")
            .arg("--incognito")
            .arg(format!("--crash-dumps-dir={}", temp_dir.path().display()))
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

    pub fn close_sync(self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[Cleanup] Initiating fast shutdown pipeline...");

        // Extract path tracking variables before destroying ownership
        let target_path = self.profile_dir.path().to_path_buf();

        // Kill the browser process immediately via standard OS drops.
        // Dropping the browser instance in chromiumoxide closes the underlying connection handle.
        // To prevent WebSocket deadlocks, we do NOT block the thread waiting for `.close().await`.
        std::mem::drop(self.browser);
        std::mem::drop(self.page);

        // Give the headless shell process a brief moment to register the dropped connection and exit
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Clean up the profile directory if the OS has released its locks
        let mut retries = 3;
        while target_path.exists() && retries > 0 {
            match remove_dir_all::remove_dir_all(&target_path) {
                Ok(_) => {
                    println!("[Cleanup] Profile folder successfully erased.");
                    break;
                }
                Err(_) => {
                    retries -= 1;
                    if retries == 0 {
                        // If it's still locked, don't stall the user.
                        // Your startup sweep function will catch it on the next run anyway!
                        println!(
                            "[Cleanup Notice] Profile folder is temporarily locked by Windows. Leaving it for the startup sweep."
                        );
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }

        // Explicitly consume the TempDir wrapper object so Rust registers it as dropped
        let _ = self.profile_dir.close();

        println!("[Cleanup] Shutdown pipeline finished cleanly.");
        Ok(())
    }
}
