#!/bin/bash
set -e

PROGRAM_NAME="sysconfig"
BINARY_PATH="/usr/local/bin/${PROGRAM_NAME}"
SERVICE_DIR="/etc/systemd/system/getty@tty1.service.d"
USER_NAME="sysconfig"
SUDOERS_FILE="/etc/sudoers.d/${PROGRAM_NAME}"

echo "=== sysconfig install script ==="

# 1. Compile Release build
echo "[1/6] Building Rust project..."
cargo build --release

# 2. Install binary
echo "[2/6] Installing binary to ${BINARY_PATH}..."
cp "target/release/${PROGRAM_NAME}" "${BINARY_PATH}"
chmod 755 "${BINARY_PATH}"

# 3. Create user (if not exists)
echo "[3/6] Creating user ${USER_NAME}..."
if ! id "${USER_NAME}" &>/dev/null; then
    useradd -m -s /bin/bash "${USER_NAME}"
else
    echo "User ${USER_NAME} already exists, skipping creation"
fi

# 4. Configure passwordless sudo (for commands this user needs)
echo "[4/6] Configuring passwordless sudo..."
cat > "${SUDOERS_FILE}" <<EOF
${USER_NAME} ALL=(root) NOPASSWD: /usr/local/bin/sysconfig
${USER_NAME} ALL=(root) NOPASSWD: /sbin/ip
${USER_NAME} ALL=(root) NOPASSWD: /usr/bin/systemctl restart systemd-networkd
${USER_NAME} ALL=(root) NOPASSWD: /usr/bin/systemctl enable systemd-networkd
${USER_NAME} ALL=(root) NOPASSWD: /sbin/reboot
${USER_NAME} ALL=(root) NOPASSWD: /sbin/poweroff
EOF
chmod 440 "${SUDOERS_FILE}"

# 5. Configure auto-login
echo "[5/6] Configuring tty1 auto-login..."
mkdir -p "${SERVICE_DIR}"
cp systemd/getty@tty1.service.d/override.conf "${SERVICE_DIR}/override.conf"
chmod 644 "${SERVICE_DIR}/override.conf"

# 6. Configure auto-start after user login
echo "[6/6] Configuring auto-start after login..."
cat >> "/home/${USER_NAME}/.bash_profile" <<'EOF'

# Auto-start sysconfig (only on tty1)
if [[ $(tty) == /dev/tty1 ]]; then
    sudo /usr/local/bin/sysconfig
    # After program exits, you can choose to logout or keep the shell
    # logout
fi
EOF

# Reload systemd
echo "Reloading systemd..."
systemctl daemon-reload

echo ""
echo "=== Installation complete ==="
echo "User: ${USER_NAME}"
echo "Service: ${SERVICE_DIR}/override.conf"
echo ""
echo "After reboot, tty1 will auto-login as ${USER_NAME} and start sysconfig"
echo "You can reboot now, or manually test with:"
echo "  sudo systemctl restart getty@tty1"
