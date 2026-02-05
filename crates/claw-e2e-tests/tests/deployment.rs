//! End-to-end tests for Deployment (claw-deploy, claw-rollback, claw-preemption).
//!
//! These tests verify:
//! 1. Deployment intent parsing and validation
//! 2. Strategy selection (Canary, Blue-Green, Rolling, Immediate)
//! 3. Deployment execution
//! 4. Rollback triggers and execution
//! 5. Deployment history
//! 6. Preemption and priority scheduling
//! 7. Resource-based victim selection

use std::time::Duration;
use claw_deploy::{
    ClusterContext, DeploymentConstraints, DeploymentExecutor, DeploymentId as DeployDeploymentId,
    DeploymentIntent, DeploymentMonitor, DeploymentState, DeploymentStatus, DeploymentStrategy,
    Environment, HealthAssessment, MetricPoint as DeployMetricPoint, StrategyHint,
    parse_intent, select_strategy,
};
use claw_rollback::{
    analyze_failure, AnalysisConfig, DefaultTriggers, DeploymentHistory, DeploymentId,
    DeploymentSnapshot, DeploymentSpec, FailureAnalyzer, LogEntry, LogLevel, Metrics,
    RollbackExecutor, RollbackPlan, RollbackStrategy, RollbackTrigger, RootCauseCategory,
    TriggerConfig, TriggerEvaluator, configure_triggers, ExecutionOptions, ComparisonOperator,
    CustomTriggerConfig,
};
use claw_preemption::{
    EvictionHandler, NoOpEvictionHandler, PreemptionCandidate, PreemptionConfig,
    PreemptionManager, PreemptionPolicy, PreemptionRequest, Preemptor, PriorityClass,
    ResourceRequirements, VictimSelectionStrategy, WorkloadId, WorkloadState,
};

// ============================================================================
// Deploy: Intent Creation
// ============================================================================

#[test]
fn test_deployment_intent_creation() {
    let intent = DeploymentIntent::new("myapp:v2.0")
        .with_replicas(5)
        .with_gpus(2)
        .with_memory_mb(8192);

    assert!(intent.validate().is_ok());
    assert_eq!(intent.image, "myapp:v2.0");
    assert_eq!(intent.replicas, Some(5));
    assert_eq!(intent.gpus, Some(2));
}

#[test]
fn test_deployment_intent_with_strategy_hint() {
    let intent = DeploymentIntent::new("api:v3.0")
        .with_replicas(10)
        .with_strategy_hint(StrategyHint::Canary { percentage: 10 });

    assert!(intent.validate().is_ok());
    assert!(matches!(intent.strategy_hint, Some(StrategyHint::Canary { percentage: 10 })));
}

#[test]
fn test_deployment_intent_with_constraints() {
    let constraints = DeploymentConstraints::new()
        .with_max_unavailable(2)
        .with_max_surge(3)
        .with_min_ready_seconds(30);

    let intent = DeploymentIntent::new("worker:v1.0")
        .with_replicas(20)
        .with_constraints(constraints);

    assert!(intent.validate().is_ok());
}

#[test]
fn test_deployment_intent_validation() {
    // Valid intent
    let valid = DeploymentIntent::new("app:v1.0");
    assert!(valid.validate().is_ok());

    // Invalid: empty image
    let invalid = DeploymentIntent::new("");
    assert!(invalid.validate().is_err());
}

// ============================================================================
// Deploy: Strategy Selection
// ============================================================================

#[test]
fn test_strategy_selection_canary() {
    let intent = DeploymentIntent::new("api:v2.0")
        .with_replicas(10)
        .with_strategy_hint(StrategyHint::Canary { percentage: 20 });

    let context = ClusterContext::new()
        .with_total_nodes(10)
        .with_available_gpus(0);

    let strategy = select_strategy(&intent, &context);

    assert!(matches!(strategy, DeploymentStrategy::Canary { .. }));
}

#[test]
fn test_strategy_selection_blue_green() {
    let intent = DeploymentIntent::new("web:v3.0")
        .with_replicas(5)
        .with_strategy_hint(StrategyHint::BlueGreen);

    let context = ClusterContext::new()
        .with_total_nodes(10)
        .with_available_gpus(0);

    let strategy = select_strategy(&intent, &context);

    assert!(matches!(strategy, DeploymentStrategy::BlueGreen { .. }));
}

