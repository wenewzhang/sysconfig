use console::Term;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::fs;
use std::path::Path;
use std::process::Command;

const NETWORK_DIR: &str = "/etc/systemd/network";

fn get_network_interfaces() -> Vec<String> {
    let mut interfaces = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/net/") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name != "lo" {
                interfaces.push(name);
            }
        }
    }
    interfaces.sort();
    interfaces
}

fn set_static_ip() -> Result<(), Box<dyn std::error::Error>> {
    let interfaces = get_network_interfaces();
    if interfaces.is_empty() {
        println!("No available network interfaces found");
        return Ok(());
    }

    let iface_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select network interface to configure")
        .items(&interfaces)
        .interact()?;
    let iface = &interfaces[iface_idx];

    let ip: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter IP address (e.g., 192.168.1.100)")
        .validate_with(|input: &String| {
            if input.parse::<std::net::Ipv4Addr>().is_ok() {
                Ok(())
            } else {
                Err("Please enter a valid IPv4 address")
            }
        })
        .interact_text()?;

    let mask = "24".to_string();

    let gateway = {
        let parts: Vec<&str> = ip.split('.').collect();
        format!("{}.{}.{}.1", parts[0], parts[1], parts[2])
    };

    let dns = gateway.clone();

    println!("\nApplying network configuration...");
    println!("\n✓ Network configuration applied:");
    println!("  Interface: {}", iface);
    println!("  IP: {}/{}", ip, mask);
    println!("  Gateway: {}", gateway);
    if !dns.is_empty() {
        println!("  DNS: {}", dns);
    }

    let dhcp_file = format!("{}/10-dhcp.network", NETWORK_DIR);
    if Path::new(&dhcp_file).exists() {
        fs::remove_file(&dhcp_file)?;
    }

    persist_systemd_networkd(iface, &ip, &mask, &gateway, &dns)?;

    if let Err(e) = update_zuti_env(&ip) {
        println!("Warning: Failed to update zuti env: {}", e);
    }

    if let Err(e) = update_webui(&ip) {
        println!("Warning: Failed to update webui: {}", e);
    }

    Ok(())
}

fn update_zuti_env(ip: &str) -> Result<(), Box<dyn std::error::Error>> {
    let zuti_env = Path::new("/etc/zuti/.env");
    if !zuti_env.exists() {
        println!("  Zuti .env file not found, skipping");
        return Ok(());
    }

    let content = fs::read_to_string(zuti_env)?;
    let new_content = content
        .lines()
        .map(|line| {
            if line.trim().starts_with("SERVER_ADDRESS=") {
                format!("SERVER_ADDRESS={}:8443", ip)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(zuti_env, new_content)?;
    println!("✓ Updated zuti SERVER_ADDRESS to {}", ip);

    let status = Command::new("systemctl")
        .args(["restart", "zuti"])
        .status()?;

    if status.success() {
        println!("✓ Zuti service restarted");
    } else {
        println!("Warning: systemctl restart zuti failed");
    }

    Ok(())
}

fn update_webui(ip: &str) -> Result<(), Box<dyn std::error::Error>> {
    let src = Path::new("/etc/nginx/sites-available/webui");
    if !src.exists() {
        println!("  Webui config not found, skipping");
        return Ok(());
    }

    let enable_src = Path::new("/etc/nginx/sites-available/webui-enable");
    let enable_link = Path::new("/etc/nginx/sites-enabled/webui");

    // Copy to webui-enable
    fs::copy(src, enable_src)?;

    // Replace 127.0.0.1:8443 with IP:8443
    let content = fs::read_to_string(enable_src)?;
    let new_content = content.replace("127.0.0.1:8443", &format!("{}:8443", ip));
    fs::write(enable_src, new_content)?;

    // Remove old symlink if exists
    if enable_link.exists() {
        fs::remove_file(enable_link)?;
    }

    // Create symlink
    std::os::unix::fs::symlink(enable_src, enable_link)?;

    println!("✓ Updated webui nginx config and enabled");

    let status = Command::new("systemctl")
        .args(["restart", "nginx"])
        .status()?;

    if status.success() {
        println!("✓ Nginx service restarted");
    } else {
        println!("Warning: systemctl restart nginx failed");
    }

    Ok(())
}

fn persist_systemd_networkd(
    iface: &str,
    ip: &str,
    mask: &str,
    gateway: &str,
    _dns: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let network_file = format!("{}/10-{}.network", NETWORK_DIR, iface);

    // 1. Ensure /etc/systemd/network exists
    fs::create_dir_all(NETWORK_DIR)?;

    // 2. Build systemd-networkd configuration content
    let content = format!(
        "[Match]\nName={}\n\n[Network]\nAddress={}/{}\nGateway={}\nDNS={}\nDNS=8.8.8.8\nDNS=114.114.114.114\n",
        iface, ip, mask, gateway, gateway
    );

    fs::write(&network_file, content)?;

    println!("✓ Configuration persisted to {}", network_file);

    // 3. Restart systemd-networkd
    let status = Command::new("systemctl")
        .args(["restart", "systemd-networkd"])
        .status()?;

    if status.success() {
        println!("✓ systemd-networkd service restarted");
    } else {
        println!("Warning: systemctl restart systemd-networkd failed");
    }

    Ok(())
}

fn get_local_ipv4_addrs() -> Vec<String> {
    let mut addrs = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/net/") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "lo" {
                continue;
            }
            if let Ok(output) = Command::new("ip")
                .args(["-4", "-o", "addr", "show", &name])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let addr = parts[3].split('/').next().unwrap_or(parts[3]);
                        if addr.parse::<std::net::Ipv4Addr>().is_ok() {
                            addrs.push(addr.to_string());
                        }
                    }
                }
            }
        }
    }
    addrs
}

