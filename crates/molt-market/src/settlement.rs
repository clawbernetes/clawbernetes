//! Settlement logic for completed jobs.
//!
//! Calculates final payments based on job duration, rates,
//! and any applicable adjustments.
//!
//! # Precision Guarantees
//!
//! All payment calculations use **fixed-point arithmetic** with the following guarantees:
//!
//! - **No floating-point**: All calculations use integer arithmetic only
//! - **No precision loss**: Intermediate calculations use `u128` to prevent overflow
//! - **Provider-friendly rounding**: When fractional tokens would result, we round UP (ceiling)
//!   to ensure providers are never underpaid due to rounding
//! - **Overflow protection**: Results that would overflow `u64` return `u64::MAX` with error handling
//!
//! ## Rounding Rules
//!
//! The payment formula is: `payment = ceiling(duration_seconds × rate_per_hour / 3600)`
//!
//! Examples:
//! - 1 second at 3600 tokens/hour = 1 token (exact)
//! - 1 second at 3599 tokens/hour = 1 token (rounded up from 0.9997)
//! - 1 second at 1 token/hour = 1 token (rounded up from 0.000278)
//!
//! This ensures providers always receive at least 1 token for any non-zero work.

use serde::{Deserialize, Serialize};

use crate::error::MarketError;

/// Input data needed to settle a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSettlementInput {
    /// The job being settled.
    pub job_id: String,
    /// Unix timestamp when job started.
    pub start_time: i64,
    /// Unix timestamp when job ended.
    pub end_time: i64,
    /// Rate per hour in tokens.
    pub rate_per_hour: u64,
    /// Amount held in escrow.
    pub escrow_amount: u64,
}

/// The result of settling a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    /// The settled job ID.
    pub job_id: String,
    /// Final amount paid to provider.
    pub amount_paid: u64,
    /// Actual job duration in seconds.
    pub duration_seconds: u64,
    /// Whether the job completed successfully.
    pub success: bool,
}

/// Seconds per hour constant for clarity.
const SECONDS_PER_HOUR: u128 = 3600;

/// Calculates payment for a given duration and hourly rate.
///
/// Uses fixed-point arithmetic with u128 intermediates to ensure:
/// - No precision loss for any valid input combination
/// - No overflow for practical values (up to u64::MAX for both inputs)
/// - Provider-friendly ceiling rounding (never underpays)
///
/// # Arguments
/// * `duration_seconds` - Job duration in seconds
/// * `rate_per_hour` - Token rate per hour
///
/// # Returns
/// The calculated payment amount in tokens.
///
/// # Precision Guarantee
/// For any `duration_seconds` and `rate_per_hour` where the exact mathematical result
/// fits in a `u64`, this function returns `ceiling(duration_seconds × rate_per_hour / 3600)`.
///
/// # Edge Cases
/// - Zero duration or zero rate returns 0
/// - Very large values that would overflow u64 are saturated to u64::MAX
///
/// # Examples
/// ```
/// use molt_market::settlement::calculate_payment;
///
/// // Exact calculation: 1 hour at 100/hour = 100 tokens
/// assert_eq!(calculate_payment(3600, 100), 100);
///
/// // Short job: 1 second at 3600/hour = 1 token (exact)
/// assert_eq!(calculate_payment(1, 3600), 1);
///
/// // Provider-friendly rounding: 1 second at 7200/hour = 2 tokens
/// assert_eq!(calculate_payment(1, 7200), 2);
/// ```
#[must_use]
pub const fn calculate_payment(duration_seconds: u64, rate_per_hour: u64) -> u64 {
    // Handle zero cases explicitly
    if duration_seconds == 0 || rate_per_hour == 0 {
        return 0;
    }

    // Use u128 for intermediate calculation to prevent overflow
    // Formula: ceiling(duration_seconds * rate_per_hour / 3600)
    let numerator = duration_seconds as u128 * rate_per_hour as u128;

    // Ceiling division: (a + b - 1) / b
    // This ensures we round UP, favoring the provider
    let payment_u128 = (numerator + SECONDS_PER_HOUR - 1) / SECONDS_PER_HOUR;

    // Saturate to u64::MAX if overflow would occur
    if payment_u128 > u64::MAX as u128 {
        u64::MAX
    } else {
        payment_u128 as u64
    }
}

