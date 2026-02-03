//! MOLT network participation command implementation.
//!
//! Provides subcommands for:
//! - Viewing MOLT participation status
//! - Joining/leaving the MOLT network
//! - Viewing earnings

use std::io::Write;

use crate::cli::{AutonomyArg, MoltCommands};
use crate::error::CliError;
use crate::output::{EarningsSummary, Message, MoltStatus, OutputFormat};

/// MOLT command executor.
pub struct MoltCommand {
    gateway_url: String,
}

impl MoltCommand {
    /// Create a new MOLT command.
    #[must_use]
    pub fn new(gateway_url: impl Into<String>) -> Self {
        Self {
            gateway_url: gateway_url.into(),
        }
    }

    /// Execute a MOLT subcommand.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    pub async fn execute<W: Write>(
        &self,
        writer: &mut W,
        format: &OutputFormat,
        command: &MoltCommands,
    ) -> Result<(), CliError> {
        self.validate_gateway_url()?;

        match command {
            MoltCommands::Status => {
                let status = self.get_status().await?;
                format.write(writer, &status)?;
            }
            MoltCommands::Join { autonomy, max_spend } => {
                self.join_network(*autonomy, max_spend.clone()).await?;
                let msg = Message::success(format!(
                    "Joined MOLT network with {autonomy:?} autonomy"
                ));
                format.write(writer, &msg)?;
            }
            MoltCommands::Leave => {
                self.leave_network().await?;
                let msg = Message::success("Left MOLT network");
                format.write(writer, &msg)?;
            }
            MoltCommands::Earnings { detailed } => {
                let earnings = self.get_earnings(*detailed).await?;
                format.write(writer, &earnings)?;
            }
        }
        Ok(())
    }

    /// Validate the gateway URL format.
    fn validate_gateway_url(&self) -> Result<(), CliError> {
        if !self.gateway_url.starts_with("ws://") && !self.gateway_url.starts_with("wss://") {
            return Err(CliError::Config(format!(
                "invalid gateway URL: {}, must start with ws:// or wss://",
                self.gateway_url
            )));
        }
        Ok(())
    }

    /// Get MOLT participation status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get_status(&self) -> Result<MoltStatus, CliError> {
        // TODO: Replace with actual gateway call
        Ok(MoltStatus {
            participating: false,
            autonomy_level: None,
            public_key: None,
            balance: None,
            total_earnings: None,
            jobs_completed: None,
        })
    }

    /// Join the MOLT network.
    ///
    /// # Errors
    ///
    /// Returns an error if joining fails.
    pub async fn join_network(
        &self,
        _autonomy: AutonomyArg,
        max_spend: Option<String>,
    ) -> Result<(), CliError> {
        // Validate max_spend if provided
        if let Some(ref spend) = max_spend {
            Self::validate_amount(spend)?;
        }

        // TODO: Replace with actual gateway call
        Ok(())
    }

    /// Leave the MOLT network.
    ///
    /// # Errors
    ///
    /// Returns an error if leaving fails.
    pub async fn leave_network(&self) -> Result<(), CliError> {
        // TODO: Replace with actual gateway call
        Ok(())
    }

    /// Get earnings summary.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get_earnings(&self, _detailed: bool) -> Result<EarningsSummary, CliError> {
        // TODO: Replace with actual gateway call
        Ok(EarningsSummary {
            total: "0.000000000 MOLT".into(),
            today: "0.000000000 MOLT".into(),
            this_week: "0.000000000 MOLT".into(),
            this_month: "0.000000000 MOLT".into(),
            jobs_completed: 0,
            avg_per_job: "0.000000000 MOLT".into(),
        })
    }

    /// Validate a MOLT amount string.
    fn validate_amount(amount: &str) -> Result<(), CliError> {
        // Basic validation - must be a positive number
        let trimmed = amount.trim();
        if trimmed.is_empty() {
            return Err(CliError::InvalidArgument("amount cannot be empty".into()));
        }

        // Try to parse as a float to validate format
        let value: f64 = trimmed
            .parse()
            .map_err(|_| CliError::InvalidArgument(format!("invalid amount: {amount}")))?;

        if value < 0.0 {
            return Err(CliError::InvalidArgument(
                "amount cannot be negative".into(),
            ));
        }

        Ok(())
    }

    /// Convert autonomy argument to display string.
    #[must_use]
    pub fn autonomy_to_string(autonomy: AutonomyArg) -> String {
        match autonomy {
            AutonomyArg::Conservative => "Conservative",
            AutonomyArg::Moderate => "Moderate",
            AutonomyArg::Aggressive => "Aggressive",
        }
        .to_string()
    }
}

