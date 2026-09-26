// Live, opt-in check using the same manager as the desktop app.
// REGIONBOX_ENV_FILE points at existing credentials; their values are never printed.
use regionbox_lib::core::Manager;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let credentials = PathBuf::from(std::env::var_os("REGIONBOX_ENV_FILE").expect("Set REGIONBOX_ENV_FILE to the saved credentials file"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../.local/live-connection-check-{}", uuid::Uuid::new_v4()));
    let manager = Manager::new(root, Some(credentials)).expect("Could not create test workspace settings");
    println!("Checking saved credentials through a temporary NordVPN connection...");
    if let Err(error) = manager.check_saved_credentials().await {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
    println!("PASS: Saved credentials established a healthy NordVPN connection.");
    let args: Vec<_> = std::env::args().collect();
    let sequential = args.iter().position(|arg| arg == "--countries").and_then(|index| args.get(index + 1));
    if args.iter().any(|arg| arg == "--workspaces") || sequential.is_some() {
        let mut success = true;
        let countries: Vec<&str> = sequential.map(|list| list.split(',').collect()).unwrap_or_else(|| vec!["US", "DE", "GB"]);
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
                Ok::<_, String>(())
            }.await;
            if let Err(error) = result {
                eprintln!("FAIL: {error}");
                eprintln!("{}", manager.logs(id).await.unwrap_or_default());
                success = false; break;
            }
            if sequential.is_some() { manager.stop(id).await.expect("Could not stop test workspace"); }
        }
        if success && sequential.is_none() { println!("PASS: US, Germany, and UK browsers are running together."); }
        if let Err(error) = manager.stop_all().await { eprintln!("Cleanup failed: {error}"); success = false; }
        if !success { std::process::exit(1); }
    }
}
