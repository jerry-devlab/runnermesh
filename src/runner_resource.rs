//! Sampled per-runner resource evidence (LRM-07 / P08-01).
//!
//! Samples are estimates with explicit timestamps and freshness TTLs. Missing,
//! stale, or unattributable evidence stays `Unknown` and must never be presented
//! as zero savings.

use serde::{Deserialize, Serialize};

/// Confidence of one resource sample. Non-localized machine vocabulary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceSampleConfidence {
    Estimate,
    Unknown,
}

/// Status of attributing a process or process tree to an enrolled runner.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessAttributionDisposition {
    /// Exactly matched the enrolled runner listener or worker PID and path.
    DirectVerified,
    /// Child process spawned under the verified runner tree.
    ChildTree,
    /// Attribution is uncertain, ambiguous, exited PID, or cross-session.
    Uncertain,
    /// Query failed or access denied.
    Unsupported,
}

/// Stable role labels for resource attribution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunnerResourceRole {
    Listener,
    Worker,
}

/// Sampled CPU consumption for a runner process or tree.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerCpuSample {
    /// Total kernel mode execution time in milliseconds.
    pub kernel_time_ms: u64,
    /// Total user mode execution time in milliseconds.
    pub user_time_ms: u64,
    /// Optional instantaneous CPU usage estimate in basis points (100 = 1.00%).
    pub cpu_percent_x100: Option<u32>,
}

impl RunnerCpuSample {
    pub fn total_time_ms(&self) -> u64 {
        self.kernel_time_ms.saturating_add(self.user_time_ms)
    }
}

/// Timestamped memory sample for one runner role (Listener or Worker).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerMemorySample {
    pub role: RunnerResourceRole,
    pub working_set_bytes: Option<u64>,
    pub sampled_at_unix_ms: u64,
    pub confidence: ResourceSampleConfidence,
}

/// Full timestamped resource sample for one runner instance or role with TTL
/// and attribution provenance (P08-01).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerResourceSample {
    pub local_runner_key: String,
    pub role: RunnerResourceRole,
    pub pid: Option<u32>,
    pub working_set_bytes: Option<u64>,
    pub cpu: Option<RunnerCpuSample>,
    pub sampled_at_unix_ms: u64,
    pub ttl_ms: u64,
    pub attribution: ProcessAttributionDisposition,
    pub confidence: ResourceSampleConfidence,
}

impl RunnerResourceSample {
    /// Default time-to-live for live resource samples (5 seconds).
    pub const DEFAULT_TTL_MS: u64 = 5_000;

    /// Creates a verified estimate sample.
    pub fn new_verified(
        local_runner_key: impl Into<String>,
        role: RunnerResourceRole,
        pid: u32,
        working_set_bytes: u64,
        cpu: Option<RunnerCpuSample>,
        sampled_at_unix_ms: u64,
    ) -> Self {
        Self {
            local_runner_key: local_runner_key.into(),
            role,
            pid: Some(pid),
            working_set_bytes: Some(working_set_bytes),
            cpu,
            sampled_at_unix_ms,
            ttl_ms: Self::DEFAULT_TTL_MS,
            attribution: ProcessAttributionDisposition::DirectVerified,
            confidence: ResourceSampleConfidence::Estimate,
        }
    }

    /// Creates an explicitly uncertain sample (ambiguous, missing, or foreign).
    pub fn new_uncertain(
        local_runner_key: impl Into<String>,
        role: RunnerResourceRole,
        sampled_at_unix_ms: u64,
    ) -> Self {
        Self {
            local_runner_key: local_runner_key.into(),
            role,
            pid: None,
            working_set_bytes: None,
            cpu: None,
            sampled_at_unix_ms,
            ttl_ms: Self::DEFAULT_TTL_MS,
            attribution: ProcessAttributionDisposition::Uncertain,
            confidence: ResourceSampleConfidence::Unknown,
        }
    }

    /// Returns whether the sample is within its validity TTL.
    pub fn is_fresh(&self, current_time_ms: u64) -> bool {
        current_time_ms.saturating_sub(self.sampled_at_unix_ms) <= self.ttl_ms
    }

