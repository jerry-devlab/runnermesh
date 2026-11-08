//! Sampled per-runner resource evidence (LRM-07).
//!
//! Samples are estimates with explicit timestamps. Missing or unattributable
//! evidence stays `Unknown` and must never be presented as zero savings.

use serde::{Deserialize, Serialize};

/// Confidence of one resource sample. Non-localized machine vocabulary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceSampleConfidence {
    Estimate,
    Unknown,
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

/// Stable role labels for resource attribution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunnerResourceRole {
    Listener,
    Worker,
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
}