/// MOLT client trait for testing.
pub trait MoltClient: Send + Sync {
    /// Get participation status.
    fn get_status(&self) -> impl std::future::Future<Output = Result<MoltStatus, CliError>> + Send;

    /// Join the network.
    fn join(
        &self,
        autonomy: AutonomyArg,
        max_spend: Option<String>,
    ) -> impl std::future::Future<Output = Result<(), CliError>> + Send;

    /// Leave the network.
    fn leave(&self) -> impl std::future::Future<Output = Result<(), CliError>> + Send;

    /// Get earnings.
    fn get_earnings(
        &self,
        detailed: bool,
    ) -> impl std::future::Future<Output = Result<EarningsSummary, CliError>> + Send;
}

/// Fake MOLT client for testing.
#[cfg(test)]
pub struct FakeMoltClient {
    status: MoltStatus,
    earnings: EarningsSummary,
    joined: std::sync::Arc<std::sync::Mutex<bool>>,
}

#[cfg(test)]
impl FakeMoltClient {
    /// Create a new fake client.
    pub fn new(status: MoltStatus, earnings: EarningsSummary) -> Self {
        Self {
            status,
            earnings,
            joined: std::sync::Arc::new(std::sync::Mutex::new(false)),
        }
    }

    /// Check if join was called.
    pub fn is_joined(&self) -> bool {
        *self.joined.lock().expect("lock")
    }
}

#[cfg(test)]
impl MoltClient for FakeMoltClient {
    async fn get_status(&self) -> Result<MoltStatus, CliError> {
        Ok(self.status.clone())
    }

    async fn join(&self, _autonomy: AutonomyArg, _max_spend: Option<String>) -> Result<(), CliError> {
        let mut joined = self.joined.lock().expect("lock");
        *joined = true;
        Ok(())
    }

    async fn leave(&self) -> Result<(), CliError> {
        let mut joined = self.joined.lock().expect("lock");
        *joined = false;
        Ok(())
    }

    async fn get_earnings(&self, _detailed: bool) -> Result<EarningsSummary, CliError> {
        Ok(self.earnings.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    fn make_test_status(participating: bool) -> MoltStatus {
        if participating {
            MoltStatus {
                participating: true,
                autonomy_level: Some("Moderate".into()),
                public_key: Some("5abc123def".into()),
                balance: Some("100.5 MOLT".into()),
                total_earnings: Some("500.25 MOLT".into()),
                jobs_completed: Some(42),
            }
        } else {
            MoltStatus {
                participating: false,
                autonomy_level: None,
                public_key: None,
                balance: None,
                total_earnings: None,
                jobs_completed: None,
            }
        }
    }

    fn make_test_earnings() -> EarningsSummary {
        EarningsSummary {
            total: "500.25 MOLT".into(),
            today: "12.5 MOLT".into(),
            this_week: "87.3 MOLT".into(),
            this_month: "342.1 MOLT".into(),
            jobs_completed: 42,
            avg_per_job: "11.91 MOLT".into(),
        }
    }

    #[test]
    fn molt_command_new() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn molt_command_validates_gateway_url() {
        let cmd = MoltCommand::new("http://invalid");
        let result = cmd.validate_gateway_url();
        assert!(result.is_err());
    }

    #[test]
    fn molt_command_accepts_valid_url() {
        let cmd = MoltCommand::new("wss://secure:443");
        assert!(cmd.validate_gateway_url().is_ok());
    }

    #[test]
    fn validate_amount_valid() {
        assert!(MoltCommand::validate_amount("100").is_ok());
        assert!(MoltCommand::validate_amount("100.5").is_ok());
        assert!(MoltCommand::validate_amount("0").is_ok());
        assert!(MoltCommand::validate_amount("0.000000001").is_ok());
    }

    #[test]
    fn validate_amount_invalid() {
        assert!(MoltCommand::validate_amount("").is_err());
        assert!(MoltCommand::validate_amount("abc").is_err());
        assert!(MoltCommand::validate_amount("-50").is_err());
    }

    #[test]
    fn autonomy_to_string_conservative() {
        let s = MoltCommand::autonomy_to_string(AutonomyArg::Conservative);
        assert_eq!(s, "Conservative");
    }

    #[test]
    fn autonomy_to_string_moderate() {
        let s = MoltCommand::autonomy_to_string(AutonomyArg::Moderate);
        assert_eq!(s, "Moderate");
    }

    #[test]
    fn autonomy_to_string_aggressive() {
        let s = MoltCommand::autonomy_to_string(AutonomyArg::Aggressive);
        assert_eq!(s, "Aggressive");
    }

    #[tokio::test]
    async fn get_status_returns_not_participating() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let status = cmd.get_status().await.expect("should get status");
        assert!(!status.participating);
    }

    #[tokio::test]
    async fn join_network_success() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let result = cmd.join_network(AutonomyArg::Moderate, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn join_network_with_max_spend() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let result = cmd
            .join_network(AutonomyArg::Conservative, Some("100.5".into()))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn join_network_invalid_max_spend() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let result = cmd
            .join_network(AutonomyArg::Conservative, Some("invalid".into()))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid amount"));
    }

    #[tokio::test]
    async fn leave_network_success() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let result = cmd.leave_network().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_earnings_success() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let earnings = cmd.get_earnings(false).await.expect("should get earnings");
        assert_eq!(earnings.jobs_completed, 0);
    }