    /// Evaluates effective confidence accounting for freshness and attribution.
    pub fn effective_confidence(&self, current_time_ms: u64) -> ResourceSampleConfidence {
        if !self.is_fresh(current_time_ms) {
            return ResourceSampleConfidence::Unknown;
        }
        match self.attribution {
            ProcessAttributionDisposition::DirectVerified
            | ProcessAttributionDisposition::ChildTree => self.confidence,
            ProcessAttributionDisposition::Uncertain
            | ProcessAttributionDisposition::Unsupported => ResourceSampleConfidence::Unknown,
        }
    }

    /// Converts to the baseline memory sample view, downgrading to Unknown if
    /// stale or unattributable.
    pub fn to_memory_sample(&self, current_time_ms: u64) -> RunnerMemorySample {
        let confidence = self.effective_confidence(current_time_ms);
        let working_set_bytes = if confidence == ResourceSampleConfidence::Estimate {
            self.working_set_bytes
        } else {
            None
        };
        RunnerMemorySample {
            role: self.role,
            working_set_bytes,
            sampled_at_unix_ms: self.sampled_at_unix_ms,
            confidence,
        }
    }
}

/// Aggregate estimate across enrolled runners. Never claims exact savings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerResourceAggregate {
    pub sampled_at_unix_ms: u64,
    pub listener_working_set_bytes: Option<u64>,
    pub worker_working_set_bytes: Option<u64>,
    pub instance_count: usize,
    pub unknown_sample_count: usize,
    pub confidence: ResourceSampleConfidence,
}

/// Detailed aggregate encompassing both memory and CPU across runners.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAggregateSnapshot {
    pub sampled_at_unix_ms: u64,
    pub listener_working_set_bytes: Option<u64>,
    pub worker_working_set_bytes: Option<u64>,
    pub listener_cpu_time_ms: Option<u64>,
    pub worker_cpu_time_ms: Option<u64>,
    pub instance_count: usize,
    pub unknown_sample_count: usize,
    pub stale_sample_count: usize,
    pub confidence: ResourceSampleConfidence,
}

impl RunnerAggregateSnapshot {
    /// Projects to the backward-compatible memory-only aggregate view.
    pub fn to_memory_aggregate(&self) -> RunnerResourceAggregate {
        RunnerResourceAggregate {
            sampled_at_unix_ms: self.sampled_at_unix_ms,
            listener_working_set_bytes: self.listener_working_set_bytes,
            worker_working_set_bytes: self.worker_working_set_bytes,
            instance_count: self.instance_count,
            unknown_sample_count: self.unknown_sample_count,
            confidence: self.confidence,
        }
    }
}

/// Aggregates detailed resource samples honoring freshness and attribution boundaries.
pub fn aggregate_runner_resources(
    samples: &[RunnerResourceSample],
    now_unix_ms: u64,
) -> RunnerAggregateSnapshot {
    let mut listener_mem = 0_u64;
    let mut worker_mem = 0_u64;
    let mut listener_cpu = 0_u64;
    let mut worker_cpu = 0_u64;
    let mut listener_seen = false;
    let mut worker_seen = false;
    let mut unknown = 0_usize;
    let mut stale = 0_usize;
    let instances = samples.len();

    for sample in samples {
        if !sample.is_fresh(now_unix_ms) {
            stale += 1;
            unknown += 1;
            continue;
        }

        match (
            sample.effective_confidence(now_unix_ms),
            sample.working_set_bytes,
        ) {
            (ResourceSampleConfidence::Estimate, Some(bytes)) => match sample.role {
                RunnerResourceRole::Listener => {
                    listener_mem = listener_mem.saturating_add(bytes);
                    if let Some(cpu) = sample.cpu {
                        listener_cpu = listener_cpu.saturating_add(cpu.total_time_ms());
                    }
                    listener_seen = true;
                }
                RunnerResourceRole::Worker => {
                    worker_mem = worker_mem.saturating_add(bytes);
                    if let Some(cpu) = sample.cpu {
                        worker_cpu = worker_cpu.saturating_add(cpu.total_time_ms());
                    }
                    worker_seen = true;
                }
            },
            _ => unknown += 1,
        }
    }

    let confidence = if unknown > 0 || samples.is_empty() {
        ResourceSampleConfidence::Unknown
    } else {
        ResourceSampleConfidence::Estimate
    };

    RunnerAggregateSnapshot {
        sampled_at_unix_ms: now_unix_ms,
        listener_working_set_bytes: listener_seen.then_some(listener_mem),
        worker_working_set_bytes: worker_seen.then_some(worker_mem),
        listener_cpu_time_ms: listener_seen.then_some(listener_cpu),
        worker_cpu_time_ms: worker_seen.then_some(worker_cpu),
        instance_count: instances,
        unknown_sample_count: unknown,
        stale_sample_count: stale,
        confidence,
    }
}

