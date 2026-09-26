use crate::core::{docker, BROWSER_IMAGE, VPN_IMAGE};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    process::Command,
    sync::Mutex,
    time::{sleep, timeout},
};

type Result<T> = std::result::Result<T, String>;
const DOCKER_DOWNLOAD: &str =
    "https://desktop.docker.com/win/main/amd64/Docker%20Desktop%20Installer.exe";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedSetup {
    dismissed: bool,
    restart_boot: Option<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub checked: bool,
    pub supported: bool,
    pub virtualization: bool,
    pub windows_ready: bool,
    pub docker_installed: bool,
    pub docker_ready: bool,
    pub vpn_image_ready: bool,
    pub browser_image_ready: bool,
    pub dismissed: bool,
    pub restart_required: bool,
    pub busy: bool,
    pub phase: String,
    pub detail: String,
    pub error: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Deserialize, Default)]
struct WindowsInfo {
    build: u32,
    boot: String,
    virtualization: bool,
}

pub struct SetupManager {
    root: PathBuf,
    saved: Mutex<SavedSetup>,
    state: Mutex<SetupStatus>,
    operation: Mutex<()>,
}

pub(crate) fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command.kill_on_drop(true);
    command
}

fn docker_directories() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        paths.push(PathBuf::from(local).join("Programs/DockerDesktop"));
    }
    if let Some(programs) = std::env::var_os("ProgramFiles") {
        paths.push(PathBuf::from(programs).join("Docker/Docker"));
    }
    paths
}

pub(crate) fn docker_cli() -> PathBuf {
    docker_directories()
        .into_iter()
        .map(|directory| directory.join("resources/bin/docker.exe"))
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("docker"))
}

fn docker_desktop() -> Option<PathBuf> {
    docker_directories()
        .into_iter()
        .map(|directory| directory.join("Docker Desktop.exe"))
        .find(|path| path.is_file())
}