    #[tokio::test]
    async fn execute_status_table() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &MoltCommands::Status)
            .await
            .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("MOLT Network Status"));
        assert!(output.contains("Not participating"));
    }

    #[tokio::test]
    async fn execute_status_json() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Json);
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &MoltCommands::Status)
            .await
            .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("\"participating\""));
    }

    #[tokio::test]
    async fn execute_join_default() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        cmd.execute(
            &mut buf,
            &format,
            &MoltCommands::Join {
                autonomy: AutonomyArg::Conservative,
                max_spend: None,
            },
        )
        .await
        .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Joined MOLT network"));
        assert!(output.contains("Conservative"));
    }

    #[tokio::test]
    async fn execute_join_aggressive() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        cmd.execute(
            &mut buf,
            &format,
            &MoltCommands::Join {
                autonomy: AutonomyArg::Aggressive,
                max_spend: Some("500".into()),
            },
        )
        .await
        .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Aggressive"));
    }

    #[tokio::test]
    async fn execute_leave() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &MoltCommands::Leave)
            .await
            .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Left MOLT network"));
    }

    #[tokio::test]
    async fn execute_earnings() {
        let cmd = MoltCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        cmd.execute(
            &mut buf,
            &format,
            &MoltCommands::Earnings { detailed: false },
        )
        .await
        .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("MOLT Earnings Summary"));
    }

    #[tokio::test]
    async fn execute_invalid_gateway() {
        let cmd = MoltCommand::new("http://invalid");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        let result = cmd
            .execute(&mut buf, &format, &MoltCommands::Status)
            .await;
        assert!(matches!(result, Err(CliError::Config(_))));
    }

    #[tokio::test]
    async fn fake_client_get_status() {
        let status = make_test_status(true);
        let earnings = make_test_earnings();
        let client = FakeMoltClient::new(status, earnings);

        let result = client.get_status().await.expect("should get status");
        assert!(result.participating);
        assert_eq!(result.autonomy_level, Some("Moderate".into()));
    }

    #[tokio::test]
    async fn fake_client_join_and_leave() {
        let status = make_test_status(false);
        let earnings = make_test_earnings();
        let client = FakeMoltClient::new(status, earnings);

        assert!(!client.is_joined());

        client
            .join(AutonomyArg::Moderate, None)
            .await
            .expect("should join");
        assert!(client.is_joined());

        client.leave().await.expect("should leave");
        assert!(!client.is_joined());
    }

    #[tokio::test]
    async fn fake_client_get_earnings() {
        let status = make_test_status(true);
        let earnings = make_test_earnings();
        let client = FakeMoltClient::new(status, earnings);

        let result = client.get_earnings(false).await.expect("should get earnings");
        assert_eq!(result.jobs_completed, 42);
        assert_eq!(result.total, "500.25 MOLT");
    }
}
