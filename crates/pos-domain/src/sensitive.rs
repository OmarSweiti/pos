//! The never-log registry (gap G-8, microstep 1.6.8).
//!
//! One list of the field names whose values never reach a log line, a crash
//! report, an error's detail or an audit payload, and the one function that
//! reads it. Everything that needs the list derives from this constant: the
//! terminal's scrubbing layer and its tests, and the audit-payload guard in
//! [`crate::audit::check_payload`]. A second, hand-written copy is how a
//! scrubber came to pass its own test while leaking a card number: the
//! Phase-1 copy of this list was once short by five names.
//!
//! It lives in `pos-domain` because the audit-payload guard does, and nothing
//! below the terminal could otherwise reach it. It is a table and a function:
//! nothing here reads, writes or logs (I-8).
//!
//! `ref/security-compliance.md` §6 is the canonical statement, and
//! `.claude/rules/security.md` repeats it for the agents. A test holds both
//! documents to this constant, so adding a name in one place and not the
//! others fails.

/// The never-log rules: exact names, suffixes and substrings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitiveFieldRules {
    /// A field with exactly one of these names, once normalized.
    pub names: &'static [&'static str],
    /// A field whose name ends with one of these.
    pub suffixes: &'static [&'static str],
    /// A field whose name contains one of these.
    pub contains: &'static [&'static str],
}

/// The registry (`ref/security-compliance.md` §6).
pub const SENSITIVE_FIELD_RULES: SensitiveFieldRules = SensitiveFieldRules {
    names: &[
        "pin",
        "pin_hash",
        "pan",
        "card_number",
        "cvv",
        "track",
        "phone",
        "email",
        "customer_name",
        "buyer_name",
        "secret_key",
        "client_id",
        "db_key",
        "token",
        "password",
        "entitlement",
        "recovery_code",
        "enrollment_code",
        "wrapped_key",
    ],
    suffixes: &["_token", "_secret", "_key", "_pin", "_hash"],
    contains: &["password"],
};

impl SensitiveFieldRules {
    /// Whether a field named `field` never has its value logged.
    ///
    /// The name is normalized first, so a remote or a webview spelling of a
    /// listed name matches it too: `cardNumber`, `Card-Number` and
    /// `CARD_NUMBER` are all `card_number`.
    #[must_use]
    pub fn matches(&self, field: &str) -> bool {
        let name = normalized(field);
        self.names.contains(&name.as_str())
            || self.suffixes.iter().any(|s| name.ends_with(s))
            || self.contains.iter().any(|c| name.contains(c))
    }
}

/// `field` in snake case: lower case, a word boundary before each capital of
/// a camel-cased name, and `_` for a hyphen, a dot or a space. A name written
/// all in capitals is only lowered.
fn normalized(field: &str) -> String {
    let all_capitals = !field.chars().any(|c| c.is_ascii_lowercase());
    let mut out = String::with_capacity(field.len() + 4);
    for c in field.chars() {
        if matches!(c, '-' | '.' | ' ') {
            out.push('_');
            continue;
        }
        if c.is_ascii_uppercase() && !all_capitals && !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// The names a document lists in its never-log rule: every backticked
    /// name in the sentence that opens the list, up to the end of the
    /// sentence that closes it.
    fn backticked(text: &str) -> Vec<&str> {
        text.split('`').skip(1).step_by(2).collect()
    }

    /// The registry is the list `ref/security-compliance.md` §6 and
    /// `.claude/rules/security.md` state: the same exact names, the same
    /// suffixes, the same contains rule. Adding a name to one and not the
    /// others fails here.
    #[test]
    fn the_registry_is_the_one_the_documents_name() {
        let compliance = include_str!("../../../docs/implementation/ref/security-compliance.md");
        let rule = include_str!("../../../.claude/rules/security.md");
        let from = |doc: &'static str, start: &str, end: &str| -> Vec<&'static str> {
            let at = doc.find(start).expect("the registry's sentence");
            let rest = &doc[at..];
            let stop = rest.find(end).expect("its end");
            backticked(&rest[..stop])
        };
        let compliance_names = from(
            compliance,
            "redacts, at any nesting depth, fields named:",
            "— plus any",
        );
        let compliance_suffixes = from(
            compliance,
            "plus any field whose name ends in",
            ", or contains",
        );
        let rule_names = from(
            rule,
            "under any canonical sensitive field name:",
            "; also redact",
        );
        let rule_suffixes = from(
            rule,
            "also redact every field ending in",
            ", and every field whose",
        );
        let names: Vec<&str> = SENSITIVE_FIELD_RULES.names.to_vec();
        let suffixes: Vec<&str> = SENSITIVE_FIELD_RULES.suffixes.to_vec();
        assert_eq!(compliance_names, names);
        assert_eq!(rule_names, names);
        assert_eq!(compliance_suffixes, suffixes);
        assert_eq!(rule_suffixes, suffixes);
        assert!(compliance.contains("or contains `password`"));
        assert!(rule.contains("every field whose name contains `password`"));
        assert_eq!(SENSITIVE_FIELD_RULES.contains, ["password"]);
    }

    #[test]
    fn every_rule_matches_and_innocent_names_do_not() {
        for name in SENSITIVE_FIELD_RULES.names {
            assert!(SENSITIVE_FIELD_RULES.matches(name), "{name}");
        }
        for suffix in SENSITIVE_FIELD_RULES.suffixes {
            assert!(
                SENSITIVE_FIELD_RULES.matches(&format!("device{suffix}")),
                "{suffix}"
            );
        }
        for sensitive in [
            "cardNumber",
            "Card-Number",
            "CARD_NUMBER",
            "deviceToken",
            "client_secret",
            "oldPassword",
            "password_reset_at",
            "wrapped.key",
            "PIN",
            "manager_pin",
        ] {
            assert!(SENSITIVE_FIELD_RULES.matches(sensitive), "{sensitive}");
        }
        for innocent in [
            "sale_id",
            "amount_minor",
            "masked_pan",
            "scheme",
            "psp_ref",
            "tender_id",
            "pinned",
            "keyboard",
            "hashtag",
            "line_void",
        ] {
            assert!(!SENSITIVE_FIELD_RULES.matches(innocent), "{innocent}");
        }
    }
}
