use core::PrivacyPolicy;

pub const BLOCKED_TELEMETRY_HOSTS: &[&str] = &[
    "statsig.anthropic.com",
    "sentry.io",
    "o1137031.ingest.sentry.io",
];

pub fn claude_policy() -> PrivacyPolicy {
    BLOCKED_TELEMETRY_HOSTS
        .iter()
        .fold(PrivacyPolicy::new(), |policy, host| {
            policy.with_blocked_host(*host)
        })
}
