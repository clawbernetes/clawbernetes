//! Settlement logic for completed jobs.
//!
//! Calculates final payments based on job duration, rates,
//! and any applicable adjustments.

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

/// Calculates payment for a given duration and hourly rate.
///
/// # Arguments
/// * `duration_seconds` - Job duration in seconds
/// * `rate_per_hour` - Token rate per hour
///
/// # Returns
/// The calculated payment amount in tokens.
pub fn calculate_payment(duration_seconds: u64, rate_per_hour: u64) -> u64 {
    // Calculate payment: (duration / 3600) * rate
    // Use careful math to avoid overflow and maintain precision
    let hours_scaled = duration_seconds * 1000 / 3600; // Scale up for precision
    (hours_scaled * rate_per_hour) / 1000
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
}