/// Builds an aggregate from per-instance samples without inventing zeros.
pub fn aggregate_runner_memory(
    samples: &[RunnerMemorySample],
    sampled_at_unix_ms: u64,
) -> RunnerResourceAggregate {
    let mut listener_total = 0_u64;
    let mut worker_total = 0_u64;
    let mut listener_seen = false;
    let mut worker_seen = false;
    let mut unknown = 0_usize;
    let mut instances = 0_usize;

    for sample in samples {
        instances += 1;
        match (sample.confidence, sample.working_set_bytes) {
            (ResourceSampleConfidence::Estimate, Some(bytes)) => match sample.role {
                RunnerResourceRole::Listener => {
                    listener_total = listener_total.saturating_add(bytes);
                    listener_seen = true;
                }
                RunnerResourceRole::Worker => {
                    worker_total = worker_total.saturating_add(bytes);
                    worker_seen = true;
                }
            },
            _ => unknown += 1,
        }
    }

    let confidence = if unknown > 0 || samples.is_empty() {
        ResourceSampleConfidence::Unknown
    } else {
        ResourceSampleConfidence::Estimate
    };

    RunnerResourceAggregate {
        sampled_at_unix_ms,
        listener_working_set_bytes: listener_seen.then_some(listener_total),
        worker_working_set_bytes: worker_seen.then_some(worker_total),
        instance_count: instances,
        unknown_sample_count: unknown,
        confidence,
    }
}

/// Formats a human-facing estimate line. Locale affects presentation only.
pub fn format_resource_estimate(aggregate: &RunnerResourceAggregate, chinese: bool) -> String {
    let listener = aggregate
        .listener_working_set_bytes
        .map(format_bytes)
        .unwrap_or_else(|| {
            if chinese {
                "未知".to_owned()
            } else {
                "unknown".to_owned()
            }
        });
    let worker = aggregate
        .worker_working_set_bytes
        .map(format_bytes)
        .unwrap_or_else(|| {
            if chinese {
                "未知".to_owned()
            } else {
                "unknown".to_owned()
            }
        });
    if chinese {
        format!(
            "资源估计(非精确) · Listener {listener} · Worker {worker} · 采样 {} ms · {:?}",
            aggregate.sampled_at_unix_ms, aggregate.confidence
        )
    } else {
        format!(
            "Resource estimate (not exact) · Listener {listener} · Worker {worker} · sampled {} ms · {:?}",
            aggregate.sampled_at_unix_ms, aggregate.confidence
        )
    }
}

