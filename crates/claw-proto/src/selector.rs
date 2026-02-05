//! GPU selection and matching logic for scheduling.
//!
//! Evaluates GPU requirements against available node capabilities.

use crate::scheduling::{ConditionRequirement, GpuRequirement, SchedulingRequirements};
use crate::types::{GpuCapability, NodeCapabilities};

/// Result of matching GPU requirements against a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchResult {
    /// Node satisfies the requirement.
    Match {
        /// GPU indices that matched.
        matched_gpus: Vec<u32>,
        /// Priority of the requirement that matched (for fallback chains).
        priority: u32,
    },
    /// Node does not satisfy the requirement.
    NoMatch {
        /// Reason for the mismatch.
        reason: String,
    },
}

impl MatchResult {
    /// Check if the result is a match.
    #[must_use]
    pub const fn is_match(&self) -> bool {
        matches!(self, Self::Match { .. })
    }
}

/// GPU selector for matching requirements against node capabilities.
#[derive(Debug, Clone, Default)]
pub struct GpuSelector;

impl GpuSelector {
    /// Create a new GPU selector.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Match a GPU requirement against node capabilities.
    ///
    /// Tries the primary requirement first, then each fallback in order.
    #[must_use]
    pub fn match_requirement(
        &self,
        requirement: &GpuRequirement,
        capabilities: &NodeCapabilities,
    ) -> MatchResult {
        // Try each requirement in the fallback chain
        for req in requirement.fallback_chain() {
            if let Some(matched) = self.try_match_single(req, capabilities) {
                return MatchResult::Match {
                    matched_gpus: matched,
                    priority: req.priority,
                };
            }
        }

        // Build reason from primary requirement
        MatchResult::NoMatch {
            reason: self.build_mismatch_reason(requirement, capabilities),
        }
    }

    /// Try to match a single requirement (no fallback).
    fn try_match_single(
        &self,
        requirement: &GpuRequirement,
        capabilities: &NodeCapabilities,
    ) -> Option<Vec<u32>> {
        // Check if we have enough GPUs
        if capabilities.gpus.len() < requirement.count as usize {
            return None;
        }

        // Filter GPUs that match the criteria
        let matching_gpus: Vec<u32> = capabilities
            .gpus
            .iter()
            .filter(|gpu| self.gpu_matches_requirement(gpu, requirement))
            .map(|gpu| gpu.index)
            .collect();

        // Check if we have enough matching GPUs
        if matching_gpus.len() >= requirement.count as usize {
            Some(matching_gpus.into_iter().take(requirement.count as usize).collect())
        } else {
            None
        }
    }

    /// Check if a single GPU matches a requirement.
    fn gpu_matches_requirement(&self, gpu: &GpuCapability, requirement: &GpuRequirement) -> bool {
        // Check minimum memory
        if let Some(min_memory) = requirement.min_memory_mib {
            if gpu.memory_mib < min_memory {
                return false;
            }
        }

        // Check model pattern
        if let Some(pattern) = &requirement.model_pattern {
            if !gpu.name.contains(pattern) {
                return false;
            }
        }

        // CEL selector evaluation (simplified - full CEL would need a CEL library)
        if let Some(selector) = &requirement.selector {
            if !self.evaluate_cel_selector(selector, gpu) {
                return false;
            }
        }

        true
    }

    /// Evaluate a CEL-like selector expression.
    ///
    /// This is a simplified evaluator. For production, use a proper CEL library.
    fn evaluate_cel_selector(&self, selector: &str, gpu: &GpuCapability) -> bool {
        // Parse simple expressions like:
        // - "device.memory_mib >= 40960"
        // - "device.name.contains('A100')"
        // - "device.memory_mib >= 40960 && device.name.contains('A100')"

        // Split by && and evaluate each part
        let parts: Vec<&str> = selector.split("&&").map(str::trim).collect();

        for part in parts {
            if !self.evaluate_cel_part(part, gpu) {
                return false;
            }
        }

        true
    }

