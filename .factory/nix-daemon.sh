#!/bin/bash
# Installed root-owned outside the checkout; used after snapshot restoration.
set -euo pipefail
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
if [[ $# -ne 0 || $(id -u) -ne 0 ]]; then
  echo 'The Droid Nix daemon helper requires root and takes no arguments.' >&2
  exit 1
fi

# Never pass session/user configuration to the privileged daemon.
daemon_env=(env -i HOME=/root PATH=/usr/sbin:/usr/bin:/sbin:/bin
  NIX_USER_CONF_FILES=/dev/null NIX_CONF_DIR=/etc/nix
  $'NIX_CONFIG=sandbox = true\nsandbox-fallback = false')
nix_bin="/nix/var/nix/profiles/default/bin"
ping_daemon() {
  "${daemon_env[@]}" "$nix_bin/nix" --extra-experimental-features nix-command \
    store ping --store daemon >/dev/null 2>&1
}

# Serialize concurrent shell entries and restart a daemon lost on restore.
exec 9>"/run/libxmtp-nix-daemon.lock"
flock 9
if ping_daemon; then
  exit 0
fi
# dpkg's launcher detaches the daemon without depending on systemd or a shell
# process surviving the setup command. No session-supplied executable or args.
"${daemon_env[@]}" "/usr/sbin/start-stop-daemon" --start --background --oknodo \
  --make-pidfile --pidfile "/run/libxmtp-nix-daemon.pid" \
  --exec "$nix_bin/nix-daemon" 9>&-
for ((attempt = 0; attempt < 100; attempt++)); do
  if ping_daemon; then
    exit 0
  fi
  sleep 0.1
done
echo 'Nix daemon did not become ready; check the cloud kernel sandbox support.' >&2
exit 1
