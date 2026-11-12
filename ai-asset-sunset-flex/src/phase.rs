// Copyright 2026 The ai-asset-sunset authors. Licensed under Apache-2.0.
//! Pure, PDK-free phase logic: configuration validation and the per-request decision.
//! Everything here is deterministic in `now`, so it is unit-tested without a gateway.

use chrono::{DateTime, Duration, Utc};

use crate::generated::config::Config;

pub const MAX_MESSAGE: usize = 500;
pub const MAX_FIELD: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Before `deprecationAt`: pure pass-through.
    Active,
    /// `deprecationAt` ≤ now < `sunsetAt`: pass-through + deprecation signals.
    HeadsUp,
    /// Inside a configured brownout window during heads-up: tombstone.
    Brownout,
    /// `sunsetAt` ≤ now < `switchOffAt`: tombstone.
    Tombstone,
    /// now ≥ `switchOffAt`: tombstone + warning (instance should be removed).
    PastSwitchOff,
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Active => "active",
            Phase::HeadsUp => "heads-up",
            Phase::Brownout => "brownout",
            Phase::Tombstone => "tombstone",
            Phase::PastSwitchOff => "switched-off",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disclosure {
    Minimal,
    Standard,
    Full,
}

#[derive(Clone, Debug)]
pub struct Exemption {
    pub client_id: String,
    pub until: DateTime<Utc>,
    pub reason: String,
}

/// Validated, normalised settings.
#[derive(Clone, Debug)]
pub struct Settings {
    pub deprecation_at: DateTime<Utc>,
    pub sunset_at: DateTime<Utc>,
    pub switch_off_at: DateTime<Utc>,
    pub identify_at: Option<DateTime<Utc>>,
    pub brownouts: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    pub exemptions: Vec<Exemption>,
    pub message: String,
    pub successor_name: String,
    pub successor_url: String,
    pub docs_url: String,
    pub contact_name: String,
    pub contact_email: String,
    pub protocol: String,
    pub mcp_tool_result: bool,
    pub llm_assistant_message: bool,
    pub disclosure: Disclosure,
    pub inject_body_metadata: bool,
    pub annotate_tool_descriptions: bool,
    pub max_body_rewrite_bytes: usize,
    pub task_grace: Duration,
    pub emit_violation: bool,
    pub cache_clamp_seconds: i64,
}

fn parse_ts(field: &str, value: &str) -> Result<DateTime<Utc>, String> {
    // RFC 3339 requires an explicit offset ("Z" or "+01:00"); a bare date is rejected.
    DateTime::parse_from_rfc3339(value.trim())
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| format!("{field}: '{value}' is not RFC 3339 with offset ({e})"))
}

/// Used by the fail-closed fallback when the rest of the configuration is invalid.
pub fn parse_sunset_only(value: &str) -> Option<DateTime<Utc>> {
    parse_ts("sunsetAt", value).ok()
}