    /// Evaluate a single CEL expression part.
    fn evaluate_cel_part(&self, part: &str, gpu: &GpuCapability) -> bool {
        let part = part.trim();

        // Handle memory comparisons
        if part.contains("device.memory_mib") || part.contains("memory_mib") {
            return self.evaluate_memory_comparison(part, gpu.memory_mib);
        }

        // Handle name contains
        if part.contains("device.name.contains") || part.contains("name.contains") {
            return self.evaluate_name_contains(part, &gpu.name);
        }

        // Handle index comparisons
        if part.contains("device.index") || part.contains("index") {
            return self.evaluate_index_comparison(part, gpu.index);
        }

        // Unknown expression - assume true (lenient)
        true
    }

    /// Evaluate memory comparison expressions.
    fn evaluate_memory_comparison(&self, expr: &str, memory_mib: u64) -> bool {
        // Extract the comparison value
        if let Some(value) = self.extract_numeric_value(expr) {
            if expr.contains(">=") {
                return memory_mib >= value;
            } else if expr.contains("<=") {
                return memory_mib <= value;
            } else if expr.contains('>') {
                return memory_mib > value;
            } else if expr.contains('<') {
                return memory_mib < value;
            } else if expr.contains("==") || expr.contains('=') {
                return memory_mib == value;
            }
        }
        false
    }

    /// Evaluate name contains expressions.
    fn evaluate_name_contains(&self, expr: &str, name: &str) -> bool {
        // Extract string value between quotes
        if let Some(pattern) = self.extract_string_value(expr) {
            return name.contains(&pattern);
        }
        false
    }

    /// Evaluate index comparison expressions.
    fn evaluate_index_comparison(&self, expr: &str, index: u32) -> bool {
        if let Some(value) = self.extract_numeric_value(expr) {
            let index = u64::from(index);
            if expr.contains(">=") {
                return index >= value;
            } else if expr.contains("<=") {
                return index <= value;
            } else if expr.contains('>') {
                return index > value;
            } else if expr.contains('<') {
                return index < value;
            } else if expr.contains("==") || expr.contains('=') {
                return index == value;
            }
        }
        false
    }

    /// Extract a numeric value from an expression.
    fn extract_numeric_value(&self, expr: &str) -> Option<u64> {
        // Find digits at the end of the expression
        let digits: String = expr.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
        let digits: String = digits.chars().rev().collect();
        digits.parse().ok()
    }

    /// Extract a string value from quotes.
    fn extract_string_value(&self, expr: &str) -> Option<String> {
        // Find content between single or double quotes
        let start = expr.find(|c| c == '\'' || c == '"')?;
        let quote_char = expr.chars().nth(start)?;
        let end = expr[start + 1..].find(quote_char)?;
        Some(expr[start + 1..start + 1 + end].to_string())
    }

    /// Build a human-readable reason for mismatch.
    fn build_mismatch_reason(
        &self,
        requirement: &GpuRequirement,
        capabilities: &NodeCapabilities,
    ) -> String {
        let available = capabilities.gpus.len();
        let required = requirement.count;

        if available < required as usize {
            return format!(
                "need {} GPUs, node has {}",
                required, available
            );
        }

        if let Some(min_memory) = requirement.min_memory_mib {
            let sufficient_memory = capabilities
                .gpus
                .iter()
                .filter(|g| g.memory_mib >= min_memory)
                .count();
            if sufficient_memory < required as usize {
                return format!(
                    "need {} GPUs with >= {}MiB VRAM, node has {}",
                    required, min_memory, sufficient_memory
                );
            }
        }

        if let Some(pattern) = &requirement.model_pattern {
            let matching = capabilities
                .gpus
                .iter()
                .filter(|g| g.name.contains(pattern))
                .count();
            if matching < required as usize {
                return format!(
                    "need {} GPUs matching '{}', node has {}",
                    required, pattern, matching
                );
            }
        }

        "GPU requirements not satisfied".to_string()
    }
}