fn format_bytes(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_keeps_unknown_distinct_from_zero() {
        let samples = vec![
            RunnerMemorySample {
                role: RunnerResourceRole::Listener,
                working_set_bytes: Some(40 * 1024 * 1024),
                sampled_at_unix_ms: 100,
                confidence: ResourceSampleConfidence::Estimate,
            },
            RunnerMemorySample {
                role: RunnerResourceRole::Worker,
                working_set_bytes: None,
                sampled_at_unix_ms: 100,
                confidence: ResourceSampleConfidence::Unknown,
            },
        ];
        let aggregate = aggregate_runner_memory(&samples, 100);
        assert_eq!(aggregate.listener_working_set_bytes, Some(40 * 1024 * 1024));
        assert_eq!(aggregate.worker_working_set_bytes, None);
        assert_eq!(aggregate.unknown_sample_count, 1);
        assert_eq!(aggregate.confidence, ResourceSampleConfidence::Unknown);
        let english = format_resource_estimate(&aggregate, false);
        assert!(english.contains("not exact"));
        assert!(english.contains("unknown"));
        assert!(!english.contains("Worker 0"));
    }

    #[test]
    fn detailed_resources_preserve_freshness_and_attribution_boundaries() {
        let sample = RunnerResourceSample::new_verified(
            "runner-a",
            RunnerResourceRole::Listener,
            1234,
            64 * 1024 * 1024,
            Some(RunnerCpuSample {
                kernel_time_ms: 100,
                user_time_ms: 200,
                cpu_percent_x100: Some(250),
            }),
            10_000,
        );

        // Fresh sample at 11_000 (age 1_000ms <= TTL 5_000ms)
        assert!(sample.is_fresh(11_000));
        assert_eq!(
            sample.effective_confidence(11_000),
            ResourceSampleConfidence::Estimate
        );
        let mem = sample.to_memory_sample(11_000);
        assert_eq!(mem.working_set_bytes, Some(64 * 1024 * 1024));
        assert_eq!(mem.confidence, ResourceSampleConfidence::Estimate);

        // Stale sample at 16_000 (age 6_000ms > TTL 5_000ms)
        assert!(!sample.is_fresh(16_000));
        assert_eq!(
            sample.effective_confidence(16_000),
            ResourceSampleConfidence::Unknown
        );
        let stale_mem = sample.to_memory_sample(16_000);
        assert_eq!(stale_mem.working_set_bytes, None);
        assert_eq!(stale_mem.confidence, ResourceSampleConfidence::Unknown);
    }

    #[test]
    fn stale_and_uncertain_attribution_never_claim_zero_savings() {
        let now = 20_000;
        let mut sample_uncertain =
            RunnerResourceSample::new_uncertain("runner-b", RunnerResourceRole::Worker, now - 500);
        sample_uncertain.working_set_bytes = Some(100 * 1024 * 1024);
        sample_uncertain.attribution = ProcessAttributionDisposition::Uncertain;

        // Effective confidence remains Unknown and to_memory_sample strips bytes to None
        assert_eq!(
            sample_uncertain.effective_confidence(now),
            ResourceSampleConfidence::Unknown
        );
        let mem = sample_uncertain.to_memory_sample(now);
        assert_eq!(mem.working_set_bytes, None);
        assert_eq!(mem.confidence, ResourceSampleConfidence::Unknown);

        let aggregate = aggregate_runner_resources(&[sample_uncertain], now);
        assert_eq!(aggregate.confidence, ResourceSampleConfidence::Unknown);
        assert_eq!(aggregate.worker_working_set_bytes, None);
        assert_eq!(aggregate.worker_cpu_time_ms, None);
        assert_eq!(aggregate.unknown_sample_count, 1);
    }

    #[test]
    fn aggregate_detailed_resources_honors_cpu_and_memory() {
        let now = 50_000;
        let samples = vec![
            RunnerResourceSample::new_verified(
                "runner-a",
                RunnerResourceRole::Listener,
                100,
                50 * 1024 * 1024,
                Some(RunnerCpuSample {
                    kernel_time_ms: 50,
                    user_time_ms: 150,
                    cpu_percent_x100: Some(150),
                }),
                now - 1_000,
            ),
            RunnerResourceSample::new_verified(
                "runner-a",
                RunnerResourceRole::Worker,
                200,
                150 * 1024 * 1024,
                Some(RunnerCpuSample {
                    kernel_time_ms: 200,
                    user_time_ms: 800,
                    cpu_percent_x100: Some(500),
                }),
                now - 1_000,
            ),
        ];

        let agg = aggregate_runner_resources(&samples, now);
        assert_eq!(agg.confidence, ResourceSampleConfidence::Estimate);
        assert_eq!(agg.listener_working_set_bytes, Some(50 * 1024 * 1024));
        assert_eq!(agg.worker_working_set_bytes, Some(150 * 1024 * 1024));
        assert_eq!(agg.listener_cpu_time_ms, Some(200));
        assert_eq!(agg.worker_cpu_time_ms, Some(1000));
        assert_eq!(agg.unknown_sample_count, 0);
        assert_eq!(agg.stale_sample_count, 0);

        let mem_agg = agg.to_memory_aggregate();
        assert_eq!(mem_agg.listener_working_set_bytes, Some(50 * 1024 * 1024));
        assert_eq!(mem_agg.worker_working_set_bytes, Some(150 * 1024 * 1024));
        assert_eq!(mem_agg.confidence, ResourceSampleConfidence::Estimate);
    }
}
