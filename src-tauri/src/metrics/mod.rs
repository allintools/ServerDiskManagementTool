use crate::errors::{AppError, AppResult};
use crate::ssh::{self, ExecOptions, Target};
use crate::state::AppState;
use crate::types::{DiskMetric, MetricSample};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

pub const METRICS_SCRIPT: &str = include_str!("metrics_v1.sh");

#[derive(Debug, Clone, Default)]
struct Cpu {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl Cpu {
    fn total(&self) -> u64 {
        self.user + self.nice + self.system + self.idle + self.iowait + self.irq + self.softirq + self.steal
    }
    fn idle_total(&self) -> u64 {
        self.idle + self.iowait
    }
}

#[derive(Debug, Clone, Default)]
struct Sample {
    cpu: Cpu,
    mem_total_kb: u64,
    mem_avail_kb: u64,
    load1: f64,
    disks: BTreeMap<String, (u64, u64)>,
}

fn parse_samples(stdout: &str) -> AppResult<(Sample, Sample, u64)> {
    let mut samples = Vec::new();
    let mut current = Sample::default();
    let mut started = false;
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        match fields[0] {
            "SAMPLE=1" => {
                started = true;
                current = Sample::default();
            }
            "SAMPLE=2" => {
                if started {
                    samples.push(current.clone());
                }
                current = Sample::default();
                started = true;
            }
            "CPU" if fields.len() >= 9 => {
                current.cpu = Cpu {
                    user: fields[1].parse().unwrap_or(0),
                    nice: fields[2].parse().unwrap_or(0),
                    system: fields[3].parse().unwrap_or(0),
                    idle: fields[4].parse().unwrap_or(0),
                    iowait: fields[5].parse().unwrap_or(0),
                    irq: fields[6].parse().unwrap_or(0),
                    softirq: fields[7].parse().unwrap_or(0),
                    steal: fields[8].parse().unwrap_or(0),
                };
            }
            "MEMTOTAL" if fields.len() >= 2 => { current.mem_total_kb = fields[1].parse().unwrap_or(0); }
            "MEMAVAIL" if fields.len() >= 2 => { current.mem_avail_kb = fields[1].parse().unwrap_or(0); }
            "LOAD" if fields.len() >= 2 => { current.load1 = fields[1].parse().unwrap_or(0.0); }
            "DISK" if fields.len() >= 4 => {
                let name = fields[1].to_string();
                let sectors_read = fields[2].parse().unwrap_or(0);
                let sectors_written = fields[3].parse().unwrap_or(0);
                current.disks.insert(name, (sectors_read, sectors_written));
            }
            _ => {}
        }
    }
    if started {
        samples.push(current);
    }
    if samples.len() < 2 {
        return Err(AppError::new("METRICS_FAILED", "未获得足够的实时指标样本"));
    }
    let first = samples[samples.len() - 2].clone();
    let second = samples[samples.len() - 1].clone();
    Ok((first, second, 1))
}

pub fn sample(state: &Arc<AppState>, server_id: &str) -> AppResult<MetricSample> {
    let server = state.db.get_server(server_id)?;
    let target = Target::from_server(&server, state.secret_for(server_id));
    let settings = state.settings();
    let options = ExecOptions {
        connect_timeout: Duration::from_secs(settings.connect_timeout_secs.max(2)),
        timeout: Duration::from_secs(12),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let output = ssh::exec_script(&target, &state.known_hosts_path, METRICS_SCRIPT, &[], &options)?;
    if !output.success() {
        return Err(ssh::classify_error(&output));
    }
    let (first, second, elapsed) = parse_samples(&output.stdout)?;
    let total_delta = second.cpu.total().saturating_sub(first.cpu.total()) as f64;
    let idle_delta = second.cpu.idle_total().saturating_sub(first.cpu.idle_total()) as f64;
    let cpu_percent = if total_delta > 0.0 {
        ((total_delta - idle_delta) / total_delta * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let mem_total = second.mem_total_kb.saturating_mul(1024);
    let mem_avail = second.mem_avail_kb.saturating_mul(1024);
    let mem_used = mem_total.saturating_sub(mem_avail);
    let mut disks = Vec::new();
    for (name, (read, write)) in &second.disks {
        let (prev_read, prev_write) = first.disks.get(name).copied().unwrap_or((*read, *write));
        let secs = elapsed.max(1) as f64;
        let read_delta = read.saturating_sub(prev_read) as f64;
        let write_delta = write.saturating_sub(prev_write) as f64;
        disks.push(DiskMetric {
            name: name.clone(),
            read_bytes_per_sec: read_delta * 512.0 / secs,
            write_bytes_per_sec: write_delta * 512.0 / secs,
            read_iops: read_delta / secs,
            write_iops: write_delta / secs,
        });
    }
    disks.sort_by(|a, b| b.read_bytes_per_sec.partial_cmp(&a.read_bytes_per_sec).unwrap_or(std::cmp::Ordering::Equal));
    Ok(MetricSample {
        collected_at: crate::util::now_ms(),
        cpu_percent,
        memory_used_bytes: mem_used,
        memory_total_bytes: mem_total,
        load1: second.load1,
        disks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_samples_and_computes_rates() {
        let raw = "SAMPLE=1\nCPU 100 0 50 1000 0 0 0 0\nMEMTOTAL 1000\nMEMAVAIL 500\nLOAD 0.5\nDISK sda 100 200\nSAMPLE=2\nCPU 150 0 60 1100 0 0 0 0\nMEMTOTAL 1000\nMEMAVAIL 400\nLOAD 0.7\nDISK sda 120 240\n";
        let (first, second, elapsed) = parse_samples(raw).unwrap();
        assert_eq!(elapsed, 1);
        assert_eq!(first.cpu.total(), 1150);
        assert_eq!(second.cpu.total(), 1310);
        assert_eq!(second.mem_total_kb, 1000);
        assert_eq!(second.disks["sda"], (120, 240));
    }
}
