use console::Term;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::fs;
use std::path::Path;
use std::process::Command;

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

    let mask: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter subnet mask prefix (1-31)")
        .default("24".to_string())
        .validate_with(|input: &String| {
            match input.parse::<u8>() {
                Ok(n) if n >= 1 && n <= 31 => Ok(()),
                _ => Err("Please enter a number between 1 and 31"),
            }
        })
        .interact_text()?;

    let gateway: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter default gateway (e.g., 192.168.1.1)")
        .validate_with(|input: &String| {
            if input.parse::<std::net::Ipv4Addr>().is_ok() {
                Ok(())
            } else {
                Err("Please enter a valid IPv4 address")
            }
        })
        .interact_text()?;

    let dns: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter DNS server (e.g., 223.5.5.5, press Enter to skip)")
        .allow_empty(true)
        .interact_text()?;

    println!("\nApplying network configuration...");

    // Clear existing IP addresses on the interface
    let status = Command::new("ip")
        .args(["addr", "flush", "dev", iface])
        .status()?;
    if !status.success() {
        println!("Warning: Failed to clear old IP address");
    }

    // Add new IP
    let cidr = format!("{}/{}", ip, mask);
    let status = Command::new("ip")
        .args(["addr", "add", &cidr, "dev", iface])
        .status()?;
    if !status.success() {
        println!("Error: Failed to set IP address");
        return Ok(());
    }

    // Bring interface up
    let _ = Command::new("ip")
        .args(["link", "set", iface, "up"])
        .status();

    // Add default route
    let status = Command::new("ip")
        .args(["route", "add", "default", "via", &gateway])
        .status();
    if let Ok(st) = status {
        if !st.success() {
            // Try replacing existing default route
            let _ = Command::new("ip")
                .args(["route", "replace", "default", "via", &gateway])
                .status();
        }
    }

    // Set DNS (if provided)
    if !dns.is_empty() {
        let resolv_entry = format!("nameserver {}\n", dns);
        if let Ok(mut existing) = fs::read_to_string("/etc/resolv.conf") {
            // Remove old nameserver lines for this interface (simple handling)
            existing = existing
                .lines()
                .filter(|line| !line.trim().starts_with("nameserver"))
                .collect::<Vec<_>>()
                .join("\n");
            existing.push('\n');
            existing.push_str(&resolv_entry);
            let _ = fs::write("/etc/resolv.conf", existing);
        } else {
            let _ = fs::write("/etc/resolv.conf", resolv_entry);
        }
    }

    println!("\n✓ Network configuration applied:");
    println!("  Interface: {}", iface);
    println!("  IP: {}/{}", ip, mask);
    println!("  Gateway: {}", gateway);
    if !dns.is_empty() {
        println!("  DNS: {}", dns);
    }

    // Persist configuration
    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Persist configuration to systemd-networkd (survives reboot)?")
        .default(true)
        .interact()?
    {
        persist_systemd_networkd(iface, &ip, &mask, &gateway, &dns)?;
    }

    Ok(())
}

fn persist_systemd_networkd(
    iface: &str,
    ip: &str,
    mask: &str,
    gateway: &str,
    dns: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let dns_line = if dns.is_empty() {
        String::new()
    } else {
        format!("DNS={}\n", dns)
    };

    let config = format!(
        "[Match]\nName={}\n\n[Network]\nAddress={}/{}\nGateway={}\n{}\n",
        iface, ip, mask, gateway, dns_line
    );

    let network_dir = Path::new("/etc/systemd/network");
    if !network_dir.exists() {
        fs::create_dir_all(network_dir)?;
    }

    let config_path = network_dir.join(format!("10-{}.network", iface));
    fs::write(&config_path, config)?;

    // Enable and restart systemd-networkd
    let _ = Command::new("systemctl")
        .args(["enable", "systemd-networkd"])
        .status();
    let status = Command::new("systemctl")
        .args(["restart", "systemd-networkd"])
        .status()?;

    if status.success() {
        println!("✓ Configuration persisted to {:?}", config_path);
    } else {
        println!("Warning: systemd-networkd restart failed; configuration written but may not be active");
    }

    Ok(())
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

    loop {
        let _ = term.clear_screen();

        let options = vec![
            "Set Manual IP for Network Interface",
            "Reboot System",
            "Power Off",
            "Exit",
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("=== System Management Tool ===")
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
