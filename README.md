# sysconfig

A system management tool for Debian 13 after automatic login, built with Rust + TUI.

## Features

1. **Set Manual IP for Network Interface** — Select a network interface and configure static IP, subnet mask, gateway, and DNS; supports persistence to systemd-networkd
2. **Reboot System**
3. **Power Off**

## How It Works

- Automatic login on tty1 is achieved via `systemd getty@.service` + `agetty --autologin`
- This program is automatically started after login
- The program executes network configuration, reboot/poweroff commands via passwordless `sudo`

## Build Requirements

- Rust 1.70+
- Debian 13 (trixie) or compatible system
- systemd

## Installation

```bash
sudo ./install.sh
```

The install script performs the following steps:
1. Compile the Release build
2. Install the binary to `/usr/local/bin/sysconfig`
3. Create the `sysconfig` user
4. Configure passwordless sudo permissions
5. Configure tty1 auto-login
6. Set the program to auto-start after login

## Manual Configuration

If you prefer not to use the install script, you can manually run:

```bash
# Build
cargo build --release

# Install
cp target/release/sysconfig /usr/local/bin/

# Configure tty1 auto-login
mkdir -p /etc/systemd/system/getty@tty1.service.d
cat > /etc/systemd/system/getty@tty1.service.d/override.conf <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin <username> --noclear %I \$TERM
EOF

# Reload systemd
systemctl daemon-reload
```

## Uninstallation

```bash
sudo rm -f /usr/local/bin/sysconfig
sudo rm -rf /etc/systemd/system/getty@tty1.service.d
sudo rm -f /etc/sudoers.d/sysconfig
sudo userdel -r sysconfig 2>/dev/null || true
sudo systemctl daemon-reload
```

## Notes

- This program requires root privileges to perform network configuration and reboot/poweroff operations
- The auto-login user executes specified commands via passwordless sudo with minimized privileges
- Network configuration persistence depends on systemd-networkd
- If the system uses NetworkManager to manage the network, additional configuration may be required