#[test]
fn test_strategy_selection_rolling() {
    let intent = DeploymentIntent::new("service:v1.5")
        .with_replicas(100)
        .with_strategy_hint(StrategyHint::Rolling { max_surge: 10, max_unavailable: 5 });

    let context = ClusterContext::new()
        .with_total_nodes(50);

    let strategy = select_strategy(&intent, &context);

    assert!(matches!(strategy, DeploymentStrategy::Rolling { .. }));
}

#[test]
fn test_strategy_selection_immediate_for_small() {
    // Small deployments should use immediate strategy
    let intent = DeploymentIntent::new("utility:v1.0")
        .with_replicas(1);

    let context = ClusterContext::new()
        .with_total_nodes(5);

    let strategy = select_strategy(&intent, &context);

    assert!(matches!(strategy, DeploymentStrategy::Immediate));
}

#[test]
fn test_auto_strategy_selection() {
    // Without a hint, system should choose based on context
    let intent = DeploymentIntent::new("large-app:v2.0")
        .with_replicas(50);

    let context = ClusterContext::new()
        .with_total_nodes(100)
        .with_production(true);

    let strategy = select_strategy(&intent, &context);

    // For production with many replicas, should choose safe strategy
    assert!(matches!(strategy,
        DeploymentStrategy::Canary { .. } |
        DeploymentStrategy::Rolling { .. }
    ));
}

// ============================================================================
// Deploy: Execution
// ============================================================================

#[test]
fn test_deployment_executor_creation() {
    let executor = DeploymentExecutor::new();
    assert!(executor.active_deployments().is_empty());
}

#[test]
fn test_deployment_state_transitions() {
    // Test valid state transitions
    let state = DeploymentState::Pending;

    // Pending -> Progressing
    assert!(state.can_transition_to(DeploymentState::Progressing));

    // Progressing -> Available
    let progressing = DeploymentState::Progressing;
    assert!(progressing.can_transition_to(DeploymentState::Available));

    // Progressing -> Failed
    assert!(progressing.can_transition_to(DeploymentState::Failed));
}

// ============================================================================
// Deploy: Monitoring
// ============================================================================

#[test]
fn test_deployment_monitoring() {
    let monitor = DeploymentMonitor::new();

    // Record metrics
    monitor.record_metric(DeployMetricPoint::new("replicas_ready", 5.0));
    monitor.record_metric(DeployMetricPoint::new("replicas_available", 5.0));
    monitor.record_metric(DeployMetricPoint::new("replicas_desired", 10.0));

    // Assess health
    let assessment = monitor.assess_health();

    assert!(matches!(assessment, HealthAssessment::Progressing | HealthAssessment::Healthy));
}

// ============================================================================
// Rollback: History Management
// ============================================================================

#[test]
fn test_deployment_history_creation() {
    let history = DeploymentHistory::new(10).unwrap();
    assert!(history.is_empty());
}

#[test]
fn test_deployment_history_recording() {
    let mut history = DeploymentHistory::new(10).unwrap();

    // Record deployments
    let v1 = DeploymentSnapshot::new(
        DeploymentId::new("v1"),
        DeploymentSpec::new("my-app", "my-app:v1.0"),
    );
    history.record(v1);

    let v2 = DeploymentSnapshot::new(
        DeploymentId::new("v2"),
        DeploymentSpec::new("my-app", "my-app:v2.0"),
    );
    history.record(v2);

    assert_eq!(history.len(), 2);

    // Get current
    let current = history.current();
    assert!(current.is_some());
    assert_eq!(current.unwrap().id.as_str(), "v2");
}

#[test]
fn test_deployment_history_capacity() {
    let mut history = DeploymentHistory::new(5).unwrap();

    // Record more than capacity
    for i in 1..=10 {
        let snapshot = DeploymentSnapshot::new(
            DeploymentId::new(format!("v{}", i)),
            DeploymentSpec::new("app", format!("app:v{}", i)),
        );
        history.record(snapshot);
    }

    // Should only keep last 5
    assert_eq!(history.len(), 5);

    // Oldest should be v6 (v1-v5 evicted)
    assert!(history.find(&DeploymentId::new("v1")).is_none());
    assert!(history.find(&DeploymentId::new("v5")).is_none());
    assert!(history.find(&DeploymentId::new("v6")).is_some());
}

