use std::process::Command;
use std::error::Error;
use std::env;
use log::info;

pub fn add_firewall_rule(port: u16) -> Result<(), Box<dyn Error>> {
    let os = env::consts::OS;

    match os {
        "windows" => {
            let output = Command::new("netsh")
                .args(&[
                    "advfirewall", "firewall", "add", "rule",
                    "name=UNICLIP", "dir=in", "action=allow",
                    &format!("protocol=UDP"), &format!("localport={}", port)
                ])
                .output()?;

            if !output.status.success() {
                return Err(format!("Failed to add firewall rule: {:?}", output).into());
            }
            info!("Firewall rule added on Windows for port {}", port);
        }
        "linux" => {
            let output = Command::new("sudo")
                .args(&["iptables", "-A", "INPUT", "-p", "udp", "--dport", &port.to_string(), "-j", "ACCEPT"])
                .output()?;

            if !output.status.success() {
                return Err(format!("Failed to add firewall rule: {:?}", output).into());
            }
            info!("Firewall rule added on Linux for port {}", port);
        }
        "macos" => {
            let output = Command::new("sudo")
                .args(&["pfctl", "-f", "/etc/pf.conf", "-e"])
                .output()?;

            if !output.status.success() {
                return Err(format!("Failed to add firewall rule: {:?}", output).into());
            }
            info!("Firewall rule added on macOS for port {}", port);
        }
        _ => {
            return Err("Unsupported OS".into());
        }
    }

    Ok(())
}