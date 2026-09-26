use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{process::Command, sync::Mutex, time::timeout};

pub const VPN_IMAGE: &str = "qmcgaw/gluetun:v3.41.0";
pub const BROWSER_IMAGE: &str = "lscr.io/linuxserver/chromium@sha256:cf6200ccdcb224feaf5d3bde4ce45b3783c926a7496c98059cae1e0db78e5b2f";
pub const COUNTRIES: [(&str, &str); 10] = [
    ("IN", "India"),
    ("US", "United States"),
    ("GB", "United Kingdom"),
    ("AU", "Australia"),
    ("CA", "Canada"),
    ("AE", "United Arab Emirates"),
    ("FR", "France"),
    ("SG", "Singapore"),
    ("DE", "Germany"),
    ("NL", "Netherlands"),
];
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub country: String,
    pub port: u16,
    #[serde(default = "new_viewer_token")]
    viewer_token: String,
}

fn new_viewer_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

async fn authorize_viewer(workspace: &Workspace) -> Result<()> {
    // Selkies secure mode requires a provisioned session before streaming starts.
    // Reapply the same table on refresh so container/app restarts recover access.
    let permissions = json!({ (workspace.viewer_token.clone()): {
        "role": "controller", "mk_control": true
    }});
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|_| "Could not initialize the local browser connection.".to_string())?
        .post(format!("http://127.0.0.1:{}/api/tokens", workspace.port))
        .bearer_auth(&workspace.viewer_token)
        .header("Content-Type", "application/json")
        .body(permissions.to_string())
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|_| {
            "Browser access is not ready. Refresh or restart this workspace.".to_string()
        })?;
    if !response.status().is_success() {
        return Err("Could not authorize the browser view. Restart this workspace.".into());
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    version: u8,
    owner: String,
    workspaces: Vec<Workspace>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkCheck {
    pub ip: String,
    pub country: String,
    pub checked_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    #[serde(flatten)]
    pub workspace: Workspace,
    pub state: String,
    pub detail: String,
    pub browser_url: Option<String>,
    pub network: Option<NetworkCheck>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub docker_ready: bool,
    pub docker_message: String,
    pub credentials_ready: bool,
    pub credentials_verified: bool,
    pub credentials_path: String,
    pub data_path: String,
    pub workspaces: Vec<WorkspaceView>,
}

pub struct Manager {
    root: PathBuf,
    credentials_path: PathBuf,
    config: Mutex<Config>,
    operation: Mutex<()>,
    phases: Mutex<HashMap<String, (String, String)>>,
    checks: Mutex<HashMap<String, NetworkCheck>>,
    last_logs: Mutex<HashMap<String, String>>,
    verified_credentials: Mutex<Option<(String, String)>>,
}

fn io_error(e: impl std::fmt::Display) -> String {
    format!("Could not access RegionBox data: {e}")
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn country_name(code: &str) -> Result<&'static str> {
    COUNTRIES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
        .ok_or_else(|| "Choose one of the supported countries.".into())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(io_error)?,
    )
    .map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)
}

pub(crate) fn docker() -> Command {
    let mut cmd = Command::new(crate::setup::docker_cli());
    #[cfg(windows)]
    {
        cmd.args(["--context", "desktop-linux"]);
        cmd.creation_flags(0x08000000);
    }
    cmd.kill_on_drop(true);
    cmd
}