#[test]
fn test_deployment_history_list_recent() {
    let mut history = DeploymentHistory::new(10).unwrap();

    for i in 1..=7 {
        let snapshot = DeploymentSnapshot::new(
            DeploymentId::new(format!("v{}", i)),
            DeploymentSpec::new("app", format!("app:v{}", i)),
        );
        history.record(snapshot);
    }

    // Get last 3
    let recent = history.list_recent(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].id.as_str(), "v7"); // Most recent first
    assert_eq!(recent[1].id.as_str(), "v6");
    assert_eq!(recent[2].id.as_str(), "v5");
}

// ============================================================================
// Rollback: Triggers
// ============================================================================

#[test]
fn test_default_rollback_triggers() {
    let triggers = DefaultTriggers::all(100.0); // 100ms baseline latency

    // Should have error rate, latency, and health check triggers
    assert!(triggers.len() >= 3);
}

#[test]
fn test_rollback_trigger_evaluation() {
    let evaluator = TriggerEvaluator::new();
    let triggers = vec![
        RollbackTrigger::error_rate(5.0),      // Fire if error rate > 5%
        RollbackTrigger::latency(200.0),       // Fire if p99 latency > 200ms
        RollbackTrigger::health_check(3),      // Fire if > 3 health check failures
    ];

    // Healthy metrics
    let healthy = Metrics::new()
        .with_error_rate(2.0)
        .with_p99_latency_ms(100.0)
        .with_health_check_failures(1);

    assert!(evaluator.evaluate_all(&triggers, &healthy).is_none());

    // Unhealthy error rate
    let high_errors = Metrics::new()
        .with_error_rate(10.0);

    let triggered = evaluator.evaluate_all(&triggers, &high_errors);
    assert!(triggered.is_some());
    assert!(matches!(triggered, Some(RollbackTrigger::ErrorRate { .. })));
}

#[test]
fn test_custom_rollback_trigger() {
    let config = TriggerConfig::new()
        .with_error_rate_threshold(3.0)  // Stricter
        .with_baseline_latency_ms(50.0)
        .with_custom_trigger(CustomTriggerConfig::new(
            "queue_depth",
            ComparisonOperator::GreaterThan,
            1000.0,
            "Queue backlog exceeded",
        ));

    let triggers = configure_triggers(&config);
    assert!(triggers.len() >= 2); // At least error_rate and custom

    let evaluator = TriggerEvaluator::new();
    let metrics = Metrics::new()
        .with_custom("queue_depth", 1500.0);

    // Custom trigger should fire
    let custom_config = &config.custom_triggers[0];
    assert!(evaluator.evaluate_custom(custom_config, &metrics));
}

// ============================================================================
// Rollback: Execution
// ============================================================================

#[test]
fn test_rollback_planning() {
    let mut history = DeploymentHistory::new(10).unwrap();

    // Record versions
    for i in 1..=5 {
        let snapshot = DeploymentSnapshot::new(
            DeploymentId::new(format!("v{}", i)),
            DeploymentSpec::new("app", format!("app:v{}", i)),
        );
        history.record(snapshot);
    }

    let mut executor = RollbackExecutor::new(history);

    // Plan rollback from v5
    let plan = executor.plan_rollback(&DeploymentId::new("v5"), None);
    assert!(plan.is_ok());

    let plan = plan.unwrap();
    assert_eq!(plan.from.id.as_str(), "v5");
    assert_eq!(plan.to.id.as_str(), "v4"); // Previous version
}

#[test]
fn test_rollback_to_specific_version() {
    let mut history = DeploymentHistory::new(10).unwrap();

    for i in 1..=5 {
        let snapshot = DeploymentSnapshot::new(
            DeploymentId::new(format!("v{}", i)),
            DeploymentSpec::new("app", format!("app:v{}", i)),
        );
        history.record(snapshot);
    }

    let mut executor = RollbackExecutor::new(history);

    // Roll back to v2 specifically
    let plan = executor.plan_rollback(
        &DeploymentId::new("v5"),
        Some(&DeploymentId::new("v2")),
    );

    assert!(plan.is_ok());
    let plan = plan.unwrap();
    assert_eq!(plan.to.id.as_str(), "v2");
}