/// Check if a node satisfies all scheduling requirements.
#[must_use]
pub fn node_satisfies_requirements(
    capabilities: &NodeCapabilities,
    requirements: &SchedulingRequirements,
) -> bool {
    // Check GPU requirement
    if let Some(gpu_req) = &requirements.gpu_requirement {
        let selector = GpuSelector::new();
        if !selector.match_requirement(gpu_req, capabilities).is_match() {
            return false;
        }
    }

    // Check node selector labels
    if !capabilities.matches_selector(&requirements.node_selector) {
        return false;
    }

    // Check required conditions
    for cond_req in &requirements.required_conditions {
        if !is_condition_satisfied(capabilities, cond_req) {
            return false;
        }
    }

    true
}

/// Check if a condition requirement is satisfied.
fn is_condition_satisfied(
    capabilities: &NodeCapabilities,
    requirement: &ConditionRequirement,
) -> bool {
    capabilities
        .get_condition(&requirement.condition_type)
        .is_some_and(|c| requirement.is_satisfied_by(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduling::{ConditionStatus, NodeCondition};

    fn make_gpu(index: u32, name: &str, memory_mib: u64) -> GpuCapability {
        GpuCapability {
            index,
            name: name.into(),
            memory_mib,
            uuid: format!("GPU-{}", index),
        }
    }

    fn make_capabilities(gpus: Vec<GpuCapability>) -> NodeCapabilities {
        let mut caps = NodeCapabilities::new(8, 16384);
        for gpu in gpus {
            caps = caps.with_gpu(gpu);
        }
        caps
    }

    // ========================================================================
    // Basic Matching Tests
    // ========================================================================

    #[test]
    fn test_simple_count_match() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(2);
        let caps = make_capabilities(vec![
            make_gpu(0, "RTX 4090", 24576),
            make_gpu(1, "RTX 4090", 24576),
        ]);

        let result = selector.match_requirement(&req, &caps);
        assert!(result.is_match());
    }

    #[test]
    fn test_simple_count_no_match() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(4);
        let caps = make_capabilities(vec![
            make_gpu(0, "RTX 4090", 24576),
            make_gpu(1, "RTX 4090", 24576),
        ]);

        let result = selector.match_requirement(&req, &caps);
        assert!(!result.is_match());
    }

    // ========================================================================
    // Memory Requirement Tests
    // ========================================================================

    #[test]
    fn test_min_memory_match() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_min_memory_mib(20000);
        let caps = make_capabilities(vec![make_gpu(0, "RTX 4090", 24576)]);

        let result = selector.match_requirement(&req, &caps);
        assert!(result.is_match());
    }

    #[test]
    fn test_min_memory_no_match() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_min_memory_mib(40000);
        let caps = make_capabilities(vec![make_gpu(0, "RTX 4090", 24576)]);

        let result = selector.match_requirement(&req, &caps);
        assert!(!result.is_match());
    }

    // ========================================================================
    // Model Pattern Tests
    // ========================================================================

    #[test]
    fn test_model_pattern_match() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_model_pattern("A100");
        let caps = make_capabilities(vec![
            make_gpu(0, "RTX 4090", 24576),
            make_gpu(1, "NVIDIA A100-SXM4-80GB", 81920),
        ]);

        let result = selector.match_requirement(&req, &caps);
        assert!(result.is_match());
        if let MatchResult::Match { matched_gpus, .. } = result {
            assert_eq!(matched_gpus, vec![1]);
        }
    }

    #[test]
    fn test_model_pattern_no_match() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_model_pattern("H100");
        let caps = make_capabilities(vec![make_gpu(0, "NVIDIA A100-SXM4-80GB", 81920)]);

        let result = selector.match_requirement(&req, &caps);
        assert!(!result.is_match());
    }

    // ========================================================================
    // CEL Selector Tests
    // ========================================================================

    #[test]
    fn test_cel_memory_selector() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_selector("device.memory_mib >= 40960");
        let caps = make_capabilities(vec![
            make_gpu(0, "RTX 4090", 24576),
            make_gpu(1, "A100-80GB", 81920),
        ]);

        let result = selector.match_requirement(&req, &caps);
        assert!(result.is_match());
        if let MatchResult::Match { matched_gpus, .. } = result {
            assert_eq!(matched_gpus, vec![1]);
        }
    }

    #[test]
    fn test_cel_name_contains_selector() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_selector("device.name.contains('A100')");
        let caps = make_capabilities(vec![
            make_gpu(0, "RTX 4090", 24576),
            make_gpu(1, "NVIDIA A100-SXM4-80GB", 81920),
        ]);

        let result = selector.match_requirement(&req, &caps);
        assert!(result.is_match());
    }

    #[test]
    fn test_cel_combined_selector() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1)
            .with_selector("device.memory_mib >= 40960 && device.name.contains('A100')");
        
        // This node has an A100 but with enough memory
        let caps = make_capabilities(vec![make_gpu(0, "NVIDIA A100-SXM4-80GB", 81920)]);
        let result = selector.match_requirement(&req, &caps);
        assert!(result.is_match());

        // This node has an A100 but not enough memory
        let caps2 = make_capabilities(vec![make_gpu(0, "NVIDIA A100-PCIE-40GB", 40960)]);
        let result2 = selector.match_requirement(&req, &caps2);
        assert!(result2.is_match()); // 40960 >= 40960

        // This node has enough memory but wrong GPU
        let caps3 = make_capabilities(vec![make_gpu(0, "NVIDIA H100", 81920)]);
        let result3 = selector.match_requirement(&req, &caps3);
        assert!(!result3.is_match()); // H100 doesn't contain A100
    }

    // ========================================================================
    // Fallback Chain Tests
    // ========================================================================

    #[test]
    fn test_fallback_to_second_option() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1)
            .with_model_pattern("H100")
            .with_priority(10)
            .with_fallback(
                GpuRequirement::new(1)
                    .with_model_pattern("A100")
                    .with_priority(5),
            );

        // Node has A100, not H100 - should fallback
        let caps = make_capabilities(vec![make_gpu(0, "NVIDIA A100-80GB", 81920)]);
        let result = selector.match_requirement(&req, &caps);

        assert!(result.is_match());
        if let MatchResult::Match { priority, .. } = result {
            assert_eq!(priority, 5); // Used fallback
        }
    }

    #[test]
    fn test_primary_match_beats_fallback() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1)
            .with_model_pattern("H100")
            .with_priority(10)
            .with_fallback(
                GpuRequirement::new(1)
                    .with_model_pattern("A100")
                    .with_priority(5),
            );

        // Node has both H100 and A100 - should use primary
        let caps = make_capabilities(vec![
            make_gpu(0, "NVIDIA H100-SXM5-80GB", 81920),
            make_gpu(1, "NVIDIA A100-80GB", 81920),
        ]);
        let result = selector.match_requirement(&req, &caps);

        assert!(result.is_match());
        if let MatchResult::Match { priority, matched_gpus, .. } = result {
            assert_eq!(priority, 10); // Used primary
            assert_eq!(matched_gpus, vec![0]); // H100 first
        }
    }

    #[test]
    fn test_deep_fallback_chain() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1)
            .with_model_pattern("H200")
            .with_priority(100)
            .with_fallback(
                GpuRequirement::new(1)
                    .with_model_pattern("H100")
                    .with_priority(50)
                    .with_fallback(
                        GpuRequirement::new(1)
                            .with_model_pattern("A100")
                            .with_priority(25)
                            .with_fallback(GpuRequirement::new(2).with_priority(10)),
                    ),
            );

        // Node has 2 RTX 4090s - should fallback all the way to count-only
        let caps = make_capabilities(vec![
            make_gpu(0, "RTX 4090", 24576),
            make_gpu(1, "RTX 4090", 24576),
        ]);
        let result = selector.match_requirement(&req, &caps);

        assert!(result.is_match());
        if let MatchResult::Match { priority, matched_gpus, .. } = result {
            assert_eq!(priority, 10); // Used last fallback
            assert_eq!(matched_gpus.len(), 2);
        }
    }

    // ========================================================================
    // Full Requirements Tests
    // ========================================================================

    #[test]
    fn test_node_satisfies_simple_requirements() {
        let caps = make_capabilities(vec![make_gpu(0, "A100-80GB", 81920)]);
        let reqs = SchedulingRequirements::new()
            .with_gpu_requirement(GpuRequirement::new(1).with_min_memory_mib(40000));

        assert!(node_satisfies_requirements(&caps, &reqs));
    }

    #[test]
    fn test_node_satisfies_label_selector() {
        let caps = make_capabilities(vec![make_gpu(0, "A100-80GB", 81920)])
            .with_label("gpu-type", "nvidia")
            .with_label("tier", "high-memory");

        let reqs = SchedulingRequirements::new()
            .with_node_selector("gpu-type", "nvidia");

        assert!(node_satisfies_requirements(&caps, &reqs));
    }

    #[test]
    fn test_node_fails_label_selector() {
        let caps = make_capabilities(vec![make_gpu(0, "A100-80GB", 81920)])
            .with_label("gpu-type", "amd");

        let reqs = SchedulingRequirements::new()
            .with_node_selector("gpu-type", "nvidia");

        assert!(!node_satisfies_requirements(&caps, &reqs));
    }

    #[test]
    fn test_node_satisfies_condition_requirement() {
        let caps = make_capabilities(vec![make_gpu(0, "A100-80GB", 81920)])
            .with_condition(NodeCondition::new("cuda-12-ready", ConditionStatus::True));

        let reqs = SchedulingRequirements::new()
            .with_condition(ConditionRequirement::must_be_true("cuda-12-ready"));

        assert!(node_satisfies_requirements(&caps, &reqs));
    }

    #[test]
    fn test_node_fails_condition_requirement() {
        let caps = make_capabilities(vec![make_gpu(0, "A100-80GB", 81920)])
            .with_condition(NodeCondition::new("cuda-12-ready", ConditionStatus::False));

        let reqs = SchedulingRequirements::new()
            .with_condition(ConditionRequirement::must_be_true("cuda-12-ready"));

        assert!(!node_satisfies_requirements(&caps, &reqs));
    }

    #[test]
    fn test_node_missing_condition() {
        let caps = make_capabilities(vec![make_gpu(0, "A100-80GB", 81920)]);

        let reqs = SchedulingRequirements::new()
            .with_condition(ConditionRequirement::must_be_true("cuda-12-ready"));

        assert!(!node_satisfies_requirements(&caps, &reqs));
    }

    // ========================================================================
    // Mismatch Reason Tests
    // ========================================================================

    #[test]
    fn test_mismatch_reason_not_enough_gpus() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(4);
        let caps = make_capabilities(vec![make_gpu(0, "A100", 81920)]);

        let result = selector.match_requirement(&req, &caps);
        if let MatchResult::NoMatch { reason } = result {
            assert!(reason.contains("need 4 GPUs"));
            assert!(reason.contains("node has 1"));
        } else {
            panic!("Expected NoMatch");
        }
    }

    #[test]
    fn test_mismatch_reason_memory() {
        let selector = GpuSelector::new();
        let req = GpuRequirement::new(1).with_min_memory_mib(100000);
        let caps = make_capabilities(vec![make_gpu(0, "A100", 81920)]);

        let result = selector.match_requirement(&req, &caps);
        if let MatchResult::NoMatch { reason } = result {
            assert!(reason.contains("100000MiB"));
        } else {
            panic!("Expected NoMatch");
        }
    }
}
