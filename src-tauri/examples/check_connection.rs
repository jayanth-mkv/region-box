// Live, opt-in check using the same manager as the desktop app.
// REGIONBOX_ENV_FILE points at existing credentials; their values are never printed.
use regionbox_lib::core::Manager;
use std::path::PathBuf;
use std::process::{Command, Output};

fn docker(args: &[&str]) -> Output {
    let mut command = Command::new("docker");
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        command.args(["--context", "desktop-linux"]);
    }
    command.args(args).output().expect("Docker check failed to run")
}

fn check_network_safety(root: &std::path::Path, id: &str) -> Result<(), String> {
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join(id).join("compose.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let vpn = doc["services"]["vpn"]["container_name"].as_str().unwrap();
    let browser = doc["services"]["browser"]["container_name"].as_str().unwrap();
    let dns = docker(&["exec", browser, "cat", "/etc/resolv.conf"]);
    if !String::from_utf8_lossy(&dns.stdout).contains("nameserver 127.0.0.1") { return Err("Browser DNS is not using the tunnel resolver".into()); }
    let ipv6 = docker(&["exec", browser, "cat", "/proc/sys/net/ipv6/conf/all/disable_ipv6"]);
    if String::from_utf8_lossy(&ipv6.stdout).trim() != "1" { return Err("IPv6 is not disabled".into()); }
    if !docker(&["pause", vpn]).status.success() { return Err("Could not interrupt test VPN".into()); }
    let blocked = !docker(&["exec", browser, "curl", "--noproxy", "*", "-fsS", "--connect-timeout", "3", "--max-time", "6", "https://1.1.1.1/cdn-cgi/trace"]).status.success();
    let restored = docker(&["unpause", vpn]).status.success();
    if !restored { return Err("Could not resume test VPN".into()); }
    if !blocked { return Err("Browser could reach the internet while its VPN was paused".into()); }
    println!("PASS: tunnel DNS, IPv6 disabled, and internet access blocked while the isolated VPN was paused.");
    Ok(())
}

#[tokio::main]
async fn main() {
    let credentials = PathBuf::from(std::env::var_os("REGIONBOX_ENV_FILE").expect("Set REGIONBOX_ENV_FILE to the saved credentials file"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../.local/live-connection-check-{}", uuid::Uuid::new_v4()));
    // Keep test viewers away from the user's running default workspaces.
    drop(Manager::new(root.clone(), Some(credentials.clone())).expect("Could not seed test settings"));
    let settings_path = root.join("workspaces.json");
    let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&settings_path).unwrap()).unwrap();
    for (index, workspace) in settings["workspaces"].as_array_mut().unwrap().iter_mut().enumerate() {
        workspace["port"] = serde_json::json!(32181 + index);
    }
    std::fs::write(settings_path, serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    let manager = Manager::new(root.clone(), Some(credentials)).expect("Could not create test workspace settings");
    println!("Checking saved credentials through a temporary NordVPN connection...");
    if let Err(error) = manager.check_saved_credentials().await {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
    println!("PASS: Saved credentials established a healthy NordVPN connection.");
    let args: Vec<_> = std::env::args().collect();
    let safety = args.iter().any(|arg| arg == "--safety");
    let sequential = args.iter().position(|arg| arg == "--countries").and_then(|index| args.get(index + 1));
    if args.iter().any(|arg| arg == "--workspaces") || sequential.is_some() || safety {
        let mut success = true;
        let countries: Vec<&str> = sequential.map(|list| list.split(',').collect()).unwrap_or_else(|| if safety { vec!["US"] } else { vec!["US", "DE", "GB"] });
        for country in countries {
            let snapshot = manager.snapshot().await;
            let workspace = if let Some(existing) = snapshot.workspaces.iter().find(|workspace| workspace.workspace.country == country) {
                existing.workspace.clone()
            } else { manager.create(format!("Test {country}"), country.into()).await.expect("Could not create isolated test workspace") };
            let id = &workspace.id;
            println!("Starting {country} browser workspace...");
            let result = async {
                manager.start(id).await?;
                let check = manager.verify(id).await?;
                if check.country != country { return Err(format!("Expected {country}, got {}", check.country)); }
                println!("PASS: {country} browser connection verified.");
                if safety {
                    check_network_safety(&root, id)?;
                    manager.verify(id).await?;
                    println!("PASS: browser internet access recovered after the VPN resumed.");
                }
                Ok::<_, String>(())
            }.await;
            if let Err(error) = result {
                eprintln!("FAIL: {error}");
                eprintln!("{}", manager.logs(id).await.unwrap_or_default());
                success = false; break;
            }
            if sequential.is_some() { manager.stop(id).await.expect("Could not stop test workspace"); }
        }
        if success && sequential.is_none() && !safety { println!("PASS: US, Germany, and UK browsers are running together."); }
        if let Err(error) = manager.stop_all().await { eprintln!("Cleanup failed: {error}"); success = false; }
        if !success { std::process::exit(1); }
    }
}