#[test]
fn test_rollback_execution() {
    let mut history = DeploymentHistory::new(10).unwrap();

    let v1 = DeploymentSnapshot::new(
        DeploymentId::new("v1"),
        DeploymentSpec::new("app", "app:v1.0")
            .with_replicas(3)
            .with_env("ENV", "production"),
    );
    history.record(v1);

    let v2 = DeploymentSnapshot::new(
        DeploymentId::new("v2"),
        DeploymentSpec::new("app", "app:v2.0")
            .with_replicas(3)
            .with_env("ENV", "production"),
    );
    history.record(v2);

    let mut executor = RollbackExecutor::new(history);

    // Plan and execute rollback
    let plan = executor.plan_rollback(&DeploymentId::new("v2"), None).unwrap();
    let result = executor.execute(&plan).unwrap();

    assert!(result.success);

    // Current should now be v1
    let current = executor.history().current();
    assert_eq!(current.unwrap().id.as_str(), "v1");
}

#[test]
fn test_rollback_dry_run() {
    let mut history = DeploymentHistory::new(10).unwrap();

    let v1 = DeploymentSnapshot::new(
        DeploymentId::new("v1"),
        DeploymentSpec::new("app", "app:v1.0"),
    );
    history.record(v1);

    let v2 = DeploymentSnapshot::new(
        DeploymentId::new("v2"),
        DeploymentSpec::new("app", "app:v2.0"),
    );
    history.record(v2);

    let options = ExecutionOptions::new().with_dry_run(true);
    let mut executor = RollbackExecutor::with_options(history, options);

    let plan = executor.plan_rollback(&DeploymentId::new("v2"), None).unwrap();
    let result = executor.execute(&plan).unwrap();

    assert!(result.success);
    assert!(result.details.contains("Dry run"));

    // History should be unchanged
    let current = executor.history().current();
    assert_eq!(current.unwrap().id.as_str(), "v2"); // Still v2
}

// ============================================================================
// Rollback: Root Cause Analysis
// ============================================================================

#[test]
fn test_root_cause_resource_exhaustion() {
    let snapshot = DeploymentSnapshot::new(
        DeploymentId::new("test"),
        DeploymentSpec::new("app", "app:v1"),
    );

    let mut metrics = Metrics::new();
    metrics.memory_utilization = 95.0;
    metrics.cpu_utilization = 98.0;

    let result = analyze_failure(&snapshot, &metrics, &[]);
    assert_eq!(result.category, RootCauseCategory::ResourceExhaustion);
}

#[test]
fn test_root_cause_dependency_failure() {
    let snapshot = DeploymentSnapshot::new(
        DeploymentId::new("test"),
        DeploymentSpec::new("app", "app:v1"),
    );

    let logs = vec![
        LogEntry::new(LogLevel::Error, "Connection refused to redis"),
        LogEntry::new(LogLevel::Error, "Database timeout after 30s"),
    ];

    let result = analyze_failure(&snapshot, &Metrics::new(), &logs);
    assert_eq!(result.category, RootCauseCategory::DependencyFailure);
}

#[test]
fn test_root_cause_config_error() {
    let snapshot = DeploymentSnapshot::new(
        DeploymentId::new("test"),
        DeploymentSpec::new("app", "app:v1")
            .with_env("API_KEY", "")       // Empty
            .with_env("SECRET", "${MISSING}"),  // Unresolved
    );

    let result = analyze_failure(&snapshot, &Metrics::new(), &[]);
    assert_eq!(result.category, RootCauseCategory::ConfigError);
}

// ============================================================================
// Preemption: Priority Classes
// ============================================================================

#[test]
fn test_builtin_priority_classes() {
    let critical = PriorityClass::system_critical();
    let high = PriorityClass::high_priority();
    let default = PriorityClass::default_priority();
    let spot = PriorityClass::spot();
    let preemptible = PriorityClass::preemptible();

    // Verify priority values
    assert_eq!(critical.value, 1000);
    assert_eq!(high.value, 750);
    assert_eq!(default.value, 500);
    assert_eq!(spot.value, 100);
    assert_eq!(preemptible.value, 0);
}