fn show_nginx_status() {
    let status = Command::new("systemctl")
        .args(["is-active", "--quiet", "nginx"])
        .status();

    let is_active = match status {
        Ok(s) => s.success(),
        Err(e) => {
            println!("Failed to check nginx status: {}", e);
            return;
        }
    };

    if !is_active {
        println!("Nginx status: inactive");
        return;
    }

    println!("Nginx status: active");

    let output = match Command::new("ss").args(["-tlnp"]).output() {
        Ok(o) => o,
        Err(_) => {
            println!("Failed to get nginx listening info");
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let local_ips = get_local_ipv4_addrs();
    let mut printed = Vec::new();

    for line in stdout.lines() {
        if line.contains("nginx") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(local_addr) = parts.get(3) {
                if let Some((ip, port_str)) = local_addr.rsplit_once(':') {
                    let ip = ip.trim_start_matches('[').trim_end_matches(']');
                    let port = match port_str.parse::<u16>() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    let is_wildcard = ip == "0.0.0.0" || ip == "*" || ip == "::";

                    if is_wildcard {
                        if local_ips.is_empty() {
                            let url = match port {
                                443 | 8443 => "https://localhost".to_string(),
                                80 => "http://localhost".to_string(),
                                _ => format!("http://localhost:{}", port),
                            };
                            if !printed.contains(&url) {
                                println!("WebUI is listening at: {}", url);
                                printed.push(url);
                            }
                        } else {
                            for addr in &local_ips {
                                let url = match port {
                                    443 | 8443 => format!("https://{}", addr),
                                    80 => format!("http://{}", addr),
                                    _ => format!("http://{}:{}", addr, port),
                                };
                                if !printed.contains(&url) {
                                    println!("WebUI is listening at: {}", url);
                                    printed.push(url);
                                }
                            }
                        }
                    } else {
                        let url = match port {
                            443 | 8443 => format!("https://{}", ip),
                            80 => format!("http://{}", ip),
                            _ => format!("http://{}:{}", ip, port),
                        };
                        if !printed.contains(&url) {
                            println!("WebUI is listening at: {}", url);
                            printed.push(url);
                        }
                    }
                }
            }
        }
    }

    if printed.is_empty() {
        println!("No nginx listening addresses found");
    }
}

fn reboot_system() {
    println!("Rebooting system...");
    let _ = Command::new("reboot").status();
}

fn poweroff_system() {
    println!("Powering off...");
    let _ = Command::new("poweroff").status();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let term = Term::stdout();

    // Check if running as root
    if unsafe { libc::getuid() } != 0 {
        println!("Warning: This program requires root privileges to work properly");
        println!("Please run with sudo or configure passwordless sudo\n");
    }

    let version = fs::read_to_string("/.data/.version")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Unknown".to_string());

    loop {
        let _ = term.clear_screen();

        show_nginx_status();
        println!();
        
        let options = vec![
            "Set Manual IP for Network Interface",
            "Reboot System",
            "Power Off",
            "Exit(Login root by: su -)",
        ];

        let prompt = format!("=== System Management Tool [{}] ===", version);
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(&prompt)
            .items(&options)
            .default(0)
            .interact()?;

        match selection {
            0 => {
                if let Err(e) = set_static_ip() {
                    println!("Operation failed: {}", e);
                }
            }
            1 => {
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("Are you sure you want to reboot the system?")
                    .interact()?
                {
                    reboot_system();
                    break;
                }
            }
            2 => {
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("Are you sure you want to power off?")
                    .interact()?
                {
                    poweroff_system();
                    break;
                }
            }
            3 => break,
            _ => unreachable!(),
        }

        println!("\nPress Enter to return to main menu...");
        let _ = term.read_line();
    }

    Ok(())
}
