use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sparrowshell_domain::TargetRef;
use std::collections::HashMap;

pub const COLLECT_COMMAND: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "printf '\\036HOST\\n'; hostname; uname -s; uname -r; uname -m; ",
    "printf '\\036OS\\n'; cat /etc/os-release; ",
    "printf '\\036CPU1\\n'; cat /proc/stat; sleep 1; ",
    "printf '\\036CPU2\\n'; cat /proc/stat; ",
    "printf '\\036MEMORY\\n'; cat /proc/meminfo; ",
    "printf '\\036UPTIME\\n'; cat /proc/uptime; cat /proc/loadavg; ",
    "printf '\\036DISKS\\n'; df -P -k; ",
    "printf '\\036PROCESSES\\n'; ps -eo pid,ppid,user,pcpu,pmem,lstart,args"
);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub target: TargetRef,
    pub collected_at: DateTime<Utc>,
    pub hostname: String,
    pub os_name: String,
    pub kernel: String,
    pub architecture: String,
    pub cpu_cores: u32,
    pub uptime_seconds: f64,
    pub load_average: [f64; 3],
    pub cpu_usage_percent: f64,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub disks: Vec<DiskUsage>,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub filesystem: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub start_id: String,
    pub command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("监控输出不完整或无法解析")]
    InvalidSample,
}

pub fn parse_sample(target: TargetRef, output: &str) -> Result<SystemSnapshot, MonitorError> {
    let sections = sections(output);
    let host = required(&sections, "HOST")?;
    if host.len() < 4 {
        return Err(MonitorError::InvalidSample);
    }
    let os_name = required(&sections, "OS")?
        .iter()
        .find_map(|line| {
            line.strip_prefix("PRETTY_NAME=")
                .map(|value| value.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| host[1].clone());
    let uptime = required(&sections, "UPTIME")?;
    let uptime_seconds = uptime
        .first()
        .and_then(|line| line.split_whitespace().next())
        .ok_or(MonitorError::InvalidSample)?
        .parse()
        .map_err(|_| MonitorError::InvalidSample)?;
    let load_values = uptime
        .get(1)
        .ok_or(MonitorError::InvalidSample)?
        .split_whitespace()
        .take(3)
        .map(|value| value.parse().map_err(|_| MonitorError::InvalidSample))
        .collect::<Result<Vec<f64>, _>>()?;
    let load_average: [f64; 3] = load_values
        .try_into()
        .map_err(|_| MonitorError::InvalidSample)?;

    Ok(SystemSnapshot {
        target,
        collected_at: Utc::now(),
        hostname: host[0].clone(),
        os_name,
        kernel: host[2].clone(),
        architecture: host[3].clone(),
        cpu_cores: cpu_cores(required(&sections, "CPU1")?)?,
        uptime_seconds,
        load_average,
        cpu_usage_percent: cpu_usage(required(&sections, "CPU1")?, required(&sections, "CPU2")?)?,
        memory_total_bytes: memory(required(&sections, "MEMORY")?)?.0,
        memory_used_bytes: memory(required(&sections, "MEMORY")?)?.1,
        disks: parse_disks(required(&sections, "DISKS")?),
        processes: parse_processes(required(&sections, "PROCESSES")?),
    })
}

fn sections(output: &str) -> HashMap<&str, Vec<String>> {
    let mut result: HashMap<&str, Vec<String>> = HashMap::new();
    let mut current = "";
    for line in output.lines() {
        if let Some(name) = line.strip_prefix('\u{1e}') {
            current = name;
            result.entry(current).or_default();
        } else if !current.is_empty() {
            result.entry(current).or_default().push(line.to_owned());
        }
    }
    result
}

fn required<'a>(
    sections: &'a HashMap<&str, Vec<String>>,
    name: &str,
) -> Result<&'a [String], MonitorError> {
    sections
        .get(name)
        .map(Vec::as_slice)
        .ok_or(MonitorError::InvalidSample)
}

fn cpu_cores(lines: &[String]) -> Result<u32, MonitorError> {
    let count = lines
        .iter()
        .filter(|line| line.starts_with("cpu") && !line.starts_with("cpu "))
        .count();
    u32::try_from(count)
        .ok()
        .filter(|count| *count > 0)
        .ok_or(MonitorError::InvalidSample)
}

fn cpu_ticks(lines: &[String]) -> Result<(u64, u64), MonitorError> {
    let values = lines
        .iter()
        .find(|line| line.starts_with("cpu "))
        .ok_or(MonitorError::InvalidSample)?
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| MonitorError::InvalidSample)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() < 8 {
        return Err(MonitorError::InvalidSample);
    }
    Ok((values.iter().sum(), values[3] + values[4]))
}