#[test]
fn test_priority_class_preemption_rules() {
    let high = PriorityClass::high_priority();
    let default = PriorityClass::default_priority();
    let spot = PriorityClass::spot();
    let critical = PriorityClass::system_critical();

    // Higher priority can preempt lower
    assert!(high.can_preempt(&default));
    assert!(high.can_preempt(&spot));
    assert!(default.can_preempt(&spot));

    // Cannot preempt same or higher
    assert!(!default.can_preempt(&high));
    assert!(!spot.can_preempt(&default));

    // System-critical cannot be preempted
    assert!(!high.can_preempt(&critical));

    // System-critical cannot preempt others (Never policy)
    assert!(!critical.can_preempt(&spot));
}

#[test]
fn test_custom_priority_class() {
    let handler = NoOpEvictionHandler::new();
    let preemptor = Preemptor::with_defaults(handler);

    // Register custom priority class
    let batch = PriorityClass::new("batch", 250, PreemptionPolicy::PreemptLowerPriority)
        .unwrap()
        .with_description("Batch processing jobs");

    assert!(preemptor.register_priority_class(batch).is_ok());

    // Verify it's registered
    let retrieved = preemptor.get_priority_class("batch");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().value, 250);
}

// ============================================================================
// Preemption: Manager Operations
// ============================================================================

#[test]
fn test_preemption_manager_creation() {
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    assert!(manager.workloads().is_empty());
}

#[test]
fn test_workload_registration() {
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    // Register workloads
    let workload1 = PreemptionCandidate::new(
        WorkloadId::new("training-job-1"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4).with_memory_gb(32));

    let workload2 = PreemptionCandidate::new(
        WorkloadId::new("inference-service"),
        PriorityClass::high_priority(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(2).with_memory_gb(16));

    manager.register_workload(workload1);
    manager.register_workload(workload2);

    assert_eq!(manager.workloads().len(), 2);

    // High priority workload should not be preemptible by default priorities
    let preemptible = manager.preemptible_workloads();
    assert!(preemptible.len() >= 1); // At least spot workload is preemptible
}

#[test]
fn test_preemption_request() {
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    // Register spot workloads
    let spot1 = PreemptionCandidate::new(
        WorkloadId::new("spot-1"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4));

    let spot2 = PreemptionCandidate::new(
        WorkloadId::new("spot-2"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4));

    manager.register_workload(spot1);
    manager.register_workload(spot2);

    // Request preemption for high-priority job
    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(4),
        PriorityClass::high_priority(),
    );

    let result = manager.request_preemption(&request);
    assert!(result.is_ok());

    let result = result.unwrap();
    assert!(result.is_successful());
    assert!(result.freed_resources.gpus >= 4);
}