/// Calculates payment with explicit rounding mode selection.
///
/// For most use cases, prefer [`calculate_payment`] which uses ceiling rounding.
///
/// # Arguments
/// * `duration_seconds` - Job duration in seconds
/// * `rate_per_hour` - Token rate per hour
/// * `round_up` - If true, use ceiling rounding (provider-friendly); if false, use floor
///
/// # Returns
/// The calculated payment amount in tokens.
#[must_use]
pub const fn calculate_payment_with_rounding(
    duration_seconds: u64,
    rate_per_hour: u64,
    round_up: bool,
) -> u64 {
    if duration_seconds == 0 || rate_per_hour == 0 {
        return 0;
    }

    let numerator = duration_seconds as u128 * rate_per_hour as u128;

    let payment_u128 = if round_up {
        // Ceiling division
        (numerator + SECONDS_PER_HOUR - 1) / SECONDS_PER_HOUR
    } else {
        // Floor division
        numerator / SECONDS_PER_HOUR
    };

    if payment_u128 > u64::MAX as u128 {
        u64::MAX
    } else {
        payment_u128 as u64
    }
}

/// Settles a completed job, calculating the final payment.
///
/// # Arguments
/// * `input` - The job settlement input data
///
/// # Returns
/// A settlement result or an error if the input is invalid.
pub fn settle_job(input: &JobSettlementInput) -> Result<SettlementResult, MarketError> {
    // Validate times
    if input.end_time < input.start_time {
        return Err(MarketError::Settlement(
            "end time cannot be before start time".to_string(),
        ));
    }

    let duration_seconds = (input.end_time - input.start_time) as u64;
    let calculated_payment = calculate_payment(duration_seconds, input.rate_per_hour);

    // Cap payment at escrow amount
    let amount_paid = calculated_payment.min(input.escrow_amount);

    Ok(SettlementResult {
        job_id: input.job_id.clone(),
        amount_paid,
        duration_seconds,
        success: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_result_creation() {
        let result = SettlementResult {
            job_id: "job-123".to_string(),
            amount_paid: 500,
            duration_seconds: 3600,
            success: true,
        };

        assert_eq!(result.job_id, "job-123");
        assert_eq!(result.amount_paid, 500);
        assert_eq!(result.duration_seconds, 3600);
        assert!(result.success);
    }

    #[test]
    fn calculate_payment_exact_hour() {
        // 1 hour at 100 tokens/hour = 100 tokens
        let payment = calculate_payment(3600, 100);
        assert_eq!(payment, 100);
    }

    #[test]
    fn calculate_payment_partial_hour() {
        // 30 minutes at 100 tokens/hour = 50 tokens
        let payment = calculate_payment(1800, 100);
        assert_eq!(payment, 50);
    }

    #[test]
    fn calculate_payment_multiple_hours() {
        // 3 hours at 50 tokens/hour = 150 tokens
        let payment = calculate_payment(10800, 50);
        assert_eq!(payment, 150);
    }

    #[test]
    fn settle_successful_job() {
        let job = JobSettlementInput {
            job_id: "job-456".to_string(),
            start_time: 1000,
            end_time: 4600, // 1 hour later
            rate_per_hour: 200,
            escrow_amount: 500,
        };

        let result = settle_job(&job).unwrap();
        assert_eq!(result.job_id, "job-456");
        assert_eq!(result.amount_paid, 200);
        assert_eq!(result.duration_seconds, 3600);
        assert!(result.success);
    }

    #[test]
    fn settle_job_caps_at_escrow() {
        // Job runs longer than escrow covers
        let job = JobSettlementInput {
            job_id: "job-789".to_string(),
            start_time: 0,
            end_time: 36000, // 10 hours
            rate_per_hour: 100,
            escrow_amount: 500, // Only 500 escrowed (5 hours worth)
        };

        let result = settle_job(&job).unwrap();
        // Should cap at escrow amount
        assert_eq!(result.amount_paid, 500);
        assert!(result.success);
    }

    #[test]
    fn settle_job_invalid_times() {
        let job = JobSettlementInput {
            job_id: "job-bad".to_string(),
            start_time: 5000,
            end_time: 1000, // end before start!
            rate_per_hour: 100,
            escrow_amount: 500,
        };

        let result = settle_job(&job);
        assert!(result.is_err());
    }

    #[test]
    fn calculate_payment_zero_duration() {
        let payment = calculate_payment(0, 100);
        assert_eq!(payment, 0);
    }

    #[test]
    fn calculate_payment_zero_rate() {
        let payment = calculate_payment(3600, 0);
        assert_eq!(payment, 0);
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn calculate_payment_quarter_hour() {
        // 15 minutes at 100 tokens/hour = 25 tokens
        let payment = calculate_payment(900, 100);
        assert_eq!(payment, 25);
    }

    #[test]
    fn calculate_payment_long_duration() {
        // 24 hours at 100 tokens/hour = 2400 tokens
        let payment = calculate_payment(86400, 100);
        assert_eq!(payment, 2400);
    }

    #[test]
    fn calculate_payment_high_rate() {
        // 1 hour at 10000 tokens/hour
        let payment = calculate_payment(3600, 10000);
        assert_eq!(payment, 10000);
    }

    #[test]
    fn calculate_payment_precision() {
        // 1 hour 30 minutes at 100 tokens/hour = 150 tokens
        let payment = calculate_payment(5400, 100);
        assert_eq!(payment, 150);
    }

    #[test]
    fn settle_exact_escrow_match() {
        let job = JobSettlementInput {
            job_id: "job-exact".to_string(),
            start_time: 0,
            end_time: 3600, // 1 hour
            rate_per_hour: 100,
            escrow_amount: 100, // Exactly 1 hour worth
        };

        let result = settle_job(&job).unwrap();
        assert_eq!(result.amount_paid, 100);
        assert_eq!(result.duration_seconds, 3600);
    }

    #[test]
    fn settle_zero_duration() {
        let job = JobSettlementInput {
            job_id: "job-zero".to_string(),
            start_time: 1000,
            end_time: 1000, // Same time
            rate_per_hour: 100,
            escrow_amount: 500,
        };

        let result = settle_job(&job).unwrap();
        assert_eq!(result.amount_paid, 0);
        assert_eq!(result.duration_seconds, 0);
        assert!(result.success);
    }

    #[test]
    fn settle_short_job() {
        // 5 minute job
        let job = JobSettlementInput {
            job_id: "job-short".to_string(),
            start_time: 0,
            end_time: 300,
            rate_per_hour: 120,
            escrow_amount: 100,
        };

        let result = settle_job(&job).unwrap();
        assert_eq!(result.duration_seconds, 300);
        // Due to integer math: (300 * 1000 / 3600) * 120 / 1000 = 83 * 120 / 1000 = 9
        // The formula uses scaled integer math, so small rounding differences are expected
        assert!(result.amount_paid > 0 && result.amount_paid <= 10);
    }

    #[test]
    fn settlement_result_serialization() {
        let result = SettlementResult {
            job_id: "job-123".to_string(),
            amount_paid: 500,
            duration_seconds: 3600,
            success: true,
        };

        let json = serde_json::to_string(&result).expect("serialize");
        let deserialized: SettlementResult = serde_json::from_str(&json).expect("deserialize");
        
        assert_eq!(result.job_id, deserialized.job_id);
        assert_eq!(result.amount_paid, deserialized.amount_paid);
        assert_eq!(result.duration_seconds, deserialized.duration_seconds);
        assert_eq!(result.success, deserialized.success);
    }

    #[test]
    fn job_settlement_input_serialization() {
        let input = JobSettlementInput {
            job_id: "job-456".to_string(),
            start_time: 1000,
            end_time: 5000,
            rate_per_hour: 200,
            escrow_amount: 1000,
        };

        let json = serde_json::to_string(&input).expect("serialize");
        let deserialized: JobSettlementInput = serde_json::from_str(&json).expect("deserialize");
        
        assert_eq!(input.job_id, deserialized.job_id);
        assert_eq!(input.rate_per_hour, deserialized.rate_per_hour);
    }

    #[test]
    fn settlement_result_clone() {
        let result = SettlementResult {
            job_id: "job-123".to_string(),
            amount_paid: 500,
            duration_seconds: 3600,
            success: true,
        };

        let cloned = result.clone();
        assert_eq!(result.job_id, cloned.job_id);
        assert_eq!(result.amount_paid, cloned.amount_paid);
    }

    #[test]
    fn job_settlement_input_clone() {
        let input = JobSettlementInput {
            job_id: "job-456".to_string(),
            start_time: 1000,
            end_time: 5000,
            rate_per_hour: 200,
            escrow_amount: 1000,
        };

        let cloned = input.clone();
        assert_eq!(input.job_id, cloned.job_id);
    }

    #[test]
    fn settlement_result_debug() {
        let result = SettlementResult {
            job_id: "job-123".to_string(),
            amount_paid: 500,
            duration_seconds: 3600,
            success: true,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("SettlementResult"));
    }

    #[test]
    fn job_settlement_input_debug() {
        let input = JobSettlementInput {
            job_id: "job-456".to_string(),
            start_time: 1000,
            end_time: 5000,
            rate_per_hour: 200,
            escrow_amount: 1000,
        };
        let debug = format!("{:?}", input);
        assert!(debug.contains("JobSettlementInput"));
    }

    #[test]
    fn settle_with_large_values() {
        let job = JobSettlementInput {
            job_id: "job-large".to_string(),
            start_time: 0,
            end_time: 86400 * 30, // 30 days in seconds
            rate_per_hour: 1000,
            escrow_amount: u64::MAX,
        };

        let result = settle_job(&job).unwrap();
        // 30 days = 720 hours at 1000/hour = 720000
        assert_eq!(result.amount_paid, 720000);
    }

    #[test]
    fn settle_escrow_less_than_calculated() {
        // Escrow is less than what would be calculated
        let job = JobSettlementInput {
            job_id: "job-under".to_string(),
            start_time: 0,
            end_time: 7200, // 2 hours
            rate_per_hour: 100,
            escrow_amount: 150, // Only 1.5 hours worth escrowed
        };

        let result = settle_job(&job).unwrap();
        // Should pay calculated (200) but capped at escrow (150)
        assert_eq!(result.amount_paid, 150);
    }

    // =========================================================================
    // BUG DEMONSTRATION TESTS - These should fail with the buggy implementation
    // =========================================================================

    #[test]
    fn bug_very_short_job_gets_zero_payment() {
        // BUG: A 1-second job at 3600 tokens/hour should pay 1 token
        // With buggy formula: 1 * 1000 / 3600 = 0, so 0 * 3600 / 1000 = 0
        // Provider gets NOTHING for actual work!
        let payment = calculate_payment(1, 3600);
        // Expected: 1 token (1 second at 3600/hour = 1 token/second)
        assert_eq!(payment, 1, "1-second job at 3600/hour should pay 1 token");
    }

    #[test]
    fn bug_precision_loss_accumulates() {
        // BUG: Precision loss is systematic and always against the provider
        // 3 seconds at 3600/hour should be exactly 3 tokens
        let payment = calculate_payment(3, 3600);
        assert_eq!(payment, 3, "3-second job at 3600/hour should pay 3 tokens");
    }

    #[test]
    fn bug_small_rate_small_duration_loses_everything() {
        // BUG: Small rate * small duration can round to 0
        // 10 seconds at 360 tokens/hour = 1 token
        // 360/hour = 0.1/second, so 10 seconds = 1 token
        let payment = calculate_payment(10, 360);
        assert_eq!(payment, 1, "10 seconds at 360/hour should pay 1 token");
    }

    #[test]
    fn bug_provider_systematically_underpaid() {
        // Run multiple short jobs and show cumulative loss
        // 60 jobs of 1 second each at 3600/hour should equal 60 tokens
        let mut total_paid = 0u64;
        for _ in 0..60 {
            total_paid += calculate_payment(1, 3600);
        }
        // One 60-second job pays correctly:
        let lump_sum = calculate_payment(60, 3600);
        
        // With correct math, both should be 60 tokens
        assert_eq!(total_paid, 60, "60 one-second jobs should pay 60 tokens total");
        assert_eq!(lump_sum, 60, "One 60-second job should pay 60 tokens");
        assert_eq!(total_paid, lump_sum, "Splitting work shouldn't change payment");
    }

    #[test]
    fn bug_rounding_should_favor_provider() {
        // 7 seconds at 3600 tokens/hour = 7 tokens exactly
        // No rounding needed, but if there were fractional tokens,
        // we should round UP (ceiling) to favor provider
        let payment = calculate_payment(7, 3600);
        assert_eq!(payment, 7);
        
        // 5 seconds at 7200 tokens/hour = 10 tokens exactly
        let payment2 = calculate_payment(5, 7200);
        assert_eq!(payment2, 10);
    }

    #[test]
    fn edge_case_very_large_values_no_overflow() {
        // Large duration and rate shouldn't overflow
        // Max practical: 1 year in seconds at high rate
        let one_year_seconds: u64 = 365 * 24 * 3600; // 31,536,000 seconds
        let high_rate: u64 = 1_000_000; // 1M tokens/hour
        
        // Should not overflow: ~8.76B tokens for a year
        let payment = calculate_payment(one_year_seconds, high_rate);
        let expected = one_year_seconds as u128 * high_rate as u128 / 3600;
        assert_eq!(payment, expected as u64);
    }

    #[test]
    fn edge_case_micro_payments_still_work() {
        // Very small rate: 1 token per hour = ~0.000278 tokens/second
        // 3600 seconds at 1 token/hour = 1 token
        let payment = calculate_payment(3600, 1);
        assert_eq!(payment, 1);
        
        // 7200 seconds (2 hours) at 1 token/hour = 2 tokens
        let payment2 = calculate_payment(7200, 1);
        assert_eq!(payment2, 2);
    }

    #[test]
    fn property_payment_is_proportional() {
        // Double the duration = double the payment
        let base_payment = calculate_payment(3600, 100);
        let double_duration = calculate_payment(7200, 100);
        assert_eq!(double_duration, base_payment * 2);
        
        // Double the rate = double the payment  
        let double_rate = calculate_payment(3600, 200);
        assert_eq!(double_rate, base_payment * 2);
    }
}
