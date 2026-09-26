use serde_json::Value;
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;

async fn get(client: &reqwest::Client, url: reqwest::Url) -> Result<String> {
    client.get(url).send().await.and_then(reqwest::Response::error_for_status)
        .map_err(|_| "Could not get current server details from NordVPN. Check your internet connection and retry.")?
        .text().await.map_err(|_| "Could not read NordVPN server details. Retry the connection.".into())
}

fn hosts(response: &Value, country: &str) -> Vec<String> {
    response.as_array().into_iter().flatten().filter_map(|server| {
        let hostname = server["hostname"].as_str()?;
        let prefix = hostname.strip_suffix(".nordvpn.com")?;
        let matches_country = server["locations"].as_array()?.iter()
            .any(|location| location["country"]["code"].as_str() == Some(country));
        let supports_openvpn = server["technologies"].as_array()?.iter()
            .any(|technology| technology["identifier"] == "openvpn_udp");
        (server["status"] == "online" && matches_country && supports_openvpn
            && !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .then(|| hostname.to_string())
    }).take(3).collect()
}

fn validate_config(config: &str, hostname: &str) -> Result<()> {
    let verified_name = format!("verify-x509-name CN={hostname}");
    if config.len() > 64 * 1024 || !config.lines().any(|line| line.trim() == verified_name)
        || !config.contains("<ca>") || !config.lines().any(|line| line.trim() == "remote-cert-tls server")
        || !config.lines().any(|line| line.starts_with("remote ")) {
        return Err("NordVPN returned an unexpected server configuration. Retry the connection.".into());
    }
    // Downloaded connection data must not introduce external scripts or plugins.
    for line in config.lines() {
        let directive = line.split_whitespace().next().unwrap_or_default();
        if ["up", "down", "route-up", "route-pre-down", "ipchange", "tls-verify", "plugin", "script-security", "config"].contains(&directive) {
            return Err("NordVPN returned an unsupported server configuration. Retry the connection.".into());
        }
    }
    Ok(())
}

pub async fn recommended_configs(country: &str) -> Result<Vec<String>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()
        .map_err(|_| "Could not initialize the server lookup.")?;
    let countries: Value = serde_json::from_str(&get(&client, "https://api.nordvpn.com/v1/servers/countries".parse().unwrap()).await?)
        .map_err(|_| "NordVPN returned an invalid country list.")?;
    let id = countries.as_array().into_iter().flatten().find(|entry| entry["code"] == country)
        .and_then(|entry| entry["id"].as_u64()).ok_or("NordVPN has no available servers for this country.")?;
    let mut url: reqwest::Url = "https://api.nordvpn.com/v1/servers/recommendations".parse().unwrap();
    url.query_pairs_mut().append_pair("filters[country_id]", &id.to_string())
        .append_pair("filters[servers_technologies][identifier]", "openvpn_udp").append_pair("limit", "3");
    let servers: Value = serde_json::from_str(&get(&client, url).await?).map_err(|_| "NordVPN returned an invalid recommendation.")?;
    let mut configs = Vec::new();
    for host in hosts(&servers, country) {
        let url = format!("https://downloads.nordcdn.com/configs/files/ovpn_udp/servers/{host}.udp.ovpn").parse().unwrap();
        if let Ok(config) = get(&client, url).await {
            if validate_config(&config, &host).is_ok() { configs.push(config); }
        }
    }
    if configs.is_empty() { return Err("No current OpenVPN configuration is available for this country. Retry in a moment.".into()); }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn recommendations_preserve_order_and_reject_wrong_countries_and_hosts() {
        let server = |host: &str, country: &str| json!({"hostname":host,"status":"online","locations":[{"country":{"code":country}}],"technologies":[{"identifier":"openvpn_udp"}]});
        let response = json!([server("us2.nordvpn.com", "US"), server("de1.nordvpn.com", "DE"), server("evil.example", "US"), server("us1.nordvpn.com", "US")]);
        assert_eq!(hosts(&response, "US"), ["us2.nordvpn.com", "us1.nordvpn.com"]);
    }
    #[test]
    fn configuration_keeps_current_ports_and_requires_server_identity() {
        let config = "client\nremote 203.0.113.2 1231\nverify-x509-name CN=us2.nordvpn.com\nremote-cert-tls server\n<ca>\ncertificate\n</ca>\n";
        assert!(validate_config(config, "us2.nordvpn.com").is_ok());
        assert!(validate_config(config, "de1.nordvpn.com").is_err());
        assert!(validate_config(&format!("{config}up /tmp/script\n"), "us2.nordvpn.com").is_err());
    }
    #[tokio::test]
    #[ignore = "Requires internet access to NordVPN's public recommendations and configuration endpoints"]
    async fn current_recommendations_exist_for_all_supported_countries() {
        for (country, _) in crate::core::COUNTRIES {
            let configs = recommended_configs(country).await.unwrap_or_else(|error| panic!("{country}: {error}"));
            assert!(!configs.is_empty(), "{country}");
        }
    }
}