#[test]
fn test_preemption_cost_limit() {
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    // Register workloads with varying costs
    let cheap = PreemptionCandidate::new(
        WorkloadId::new("cheap"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(2))
    .with_preemption_cost(10.0);

    let expensive = PreemptionCandidate::new(
        WorkloadId::new("expensive"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(2))
    .with_preemption_cost(1000.0);

    manager.register_workload(cheap);
    manager.register_workload(expensive);

    // Request with cost limit
    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(2),
        PriorityClass::high_priority(),
    )
    .with_max_cost(50.0);

    let result = manager.request_preemption(&request).unwrap();

    // Should only evict the cheap workload
    assert_eq!(result.evicted_count(), 1);
    assert!(result.total_cost <= 50.0);
}

// ============================================================================
// Preemption: Victim Selection Strategies
// ============================================================================

#[test]
fn test_victim_selection_lowest_priority() {
    let handler = NoOpEvictionHandler::new();
    let config = PreemptionConfig::new()
        .with_victim_selection(VictimSelectionStrategy::LowestPriority);
    let preemptor = Preemptor::new(config, handler);

    let candidates = vec![
        PreemptionCandidate::new(WorkloadId::new("default-job"), PriorityClass::default_priority())
            .with_resources(ResourceRequirements::new().with_gpus(4)),
        PreemptionCandidate::new(WorkloadId::new("spot-job"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(4)),
        PreemptionCandidate::new(WorkloadId::new("preemptible-job"), PriorityClass::preemptible())
            .with_resources(ResourceRequirements::new().with_gpus(4)),
    ];

    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(4),
        PriorityClass::high_priority(),
    );

    let victim_set = preemptor.find_victims(&request, &candidates);

    // Should select preemptible first (lowest priority)
    assert!(victim_set.satisfies_request);
    assert_eq!(victim_set.victims[0].workload_id.as_str(), "preemptible-job");
}

#[test]
fn test_victim_selection_most_resources() {
    let handler = NoOpEvictionHandler::new();
    let config = PreemptionConfig::new()
        .with_victim_selection(VictimSelectionStrategy::MostResources);
    let preemptor = Preemptor::new(config, handler);

    let candidates = vec![
        PreemptionCandidate::new(WorkloadId::new("small"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(1)),
        PreemptionCandidate::new(WorkloadId::new("large"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(8)),
        PreemptionCandidate::new(WorkloadId::new("medium"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(4)),
    ];

    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(8),
        PriorityClass::high_priority(),
    );

    let victim_set = preemptor.find_victims(&request, &candidates);

    // Should select large first (most resources)
    assert!(victim_set.satisfies_request);
    assert_eq!(victim_set.victims[0].workload_id.as_str(), "large");
}

#[test]
fn test_victim_selection_lowest_cost() {
    let handler = NoOpEvictionHandler::new();
    let config = PreemptionConfig::new()
        .with_victim_selection(VictimSelectionStrategy::LowestCost);
    let preemptor = Preemptor::new(config, handler);

    let candidates = vec![
        PreemptionCandidate::new(WorkloadId::new("expensive"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(4))
            .with_preemption_cost(100.0),
        PreemptionCandidate::new(WorkloadId::new("cheap"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(4))
            .with_preemption_cost(10.0),
        PreemptionCandidate::new(WorkloadId::new("moderate"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(4))
            .with_preemption_cost(50.0),
    ];

    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(4),
        PriorityClass::high_priority(),
    );

    let victim_set = preemptor.find_victims(&request, &candidates);

    // Should select cheap first
    assert!(victim_set.satisfies_request);
    assert_eq!(victim_set.victims[0].workload_id.as_str(), "cheap");
}

// ============================================================================
// Preemption: Workload State Management
// ============================================================================

#[test]
fn test_workload_state_transitions() {
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    let workload = PreemptionCandidate::new(
        WorkloadId::new("stateful"),
        PriorityClass::spot(),
    );

    manager.register_workload(workload);

    // Initial state
    let w = manager.get_workload(&WorkloadId::new("stateful")).unwrap();
    assert_eq!(w.state, WorkloadState::Running);
    assert!(w.can_be_preempted());

    // Transition to evicting
    manager.update_workload_state(&WorkloadId::new("stateful"), WorkloadState::Evicting);
    let w = manager.get_workload(&WorkloadId::new("stateful")).unwrap();
    assert!(!w.can_be_preempted()); // Cannot preempt while evicting

    // Transition to evicted
    manager.update_workload_state(&WorkloadId::new("stateful"), WorkloadState::Evicted);
    let w = manager.get_workload(&WorkloadId::new("stateful")).unwrap();
    assert!(w.state.is_terminal());
}

#[test]
fn test_graceful_eviction() {
    let handler = NoOpEvictionHandler::new();
    let config = PreemptionConfig::new()
        .with_default_grace_period(Duration::from_secs(30));
    let preemptor = Preemptor::new(config, handler);

    // Workload with custom grace period
    let workload = PreemptionCandidate::new(
        WorkloadId::new("graceful"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4))
    .with_grace_period(Duration::from_secs(60));

    let result = preemptor.evict(&[workload]);
    assert!(result.is_ok());
}

// ============================================================================
// Preemption: Node-specific
// ============================================================================

#[test]
fn test_node_specific_preemption() {
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    // Register workloads on different nodes
    let node1_workload = PreemptionCandidate::new(
        WorkloadId::new("node1-job"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4))
    .with_node("node-1");

    let node2_workload = PreemptionCandidate::new(
        WorkloadId::new("node2-job"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4))
    .with_node("node-2");

    manager.register_workload(node1_workload);
    manager.register_workload(node2_workload);

    // Request preemption on specific node
    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(4),
        PriorityClass::high_priority(),
    )
    .with_node("node-1");

    let candidates = manager.preemptible_workloads();
    let victim_set = manager.preemptor().find_victims(&request, &candidates);

    // Should only consider node-1 workloads
    assert!(victim_set.satisfies_request);
    assert!(victim_set.victims.iter()
        .all(|v| v.node_id == Some("node-1".into())));
}

// ============================================================================
// Integration: Full Deployment Workflow
// ============================================================================

#[test]
fn test_full_deployment_rollback_workflow() {
    // 1. Create deployment history
    let mut history = DeploymentHistory::new(10).unwrap();

    // 2. Record initial deployment (v1)
    let v1_spec = DeploymentSpec::new("my-service", "my-service:v1.0")
        .with_replicas(5)
        .with_env("ENV", "production");

    let v1 = DeploymentSnapshot::new(DeploymentId::new("v1"), v1_spec);
    history.record(v1);

    // 3. Record update deployment (v2) - this will have issues
    let v2_spec = DeploymentSpec::new("my-service", "my-service:v2.0")
        .with_replicas(5)
        .with_env("ENV", "production");

    let v2 = DeploymentSnapshot::new(DeploymentId::new("v2"), v2_spec);
    history.record(v2);

    // 4. Set up rollback triggers
    let triggers = DefaultTriggers::all(100.0);
    let evaluator = TriggerEvaluator::new();

    // 5. Simulate unhealthy metrics
    let bad_metrics = Metrics::new()
        .with_error_rate(15.0)  // 15% errors, way over 5% threshold
        .with_p99_latency_ms(250.0);

    // 6. Check if rollback is needed
    let triggered = evaluator.evaluate_all(&triggers, &bad_metrics);
    assert!(triggered.is_some());

    // 7. Execute rollback
    let mut executor = RollbackExecutor::new(history);
    let plan = executor.plan_rollback(&DeploymentId::new("v2"), None).unwrap();

    assert_eq!(plan.from.id.as_str(), "v2");
    assert_eq!(plan.to.id.as_str(), "v1");

    let result = executor.execute(&plan).unwrap();
    assert!(result.success);

    // 8. Verify rollback
    let current = executor.history().current().unwrap();
    assert_eq!(current.id.as_str(), "v1");
}

#[test]
fn test_full_preemption_workflow() {
    // 1. Create preemption manager
    let handler = NoOpEvictionHandler::new();
    let manager = PreemptionManager::with_defaults(handler);

    // 2. Register existing workloads
    let critical_workload = PreemptionCandidate::new(
        WorkloadId::new("production-api"),
        PriorityClass::system_critical(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(2));

    let high_workload = PreemptionCandidate::new(
        WorkloadId::new("inference-service"),
        PriorityClass::high_priority(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4));

    let spot_workload1 = PreemptionCandidate::new(
        WorkloadId::new("ml-training-1"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(8))
    .with_preemption_cost(50.0);

    let spot_workload2 = PreemptionCandidate::new(
        WorkloadId::new("ml-training-2"),
        PriorityClass::spot(),
    )
    .with_resources(ResourceRequirements::new().with_gpus(4))
    .with_preemption_cost(25.0);

    manager.register_workload(critical_workload);
    manager.register_workload(high_workload);
    manager.register_workload(spot_workload1);
    manager.register_workload(spot_workload2);

    assert_eq!(manager.workloads().len(), 4);

    // 3. New high-priority job arrives needing 8 GPUs
    let request = PreemptionRequest::new(
        ResourceRequirements::new().with_gpus(8),
        PriorityClass::high_priority(),
    );

    // 4. Request preemption
    let result = manager.request_preemption(&request).unwrap();

    // 5. Verify results
    assert!(result.is_successful());
    assert!(result.freed_resources.gpus >= 8);

    // Should have evicted spot workloads, not critical or high priority
    for workload_id in &result.evicted_workloads {
        let workload = manager.get_workload(workload_id);
        // Evicted workloads should be spot priority
        if let Some(w) = workload {
            assert!(w.priority.value <= 100);
        }
    }

    // 6. Verify eviction history
    let history = manager.eviction_history();
    assert!(!history.is_empty());
}