/// Strip control characters (CR/LF header injection) and cap length.
pub fn clean(value: Option<&String>, max: usize) -> String {
    value
        .map(|v| v.chars().filter(|c| !c.is_control()).take(max).collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string()
}

impl Settings {
    pub fn from_config(c: &Config) -> Result<Settings, Vec<String>> {
        let mut errors = Vec::new();
        let dep = parse_ts("deprecationAt", &c.deprecation_at);
        let sun = parse_ts("sunsetAt", &c.sunset_at);
        let off = parse_ts("switchOffAt", &c.switch_off_at);
        let (dep, sun, off) = match (dep, sun, off) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            (a, b, c) => {
                for r in [a, b, c] {
                    if let Err(e) = r {
                        errors.push(e);
                    }
                }
                return Err(errors);
            }
        };
        if dep >= sun {
            errors.push("deprecationAt must be before sunsetAt".into());
        }
        if sun > off {
            errors.push("sunsetAt must not be after switchOffAt".into());
        }

        let identify_at = match c.identify_yourself_at.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(v) => match parse_ts("identifyYourselfAt", v) {
                Ok(t) => Some(t),
                Err(e) => {
                    errors.push(e);
                    None
                }
            },
        };

        let mut brownouts = Vec::new();
        for (i, b) in c.brownouts.iter().flatten().enumerate() {
            match parse_ts(&format!("brownouts[{i}].start"), &b.start) {
                Ok(start) => {
                    let mins = b.duration_minutes.clamp(1, 1440);
                    let end = start + Duration::minutes(mins);
                    if start < dep || end > sun {
                        errors.push(format!("brownouts[{i}] must lie inside the heads-up phase"));
                    }
                    brownouts.push((start, end));
                }
                Err(e) => errors.push(e),
            }
        }

        let mut exemptions = Vec::new();
        for (i, x) in c.exemptions.iter().flatten().enumerate() {
            let id = x.client_id.trim();
            if id.is_empty() {
                errors.push(format!("exemptions[{i}].clientId must not be empty"));
                continue;
            }
            match parse_ts(&format!("exemptions[{i}].until"), &x.until) {
                // An exemption can never outlive the planned switch-off.
                Ok(until) => exemptions.push(Exemption {
                    client_id: id.to_string(),
                    until: until.min(off),
                    reason: clean(x.reason.as_ref(), MAX_FIELD),
                }),
                Err(e) => errors.push(e),
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let message = {
            let m = clean(c.message.as_ref(), MAX_MESSAGE);
            if m.is_empty() {
                "This service is deprecated and will be retired.".to_string()
            } else {
                m
            }
        };

        Ok(Settings {
            deprecation_at: dep,
            sunset_at: sun,
            switch_off_at: off,
            identify_at,
            brownouts,
            exemptions,
            message,
            successor_name: clean(c.successor_name.as_ref(), MAX_FIELD),
            successor_url: clean(c.successor_url.as_ref(), MAX_FIELD),
            docs_url: clean(c.docs_url.as_ref(), MAX_FIELD),
            contact_name: clean(c.contact_name.as_ref(), MAX_FIELD),
            contact_email: clean(c.contact_email.as_ref(), MAX_FIELD),
            protocol: c.protocol.clone().unwrap_or_else(|| "auto".into()),
            mcp_tool_result: c.mcp_tombstone_style.as_deref() != Some("jsonRpcError"),
            llm_assistant_message: c.llm_tombstone_style.as_deref() == Some("assistantMessage"),
            disclosure: match c.disclosure.as_deref() {
                Some("minimal") => Disclosure::Minimal,
                Some("full") => Disclosure::Full,
                _ => Disclosure::Standard,
            },
            inject_body_metadata: c.inject_body_metadata.unwrap_or(true),
            annotate_tool_descriptions: c.annotate_tool_descriptions.unwrap_or(false),
            max_body_rewrite_bytes: c
                .max_body_rewrite_bytes
                .unwrap_or(262_144)
                .clamp(1024, 1_048_576) as usize,
            task_grace: Duration::minutes(c.task_grace_minutes.unwrap_or(1440).clamp(0, 20_160)),
            emit_violation: c.emit_violation.unwrap_or(true),
            cache_clamp_seconds: c.cache_clamp_seconds.unwrap_or(3600).max(0),
        })
    }

    pub fn phase_at(&self, now: DateTime<Utc>) -> Phase {
        if now < self.deprecation_at {
            Phase::Active
        } else if now < self.sunset_at {
            if self.brownouts.iter().any(|(s, e)| now >= *s && now < *e) {
                Phase::Brownout
            } else {
                Phase::HeadsUp
            }
        } else if now < self.switch_off_at {
            Phase::Tombstone
        } else {
            Phase::PastSwitchOff
        }
    }

    pub fn exemption_for(&self, client_id: Option<&str>, now: DateTime<Utc>) -> Option<&Exemption> {
        // Only identified clients (client_id from the PDK Authentication object) can be exempt.
        let id = client_id?;
        self.exemptions.iter().find(|x| x.client_id == id && now < x.until)
    }
}

/// What kind of call this is, as far as the sunset decision is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// Starts new work (tools/call, message/send, chat completion, any REST call).
    NewWork,
    /// Follow-up on work started before sunset (A2A tasks/get, tasks/cancel, …).
    TaskFollowUp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do (before deprecationAt).
    Pass,
    /// Forward upstream and decorate the response.
    HeadsUp { exempt: bool, reason: Option<String> },
    /// Answer locally, do not call upstream.
    Tombstone { why: &'static str },
}

/// Does this phase *possibly* require a local answer? Used to choose the buffering strategy
/// before the body is read (only tombstone paths buffer the request body).
pub fn may_tombstone(s: &Settings, phase: Phase, client_id: Option<&str>, now: DateTime<Utc>) -> bool {
    match phase {
        Phase::Active => false,
        Phase::HeadsUp => s.identify_at.map_or(false, |t| now >= t) && client_id.is_none(),
        _ => s.exemption_for(client_id, now).is_none(),
    }
}

pub fn decide(
    s: &Settings,
    now: DateTime<Utc>,
    client_id: Option<&str>,
    kind: CallKind,
) -> Decision {
    let phase = s.phase_at(now);
    if phase == Phase::Active {
        return Decision::Pass;
    }
    if let Some(x) = s.exemption_for(client_id, now) {
        return Decision::HeadsUp { exempt: true, reason: Some(x.reason.clone()) };
    }
    match phase {
        Phase::Active => Decision::Pass,
        Phase::HeadsUp => {
            if client_id.is_none() && s.identify_at.map_or(false, |t| now >= t) {
                Decision::Tombstone { why: "unidentified-client" }
            } else {
                Decision::HeadsUp { exempt: false, reason: None }
            }
        }
        Phase::Brownout => Decision::Tombstone { why: "brownout" },
        Phase::Tombstone | Phase::PastSwitchOff => {
            if kind == CallKind::TaskFollowUp && now < (s.sunset_at + s.task_grace).min(s.switch_off_at) {
                Decision::HeadsUp { exempt: false, reason: Some("task-grace".into()) }
            } else if phase == Phase::PastSwitchOff {
                Decision::Tombstone { why: "past-switch-off" }
            } else {
                Decision::Tombstone { why: "sunset" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::config::{Brownouts0Config, Config, Exemptions0Config};

    fn cfg() -> Config {
        Config {
            deprecation_at: "2026-12-01T00:00:00Z".into(),
            sunset_at: "2027-01-01T00:00:00Z".into(),
            switch_off_at: "2027-01-15T00:00:00Z".into(),
            message: None,
            successor_name: Some("orders-agent-v2".into()),
            successor_url: None,
            docs_url: Some("https://example.com/sunset".into()),
            contact_name: None,
            contact_email: None,
            protocol: None,
            mcp_tombstone_style: None,
            llm_tombstone_style: None,
            disclosure: None,
            inject_body_metadata: None,
            annotate_tool_descriptions: None,
            max_body_rewrite_bytes: None,
            task_grace_minutes: Some(60),
            identify_yourself_at: None,
            brownouts: None,
            exemptions: None,
            emit_violation: None,
            cache_clamp_seconds: None,
        }
    }

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn phases_follow_dates() {
        let s = Settings::from_config(&cfg()).unwrap();
        assert_eq!(s.phase_at(t("2026-11-30T23:59:59Z")), Phase::Active);
        assert_eq!(s.phase_at(t("2026-12-01T00:00:00Z")), Phase::HeadsUp);
        assert_eq!(s.phase_at(t("2027-01-01T00:00:00Z")), Phase::Tombstone);
        assert_eq!(s.phase_at(t("2027-01-15T00:00:00Z")), Phase::PastSwitchOff);
    }

    #[test]
    fn offsets_are_normalised() {
        let mut c = cfg();
        c.deprecation_at = "2026-12-01T01:00:00+01:00".into();
        let s = Settings::from_config(&c).unwrap();
        assert_eq!(s.deprecation_at, t("2026-12-01T00:00:00Z"));
    }

    #[test]
    fn rejects_dates_without_offset_and_wrong_order() {
        let mut c = cfg();
        c.deprecation_at = "2026-12-01".into();
        assert!(Settings::from_config(&c).is_err());
        let mut c = cfg();
        c.sunset_at = "2026-11-01T00:00:00Z".into();
        let errs = Settings::from_config(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("before sunsetAt")));
    }

    #[test]
    fn brownout_inside_heads_up_only() {
        let mut c = cfg();
        c.brownouts = Some(vec![Brownouts0Config { start: "2026-12-10T09:00:00Z".into(), duration_minutes: 60 }]);
        let s = Settings::from_config(&c).unwrap();
        assert_eq!(s.phase_at(t("2026-12-10T09:30:00Z")), Phase::Brownout);
        assert_eq!(s.phase_at(t("2026-12-10T10:00:00Z")), Phase::HeadsUp);

        let mut c = cfg();
        c.brownouts = Some(vec![Brownouts0Config { start: "2026-12-31T23:30:00Z".into(), duration_minutes: 60 }]);
        assert!(Settings::from_config(&c).is_err());
    }

    #[test]
    fn exemptions_need_identity_and_are_capped() {
        let mut c = cfg();
        c.exemptions = Some(vec![Exemptions0Config {
            client_id: "payroll".into(),
            until: "2099-01-01T00:00:00Z".into(),
            reason: Some("CHG-1234".into()),
        }]);
        let s = Settings::from_config(&c).unwrap();
        assert_eq!(s.exemptions[0].until, s.switch_off_at);
        let now = t("2027-01-05T00:00:00Z");
        assert!(matches!(decide(&s, now, Some("payroll"), CallKind::NewWork), Decision::HeadsUp { exempt: true, .. }));
        assert!(matches!(decide(&s, now, None, CallKind::NewWork), Decision::Tombstone { .. }));
        assert!(matches!(decide(&s, now, Some("other"), CallKind::NewWork), Decision::Tombstone { .. }));
    }

    #[test]
    fn task_follow_ups_get_grace() {
        let s = Settings::from_config(&cfg()).unwrap();
        let in_grace = t("2027-01-01T00:30:00Z");
        let after = t("2027-01-01T01:00:01Z");
        assert!(matches!(decide(&s, in_grace, None, CallKind::TaskFollowUp), Decision::HeadsUp { .. }));
        assert!(matches!(decide(&s, in_grace, None, CallKind::NewWork), Decision::Tombstone { .. }));
        assert!(matches!(decide(&s, after, None, CallKind::TaskFollowUp), Decision::Tombstone { .. }));
    }

    #[test]
    fn identify_yourself_tombstones_anonymous_early() {
        let mut c = cfg();
        c.identify_yourself_at = Some("2026-12-15T00:00:00Z".into());
        let s = Settings::from_config(&c).unwrap();
        let now = t("2026-12-20T00:00:00Z");
        assert!(matches!(decide(&s, now, None, CallKind::NewWork), Decision::Tombstone { why: "unidentified-client" }));
        assert!(matches!(decide(&s, now, Some("app"), CallKind::NewWork), Decision::HeadsUp { exempt: false, .. }));
    }

    #[test]
    fn clean_strips_header_injection() {
        let v = "ok\r\nset-cookie: x".to_string();
        assert_eq!(clean(Some(&v), 100), "okset-cookie: x");
    }
}
