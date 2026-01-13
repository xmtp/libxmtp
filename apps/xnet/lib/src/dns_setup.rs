//! DNS setup helper for configuring host to resolve *.xmtpd.local
//!
//! Provides instructions and utilities for configuring the host system
//! to use CoreDNS for resolving XMTP local hostnames.

use crate::constants::COREDNS_PORT;

/// Print instructions for setting up DNS resolution for *.xmtpd.local
///
/// This function displays platform-specific instructions for configuring
/// the system to use CoreDNS for resolving XMTP local hostnames.
pub fn print_dns_setup_instructions() {
    println!("\n{}", "=".repeat(80));
    println!("DNS SETUP REQUIRED");
    println!("{}", "=".repeat(80));
    println!("\nTo access XMTP services by hostname (e.g., node0.xmtpd.local), configure DNS:\n");

    #[cfg(target_os = "macos")]
    print_macos_instructions();

    #[cfg(target_os = "linux")]
    print_linux_instructions();

    println!("\n{}", "=".repeat(80));
    println!("After setup, verify with:");
    println!("  dig @localhost -p {} node0.xmtpd.local", COREDNS_PORT);
    println!("  # Should return: 127.0.0.1");
    println!("{}\n", "=".repeat(80));
}

#[cfg(target_os = "macos")]
fn print_macos_instructions() {
    println!("macOS Setup:");
    println!("  1. Create a resolver configuration:");
    println!("     sudo mkdir -p /etc/resolver");
    println!("     sudo tee /etc/resolver/xmtpd.local <<EOF");
    println!("nameserver 127.0.0.1");
    println!("port {}", COREDNS_PORT);
    println!("EOF");
    println!("\n  2. Verify the resolver is active:");
    println!("     scutil --dns | grep xmtpd.local");
    println!("\n  3. Test resolution:");
    println!("     ping node0.xmtpd.local");
}

#[cfg(target_os = "linux")]
fn print_linux_instructions() {
    println!("Linux Setup (systemd-resolved):");
    println!("  1. Create a resolved configuration:");
    println!("     sudo tee /etc/systemd/resolved.conf.d/xmtp.conf <<EOF");
    println!("[Resolve]");
    println!("DNS=127.0.0.1:{}", COREDNS_PORT);
    println!("Domains=~xmtpd.local");
    println!("EOF");
    println!("\n  2. Restart systemd-resolved:");
    println!("     sudo systemctl restart systemd-resolved");
    println!("\n  3. Verify configuration:");
    println!("     resolvectl status");
    println!("\nLinux Setup (manual /etc/resolv.conf):");
    println!("  1. Add to /etc/resolv.conf (may be overwritten by DHCP):");
    println!("     nameserver 127.0.0.1");
    println!("\n  2. Consider using dnsmasq or unbound for persistent configuration");
}

/// Check if DNS is configured correctly for *.xmtpd.local
///
/// Attempts to resolve a test hostname and returns `Ok(())` if it resolves
/// to 127.0.0.1, or an error describing the failure.
pub async fn check_dns_configured() -> color_eyre::eyre::Result<()> {
    use color_eyre::eyre::eyre;
    match tokio::net::lookup_host("node0.xmtpd.local:80").await {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) if addr.ip().to_string() == "127.0.0.1" => Ok(()),
            Some(addr) => Err(eyre!(
                "node0.xmtpd.local resolved to {} instead of 127.0.0.1",
                addr.ip()
            )),
            None => Err(eyre!(
                "node0.xmtpd.local resolved but returned no addresses"
            )),
        },
        Err(e) => Err(eyre!("DNS lookup for node0.xmtpd.local failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_instructions() {
        // Just verify the function doesn't panic
        print_dns_setup_instructions();
    }
}