fn decode_output(bytes: &[u8]) -> String {
    // WSL emits UTF-16 on some Windows installations even when output is piped.
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.iter().take(80).any(|byte| *byte == 0) {
        String::from_utf16_lossy(
            &bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        )
        .trim_start_matches('\u{feff}')
        .to_string()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

async fn output(mut command: Command, seconds: u64) -> Result<String> {
    let result = timeout(Duration::from_secs(seconds), command.output())
        .await
        .map_err(|_| {
            "This step took too long. Check the installer or Docker Desktop, then retry."
                .to_string()
        })?
        .map_err(|_| {
            "The setup command could not start. Try again or use the installation help.".to_string()
        })?;
    if !result.status.success() {
        let detail = format!(
            "{} {}",
            decode_output(&result.stdout),
            decode_output(&result.stderr)
        );
        return Err(detail.trim().chars().take(1500).collect::<String>());
    }
    Ok(decode_output(&result.stdout))
}

fn powershell(script: &str) -> Command {
    let mut command = hidden_command("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    command
}

async fn windows_info() -> WindowsInfo {
    let script = "$ErrorActionPreference='Stop'; $os=Get-CimInstance Win32_OperatingSystem; $computer=Get-CimInstance Win32_ComputerSystem; $cpu=Get-CimInstance Win32_Processor | Select-Object -First 1; @{build=[int]$os.BuildNumber;boot=$os.LastBootUpTime.ToUniversalTime().Ticks.ToString();virtualization=[bool]($computer.HypervisorPresent -or $cpu.VirtualizationFirmwareEnabled)} | ConvertTo-Json -Compress";
    output(powershell(script), 15)
        .await
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn modern_wsl(version: &str) -> bool {
    let numbers: Vec<u32> = version
        .lines()
        .next()
        .unwrap_or("")
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .take(3)
        .collect();
    numbers.len() == 3 && (numbers[0], numbers[1], numbers[2]) >= (2, 1, 5)
}

async fn docker_ready() -> bool {
    let mut command = docker();
    command.args(["info", "--format", "{{.OSType}}"]);
    if output(command, 8)
        .await
        .is_ok_and(|value| value.trim() == "linux")
    {
        let mut compose = docker();
        compose.args(["compose", "version"]);
        return output(compose, 5).await.is_ok();
    }
    false
}

async fn image_ready(image: &str) -> bool {
    let mut command = docker();
    command.args(["image", "inspect", "--format", "{{.Id}}", image]);
    output(command, 8).await.is_ok()
}

impl SetupManager {
    pub fn new(root: PathBuf) -> Result<Arc<Self>> {
        fs::create_dir_all(&root).map_err(|_| "Could not create the setup folder.".to_string())?;
        let saved = match fs::read(root.join("setup.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "Setup settings could not be read.".to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => SavedSetup::default(),
            Err(_) => return Err("Could not read setup settings.".into()),
        };
        Ok(Arc::new(Self {
            root,
            saved: Mutex::new(saved),
            state: Mutex::new(SetupStatus::default()),
            operation: Mutex::new(()),
        }))
    }

    async fn save(&self) -> Result<()> {
        let saved = self.saved.lock().await;
        let bytes = serde_json::to_vec_pretty(&*saved)
            .map_err(|_| "Could not save setup progress.".to_string())?;
        let temp = self.root.join("setup.tmp");
        fs::write(&temp, bytes)
            .and_then(|_| fs::rename(temp, self.root.join("setup.json")))
            .map_err(|_| "Could not save setup progress.".to_string())
    }

    pub async fn dismiss(&self) -> Result<()> {
        if self.state.lock().await.busy {
            return Err("Wait for the current setup step to finish.".into());
        }
        self.saved.lock().await.dismissed = true;
        self.save().await?;
        self.state.lock().await.dismissed = true;
        Ok(())
    }

    pub async fn is_busy(&self) -> bool {
        self.state.lock().await.busy
    }

    async fn progress(&self, phase: &str, detail: &str) {
        let mut state = self.state.lock().await;
        state.phase = phase.into();
        state.detail = detail.into();
        state.downloaded = 0;
        state.total = None;
    }

    async fn detect(&self) {
        let ready = docker_ready().await;
        let info = windows_info().await;
        let mut wsl = hidden_command("wsl.exe");
        wsl.arg("--version");
        let version = output(wsl, 10).await.unwrap_or_default();
        let mut wsl_status = hidden_command("wsl.exe");
        wsl_status.arg("--status");
        let wsl_ready = modern_wsl(&version) && output(wsl_status, 10).await.is_ok();
        let vpn_image_ready = ready && image_ready(VPN_IMAGE).await;
        let browser_image_ready = ready && image_ready(BROWSER_IMAGE).await;
        let mut saved = self.saved.lock().await;
        let reboot_finished = saved
            .restart_boot
            .as_ref()
            .is_some_and(|boot| !info.boot.is_empty() && *boot != info.boot);
        if reboot_finished {
            saved.restart_boot = None;
        }
        let mut state = self.state.lock().await;
        state.checked = true;
        state.supported = cfg!(all(windows, target_arch = "x86_64"))
            && (info.build == 19045 || info.build >= 22631);
        state.virtualization = info.virtualization || ready;
        state.restart_required = saved.restart_boot.is_some();
        state.windows_ready = (wsl_ready || ready) && !state.restart_required;
        state.docker_installed = docker_desktop().is_some() || ready;
        state.docker_ready = ready;
        state.vpn_image_ready = vpn_image_ready;
        state.browser_image_ready = browser_image_ready;
        state.dismissed = saved.dismissed;
        drop(state);
        drop(saved);
        if reboot_finished {
            let _ = self.save().await;
        }
    }

    pub async fn status(&self) -> SetupStatus {
        // Polling during a long action reads progress without launching competing probes.
        if let Ok(_guard) = self.operation.try_lock() {
            self.detect().await;
        }
        self.state.lock().await.clone()
    }

    pub async fn run(&self, action: &str) -> Result<()> {
        if !["prepare", "images", "start_docker"].contains(&action) {
            return Err("Unknown setup action.".into());
        }
        if self.state.lock().await.busy {
            return Err("Setup is already running.".into());
        }
        let _guard = self.operation.lock().await;
        {
            let mut state = self.state.lock().await;
            state.busy = true;
            state.error = None;
        }
        self.progress("checking", "Checking this PC and existing installations")
            .await;
        self.detect().await;
        let result = match action {
            "prepare" => self.prepare().await,
            "images" => self.download_images().await,
            "start_docker" => self.start_docker().await,
            _ => unreachable!(),
        };
        self.detect().await;
        let mut state = self.state.lock().await;
        state.busy = false;
        if let Err(error) = &result {
            state.error = Some(if error.trim().is_empty() {
                "Setup could not finish. Check Docker Desktop and retry.".into()
            } else {
                error.clone()
            });
            state.phase = "error".into();
        } else {
            state.phase = if state.restart_required {
                "restart"
            } else {
                "ready"
            }
            .into();
            state.detail = if state.restart_required {
                "Restart Windows, then reopen RegionBox to continue."
            } else {
                "This step is ready."
            }
            .into();
        }
        result
    }

    async fn prepare(&self) -> Result<()> {
        let state = self.state.lock().await.clone();
        if !state.supported {
            return Err("This version needs a supported Windows 10 or Windows 11 PC with an Intel or AMD 64-bit processor. Open Windows setup help for requirements.".into());
        }
        if !state.virtualization {
            return Err("Enable virtualization in this PC's BIOS or UEFI, then restart Windows. Open Windows setup help for the steps.".into());
        }
        if state.restart_required {
            return Ok(());
        }
        if !state.windows_ready {
            self.progress("installing_wsl", "Preparing Windows support. Approve the Windows administrator prompt if it appears.").await;
            let boot = windows_info().await.boot;
            if boot.is_empty() {
                return Err("Could not check Windows restart information. Retry setup.".into());
            }
            // Fixed arguments only. Windows owns the elevation prompt; RegionBox stays unelevated.
            let mut version_command = hidden_command("wsl.exe");
            version_command.arg("--version");
            let old_version = output(version_command, 10)
                .await
                .ok()
                .is_some_and(|version| !modern_wsl(&version));
            let script = if old_version {
                r"$ErrorActionPreference='Stop'; try { $p=Start-Process -FilePath (Join-Path $env:WINDIR 'System32\wsl.exe') -ArgumentList '--update','--web-download' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; if ($p.ExitCode -notin 0,3010,1641) { throw 'Windows support update did not complete. Retry or use Windows setup help.' } } catch { Write-Error $_; exit 1 }"
            } else {
                r"$ErrorActionPreference='Stop'; try { $p=Start-Process -FilePath (Join-Path $env:WINDIR 'System32\wsl.exe') -ArgumentList '--install','--no-distribution','--web-download' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; if ($p.ExitCode -notin 0,3010,1641) { throw 'Windows support installation did not complete. Retry or use Windows setup help.' } } catch { Write-Error $_; exit 1 }"
            };
            output(powershell(script), 1200).await?;
            self.saved.lock().await.restart_boot = Some(boot);
            self.save().await?;
            return Ok(());
        }
        if !state.docker_installed {
            self.install_docker().await?;
        }
        if !state.docker_ready {
            self.start_docker().await?;
        }
        Ok(())
    }

    async fn install_docker(&self) -> Result<()> {
        self.progress(
            "downloading_docker",
            "Downloading Docker Desktop from Docker",
        )
        .await;
        let directory = self.root.join("downloads");
        fs::create_dir_all(&directory)
            .map_err(|_| "Could not create the download folder.".to_string())?;
        let partial = directory.join("docker-installer.part");
        let installer = directory.join("Docker Desktop Installer.exe");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1200))
            .build()
            .map_err(|_| "Could not start the download.".to_string())?;
        let mut response = client
            .get(DOCKER_DOWNLOAD)
            .send()
            .await
            .map_err(|_| {
                "Could not download Docker Desktop. Check your internet connection and retry."
                    .to_string()
            })?
            .error_for_status()
            .map_err(|_| {
                "Docker's download is unavailable. Retry or open Docker setup help.".to_string()
            })?;
        let mut file =
            fs::File::create(&partial).map_err(|_| "Could not save the installer.".to_string())?;
        self.state.lock().await.total = response.content_length();
        use std::io::Write;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "The download was interrupted. Retry setup.".to_string())?
        {
            file.write_all(&chunk)
                .map_err(|_| "Could not save the installer. Check free disk space.".to_string())?;
            self.state.lock().await.downloaded += chunk.len() as u64;
        }
        drop(file);
        fs::rename(partial, &installer)
            .map_err(|_| "Could not finish saving the installer.".to_string())?;
        self.progress(
            "verifying_docker",
            "Checking the Docker installer signature",
        )
        .await;
        let mut verify = powershell(
            r#"$ErrorActionPreference='Stop'; Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1'); $signature=Get-AuthenticodeSignature -LiteralPath $env:REGIONBOX_INSTALLER; if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O="?Docker Inc\.?"?(,|$)') { throw 'Docker installer signature could not be verified. Retry the download.' }"#,
        );
        verify.env("REGIONBOX_INSTALLER", &installer);
        output(verify, 30).await?;
        self.progress(
            "installing_docker",
            "Installing Docker Desktop for your Windows account",
        )
        .await;
        let mut install = hidden_command(&installer);
        // Leave agreement acceptance to Docker's own first-run screen.
        install.args(["install", "--user", "--quiet", "--backend=wsl-2"]);
        output(install, 1200).await?;
        if docker_desktop().is_none() {
            return Err("Docker's installer finished, but the app was not found. Open Docker setup help or retry.".into());
        }
        Ok(())
    }

    async fn start_docker(&self) -> Result<()> {
        if docker_ready().await {
            return Ok(());
        }
        let desktop = docker_desktop()
            .ok_or("Docker Desktop is not installed. Choose Set up this PC first.")?;
        self.progress("starting_docker", "Starting Docker Desktop. Complete any first-run prompts in its window; RegionBox will continue automatically.").await;
        let mut command = hidden_command(desktop);
        command.kill_on_drop(false).spawn().map_err(|_| {
            "Could not open Docker Desktop. Open it from the Windows Start menu, then retry."
                .to_string()
        })?;
        for _ in 0..60 {
            if docker_ready().await {
                return Ok(());
            }
            sleep(Duration::from_secs(2)).await;
        }
        Err("Docker is not ready yet. Complete its first-run prompts, use Linux containers, and retry. If Docker asks for a Windows restart, reopen RegionBox afterward.".into())
    }

    async fn download_images(&self) -> Result<()> {
        if !docker_ready().await {
            return Err("Start Docker Desktop before downloading browser files.".into());
        }
        for (image, phase, detail) in [
            (
                VPN_IMAGE,
                "downloading_vpn",
                "Downloading VPN files (1 of 2)",
            ),
            (
                BROWSER_IMAGE,
                "downloading_browser",
                "Downloading Chromium files (2 of 2). This can take several minutes.",
            ),
        ] {
            if image_ready(image).await {
                continue;
            }
            self.progress(phase, detail).await;
            let mut command = docker();
            command.args(["pull", "--quiet", image]);
            output(command, 1200).await.map_err(|_| "The browser files could not finish downloading. Check your connection and Docker disk space, then retry. Downloaded layers will be reused.".to_string())?;
        }
        Ok(())
    }
}

pub async fn open_help(topic: &str) -> Result<()> {
    let url = match topic {
        "docker" => "https://docs.docker.com/desktop/setup/install/windows-install/",
        "windows" => "https://learn.microsoft.com/en-us/windows/wsl/install",
        "nord" => "https://my.nordaccount.com/dashboard/nordvpn/",
        _ => return Err("Unknown help page.".into()),
    };
    let mut command = powershell("Start-Process -FilePath $env:REGIONBOX_HELP_URL");
    command.env("REGIONBOX_HELP_URL", url);
    output(command, 15).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wsl_version_and_windows_output_are_parsed() {
        assert!(modern_wsl("WSL version: 2.6.1.0\nKernel version: 6.6.87"));
        assert!(modern_wsl("WSL-Version: 2.1.5.0"));
        assert!(!modern_wsl("WSL version: 2.1.4.0"));
        assert!(!modern_wsl("Windows Subsystem for Linux is not installed."));
        let bytes: Vec<u8> = "WSL version: 2.6.1"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(modern_wsl(&decode_output(&bytes)));
    }
    #[tokio::test]
    async fn dismissal_and_restart_progress_survive_relaunch() {
        let root = std::env::temp_dir().join(format!("regionbox-test-{}", uuid::Uuid::new_v4()));
        let manager = SetupManager::new(root.clone()).unwrap();
        manager.saved.lock().await.restart_boot = Some("boot-1".into());
        manager.dismiss().await.unwrap();
        let reloaded = SetupManager::new(root.clone()).unwrap();
        let saved = reloaded.saved.lock().await;
        assert!(saved.dismissed);
        assert_eq!(saved.restart_boot.as_deref(), Some("boot-1"));
        assert!(reloaded.run("arbitrary-shell-command").await.is_err());
        drop(saved);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(resolved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("regionbox-test-"));
        fs::remove_dir_all(resolved).unwrap();
    }
}