fn cpu_usage(first: &[String], second: &[String]) -> Result<f64, MonitorError> {
    let (first_total, first_idle) = cpu_ticks(first)?;
    let (second_total, second_idle) = cpu_ticks(second)?;
    let total = second_total.saturating_sub(first_total);
    if total == 0 {
        return Err(MonitorError::InvalidSample);
    }
    let idle = second_idle.saturating_sub(first_idle);
    Ok(((total.saturating_sub(idle)) as f64 * 100.0 / total as f64).clamp(0.0, 100.0))
}

fn memory(lines: &[String]) -> Result<(u64, u64), MonitorError> {
    let values: HashMap<_, _> = lines
        .iter()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            let kib = value.split_whitespace().next()?.parse::<u64>().ok()?;
            Some((name, kib.saturating_mul(1024)))
        })
        .collect();
    let total = *values.get("MemTotal").ok_or(MonitorError::InvalidSample)?;
    let available = values
        .get("MemAvailable")
        .copied()
        .or_else(|| {
            Some(
                values
                    .get("MemFree")?
                    .saturating_add(*values.get("Buffers")?)
                    .saturating_add(*values.get("Cached")?),
            )
        })
        .ok_or(MonitorError::InvalidSample)?;
    Ok((total, total.saturating_sub(available)))
}

fn parse_disks(lines: &[String]) -> Vec<DiskUsage> {
    lines
        .iter()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 6 {
                return None;
            }
            let (Ok(total), Ok(used), Ok(available)) = (
                fields[1]
                    .parse::<u64>()
                    .map(|value| value.saturating_mul(1024)),
                fields[2]
                    .parse::<u64>()
                    .map(|value| value.saturating_mul(1024)),
                fields[3]
                    .parse::<u64>()
                    .map(|value| value.saturating_mul(1024)),
            ) else {
                return None;
            };
            if total == 0 {
                return None;
            }
            Some(DiskUsage {
                filesystem: fields[0].to_owned(),
                mount_point: fields[5..].join(" "),
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
            })
        })
        .collect()
}

fn parse_processes(lines: &[String]) -> Vec<ProcessInfo> {
    lines
        .iter()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 11 {
                return None;
            }
            Some(ProcessInfo {
                pid: fields[0].parse().ok()?,
                ppid: fields[1].parse().ok()?,
                user: fields[2].to_owned(),
                cpu_percent: fields[3].parse().ok()?,
                memory_percent: fields[4].parse().ok()?,
                start_id: fields[5..10].join(" "),
                command: fields[10..].join(" "),
            })
        })
        .take(1000)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sparrowshell_domain::{ServerId, SessionId};

    fn target() -> TargetRef {
        TargetRef {
            server_id: ServerId::new(),
            session_id: SessionId::new(),
            generation: 7,
        }
    }

    #[test]
    fn parses_realistic_linux_sample_without_faking_missing_values() {
        let sample = concat!(
            "\u{1e}HOST\nweb01\nLinux\n6.8.1\naarch64\n",
            "\u{1e}OS\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n",
            "\u{1e}CPU1\ncpu  100 0 100 800 0 0 0 0 0 0\ncpu0 100 0 100 800 0 0 0 0 0 0\n",
            "\u{1e}CPU2\ncpu  150 0 150 900 0 0 0 0 0 0\ncpu0 150 0 150 900 0 0 0 0 0 0\n",
            "\u{1e}MEMORY\nMemTotal: 2048 kB\nMemAvailable: 512 kB\n",
            "\u{1e}UPTIME\n120.5 200.0\n0.25 0.5 1.0 1/100 20\n",
            "\u{1e}DISKS\nFilesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vda1 10240 4096 6144 40% /\ntmpfs 100 0 100 0% /dev/shm\n",
            "\u{1e}PROCESSES\n PID PPID USER %CPU %MEM STARTED COMMAND\n 42 1 root 12.5 3.0 Thu Oct 29 08:00:00 2026 /usr/bin/demo --serve\n"
        );
        let snapshot = parse_sample(target(), sample).unwrap();
        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.hostname, "web01");
        assert_eq!(snapshot.cpu_cores, 1);
        assert_eq!(snapshot.cpu_usage_percent, 50.0);
        assert_eq!(snapshot.memory_total_bytes, 2048 * 1024);
        assert_eq!(snapshot.memory_used_bytes, 1536 * 1024);
        assert_eq!(snapshot.disks.len(), 2);
        assert_eq!(snapshot.disks[0].mount_point, "/");
        assert_eq!(snapshot.disks[1].mount_point, "/dev/shm");
        assert_eq!(snapshot.processes[0].pid, 42);
        assert_eq!(snapshot.processes[0].start_id, "Thu Oct 29 08:00:00 2026");
        assert_eq!(snapshot.processes[0].command, "/usr/bin/demo --serve");
    }

    #[test]
    fn rejects_missing_cpu_instead_of_returning_zero() {
        let error = parse_sample(target(), "\u{1e}HOST\nhost\n").unwrap_err();
        assert!(matches!(error, MonitorError::InvalidSample));
    }
}
