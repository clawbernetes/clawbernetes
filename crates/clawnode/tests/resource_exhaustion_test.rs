//! Resource Exhaustion Prevention Tests
//!
//! These tests verify that the node properly protects against resource exhaustion attacks:
//! - Workloads exceeding limits are rejected
//! - Multiple workloads share resources fairly
//! - Node doesn't accept work beyond capacity
//! - Runaway workloads are detected and can be killed

use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use clawnode::resources::{
    EffectiveResourceLimits, ExecutionWatchdog, NodeCapacity, ResourceLimits, ResourceTracker,
    ResourceUsage,
};
use clawnode::NodeError;

// ============================================================================
// Test: Workloads Exceeding Limits Are Rejected
// ============================================================================

#[test]
fn test_workload_exceeding_memory_limit_rejected() {
    // Node with 16 GiB total memory, 10% reserved = 14.4 GiB allocatable
    let capacity = NodeCapacity::new(
        16 * 1024 * 1024 * 1024, // 16 GiB
        8,
        100 * 1024 * 1024 * 1024,
    )
    .with_system_reserved_percent(10)
    .with_max_concurrent_workloads(100);

    let tracker = ResourceTracker::new(capacity);

    // Try to allocate 20 GiB - should fail
    let limits = EffectiveResourceLimits {
        memory_bytes: 20 * 1024 * 1024 * 1024, // 20 GiB > 14.4 GiB allocatable
        cpu_cores: 1.0,
        disk_bytes: 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    let result = tracker.can_accept_workload(&limits);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NodeError::InsufficientMemory { .. }));
}

#[test]
fn test_workload_exceeding_cpu_limit_rejected() {
    // Node with 8 cores, 10% reserved = 7.2 allocatable
    let capacity = NodeCapacity::new(1024 * 1024 * 1024, 8, 100 * 1024 * 1024 * 1024)
        .with_system_reserved_percent(10)
        .with_max_concurrent_workloads(100);

    let tracker = ResourceTracker::new(capacity);

    // Try to allocate 10 cores - should fail
    let limits = EffectiveResourceLimits {
        memory_bytes: 1024,
        cpu_cores: 10.0, // 10 > 7.2 allocatable
        disk_bytes: 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    let result = tracker.can_accept_workload(&limits);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NodeError::InsufficientCpu { .. }));
}

#[test]
fn test_workload_exceeding_disk_limit_rejected() {
    // Node with 100 GiB disk, 10% reserved = 90 GiB allocatable
    let capacity = NodeCapacity::new(1024 * 1024 * 1024, 8, 100 * 1024 * 1024 * 1024)
        .with_system_reserved_percent(10)
        .with_max_concurrent_workloads(100);

    let tracker = ResourceTracker::new(capacity);

    // Try to allocate 95 GiB - should fail
    let limits = EffectiveResourceLimits {
        memory_bytes: 1024,
        cpu_cores: 1.0,
        disk_bytes: 95 * 1024 * 1024 * 1024, // 95 > 90 allocatable
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    let result = tracker.can_accept_workload(&limits);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NodeError::InsufficientDisk { .. }));
}

#[test]
fn test_workload_with_invalid_resource_limits_rejected() {
    // Zero memory should be rejected
    let limits = ResourceLimits::new().with_memory_bytes(0);
    assert!(limits.validate().is_err());

    // Negative CPU should be rejected
    let limits = ResourceLimits::new().with_cpu_cores(-1.0);
    assert!(limits.validate().is_err());

    // Excessive CPU should be rejected
    let limits = ResourceLimits::new().with_cpu_cores(2000.0);
    assert!(limits.validate().is_err());

    // Invalid OOM score should be rejected
    let limits = ResourceLimits::new().with_oom_score_adj(2000);
    assert!(limits.validate().is_err());
}

// ============================================================================
// Test: Multiple Workloads Share Resources Fairly
// ============================================================================

