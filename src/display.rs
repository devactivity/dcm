use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use humansize::{format_size, DECIMAL};
use prettytable::{format, row, Table};

use crate::docker::{Container, ContainerStats, Port};

pub fn print_containers_table(containers: &[Container], quiet: bool) -> Result<()> {
    if containers.is_empty() {
        println!("no containers found");
        return Ok(());
    }

    if quiet {
        for container in containers {
            println!("{}", container.id.chars().take(12).collect::<String>());
        }

        return Ok(());
    }

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    // add header row
    table.set_titles(row![
        "CONTAINER ID",
        "IMAGE",
        "COMMAND",
        "CREATED",
        "STATUS",
        "PORTS",
        "NAMES",
    ]);

    for container in containers {
        let short_id = container.id.chars().take(12).collect::<String>();

        let mut name = String::new();
        if let Some(names) = &container.names {
            if !names.is_empty() {
                name = names[0].trim_start_matches('/').to_string();
            }
        }

        let created = DateTime::<Utc>::from_timestamp(container.created as i64, 0)
            .map(|dt| {
                let now = Utc::now();
                let duration = now.signed_duration_since(dt);

                if duration.num_days() > 0 {
                    format!("{} days ago", duration.num_days())
                } else if duration.num_hours() > 0 {
                    format!("{} hours ago", duration.num_hours())
                } else if duration.num_minutes() > 0 {
                    format!("{} minutes ago", duration.num_minutes())
                } else {
                    format!("{} seconds ago", duration.num_seconds())
                }
            })
            .unwrap_or_else(|| "Unknown".to_string());

        let status = match container.state.as_str() {
            "running" => container.status.green(),
            "paused" => container.status.yellow(),
            "exited" => container.status.red(),
            _ => container.status.normal(),
        };

        let ports = match &container.ports {
            Some(ports) if !ports.is_empty() => ports
                .iter()
                .map(|p| format_port(p))
                .collect::<Vec<String>>()
                .join(", "),
            _ => String::new(),
        };

        // truncate command if too long
        let command = if container.command.len() > 20 {
            format!("{}...", &container.command[..17])
        } else {
            container.command.clone()
        };

        table.add_row(row![
            short_id,
            container.image,
            command,
            created,
            status,
            ports,
            name
        ]);
    }
    Ok(())
}

fn format_port(port: &Port) -> String {
    match (port.ip.as_ref(), port.pub_port) {
        (Some(ip), Some(public_port)) => {
            format!(
                "{}:{}->{}/{}",
                ip, public_port, port.private_port, port.port_type
            )
        }
        (None, Some(public_port)) => {
            format!("{}:{}/{}", public_port, port.private_port, port.port_type)
        }
        _ => format!("{}/{}", port.private_port, port.port_type),
    }
}

pub fn print_stats_header() {
    let mut table = Table::new();

    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    table.set_titles(row![
        "CONTAINER ID",
        "NAME",
        "CPU %",
        "MEM USAGE / LIMIT",
        "MEM %",
        "NET I/O",
        "BLOCK I/O",
    ]);

    table.printstd();
}

pub fn print_container_stats(stats: &ContainerStats) -> Result<()> {
    let short_id = stats.id.chars().take(12).collect::<String>();

    let cpu_percentage = calculate_cpu_percentage(stats);

    let memory_usage = stats.memory_stats.usage.unwrap_or(0);
    let memory_limit = stats.memory_stats.limit.unwrap_or(0);

    let memory_usage_str = format_size(memory_usage, DECIMAL);
    let memory_limit_str = format_size(memory_limit, DECIMAL);

    let memory_str = format!("{memory_usage_str} / {memory_limit_str}");

    // calculate memory percentage
    let memory_percent = if memory_limit > 0 {
        (memory_usage as f64 / memory_limit as f64) * 100.0
    } else {
        0.0
    };

    // format network I/O
    let mut net_rx = 0;
    let mut net_tx = 0;

    if let Some(networks) = &stats.networks {
        for (_, network) in networks {
            net_rx += network.rx_bytes;
            net_tx += network.tx_bytes;
        }
    }

    let net_io = format!(
        "{} / {}",
        format_size(net_rx, DECIMAL),
        format_size(net_tx, DECIMAL),
    );

    // block I/O is not available
    let block_io = "N/A".to_string();

    let mut table = Table::new();

    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    table.add_row(row![
        short_id,
        stats.name.trim_start_matches('/'),
        format!("{:.2}%", cpu_percentage),
        memory_str,
        format!("{:.2}%", memory_percent),
        net_io,
        block_io
    ]);

    table.printstd();
    Ok(())
}

fn calculate_cpu_percentage(stats: &ContainerStats) -> f64 {
    let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
        - stats.precpu_stats.cpu_usage.total_usage as f64;

    let system_cpu_delta = match (
        stats.cpu_stats.system_cpu_usage,
        stats.precpu_stats.system_cpu_usage,
    ) {
        (Some(current), Some(previous)) => current as f64 - previous as f64,
        _ => 0.0,
    };

    let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;

    if system_cpu_delta > 0.0 && cpu_delta > 0.0 {
        (cpu_delta / system_cpu_delta) * num_cpus * 100.0
    } else {
        0.0
    }
}