async fn execute(mut command: Command, seconds: u64) -> Result<String> {
    let output = timeout(Duration::from_secs(seconds), command.output())
        .await
        .map_err(|_| "Docker took too long. Check Docker Desktop, then retry.".to_string())?
        .map_err(|e| format!("Could not run Docker. Install and start Docker Desktop. {e}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(if message.trim().is_empty() {
            "Docker could not complete this action.".into()
        } else {
            message.chars().take(6000).collect()
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn docker_args(args: &[&str], seconds: u64) -> Result<String> {
    let mut cmd = docker();
    cmd.args(args);
    execute(cmd, seconds).await
}

fn credentials(path: &Path) -> Result<(String, String)> {
    let values: HashMap<String, String> = dotenvy::from_path_iter(path)
        .map_err(|_| "Add NordVPN service credentials in Settings.".to_string())?
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| {
            "The credentials file is not valid. Save your service credentials again in Settings."
                .to_string()
        })?;
    let user = values
        .get("NORDVPN_SERVICE_USER")
        .cloned()
        .unwrap_or_default();
    let password = values
        .get("NORDVPN_SERVICE_PASSWORD")
        .cloned()
        .unwrap_or_default();
    validate_credentials(&user, &password)?;
    Ok((user, password))
}

fn validate_credentials(user: &str, password: &str) -> Result<()> {
    if user.trim().is_empty() || password.trim().is_empty() {
        return Err("Add both NordVPN service credentials before starting a workspace.".into());
    }
    if [user, password]
        .iter()
        .any(|v| v.len() > 512 || v.chars().any(|c| c.is_control() || c == '\''))
    {
        return Err("Service credentials contain unsupported characters. Copy them again from Nord Account.".into());
    }
    if user.contains('@') {
        return Err("Use NordVPN's service username, rather than your email address.".into());
    }
    Ok(())
}

fn vpn_failure(logs: &str) -> String {
    if logs.lines().any(|line| line.contains("[openvpn]") && line.contains("AUTH_FAILED")) {
        "NordVPN rejected this login. Check both service credentials and your active NordVPN subscription, then try again.".into()
    } else if logs.contains("Initialization Sequence Completed") {
        "NordVPN accepted the login, but the VPN connection did not become ready. Check your internet connection and retry.".into()
    } else {
        "Could not connect to a NordVPN server. Your credentials could not be verified. Check your internet connection and retry.".into()
    }
}

async fn wait_for_vpn(container: &str) -> Result<()> {
    let mut logs = String::new();
    let mut authenticated = false;
    // Gluetun may report unhealthy while retrying an unavailable server.
    // Give it time to establish a tunnel, but fail promptly on a login rejection.
    for _ in 0..30 {
        logs = docker_args(&["logs", "--tail", "200", container], 10).await?;
        authenticated |= logs.contains("Initialization Sequence Completed");
        let failure = vpn_failure(&logs);
        if failure.starts_with("NordVPN rejected") { return Err(failure); }
        let state = docker_args(&["inspect", "--format", "{{.State.Status}} {{.State.Health.Status}}", container], 10).await?;
        if state.trim() == "running healthy" && authenticated { return Ok(()); }
        if state.starts_with("exited") { return Err(failure); }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    Err(vpn_failure(&logs))
}

async fn connect_recommended(path: &Path, container: &str, configs: &[String]) -> Result<()> {
    let mut failure = "No recommended server is available. Retry the connection.".to_string();
    for (index, config) in configs.iter().enumerate() {
        if index > 0 {
            let mut command = docker();
            command.args(["compose", "--file"]).arg(path).args(["down", "--timeout", "5"]);
            execute(command, 30).await?;
        }
        fs::write(path.with_file_name("vpn.ovpn"), config).map_err(io_error)?;
        let mut command = docker();
        command.args(["compose", "--file"]).arg(path).args(["up", "--detach", "vpn"]);
        execute(command, 40).await?;
        match wait_for_vpn(container).await {
            Ok(()) => return Ok(()),
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

pub fn compose_document(owner: &str, workspace: &Workspace) -> Result<Value> {
    country_name(&workspace.country)?;
    let project = format!("regionbox-{owner}-{}", workspace.id);
    let labels = json!({"com.regionbox.owner": owner, "com.regionbox.workspace": workspace.id});
    Ok(json!({
        "name": project,
        "services": {
            "vpn": {
                "image": VPN_IMAGE, "container_name": format!("{project}-vpn"),
                "cap_add": ["NET_ADMIN"], "devices": ["/dev/net/tun:/dev/net/tun"],
                "environment": {
                    "VPN_SERVICE_PROVIDER": "custom", "VPN_TYPE": "openvpn",
                    "OPENVPN_CUSTOM_CONFIG": "/gluetun/custom.conf",
                    "OPENVPN_USER_SECRETFILE": "/run/secrets/nord_user",
                    "OPENVPN_PASSWORD_SECRETFILE": "/run/secrets/nord_password",
                    "FIREWALL_INPUT_PORTS": "3000",
                    "HTTP_CONTROL_SERVER_ADDRESS": "127.0.0.1:8000",
                    "TZ": "Etc/UTC"
                },
                "volumes": ["./vpn.ovpn:/gluetun/custom.conf:ro"],
                "secrets": ["nord_user", "nord_password"],
                "sysctls": {"net.ipv6.conf.all.disable_ipv6": "1"},
                "ports": [{"target":3000, "published": workspace.port.to_string(), "host_ip":"127.0.0.1", "protocol":"tcp"}],
                "labels": labels, "restart": "unless-stopped"
            },
            "browser": {
                "image": BROWSER_IMAGE, "container_name": format!("{project}-browser"),
                "network_mode": "service:vpn",
                "depends_on": {"vpn": {"condition": "service_healthy"}},
                "environment": {
                    "PUID": "1000", "PGID": "1000", "TZ": "Etc/UTC",
                    "TITLE": format!("RegionBox · {}", workspace.name.replace('$', "$$")),
                    "CHROME_CLI": "--no-first-run --disable-dev-shm-usage https://example.com",
                    "SELKIES_FRAMERATE": "20", "SELKIES_AUDIO_ENABLED": "false",
                    "SELKIES_MICROPHONE_ENABLED": "false", "SELKIES_ENABLE_SHARING": "false",
                    "SELKIES_MASTER_TOKEN": workspace.viewer_token,
                    "SELKIES_ALLOWED_ORIGINS": format!("http://127.0.0.1:{}", workspace.port),
                    "NO_GAMEPAD": "true", "NO_WEBCAM": "true"
                },
                "volumes": ["profile:/config", "./resolv.conf:/etc/resolv.conf:ro"],
                "shm_size": "1gb", "mem_limit": "2g",
                "healthcheck": {"test":["CMD", "curl", "-fsS", "--max-time", "3", "http://127.0.0.1:3000/api/health"], "interval":"10s", "timeout":"5s", "start_period":"40s", "retries":12},
                "labels": labels, "restart": "unless-stopped"
            }
        },
        "volumes": {"profile": {}},
        "secrets": {"nord_user": {"file":"./nord-user"}, "nord_password":{"file":"./nord-password"}}
    }))
}

impl Manager {
    pub fn new(root: PathBuf, credentials_path: Option<PathBuf>) -> Result<Arc<Self>> {
        fs::create_dir_all(&root).map_err(io_error)?;
        let config_path = root.join("workspaces.json");
        let config: Config = if config_path.exists() {
            serde_json::from_slice(&fs::read(&config_path).map_err(io_error)?)
                .map_err(|_| "Workspace settings are damaged. Preserve the data folder before repairing workspaces.json.".to_string())?
        } else {
            let mut workspaces = Vec::new();
            for (index, country) in ["US", "DE", "GB"].iter().enumerate() {
                let name = country_name(country)?;
                workspaces.push(Workspace {
                    id: country.to_lowercase(),
                    name: name.to_string(),
                    country: country.to_string(),
                    port: 32101 + index as u16,
                    viewer_token: new_viewer_token(),
                });
            }
            let config = Config {
                version: 1,
                owner: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
                workspaces,
            };
            write_json(&config_path, &config)?;
            config
        };
        if config.version != 1
            || config.owner.len() != 8
            || !config.owner.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(
                "Unsupported workspace settings. Preserve your data folder before repairing it."
                    .into(),
            );
        }
        for w in &config.workspaces {
            if w.id.is_empty() || !w.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err("Invalid workspace ID in settings.".into());
            }
            country_name(&w.country)?;
            if w.viewer_token.len() != 64 || !w.viewer_token.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Invalid browser access token in workspace settings.".into());
            }
        }
        // Persist tokens generated when migrating an earlier prototype config.
        write_json(&config_path, &config)?;
        let credentials_path = credentials_path.unwrap_or_else(|| root.join(".env"));
        Ok(Arc::new(Self {
            root,
            credentials_path,
            config: Mutex::new(config),
            operation: Mutex::new(()),
            phases: Mutex::new(HashMap::new()),
            checks: Mutex::new(HashMap::new()),
            last_logs: Mutex::new(HashMap::new()),
            verified_credentials: Mutex::new(None),
        }))
    }

    async fn workspace(&self, id: &str) -> Result<(String, Workspace)> {
        let config = self.config.lock().await;
        let workspace = config
            .workspaces
            .iter()
            .find(|w| w.id == id)
            .cloned()
            .ok_or("Workspace not found.")?;
        Ok((config.owner.clone(), workspace))
    }

    async fn phase(&self, id: &str, state: &str, detail: &str) {
        self.phases
            .lock()
            .await
            .insert(id.into(), (state.into(), detail.into()));
    }

    pub fn redact(&self, message: &str) -> String {
        match credentials(&self.credentials_path) {
            Ok((user, password)) => message
                .replace(&user, "[redacted]")
                .replace(&password, "[redacted]"),
            Err(_) => message.to_string(),
        }
    }

    async fn compose(&self, id: &str, args: &[&str], seconds: u64) -> Result<String> {
        let (owner, _) = self.workspace(id).await?;
        let path = self.root.join(id).join("compose.json");
        let mut cmd = docker();
        cmd.args([
            "compose",
            "--project-name",
            &format!("regionbox-{owner}-{id}"),
            "--file",
        ])
        .arg(&path)
        .args(args);
        execute(cmd, seconds).await.map_err(|e| self.redact(&e))
    }

    pub async fn snapshot(&self) -> Snapshot {
        let config = self.config.lock().await.clone();
        let filter = format!("label=com.regionbox.owner={}", config.owner);
        let listing = docker_args(
            &["ps", "--all", "--filter", &filter, "--format", "{{json .}}"],
            12,
        )
        .await;
        let (docker_ready, docker_message): (bool, String) = match &listing {
            Ok(_) => (true, "Docker is ready".into()),
            Err(_) => (
                false,
                "Start Docker Desktop with Linux containers, then refresh.".into(),
            ),
        };
        let containers: HashMap<String, Value> = listing
            .unwrap_or_default()
            .lines()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .filter_map(|v| {
                v.get("Names")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(|name| (name, v))
            })
            .collect();
        let mut viewer_errors = HashMap::new();
        for workspace in &config.workspaces {
            let name = format!("regionbox-{}-{}-browser", config.owner, workspace.id);
            let ready = containers.get(&name).is_some_and(|v| {
                v["State"].as_str() == Some("running")
                    && v["Status"]
                        .as_str()
                        .is_some_and(|s| s.contains("(healthy)"))
            });
            if ready {
                if let Err(error) = authorize_viewer(workspace).await {
                    viewer_errors.insert(workspace.id.clone(), error);
                }
            }
        }
        let phases = self.phases.lock().await;
        let mut checks = self.checks.lock().await;
        let workspaces = config
            .workspaces
            .iter()
            .map(|workspace| {
                let project = format!("regionbox-{}-{}", config.owner, workspace.id);
                let vpn = containers.get(&format!("{project}-vpn"));
                let browser = containers.get(&format!("{project}-browser"));
                let running = |v: Option<&Value>| {
                    v.and_then(|v| v.get("State")).and_then(Value::as_str) == Some("running")
                };
                let healthy = |v: Option<&Value>| {
                    v.and_then(|v| v.get("Status"))
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.contains("(healthy)"))
                };
                let (mut state, mut detail) = if !docker_ready {
                    ("unavailable".into(), docker_message.clone())
                } else if running(vpn) && running(browser) && healthy(vpn) && healthy(browser) {
                    ("running".into(), "Browser and VPN are running".into())
                } else if vpn.is_some() || browser.is_some() {
                    (
                        "unhealthy".into(),
                        "Connection is not ready. Restart this workspace.".into(),
                    )
                } else {
                    ("stopped".into(), "Browser data is saved".into())
                };
                if state == "running" {
                    if let Some(error) = viewer_errors.get(&workspace.id) {
                        state = "unhealthy".into();
                        detail = error.clone();
                    }
                }
                if docker_ready {
                    if let Some(p) = phases.get(&workspace.id) {
                        state = p.0.clone();
                        detail = p.1.clone();
                    }
                }
                let available = state == "running";
                if !available {
                    checks.remove(&workspace.id);
                }
                WorkspaceView {
                    workspace: workspace.clone(),
                    state,
                    detail,
                    browser_url: available.then(|| {
                        format!(
                            "http://127.0.0.1:{}/?token={}",
                            workspace.port, workspace.viewer_token
                        )
                    }),
                    network: if available {
                        checks.get(&workspace.id).cloned()
                    } else {
                        None
                    },
                }
            })
            .collect();
        let saved_credentials = credentials(&self.credentials_path).ok();
        let credentials_verified = saved_credentials.is_some()
            && saved_credentials.as_ref() == self.verified_credentials.lock().await.as_ref();
        Snapshot {
            docker_ready,
            docker_message,
            credentials_ready: saved_credentials.is_some(),
            credentials_verified,
            credentials_path: self.credentials_path.display().to_string(),
            data_path: self.root.display().to_string(),
            workspaces,
        }
    }

    pub async fn save_credentials(&self, user: String, password: String) -> Result<()> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "Wait for the current workspace action to finish.")?;
        let user = user.trim();
        let password = password.trim();
        validate_credentials(user, password)?;
        *self.verified_credentials.lock().await = None;
        self.probe_credentials(user, password).await?;
        if let Some(parent) = self.credentials_path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::write(
            &self.credentials_path,
            format!("NORDVPN_SERVICE_USER='{user}'\nNORDVPN_SERVICE_PASSWORD='{password}'\n"),
        )
        .map_err(io_error)?;
        *self.verified_credentials.lock().await = Some((user.into(), password.into()));
        Ok(())
    }

    pub fn is_busy(&self) -> bool {
        self.operation.try_lock().is_err()
    }

    pub async fn check_saved_credentials(&self) -> Result<()> {
        let _operation = self.operation.try_lock()
            .map_err(|_| "Wait for the current workspace action to finish.")?;
        let (user, password) = credentials(&self.credentials_path)?;
        *self.verified_credentials.lock().await = None;
        self.probe_credentials(&user, &password).await?;
        *self.verified_credentials.lock().await = Some((user, password));
        Ok(())
    }

    async fn probe_credentials(&self, user: &str, password: &str) -> Result<()> {
        // Use the same VPN configuration as a workspace, without starting a browser
        // or publishing a port. A check never replaces an existing workspace.
        let owner = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        let directory = self.root.join(format!("credential-check-{owner}"));
        let path = directory.join("compose.json");
        let workspace = Workspace { id: "check".into(), name: "Credential check".into(),
            country: "US".into(), port: 32100, viewer_token: new_viewer_token() };
        let mut document = compose_document(&owner, &workspace)?;
        document["services"].as_object_mut().unwrap().remove("browser");
        document.as_object_mut().unwrap().remove("volumes");
        document["services"]["vpn"].as_object_mut().unwrap().remove("ports");
        document["services"]["vpn"]["restart"] = json!("no");
        let container = format!("regionbox-{owner}-check-vpn");
        let result: Result<()> = async {
            docker_args(&["image", "inspect", "--format", "{{.Id}}", VPN_IMAGE], 15).await
                .map_err(|_| "Start Docker and download the VPN files in Setup, then check your credentials again.")?;
            fs::create_dir_all(&directory).map_err(io_error)?;
            fs::write(directory.join("nord-user"), user).map_err(io_error)?;
            fs::write(directory.join("nord-password"), password).map_err(io_error)?;
            write_json(&path, &document)?;
            let configs = crate::nord::recommended_configs("US").await?;
            connect_recommended(&path, &container, &configs).await
        }.await;
        let cleanup = if path.exists() {
            let mut command = docker();
            command.args(["compose", "--file"]).arg(&path).args(["down", "--timeout", "5"]);
            execute(command, 30).await.map(|_| ())
        } else { Ok(()) };
        for file in ["nord-user", "nord-password", "compose.json", "compose.tmp", "vpn.ovpn"] {
            let _ = fs::remove_file(directory.join(file));
        }
        let _ = fs::remove_dir(&directory);
        if cleanup.is_err() {
            return Err("The credential check could not clean up its temporary connection. Restart Docker Desktop before retrying.".into());
        }
        result.map_err(|message| message.replace(user, "[redacted]").replace(password, "[redacted]"))
    }

    pub async fn create(&self, name: String, country: String) -> Result<Workspace> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "Wait for the current workspace action to finish.")?;
        country_name(&country)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
            return Err("Enter a workspace name between 1 and 48 characters.".into());
        }
        let mut config = self.config.lock().await;
        if config.workspaces.len() >= 10 {
            return Err("This prototype supports up to 10 workspaces. Each running workspace uses a NordVPN connection.".into());
        }
        let port = (32101..32201)
            .find(|p| {
                !config.workspaces.iter().any(|w| w.port == *p)
                    && TcpListener::bind(("127.0.0.1", *p)).is_ok()
            })
            .ok_or("No free browser port was found.")?;
        let workspace = Workspace {
            id: uuid::Uuid::new_v4().simple().to_string()[..12].to_string(),
            name: name.into(),
            country,
            port,
            viewer_token: new_viewer_token(),
        };
        let mut updated = config.clone();
        updated.workspaces.push(workspace.clone());
        write_json(&self.root.join("workspaces.json"), &updated)?;
        *config = updated;
        Ok(workspace)
    }

    async fn prepare(&self, owner: &str, workspace: &Workspace) -> Result<()> {
        let (user, password) = credentials(&self.credentials_path)?;
        let directory = self.root.join(&workspace.id);
        fs::create_dir_all(&directory).map_err(io_error)?;
        fs::write(directory.join("nord-user"), user).map_err(io_error)?;
        fs::write(directory.join("nord-password"), password).map_err(io_error)?;
        // Docker's embedded resolver can forward queries outside the shared VPN namespace.
        // Bind an explicit loopback resolver so Chromium uses Gluetun's DNS service.
        fs::write(
            directory.join("resolv.conf"),
            "nameserver 127.0.0.1\noptions timeout:2 attempts:2\n",
        )
        .map_err(io_error)?;
        write_json(
            &directory.join("compose.json"),
            &compose_document(owner, workspace)?,
        )
    }

    pub async fn start(&self, id: &str) -> Result<()> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "Wait for the current workspace action to finish.")?;
        let (owner, workspace) = self.workspace(id).await?;
        let login = credentials(&self.credentials_path)?;
        self.phase(id, "starting", "Preparing workspace").await;
        self.checks.lock().await.remove(id);
        let result: Result<()> = async {
            self.prepare(&owner, &workspace).await?;
            for image in [VPN_IMAGE, BROWSER_IMAGE] {
                if docker_args(&["image", "inspect", "--format", "{{.Id}}", image], 15).await.is_err() {
                    self.phase(id, "starting", "Downloading browser and VPN files. First start can take several minutes.").await;
                    docker_args(&["pull", image], 900).await?;
                }
            }
            self.phase(id, "starting", "Finding NordVPN’s recommended servers for this country").await;
            let configs = crate::nord::recommended_configs(&workspace.country).await?;
            // Recreate the pair together: a replacement VPN means a new network namespace.
            // Named profile volumes survive `down` because --volumes is never used.
            self.compose(id, &["down", "--timeout", "10"], 60).await?;
            self.phase(id, "starting", "Connecting to a recommended NordVPN server").await;
            connect_recommended(&self.root.join(id).join("compose.json"), &format!("regionbox-{owner}-{id}-vpn"), &configs).await?;
            *self.verified_credentials.lock().await = Some(login);
            self.phase(id, "starting", "NordVPN is connected. Starting Chromium").await;
            self.compose(id, &["up", "--detach", "--wait", "--wait-timeout", "180"], 210).await?;
            self.phase(id, "starting", "Checking the browser connection").await;
            let url = format!("http://127.0.0.1:{}/", workspace.port);
            let response = reqwest::Client::new().get(url).timeout(Duration::from_secs(8)).send().await
                .map_err(|_| "Chromium started, but its browser view is not responding. Restart the workspace.".to_string())?;
            if !response.status().is_success() { return Err("The browser view returned an error. Restart this workspace.".into()); }
            authorize_viewer(&workspace).await?;
            Ok(())
        }.await;
        if let Err(error) = result {
            let logs = self.logs(id).await.unwrap_or_default();
            self.last_logs.lock().await.insert(id.into(), logs.clone());
            let cleanup = self.compose(id, &["down", "--timeout", "10"], 60).await;
            let mut message = if error.contains("vpn") && (error.contains("unhealthy") || error.contains("dependency")) {
                vpn_failure(&logs)
            } else { self.redact(&error) };
            if logs.lines().any(|line| line.contains("[openvpn]") && line.contains("AUTH_FAILED")) {
                message = vpn_failure(&logs);
                *self.verified_credentials.lock().await = None;
            }
            if cleanup.is_err() {
                message
                    .push_str(" Cleanup also failed. Use Stop to retry when Docker is available.");
            }
            self.phase(id, "error", &message).await;
            return Err(message);
        }
        self.phases.lock().await.remove(id);
        // A geolocation service outage must not be mistaken for a failed VPN tunnel.
        // The UI keeps the location explicitly unverified until a successful check.
        let _ = self.verify(id).await;
        Ok(())
    }

    pub async fn stop(&self, id: &str) -> Result<()> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "Wait for the current workspace action to finish.")?;
        self.workspace(id).await?;
        self.phase(id, "stopping", "Stopping browser and VPN").await;
        let result = if self.root.join(id).join("compose.json").exists() {
            self.compose(id, &["down", "--timeout", "10"], 60)
                .await
                .map(|_| ())
        } else {
            Ok(())
        };
        self.checks.lock().await.remove(id);
        match &result {
            Ok(_) => {
                self.phases.lock().await.remove(id);
            }
            Err(e) => self.phase(id, "error", e).await,
        }
        result
    }

    pub async fn stop_all(&self) -> Result<()> {
        let ids: Vec<String> = self
            .config
            .lock()
            .await
            .workspaces
            .iter()
            .map(|w| w.id.clone())
            .collect();
        let mut failures = Vec::new();
        for id in ids {
            if let Err(e) = self.stop(&id).await {
                failures.push(format!("{id}: {e}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    pub async fn verify(&self, id: &str) -> Result<NetworkCheck> {
        let (owner, _) = self.workspace(id).await?;
        let container = format!("regionbox-{owner}-{id}-browser");
        self.checks.lock().await.remove(id);
        let mut verified = None;
        // The marketing website can challenge VPN addresses. Cloudflare's resolver
        // endpoint provides the same trace without relying on that site's policy.
        for endpoint in ["https://1.1.1.1/cdn-cgi/trace", "https://www.cloudflare.com/cdn-cgi/trace"] {
            if let Ok(trace) = docker_args(&["exec", &container, "curl", "-fsS", "--connect-timeout", "8", "--max-time", "15", endpoint], 20).await {
                if let Ok(check) = parse_trace(&trace) { verified = Some(check); break; }
            }
        }
        let check = verified.ok_or("Could not verify the browser IP. Check the connection and try Check IP again.")?;
        self.checks.lock().await.insert(id.into(), check.clone());
        Ok(check)
    }

    pub async fn logs(&self, id: &str) -> Result<String> {
        if !self.root.join(id).join("compose.json").exists() {
            self.workspace(id).await?;
            return Ok("This workspace has not been started yet.".into());
        }
        let text = self
            .compose(id, &["logs", "--no-color", "--tail", "35"], 15)
            .await?;
        if text.trim().is_empty() {
            return Ok(self
                .last_logs
                .lock()
                .await
                .get(id)
                .cloned()
                .unwrap_or_else(|| {
                    "No recent logs. Start this workspace to collect diagnostics.".into()
                }));
        }
        let (_, workspace) = self.workspace(id).await?;
        Ok(self
            .redact(&text)
            .replace(&workspace.viewer_token, "[viewer token redacted]"))
    }
}

fn parse_trace(trace: &str) -> Result<NetworkCheck> {
    let values: HashMap<&str, &str> = trace
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    let ip = values
        .get("ip")
        .ok_or("The IP service returned an incomplete response. Try Check IP again.")?;
    ip.parse::<std::net::IpAddr>()
        .map_err(|_| "The IP service returned an invalid address.".to_string())?;
    let country = values
        .get("loc")
        .filter(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_uppercase()))
        .ok_or("The IP service did not report a country.")?;
    Ok(NetworkCheck {
        ip: (*ip).into(),
        country: (*country).into(),
        checked_at: now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cleanup_test_dir(root: &Path) {
        let resolved = root.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert!(resolved.starts_with(&temporary));
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("regionbox-test-"));
        fs::remove_dir_all(resolved).unwrap();
    }
    #[test]
    fn network_isolation_and_persistence_are_enforced() {
        for (country, _) in COUNTRIES {
            let w = Workspace {
                id: country.to_lowercase(),
                name: "A profile".into(),
                country: country.into(),
                port: 32101,
                viewer_token: new_viewer_token(),
            };
            let document = compose_document("abcd1234", &w).unwrap();
            let vpn = &document["services"]["vpn"];
            let browser = &document["services"]["browser"];
            assert_eq!(browser["network_mode"], "service:vpn");
            assert_eq!(browser["depends_on"]["vpn"]["condition"], "service_healthy");
            assert_eq!(vpn["ports"][0]["host_ip"], "127.0.0.1");
            assert!(browser.get("ports").is_none());
            assert!(vpn["environment"]
                .get("FIREWALL_OUTBOUND_SUBNETS")
                .is_none());
            assert_eq!(vpn["sysctls"]["net.ipv6.conf.all.disable_ipv6"], "1");
            assert_eq!(browser["volumes"][0], "profile:/config");
            assert_eq!(browser["volumes"][1], "./resolv.conf:/etc/resolv.conf:ro");
            assert!(vpn["environment"].get("OPENVPN_PASSWORD").is_none());
            assert_eq!(
                browser["environment"]["SELKIES_MASTER_TOKEN"],
                w.viewer_token
            );
        }
    }
    #[test]
    fn bad_country_and_malformed_ip_are_rejected() {
        assert!(country_name("US; echo oops").is_err());
        assert!(parse_trace("ip=not-an-ip\nloc=US").is_err());
        assert!(parse_trace("ip=1.1.1.1").is_err());
        let check = parse_trace("ip=203.0.113.20\nloc=GB\ntls=TLSv1.3").unwrap();
        assert_eq!(check.country, "GB");
    }
    #[test]
    fn credentials_reject_account_login_and_injection() {
        assert!(validate_credentials("a@example.com", "password").is_err());
        assert!(validate_credentials("service", "value\nOTHER=1").is_err());
        assert!(validate_credentials("", "password").is_err());
        assert!(validate_credentials("service-user", "test-password").is_ok());
    }
    #[test]
    fn vpn_errors_distinguish_login_rejection_from_connection_failure() {
        assert!(vpn_failure("INFO [openvpn] AUTH: Received control message: AUTH_FAILED").contains("rejected"));
        assert!(vpn_failure("Help: AUTH_FAILED can mean expired credentials").contains("could not be verified"));
        assert!(vpn_failure("INFO [openvpn] TLS key negotiation failed").contains("could not be verified"));
        assert!(vpn_failure("INFO [openvpn] Initialization Sequence Completed\nDNS timeout").contains("accepted the login"));
    }
    #[tokio::test]
    async fn new_workspaces_persist_and_do_not_share_ports() {
        let root = std::env::temp_dir().join(format!("regionbox-test-{}", uuid::Uuid::new_v4()));
        let manager = Manager::new(root.clone(), None).unwrap();
        let created = manager
            .create("Extra US".into(), "US".into())
            .await
            .unwrap();
        assert!(created.port > 32103);
        assert!(manager.create(" ".into(), "US".into()).await.is_err());
        drop(manager);
        let reloaded = Manager::new(root.clone(), None).unwrap();
        assert_eq!(reloaded.config.lock().await.workspaces.len(), 4);
        assert_eq!(
            reloaded.workspace(&created.id).await.unwrap().1.name,
            "Extra US"
        );
        cleanup_test_dir(&root);
    }
    #[tokio::test]
    async fn missing_credentials_cannot_create_or_start_a_container() {
        let root = std::env::temp_dir().join(format!("regionbox-test-{}", uuid::Uuid::new_v4()));
        let manager = Manager::new(root.clone(), None).unwrap();
        assert!(manager
            .start("us")
            .await
            .unwrap_err()
            .contains("credentials"));
        assert!(!root.join("us").exists());
        cleanup_test_dir(&root);
    }
    #[test]
    #[ignore = "Requires Docker Compose; validates generated configuration without starting containers"]
    fn docker_compose_accepts_all_countries_and_escapes_names() {
        let root = std::env::temp_dir().join(format!("regionbox-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("nord-user"), "test-only-user").unwrap();
        fs::write(root.join("nord-password"), "test-only-password").unwrap();
        fs::write(root.join("resolv.conf"), "nameserver 127.0.0.1\n").unwrap();
        fs::write(root.join("vpn.ovpn"), "client\n").unwrap();
        for (country, _) in COUNTRIES {
            let workspace = Workspace {
                id: country.to_lowercase(),
                name: "Literal ${HOME}".into(),
                country: country.into(),
                port: 32101,
                viewer_token: new_viewer_token(),
            };
            let document = compose_document("abcd1234", &workspace).unwrap();
            assert_eq!(
                document["services"]["browser"]["environment"]["TITLE"],
                "RegionBox · Literal $${HOME}"
            );
            let path = root.join("compose.json");
            write_json(&path, &document).unwrap();
            let output = std::process::Command::new("docker")
                .args(["compose", "--file"])
                .arg(path)
                .args(["config", "--quiet"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        cleanup_test_dir(&root);
    }
}