#[test]
fn test_multiple_workloads_share_resources() {
    // Node with 32 GiB memory, 16 cores
    let capacity = NodeCapacity::new(
        32 * 1024 * 1024 * 1024, // 32 GiB
        16,
        500 * 1024 * 1024 * 1024,
    )
    .with_system_reserved_percent(10) // ~28.8 GiB allocatable
    .with_max_concurrent_workloads(100);

    let tracker = ResourceTracker::new(capacity);

    // Workload 1: 8 GiB, 4 cores
    let limits1 = EffectiveResourceLimits {
        memory_bytes: 8 * 1024 * 1024 * 1024,
        cpu_cores: 4.0,
        disk_bytes: 10 * 1024 * 1024 * 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    // Workload 2: 8 GiB, 4 cores
    let limits2 = limits1.clone();

    // Workload 3: 8 GiB, 4 cores
    let limits3 = limits1.clone();

    // All three should fit
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    assert!(tracker.reserve(id1, &limits1).is_ok());
    assert!(tracker.reserve(id2, &limits2).is_ok());
    assert!(tracker.reserve(id3, &limits3).is_ok());

    assert_eq!(tracker.workload_count(), 3);
    assert_eq!(tracker.allocated_memory(), 24 * 1024 * 1024 * 1024);
    assert!((tracker.allocated_cpu_cores() - 12.0).abs() < 0.01);

    // Fourth workload of same size should fail (would exceed memory)
    let limits4 = limits1.clone();
    let result = tracker.can_accept_workload(&limits4);
    assert!(result.is_err());
}

#[test]
fn test_resource_release_allows_new_workloads() {
    // Node with limited resources
    let capacity = NodeCapacity::new(
        10 * 1024 * 1024 * 1024, // 10 GiB
        4,
        50 * 1024 * 1024 * 1024,
    )
    .with_system_reserved_percent(10)
    .with_max_concurrent_workloads(10);

    let tracker = ResourceTracker::new(capacity);

    let limits = EffectiveResourceLimits {
        memory_bytes: 4 * 1024 * 1024 * 1024, // ~4 GiB each
        cpu_cores: 1.5,
        disk_bytes: 5 * 1024 * 1024 * 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    // First two workloads fit
    assert!(tracker.reserve(id1, &limits).is_ok());
    assert!(tracker.reserve(id2, &limits).is_ok());

    // Third would exceed capacity
    let id3 = Uuid::new_v4();
    assert!(tracker.can_accept_workload(&limits).is_err());

    // Release first workload
    tracker.release(id1, &limits);

    // Now third can be accepted
    assert!(tracker.reserve(id3, &limits).is_ok());
    assert_eq!(tracker.workload_count(), 2);
}

// ============================================================================
// Test: Node Doesn't Accept Work Beyond Capacity
// ============================================================================

#[test]
fn test_max_concurrent_workloads_enforced() {
    // Node with high resources but low concurrent limit
    let capacity = NodeCapacity::new(
        1024 * 1024 * 1024 * 1024, // 1 TiB
        128,
        1024 * 1024 * 1024 * 1024,
    )
    .with_system_reserved_percent(5)
    .with_max_concurrent_workloads(3); // Only 3 concurrent

    let tracker = ResourceTracker::new(capacity);

    // Tiny workloads
    let limits = EffectiveResourceLimits {
        memory_bytes: 1024,
        cpu_cores: 0.1,
        disk_bytes: 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 10,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    // Reserve 3 workloads (max)
    assert!(tracker.reserve(Uuid::new_v4(), &limits).is_ok());
    assert!(tracker.reserve(Uuid::new_v4(), &limits).is_ok());
    assert!(tracker.reserve(Uuid::new_v4(), &limits).is_ok());

    // Fourth should fail even though resources are available
    let result = tracker.can_accept_workload(&limits);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NodeError::MaxWorkloadsExceeded { current: 3, max: 3 }));
}

#[test]
fn test_system_reserved_resources_protected() {
    // Node with 10 GiB memory, 25% reserved = 7.5 GiB allocatable
    let capacity = NodeCapacity::new(
        10 * 1024 * 1024 * 1024, // 10 GiB
        8,
        100 * 1024 * 1024 * 1024,
    )
    .with_system_reserved_percent(25)
    .with_max_concurrent_workloads(100);

    // Verify allocatable is correctly calculated
    assert_eq!(
        capacity.allocatable_memory_bytes(),
        (10 * 1024 * 1024 * 1024) * 75 / 100 // 7.5 GiB
    );

    let tracker = ResourceTracker::new(capacity);

    // Try to allocate 8 GiB - should fail (exceeds 7.5 GiB allocatable)
    let limits = EffectiveResourceLimits {
        memory_bytes: 8 * 1024 * 1024 * 1024,
        cpu_cores: 1.0,
        disk_bytes: 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    let result = tracker.can_accept_workload(&limits);
    assert!(result.is_err());
}

#[test]
fn test_cumulative_allocation_tracking() {
    // Node with 20 GiB memory
    let capacity = NodeCapacity::new(
        20 * 1024 * 1024 * 1024,
        16,
        200 * 1024 * 1024 * 1024,
    )
    .with_system_reserved_percent(10) // 18 GiB allocatable
    .with_max_concurrent_workloads(100);

    let tracker = ResourceTracker::new(capacity);

    // Allocate 5 workloads of 3 GiB each = 15 GiB
    let limits = EffectiveResourceLimits {
        memory_bytes: 3 * 1024 * 1024 * 1024,
        cpu_cores: 2.0,
        disk_bytes: 10 * 1024 * 1024 * 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    for _ in 0..5 {
        assert!(tracker.reserve(Uuid::new_v4(), &limits).is_ok());
    }

    assert_eq!(tracker.allocated_memory(), 15 * 1024 * 1024 * 1024);

    // Next 3 GiB workload should succeed (15 + 3 = 18, exactly at limit)
    assert!(tracker.reserve(Uuid::new_v4(), &limits).is_ok());

    // Next should fail (would exceed 18 GiB)
    let result = tracker.can_accept_workload(&limits);
    assert!(result.is_err());
}

// ============================================================================
// Test: Runaway Workload Detection
// ============================================================================

#[test]
fn test_execution_timeout_detection() {
    let watchdog = ExecutionWatchdog::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    // Register workloads with different timeouts
    watchdog.register(id1, Some(Duration::from_millis(5))); // 5ms timeout
    watchdog.register(id2, Some(Duration::from_secs(3600))); // 1 hour timeout
    watchdog.register(id3, None); // No timeout

    // Wait for short timeout to expire
    std::thread::sleep(Duration::from_millis(20));

    let timed_out = watchdog.check_timeouts();
    assert_eq!(timed_out.len(), 1);
    assert_eq!(timed_out[0], id1);

    // id2 and id3 should not be timed out
    assert!(!timed_out.contains(&id2));
    assert!(!timed_out.contains(&id3));
}

#[test]
fn test_resource_violation_detection() {
    let capacity = NodeCapacity::default();
    let tracker = ResourceTracker::new(capacity);

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let limits = EffectiveResourceLimits {
        memory_bytes: 1000,
        cpu_cores: 2.0,
        disk_bytes: 5000,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    tracker.reserve(id1, &limits).unwrap();
    tracker.reserve(id2, &limits).unwrap();

    // Update usage: id1 exceeds memory, id2 is fine
    let mut usage1 = ResourceUsage::new();
    usage1.memory_bytes = 2000; // Exceeds 1000 limit
    usage1.cpu_usage = 1.0;
    tracker.update_usage(id1, usage1);

    let mut usage2 = ResourceUsage::new();
    usage2.memory_bytes = 500; // Under limit
    usage2.cpu_usage = 1.0;
    tracker.update_usage(id2, usage2);

    // Check for violations
    let mut limits_map = HashMap::new();
    limits_map.insert(id1, limits.clone());
    limits_map.insert(id2, limits);

    let violations = tracker.check_violations(&limits_map);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].0, id1);
    assert!(violations[0].1.contains("memory"));
}

#[test]
fn test_resource_alert_generation() {
    // Node with small capacity
    let capacity = NodeCapacity::new(1000, 10, 10000)
        .with_max_concurrent_workloads(10);

    let tracker = ResourceTracker::new(capacity).with_alert_threshold(80);

    // Reserve workload using 85% of memory (850 of 900 allocatable)
    let limits = EffectiveResourceLimits {
        memory_bytes: 850,
        cpu_cores: 1.0,
        disk_bytes: 100,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    tracker.reserve(Uuid::new_v4(), &limits).unwrap();

    // Should generate memory alert
    let alerts = tracker.check_alerts();
    assert!(!alerts.is_empty());

    let memory_alert = alerts.iter().find(|a| {
        matches!(a.resource, clawnode::ResourceType::Memory)
    });
    assert!(memory_alert.is_some());
    assert!(memory_alert.unwrap().current_percent > 80.0);
}

// ============================================================================
// Test: Resource Limits Validation
// ============================================================================

#[test]
fn test_resource_limits_resolve_applies_defaults() {
    let requested = ResourceLimits::default(); // All None
    let capacity = NodeCapacity::default();

    let result = clawnode::resources::resolve_limits(&requested, &capacity, 0);
    assert!(result.is_ok());

    let effective = result.unwrap();
    // Should have sensible defaults
    assert!(effective.memory_bytes > 0);
    assert!(effective.cpu_cores > 0.0);
    assert!(effective.disk_bytes > 0);
}

#[test]
fn test_resource_limits_resolve_respects_capacity() {
    // Request more than capacity
    let requested = ResourceLimits::new()
        .with_memory_bytes(100 * 1024 * 1024 * 1024); // 100 GiB

    let capacity = NodeCapacity::new(
        16 * 1024 * 1024 * 1024, // 16 GiB total
        8,
        100 * 1024 * 1024 * 1024,
    );

    let result = clawnode::resources::resolve_limits(&requested, &capacity, 0);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NodeError::ResourceExceedsCapacity { .. }));
}

#[test]
fn test_node_capacity_validation() {
    // Valid capacity
    let capacity = NodeCapacity::new(1024, 4, 10240)
        .with_gpus(vec![24576, 24576])
        .with_max_concurrent_workloads(10);
    assert!(capacity.validate().is_ok());

    // Invalid: zero memory
    let mut invalid = NodeCapacity::default();
    invalid.total_memory_bytes = 0;
    assert!(invalid.validate().is_err());

    // Invalid: zero CPU
    let mut invalid = NodeCapacity::default();
    invalid.total_cpu_cores = 0;
    assert!(invalid.validate().is_err());

    // Invalid: zero max workloads
    let mut invalid = NodeCapacity::default();
    invalid.max_concurrent_workloads = 0;
    assert!(invalid.validate().is_err());

    // Invalid: GPU count mismatch
    let mut invalid = NodeCapacity::default();
    invalid.total_gpus = 4;
    invalid.gpu_memory_mib = vec![24576, 24576]; // Only 2 entries
    assert!(invalid.validate().is_err());
}

// ============================================================================
// Test: Edge Cases
// ============================================================================

#[test]
fn test_zero_gpu_workload_accepted() {
    let capacity = NodeCapacity::new(16 * 1024 * 1024 * 1024, 8, 100 * 1024 * 1024 * 1024)
        .with_gpus(vec![24576, 24576]) // 2 GPUs available
        .with_max_concurrent_workloads(100);

    let tracker = ResourceTracker::new(capacity);

    // Workload requesting 0 GPUs should be fine
    let limits = EffectiveResourceLimits {
        memory_bytes: 1024 * 1024 * 1024,
        cpu_cores: 2.0,
        disk_bytes: 10 * 1024 * 1024 * 1024,
        gpu_memory_mib: 0, // No GPU
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    assert!(tracker.can_accept_workload(&limits).is_ok());
}

#[test]
fn test_empty_release_is_safe() {
    let capacity = NodeCapacity::default();
    let tracker = ResourceTracker::new(capacity);

    // Releasing resources for a workload that was never reserved should be safe
    let fake_id = Uuid::new_v4();
    let limits = EffectiveResourceLimits {
        memory_bytes: 1024,
        cpu_cores: 1.0,
        disk_bytes: 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 100,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    // Should not panic
    tracker.release(fake_id, &limits);
    assert_eq!(tracker.workload_count(), 0);
}

#[test]
fn test_watchdog_unregister_nonexistent_is_safe() {
    let watchdog = ExecutionWatchdog::new();
    let fake_id = Uuid::new_v4();

    // Should not panic
    watchdog.unregister(fake_id);
}

#[test]
fn test_usage_tracking_for_nonexistent_workload() {
    let capacity = NodeCapacity::default();
    let tracker = ResourceTracker::new(capacity);

    let fake_id = Uuid::new_v4();
    assert!(tracker.get_usage(fake_id).is_none());
}

#[test]
fn test_resource_usage_percent_calculations() {
    let mut usage = ResourceUsage::new();
    usage.memory_bytes = 500;

    // 50% of 1000
    assert!((usage.memory_percent(1000) - 50.0).abs() < 0.01);

    // Division by zero protection
    assert_eq!(usage.memory_percent(0), 0.0);
}

#[test]
fn test_resource_limits_with_execution_time() {
    let limits = ResourceLimits::new()
        .with_memory_bytes(1024)
        .with_max_execution_time(Duration::from_secs(3600));

    assert_eq!(limits.max_execution_time, Some(Duration::from_secs(3600)));
    assert!(limits.validate().is_ok());
}

// ============================================================================
// Test: Concurrent Access Safety (basic)
// ============================================================================

#[test]
fn test_tracker_concurrent_reserves() {
    use std::sync::Arc;
    use std::thread;

    let capacity = NodeCapacity::new(
        1024 * 1024 * 1024 * 1024, // Very large
        1024,
        1024 * 1024 * 1024 * 1024,
    )
    .with_max_concurrent_workloads(1000);

    let tracker = Arc::new(ResourceTracker::new(capacity));

    let limits = EffectiveResourceLimits {
        memory_bytes: 1024,
        cpu_cores: 0.1,
        disk_bytes: 1024,
        gpu_memory_mib: 0,
        network_bandwidth_mbps: 10,
        max_execution_time: None,
        oom_score_adj: 0,
    };

    let mut handles = vec![];

    // Spawn 10 threads, each reserving 10 workloads
    for _ in 0..10 {
        let tracker = Arc::clone(&tracker);
        let limits = limits.clone();

        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                let _ = tracker.reserve(Uuid::new_v4(), &limits);
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Should have reserved 100 workloads
    assert_eq!(tracker.workload_count(), 100);
}
