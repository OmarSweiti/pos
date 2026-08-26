//! Pure time values, business-date rules, and injected clock ports.
//!
//! This module never acquires either a wall clock or a monotonic clock. The
//! shell supplies both readings as values (I-8), and no timestamp produced here
//! is a causal ordering authority (I-7). Document and delivery order continue
//! to come from repository-owned sequences.

use core::fmt;

use serde::{Deserialize, Deserializer, Serialize};

const MILLIS_PER_SECOND: i64 = 1_000;
const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;

// These are Jiff's stable millisecond boundaries. Keeping the pure value inside
// the shell resolver's range means every valid `Timestamp` can be converted to
// a zoned instant without a second, adapter-only notion of valid time.
const MIN_EPOCH_MILLISECONDS: i64 = -377_705_023_201_000;
const MAX_EPOCH_MILLISECONDS: i64 = 253_402_207_200_000;

/// A UTC instant, as integer milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// The earliest instant accepted by both the domain and the shell's Jiff
    /// resolver.
    pub const MIN: Timestamp = Timestamp(MIN_EPOCH_MILLISECONDS);

    /// The latest instant accepted by both the domain and the shell's Jiff
    /// resolver.
    pub const MAX: Timestamp = Timestamp(MAX_EPOCH_MILLISECONDS);

    /// Construct a timestamp after checking the shared representable range.
    pub const fn from_epoch_milliseconds(epoch_milliseconds: i64) -> Result<Timestamp, TimeError> {
        if epoch_milliseconds < MIN_EPOCH_MILLISECONDS
            || epoch_milliseconds > MAX_EPOCH_MILLISECONDS
        {
            return Err(TimeError::OutOfRange(epoch_milliseconds));
        }
        Ok(Timestamp(epoch_milliseconds))
    }

    /// Return the UTC milliseconds carried by this value.
    pub const fn epoch_milliseconds(self) -> i64 {
        self.0
    }

    /// Render the canonical UTC storage form, always with millisecond precision.
    pub fn to_iso8601(self) -> String {
        let day = self.0.div_euclid(MILLIS_PER_DAY);
        let within_day = self.0.rem_euclid(MILLIS_PER_DAY);
        let (year, month, date) = civil_from_days(day);
        let hour = within_day / MILLIS_PER_HOUR;
        let within_hour = within_day % MILLIS_PER_HOUR;
        let minute = within_hour / MILLIS_PER_MINUTE;
        let within_minute = within_hour % MILLIS_PER_MINUTE;
        let second = within_minute / MILLIS_PER_SECOND;
        let millisecond = within_minute % MILLIS_PER_SECOND;

        format!(
            "{}-{month:02}-{date:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}Z",
            format_year(year),
        )
    }

    /// Parse the canonical ISO-8601 UTC form emitted by [`Timestamp::to_iso8601`].
    pub fn parse_iso8601(input: &str) -> Result<Timestamp, TimeError> {
        let parse_error = || TimeError::Parse(input.to_owned());
        let (date, time) = input.split_once('T').ok_or_else(parse_error)?;
        if time.contains('T') {
            return Err(parse_error());
        }
        let time = time.strip_suffix('Z').ok_or_else(parse_error)?;
        let (year, month, day) = parse_date_parts(date).ok_or_else(parse_error)?;
        if !is_calendar_date(i64::from(year), month, day) {
            return Err(TimeError::NotACalendarDate(year, month, day));
        }

        let mut hms = time.split(':');
        let hour = hms
            .next()
            .and_then(parse_two_digits)
            .ok_or_else(parse_error)?;
        let minute = hms
            .next()
            .and_then(parse_two_digits)
            .ok_or_else(parse_error)?;
        let second_and_millis = hms.next().ok_or_else(parse_error)?;
        if hms.next().is_some() {
            return Err(parse_error());
        }
        let (second, millisecond) = second_and_millis.split_once('.').ok_or_else(parse_error)?;
        let second = parse_two_digits(second).ok_or_else(parse_error)?;
        let millisecond = parse_three_digits(millisecond).ok_or_else(parse_error)?;
        if hour >= 24 || minute >= 60 || second >= 60 {
            return Err(parse_error());
        }

        let days = i128::from(days_from_civil(i64::from(year), month, day));
        let milliseconds = days * i128::from(MILLIS_PER_DAY)
            + i128::from(hour) * i128::from(MILLIS_PER_HOUR)
            + i128::from(minute) * i128::from(MILLIS_PER_MINUTE)
            + i128::from(second) * i128::from(MILLIS_PER_SECOND)
            + i128::from(millisecond);
        let milliseconds = i64::try_from(milliseconds)
            .map_err(|_| TimeError::OutOfRange(saturating_i128_to_i64(milliseconds)))?;
        Timestamp::from_epoch_milliseconds(milliseconds)
    }

    fn saturating_add_milliseconds(self, milliseconds: i64) -> Timestamp {
        timestamp_from_i128_saturating(i128::from(self.0) + i128::from(milliseconds))
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let milliseconds = i64::deserialize(deserializer)?;
        Timestamp::from_epoch_milliseconds(milliseconds).map_err(serde::de::Error::custom)
    }
}

impl From<Timestamp> for i64 {
    fn from(value: Timestamp) -> Self {
        value.epoch_milliseconds()
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_iso8601())
    }
}

/// An injected UTC clock. Implementations live outside the pure domain crate.
pub trait Clock {
    /// Return the source's current UTC reading.
    fn now(&self) -> Timestamp;
}

/// A deterministic clock for server code, fixtures, and integration tests.
///
/// It is deliberately available in normal builds, just like `SeqIdSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock {
    now: Timestamp,
}

impl FixedClock {
    /// Construct a clock frozen at `now`.
    pub const fn new(now: Timestamp) -> FixedClock {
        FixedClock { now }
    }

    /// Move the deterministic source to another caller-supplied instant.
    pub fn set(&mut self, now: Timestamp) {
        self.now = now;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.now
    }
}

/// A store-local trading day, distinct from a wall-clock date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BusinessDate {
    y: i16,
    m: u8,
    d: u8,
}

impl BusinessDate {
    /// Construct a real proleptic-Gregorian calendar date.
    pub fn new(y: i16, m: u8, d: u8) -> Result<BusinessDate, TimeError> {
        if !is_calendar_date(i64::from(y), m, d) {
            return Err(TimeError::NotACalendarDate(y, m, d));
        }
        Ok(BusinessDate { y, m, d })
    }

    /// Calendar year.
    pub const fn year(self) -> i16 {
        self.y
    }

    /// Calendar month, from 1 through 12.
    pub const fn month(self) -> u8 {
        self.m
    }

    /// Day of month, from 1 through that month's last day.
    pub const fn day(self) -> u8 {
        self.d
    }

    /// Render `YYYY-MM-DD` (or ISO expanded-year form outside four digits).
    pub fn to_iso(self) -> String {
        format!(
            "{}-{:02}-{:02}",
            format_year(i64::from(self.y)),
            self.m,
            self.d
        )
    }

    /// Parse a calendar date and validate its Gregorian day/month combination.
    pub fn parse(input: &str) -> Result<BusinessDate, TimeError> {
        let (year, month, day) =
            parse_date_parts(input).ok_or_else(|| TimeError::ParseDate(input.to_owned()))?;
        BusinessDate::new(year, month, day)
    }

    /// The following calendar date.
    ///
    /// The `i16` ceiling has no successor; at that unreachable-for-`Timestamp`
    /// endpoint this total API remains at the ceiling instead of panicking.
    pub fn succ(self) -> BusinessDate {
        let last_day = days_in_month(i64::from(self.y), self.m);
        if self.d < last_day {
            return BusinessDate {
                d: self.d + 1,
                ..self
            };
        }
        if self.m < 12 {
            return BusinessDate {
                y: self.y,
                m: self.m + 1,
                d: 1,
            };
        }
        BusinessDate {
            y: self.y.saturating_add(1),
            m: if self.y == i16::MAX { 12 } else { 1 },
            d: if self.y == i16::MAX { 31 } else { 1 },
        }
    }
}

impl<'de> Deserialize<'de> for BusinessDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct BusinessDateWire {
            y: i16,
            m: u8,
            d: u8,
        }

        let wire = BusinessDateWire::deserialize(deserializer)?;
        BusinessDate::new(wire.y, wire.m, wire.d).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for BusinessDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_iso())
    }
}

/// The already-resolved local-time rule for one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayBoundary {
    utc_offset_minutes: i16,
    cutover_minutes: u16,
}

impl DayBoundary {
    /// The default 04:00 local trading-day boundary.
    pub const DEFAULT_CUTOVER_MINUTES: u16 = 240;

    /// Jiff's supported whole-minute offset range. IANA zones occupy a narrower
    /// subset, but the shell remains the authority that resolves the zone.
    const MIN_UTC_OFFSET_MINUTES: i16 = -1_559;

    /// Jiff's supported whole-minute offset range.
    const MAX_UTC_OFFSET_MINUTES: i16 = 1_559;

    /// Validate a resolved UTC offset and a local minute-of-day cutover.
    pub fn new(utc_offset_minutes: i16, cutover_minutes: u16) -> Result<DayBoundary, TimeError> {
        if !(Self::MIN_UTC_OFFSET_MINUTES..=Self::MAX_UTC_OFFSET_MINUTES)
            .contains(&utc_offset_minutes)
        {
            return Err(TimeError::BadOffset(utc_offset_minutes));
        }
        if cutover_minutes >= 1_440 {
            return Err(TimeError::BadCutover(cutover_minutes));
        }
        Ok(DayBoundary {
            utc_offset_minutes,
            cutover_minutes,
        })
    }

    /// The zone offset resolved by the shell for the instant being converted.
    pub const fn utc_offset_minutes(self) -> i16 {
        self.utc_offset_minutes
    }

    /// The store's local trading-day cutover, as minutes after midnight.
    pub const fn cutover_minutes(self) -> u16 {
        self.cutover_minutes
    }
}

/// Resolve the trading day of a shift opening.
///
/// `boundary.utc_offset_minutes` is already resolved for `opened_at` from the
/// store's IANA zone. This pure function does not know or acquire a zone.
pub fn business_date_of(opened_at: Timestamp, boundary: DayBoundary) -> BusinessDate {
    let local_milliseconds = i128::from(opened_at.epoch_milliseconds())
        + i128::from(boundary.utc_offset_minutes()) * i128::from(MILLIS_PER_MINUTE);
    let local_day = local_milliseconds.div_euclid(i128::from(MILLIS_PER_DAY));
    let within_day = local_milliseconds.rem_euclid(i128::from(MILLIS_PER_DAY));
    let local_minute = within_day / i128::from(MILLIS_PER_MINUTE);
    let business_day = if local_minute < i128::from(boundary.cutover_minutes()) {
        local_day - 1
    } else {
        local_day
    };
    let business_day = saturating_i128_to_i64(business_day);
    let (year, month, day) = civil_from_days(business_day);

    // Timestamp's bounded range plus Jiff's maximum offset can only produce
    // years around -10_000..=10_000, comfortably inside i16.
    let year = match i16::try_from(year) {
        Ok(year) => year,
        Err(_) if year.is_negative() => i16::MIN,
        Err(_) => i16::MAX,
    };
    BusinessDate {
        y: year,
        m: month,
        d: day,
    }
}

/// A detected clock discontinuity, suitable for an append-only audit entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockAnomaly {
    /// The wall clock moved before the last timestamp already emitted.
    JumpedBack { by_ms: i64, at: Timestamp },
    /// The wall clock moved implausibly far forward.
    JumpedForward { by_ms: i64, at: Timestamp },
    /// Boot-monotonic continuity was lost before a new trust anchor existed.
    MonotonicReset { at: Timestamp },
}

/// A wall-clock wrapper that never emits a value below its high-water mark.
#[derive(Debug)]
pub struct MonotonicClock<C: Clock> {
    source: C,
    last_emitted: Option<Timestamp>,
}

impl<C: Clock> MonotonicClock<C> {
    /// Start with no previously emitted timestamp.
    pub fn new(source: C) -> MonotonicClock<C> {
        MonotonicClock {
            source,
            last_emitted: None,
        }
    }

    /// Restore a persisted high-water timestamp before reading the source.
    pub fn with_high_water(source: C, high_water: Timestamp) -> MonotonicClock<C> {
        MonotonicClock {
            source,
            last_emitted: Some(high_water),
        }
    }

    /// Mutably access a deterministic or shell-owned source.
    pub fn source_mut(&mut self) -> &mut C {
        &mut self.source
    }

    /// Read the injected source, clamping a backward jump to the high water.
    pub fn now(&mut self) -> (Timestamp, Option<ClockAnomaly>) {
        let observed = self.source.now();
        let Some(previous) = self.last_emitted else {
            self.last_emitted = Some(observed);
            return (observed, None);
        };
        if observed >= previous {
            self.last_emitted = Some(observed);
            return (observed, None);
        }

        let by_ms = saturating_i128_to_i64(
            i128::from(previous.epoch_milliseconds()) - i128::from(observed.epoch_milliseconds()),
        );
        (
            previous,
            Some(ClockAnomaly::JumpedBack {
                by_ms,
                at: observed,
            }),
        )
    }
}

/// The register's persisted assessment of its clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    /// The last server-authenticated UTC instant, or `None` before first trust.
    pub last_trusted_at: Option<Timestamp>,
    /// The device wall clock captured beside `last_trusted_at`.
    pub device_at_trust: Option<Timestamp>,
    /// The boot-monotonic counter reading captured at the trusted instant.
    /// Subtracting it from the caller's current reading projects elapsed time;
    /// a smaller current reading proves that boot continuity was lost.
    pub monotonic_since_trust_ms: Option<i64>,
    /// The largest timestamp this register has ever issued (E.6).
    pub high_water: Timestamp,
    /// An unresolved discontinuity retained for audit and confidence policy.
    pub anomaly: Option<ClockAnomaly>,
}

impl ClockState {
    /// Record shell-observed loss of boot-monotonic continuity.
    ///
    /// A bare counter cannot identify its boot: a new boot can eventually
    /// reach a value larger than the old anchor. The shell therefore calls
    /// this transition when its boot identity changes. Clearing the monotonic
    /// anchor forces device-time fallback until the next authenticated anchor.
    pub fn note_monotonic_reset(&mut self, at: Timestamp) {
        self.monotonic_since_trust_ms = None;
        self.anomaly = Some(ClockAnomaly::MonotonicReset { at });
    }
}

/// How much confidence the register has in time-dependent decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockConfidence {
    /// Device progression agrees with the monotonic trusted projection.
    Trusted,
    /// The projection is consistent but older than the policy permits silently.
    Stale { age_ms: i64 },
    /// Progression disagrees, an anchor is incomplete, or monotonic time reset.
    Suspect { skew_ms: i64 },
    /// No server or provisioning flow has authenticated time yet.
    Untrusted,
}

impl ClockConfidence {
    /// Clock confidence never closes a till that has a queue.
    pub const fn permits_sale(self) -> bool {
        true
    }

    /// Every confidence level permits opening a shift.
    pub const fn permits_shift_open(self) -> bool {
        true
    }

    /// Suspect and never-trusted clocks require an audited operator choice.
    pub const fn requires_business_date_confirmation(self) -> bool {
        matches!(
            self,
            ClockConfidence::Suspect { .. } | ClockConfidence::Untrusted
        )
    }

    /// Every level below trusted is visible to the operator.
    pub const fn raises_time_alarm(self) -> bool {
        !matches!(self, ClockConfidence::Trusted)
    }

    /// Current 2.7.0-owned default: clearance waits for authenticated time.
    ///
    /// This is deliberately policy, not a claim that the open fiscal question
    /// has been settled.
    pub const fn defers_fiscal_issue_date(self) -> bool {
        matches!(
            self,
            ClockConfidence::Suspect { .. } | ClockConfidence::Untrusted
        )
    }
}

/// Thresholds for clock-confidence classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockPolicy {
    /// Maximum device-versus-monotonic progression skew.
    pub tolerance_ms: i64,
    /// Maximum age of a consistent authenticated anchor before an alarm.
    pub max_trust_age_ms: i64,
}

impl Default for ClockPolicy {
    fn default() -> ClockPolicy {
        ClockPolicy {
            tolerance_ms: 120_000,
            max_trust_age_ms: 604_800_000,
        }
    }
}

/// Classify the device clock against its authenticated monotonic projection.
pub fn clock_confidence(
    state: &ClockState,
    device_now: Timestamp,
    monotonic_now_ms: i64,
    policy: &ClockPolicy,
) -> ClockConfidence {
    let Some(last_trusted_at) = state.last_trusted_at else {
        return ClockConfidence::Untrusted;
    };
    if state.device_at_trust.is_none() {
        return ClockConfidence::Suspect { skew_ms: 0 };
    }
    let Some(monotonic_at_trust_ms) = state.monotonic_since_trust_ms else {
        return ClockConfidence::Suspect { skew_ms: 0 };
    };
    if monotonic_at_trust_ms < 0 || monotonic_now_ms < monotonic_at_trust_ms {
        return ClockConfidence::Suspect { skew_ms: 0 };
    }

    let age_ms = monotonic_now_ms - monotonic_at_trust_ms;
    let projected_trusted_now = last_trusted_at.saturating_add_milliseconds(age_ms);
    let skew_ms = timestamp_difference(device_now, projected_trusted_now);

    if let Some(anomaly) = state.anomaly {
        let anomaly_skew = match anomaly {
            ClockAnomaly::JumpedBack { by_ms, .. } => by_ms.saturating_abs().saturating_neg(),
            ClockAnomaly::JumpedForward { by_ms, .. } => by_ms.saturating_abs(),
            ClockAnomaly::MonotonicReset { .. } => skew_ms,
        };
        return ClockConfidence::Suspect {
            skew_ms: anomaly_skew,
        };
    }

    let tolerance_ms = i128::from(policy.tolerance_ms.max(0));
    if i128::from(skew_ms).abs() > tolerance_ms {
        return ClockConfidence::Suspect { skew_ms };
    }
    if age_ms > policy.max_trust_age_ms.max(0) {
        return ClockConfidence::Stale { age_ms };
    }

    ClockConfidence::Trusted
}

/// The time used by business rules: trusted projection when continuous,
/// device time otherwise, and never below the persisted high water.
pub fn effective_now(
    state: &ClockState,
    device_now: Timestamp,
    monotonic_now_ms: i64,
) -> Timestamp {
    let monotonic_was_reset = matches!(state.anomaly, Some(ClockAnomaly::MonotonicReset { .. }));
    let projected = match (state.last_trusted_at, state.monotonic_since_trust_ms) {
        (Some(last_trusted_at), Some(monotonic_at_trust_ms))
            if !monotonic_was_reset
                && monotonic_at_trust_ms >= 0
                && monotonic_now_ms >= monotonic_at_trust_ms =>
        {
            last_trusted_at.saturating_add_milliseconds(monotonic_now_ms - monotonic_at_trust_ms)
        }
        _ => device_now,
    };
    projected.max(state.high_water)
}

/// Exhaustive validation and parsing errors for pure time values.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TimeError {
    /// The UTC timestamp text is not canonical ISO-8601 with milliseconds.
    #[error("cannot parse {0:?} as an ISO-8601 UTC timestamp")]
    Parse(String),
    /// The calendar-date text is not canonical ISO form.
    #[error("cannot parse {0:?} as a YYYY-MM-DD date")]
    ParseDate(String),
    /// The epoch value cannot be represented by both domain and shell.
    #[error("timestamp {0} is outside the representable range")]
    OutOfRange(i64),
    /// The supplied year/month/day combination is not Gregorian.
    #[error("{0}-{1}-{2} is not a real calendar date")]
    NotACalendarDate(i16, u8, u8),
    /// The supplied offset exceeds Jiff's real offset range.
    #[error("utc offset {0} minutes is not a real offset")]
    BadOffset(i16),
    /// The cutover is not a minute within a local calendar day.
    #[error("cutover {0} minutes is not inside a day")]
    BadCutover(u16),
}

fn timestamp_difference(left: Timestamp, right: Timestamp) -> i64 {
    saturating_i128_to_i64(
        i128::from(left.epoch_milliseconds()) - i128::from(right.epoch_milliseconds()),
    )
}

fn timestamp_from_i128_saturating(milliseconds: i128) -> Timestamp {
    let bounded = milliseconds.clamp(
        i128::from(MIN_EPOCH_MILLISECONDS),
        i128::from(MAX_EPOCH_MILLISECONDS),
    );
    Timestamp(saturating_i128_to_i64(bounded))
}

fn saturating_i128_to_i64(value: i128) -> i64 {
    if value < i128::from(i64::MIN) {
        i64::MIN
    } else if value > i128::from(i64::MAX) {
        i64::MAX
    } else {
        value as i64
    }
}

fn parse_two_digits(input: &str) -> Option<u8> {
    if input.len() != 2 || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    input.parse().ok()
}

fn parse_three_digits(input: &str) -> Option<u16> {
    if input.len() != 3 || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    input.parse().ok()
}

fn parse_date_parts(input: &str) -> Option<(i16, u8, u8)> {
    let mut parts = input.rsplitn(3, '-');
    let day = parts.next().and_then(parse_two_digits)?;
    let month = parts.next().and_then(parse_two_digits)?;
    let year = parts.next()?;
    let year_text = year;
    let (sign, digits) = if let Some(digits) = year_text.strip_prefix('-') {
        (-1i32, digits)
    } else if let Some(digits) = year_text.strip_prefix('+') {
        (1i32, digits)
    } else {
        (1i32, year_text)
    };
    if digits.len() < 4 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if sign > 0 && !year_text.starts_with('+') && digits.len() != 4 {
        return None;
    }
    let magnitude = digits.parse::<i32>().ok()?;
    let signed = magnitude.checked_mul(sign)?;
    let year = i16::try_from(signed).ok()?;
    if format_year(i64::from(year)) != year_text {
        return None;
    }
    Some((year, month, day))
}

fn format_year(year: i64) -> String {
    if (0..=9_999).contains(&year) {
        format!("{year:04}")
    } else if year.is_negative() {
        format!("-{:04}", year.saturating_abs())
    } else {
        format!("+{year:04}")
    }
}

const fn is_leap_year(year: i64) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

const fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_calendar_date(year: i64, month: u8, day: u8) -> bool {
    day > 0 && day <= days_in_month(year, month)
}

/// Days since 1970-01-01 from a proleptic-Gregorian date.
fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Proleptic-Gregorian date from days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u8, day as u8)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;

    use super::*;

    const BASE_MILLIS: i64 = 1_767_225_600_000; // 2026-01-01T00:00:00.000Z
    const YEAR_MILLIS: i64 = 365 * MILLIS_PER_DAY;

    // Covers every ordinal day across common and leap years, the full validated
    // offset range (with common IANA values biased in), every cutover including
    // midnight/04:00/end-of-day, and any two minutes in one cutover-to-cutover
    // trading window. It deliberately excludes an offset transition inside the
    // window: the shell resolves each instant separately, and that adapter rule
    // has its own tests.
    fn business_day_cases() -> impl Strategy<Value = (i16, u16, i16, u16, u16, u16)> {
        (
            prop_oneof![Just(2023i16), Just(2024i16), Just(2025i16), Just(2026i16)],
            prop_oneof![
                Just(0u16),
                Just(58),
                Just(59),
                Just(364),
                Just(365),
                0u16..=365
            ],
            prop_oneof![
                Just(DayBoundary::MIN_UTC_OFFSET_MINUTES),
                Just(-720i16),
                Just(0i16),
                Just(180i16),
                Just(840i16),
                Just(DayBoundary::MAX_UTC_OFFSET_MINUTES),
                DayBoundary::MIN_UTC_OFFSET_MINUTES..=DayBoundary::MAX_UTC_OFFSET_MINUTES,
            ],
            prop_oneof![Just(0u16), Just(1), Just(240), Just(1_439), 0u16..1_440],
            prop_oneof![Just(0u16), Just(1), Just(239), Just(1_439), 0u16..1_440],
            prop_oneof![Just(0u16), Just(1), Just(240), Just(1_439), 0u16..1_440],
        )
            .prop_filter(
                "ordinal exists in the generated year",
                |(year, ordinal, ..)| {
                    *ordinal
                        < if is_leap_year(i64::from(*year)) {
                            366
                        } else {
                            365
                        }
                },
            )
    }

    // Covers every day of common and leap years, all validated offsets, and
    // every cutover with explicit bias at midnight, 00:01, 04:00 and 23:59. It
    // deliberately excludes sub-millisecond instants because Timestamp stores
    // integer milliseconds and no finer distinction exists in the product.
    fn cutover_boundary_cases() -> impl Strategy<Value = (i16, u16, i16, u16)> {
        (
            prop_oneof![Just(2023i16), Just(2024i16), Just(2025i16), Just(2026i16)],
            prop_oneof![
                Just(0u16),
                Just(58),
                Just(59),
                Just(364),
                Just(365),
                0u16..=365
            ],
            prop_oneof![
                Just(DayBoundary::MIN_UTC_OFFSET_MINUTES),
                Just(-720i16),
                Just(0i16),
                Just(180i16),
                Just(840i16),
                Just(DayBoundary::MAX_UTC_OFFSET_MINUTES),
                DayBoundary::MIN_UTC_OFFSET_MINUTES..=DayBoundary::MAX_UTC_OFFSET_MINUTES,
            ],
            prop_oneof![Just(0u16), Just(1), Just(240), Just(1_439), 0u16..1_440],
        )
            .prop_filter(
                "ordinal exists in the generated year",
                |(year, ordinal, ..)| {
                    *ordinal
                        < if is_leap_year(i64::from(*year)) {
                            366
                        } else {
                            365
                        }
                },
            )
    }

    // Covers sequences of equal, forward and backward wall-clock readings over
    // a two-year span, including zero and one-millisecond jumps. It deliberately
    // excludes concurrent callers: this value guard owns emitted order, while
    // thread scheduling and causal order belong to repository sequences (I-7).
    fn clock_reading_sequences() -> impl Strategy<Value = (i64, Vec<i64>)> {
        (
            (BASE_MILLIS - YEAR_MILLIS)..=(BASE_MILLIS + YEAR_MILLIS),
            prop::collection::vec(
                prop_oneof![
                    Just(0i64),
                    Just(-1i64),
                    Just(1i64),
                    -YEAR_MILLIS..=YEAR_MILLIS,
                ],
                1..=64,
            ),
        )
    }

    #[derive(Debug)]
    struct EffectiveNowCase {
        device_delta: i64,
        high_water_delta: i64,
        monotonic_now_ms: i64,
        anchor_kind: u8,
        anomaly_kind: u8,
    }

    // Covers device and high-water values on either side of one another,
    // trusted, absent and reset/future monotonic anchors, and all anomaly kinds.
    // It deliberately excludes invalid Timestamp encodings, which validated
    // construction and deserialization test separately.
    fn effective_now_cases() -> impl Strategy<Value = EffectiveNowCase> {
        (
            -YEAR_MILLIS..=YEAR_MILLIS,
            -YEAR_MILLIS..=YEAR_MILLIS,
            0i64..=YEAR_MILLIS,
            0u8..=3,
            0u8..=3,
        )
            .prop_map(
                |(device_delta, high_water_delta, monotonic_now_ms, anchor_kind, anomaly_kind)| {
                    EffectiveNowCase {
                        device_delta,
                        high_water_delta,
                        monotonic_now_ms,
                        anchor_kind,
                        anomaly_kind,
                    }
                },
            )
    }

    // Covers trusted and stale ages, both skew directions, the exact tolerance
    // boundary and ordered absolute skews from zero through one year. It
    // deliberately excludes never-trusted and reset states because skew
    // monotonicity is undefined when no trusted projection exists.
    fn confidence_skew_cases() -> impl Strategy<Value = (i64, i64, i64, bool)> {
        (
            prop_oneof![
                Just(0i64),
                Just(120_000i64),
                Just(604_800_000i64),
                Just(604_800_001i64),
                0i64..=YEAR_MILLIS,
            ],
            prop_oneof![Just(0i64), Just(120_000i64), 0i64..=YEAR_MILLIS],
            prop_oneof![Just(120_001i64), Just(YEAR_MILLIS), 0i64..=YEAR_MILLIS],
            any::<bool>(),
        )
            .prop_map(|(age, first, second, negative)| {
                (age, first.min(second), first.max(second), negative)
            })
    }

    fn timestamp(input: &str) -> Timestamp {
        Timestamp::parse_iso8601(input).unwrap()
    }

    fn trusted_state(anchor: Timestamp) -> ClockState {
        ClockState {
            last_trusted_at: Some(anchor),
            device_at_trust: Some(anchor),
            monotonic_since_trust_ms: Some(10_000),
            high_water: anchor,
            anomaly: None,
        }
    }

    #[test]
    fn timestamp_round_trips_canonical_utc_milliseconds() {
        for text in [
            "0001-01-01T00:00:00.000Z",
            "1969-12-31T23:59:59.999Z",
            "1970-01-01T00:00:00.000Z",
            "2000-02-29T23:59:59.999Z",
            "2026-08-20T07:15:22.418Z",
            "9999-12-30T22:00:00.000Z",
        ] {
            let parsed = Timestamp::parse_iso8601(text).unwrap();
            assert_eq!(parsed.to_iso8601(), text);
            assert_eq!(Timestamp::parse_iso8601(&parsed.to_string()), Ok(parsed));
        }
    }

    #[test]
    fn timestamps_match_independent_epoch_and_calendar_anchors() {
        assert_eq!(
            Timestamp::from_epoch_milliseconds(0).unwrap().to_iso8601(),
            "1970-01-01T00:00:00.000Z"
        );
        assert_eq!(
            Timestamp::from_epoch_milliseconds(-1).unwrap().to_iso8601(),
            "1969-12-31T23:59:59.999Z"
        );
        assert_eq!(
            timestamp("2026-01-01T00:00:00.000Z").epoch_milliseconds(),
            BASE_MILLIS
        );
        assert_eq!(
            Timestamp::parse_iso8601(&Timestamp::MIN.to_iso8601()),
            Ok(Timestamp::MIN)
        );
        assert_eq!(
            Timestamp::parse_iso8601(&Timestamp::MAX.to_iso8601()),
            Ok(Timestamp::MAX)
        );
        assert!(BusinessDate::new(2000, 2, 29).is_ok());
        assert_eq!(
            BusinessDate::new(1900, 2, 29),
            Err(TimeError::NotACalendarDate(1900, 2, 29))
        );
        assert_eq!(
            BusinessDate::new(2100, 2, 29),
            Err(TimeError::NotACalendarDate(2100, 2, 29))
        );
    }

    #[test]
    fn invalid_time_values_return_named_errors() {
        assert_eq!(
            Timestamp::parse_iso8601("2026-02-31T00:00:00.000Z"),
            Err(TimeError::NotACalendarDate(2026, 2, 31))
        );
        assert_eq!(
            Timestamp::parse_iso8601("2026-01-01T24:00:00.000Z"),
            Err(TimeError::Parse("2026-01-01T24:00:00.000Z".to_owned()))
        );
        assert_eq!(
            Timestamp::parse_iso8601("+0001-01-01T00:00:00.000Z"),
            Err(TimeError::Parse("+0001-01-01T00:00:00.000Z".to_owned()))
        );
        assert_eq!(
            BusinessDate::parse("-0000-01-01"),
            Err(TimeError::ParseDate("-0000-01-01".to_owned()))
        );
        assert_eq!(
            Timestamp::from_epoch_milliseconds(i64::MAX),
            Err(TimeError::OutOfRange(i64::MAX))
        );
    }

    #[test]
    fn business_date_construction_refuses_february_31() {
        assert_eq!(
            BusinessDate::new(2026, 2, 31),
            Err(TimeError::NotACalendarDate(2026, 2, 31))
        );
        assert_eq!(
            BusinessDate::parse("2026-02-31"),
            Err(TimeError::NotACalendarDate(2026, 2, 31))
        );
        assert_eq!(
            serde_json::from_str::<BusinessDate>(r#"{"y":2026,"m":2,"d":31}"#)
                .unwrap_err()
                .to_string(),
            "2026-2-31 is not a real calendar date"
        );
        assert_eq!(
            BusinessDate::new(2024, 2, 29).unwrap().to_iso(),
            "2024-02-29"
        );
        assert_eq!(
            BusinessDate::new(i16::MAX, 12, 31).unwrap().succ(),
            BusinessDate::new(i16::MAX, 12, 31).unwrap()
        );
    }

    #[test]
    fn day_boundary_construction_refuses_corrupt_settings() {
        assert_eq!(
            DayBoundary::new(4_000, DayBoundary::DEFAULT_CUTOVER_MINUTES),
            Err(TimeError::BadOffset(4_000))
        );
        assert_eq!(
            DayBoundary::new(180, 1_440),
            Err(TimeError::BadCutover(1_440))
        );
        assert_eq!(
            DayBoundary::new(180, DayBoundary::DEFAULT_CUTOVER_MINUTES)
                .unwrap()
                .utc_offset_minutes(),
            180
        );
    }

    #[test]
    fn shift_opened_at_0030_belongs_to_previous_day() {
        let opened_at = timestamp("2026-08-20T21:30:00.000Z");
        let boundary = DayBoundary::new(180, 240).unwrap();
        assert_eq!(
            business_date_of(opened_at, boundary),
            BusinessDate::new(2026, 8, 20).unwrap()
        );
    }

    #[test]
    fn clock_jump_back_reports_anomaly() {
        let first = Timestamp::from_epoch_milliseconds(1_000).unwrap();
        let jumped_to = Timestamp::from_epoch_milliseconds(900).unwrap();
        let mut guarded = MonotonicClock::new(FixedClock::new(first));
        assert_eq!(guarded.now(), (first, None));

        guarded.source_mut().set(jumped_to);
        assert_eq!(
            guarded.now(),
            (
                first,
                Some(ClockAnomaly::JumpedBack {
                    by_ms: 100,
                    at: jumped_to
                })
            )
        );
    }

    #[test]
    fn a_never_synced_register_is_untrusted_not_trusted() {
        let now = timestamp("2026-08-20T07:15:22.418Z");
        let state = ClockState {
            last_trusted_at: None,
            device_at_trust: None,
            monotonic_since_trust_ms: None,
            high_water: now,
            anomaly: None,
        };
        assert_eq!(
            clock_confidence(&state, now, 50_000, &ClockPolicy::default()),
            ClockConfidence::Untrusted
        );
    }

    #[test]
    fn wall_clock_moved_forward_a_year_is_suspect() {
        let anchor = timestamp("2026-01-01T00:00:00.000Z");
        let state = trusted_state(anchor);
        let device_now = anchor.saturating_add_milliseconds(YEAR_MILLIS);
        assert!(matches!(
            clock_confidence(&state, device_now, 10_000 + MILLIS_PER_HOUR, &ClockPolicy::default()),
            ClockConfidence::Suspect { skew_ms } if skew_ms > 0
        ));
    }

    #[test]
    fn a_reboot_without_an_anchor_is_a_monotonic_reset() {
        let anchor = timestamp("2026-01-01T00:00:00.000Z");
        let device_now = anchor.saturating_add_milliseconds(MILLIS_PER_HOUR);
        let mut state = trusted_state(anchor);
        state.note_monotonic_reset(device_now);
        assert!(matches!(
            state.anomaly,
            Some(ClockAnomaly::MonotonicReset { at }) if at == device_now
        ));
        assert!(matches!(
            clock_confidence(&state, device_now, 1_000, &ClockPolicy::default()),
            ClockConfidence::Suspect { .. }
        ));
        assert_eq!(effective_now(&state, device_now, 1_000), device_now);
    }

    #[test]
    fn confidence_uses_trusted_utc_and_obeys_policy_edges() {
        let anchor = timestamp("2026-01-01T00:00:00.000Z");
        let policy = ClockPolicy::default();
        let state = trusted_state(anchor);

        assert_eq!(
            clock_confidence(&state, anchor, 10_000, &policy),
            ClockConfidence::Trusted
        );
        let at_tolerance = anchor.saturating_add_milliseconds(policy.tolerance_ms);
        assert_eq!(
            clock_confidence(&state, at_tolerance, 10_000, &policy),
            ClockConfidence::Trusted
        );
        let beyond_positive = anchor.saturating_add_milliseconds(policy.tolerance_ms + 1);
        assert_eq!(
            clock_confidence(&state, beyond_positive, 10_000, &policy),
            ClockConfidence::Suspect {
                skew_ms: policy.tolerance_ms + 1
            }
        );
        let beyond_negative = anchor.saturating_add_milliseconds(-policy.tolerance_ms - 1);
        assert_eq!(
            clock_confidence(&state, beyond_negative, 10_000, &policy),
            ClockConfidence::Suspect {
                skew_ms: -policy.tolerance_ms - 1
            }
        );

        let exactly_max_age = anchor.saturating_add_milliseconds(policy.max_trust_age_ms);
        assert_eq!(
            clock_confidence(
                &state,
                exactly_max_age,
                10_000 + policy.max_trust_age_ms,
                &policy,
            ),
            ClockConfidence::Trusted
        );
        let stale_age = policy.max_trust_age_ms + 1;
        assert_eq!(
            clock_confidence(
                &state,
                anchor.saturating_add_milliseconds(stale_age),
                10_000 + stale_age,
                &policy,
            ),
            ClockConfidence::Stale { age_ms: stale_age }
        );

        let already_wrong_device = ClockState {
            device_at_trust: Some(anchor.saturating_add_milliseconds(YEAR_MILLIS)),
            ..state
        };
        assert!(matches!(
            clock_confidence(
                &already_wrong_device,
                anchor.saturating_add_milliseconds(YEAR_MILLIS),
                10_000,
                &policy,
            ),
            ClockConfidence::Suspect { skew_ms } if skew_ms == YEAR_MILLIS
        ));
    }

    #[test]
    fn effective_now_selects_projection_fallback_then_high_water() {
        let anchor = timestamp("2026-01-01T00:00:00.000Z");
        let device_now = anchor.saturating_add_milliseconds(MILLIS_PER_HOUR);
        let projected = anchor.saturating_add_milliseconds(1_000);
        let mut state = trusted_state(anchor);

        assert_eq!(effective_now(&state, device_now, 11_000), projected);
        state.high_water = projected.saturating_add_milliseconds(1);
        assert_eq!(effective_now(&state, device_now, 11_000), state.high_water);

        state.note_monotonic_reset(device_now);
        state.high_water = anchor;
        assert_eq!(effective_now(&state, device_now, 1_000), device_now);
        state.high_water = device_now.saturating_add_milliseconds(1);
        assert_eq!(effective_now(&state, device_now, 1_000), state.high_water);
    }

    #[test]
    fn clock_policy_defaults_match_the_trust_contract() {
        assert_eq!(
            ClockPolicy::default(),
            ClockPolicy {
                tolerance_ms: 120_000,
                max_trust_age_ms: 604_800_000
            }
        );
    }

    #[test]
    fn no_clock_confidence_refuses_a_sale() {
        let levels = [
            ClockConfidence::Trusted,
            ClockConfidence::Stale {
                age_ms: 604_800_001,
            },
            ClockConfidence::Suspect { skew_ms: -120_001 },
            ClockConfidence::Untrusted,
        ];
        for confidence in levels {
            assert!(confidence.permits_sale());
            assert!(confidence.permits_shift_open());
        }

        assert!(!levels[0].raises_time_alarm());
        assert!(levels[1].raises_time_alarm());
        assert!(!levels[0].requires_business_date_confirmation());
        assert!(!levels[1].requires_business_date_confirmation());
        assert!(levels[2].requires_business_date_confirmation());
        assert!(levels[3].requires_business_date_confirmation());
        assert!(!levels[0].defers_fiscal_issue_date());
        assert!(!levels[1].defers_fiscal_issue_date());
        assert!(levels[2].defers_fiscal_issue_date());
        assert!(levels[3].defers_fiscal_issue_date());
    }

    proptest! {
        // One shared configuration for every domain property: 4,096 cases,
        // repository seed/persistence, and only a raising environment override.
        #![proptest_config(domain_proptest_config())]

        /// Every instant from one local cutover up to the next belongs to one
        /// trading date, even when that window crosses midnight or a year edge.
        #[test]
        fn prop_business_date_stable_across_shift(
            (year, ordinal, offset, cutover, first_minute, second_minute) in business_day_cases()
        ) {
            let day = days_from_civil(i64::from(year), 1, 1) + i64::from(ordinal);
            let start_local = day * MILLIS_PER_DAY + i64::from(cutover) * MILLIS_PER_MINUTE;
            let first_utc = start_local + i64::from(first_minute) * MILLIS_PER_MINUTE
                - i64::from(offset) * MILLIS_PER_MINUTE;
            let second_utc = start_local + i64::from(second_minute) * MILLIS_PER_MINUTE
                - i64::from(offset) * MILLIS_PER_MINUTE;
            let boundary = DayBoundary::new(offset, cutover).unwrap();
            let mut expected = BusinessDate::new(year, 1, 1).unwrap();
            for _ in 0..ordinal {
                expected = expected.succ();
            }

            prop_assert_eq!(
                business_date_of(Timestamp::from_epoch_milliseconds(first_utc).unwrap(), boundary),
                expected,
            );
            prop_assert_eq!(
                business_date_of(Timestamp::from_epoch_milliseconds(second_utc).unwrap(), boundary),
                expected,
            );
        }

        /// Immediately before a cutover is yesterday; at and after it is today,
        /// so advancing through a boundary changes by exactly one calendar day.
        #[test]
        fn prop_cutover_boundary_never_skips_a_day(
            (year, ordinal, offset, cutover) in cutover_boundary_cases()
        ) {
            let day = days_from_civil(i64::from(year), 1, 1) + i64::from(ordinal);
            let cutover_local = day * MILLIS_PER_DAY + i64::from(cutover) * MILLIS_PER_MINUTE;
            let cutover_utc = cutover_local - i64::from(offset) * MILLIS_PER_MINUTE;
            let boundary = DayBoundary::new(offset, cutover).unwrap();
            let before = business_date_of(
                Timestamp::from_epoch_milliseconds(cutover_utc - 1).unwrap(),
                boundary,
            );
            let at = business_date_of(
                Timestamp::from_epoch_milliseconds(cutover_utc).unwrap(),
                boundary,
            );
            let after = business_date_of(
                Timestamp::from_epoch_milliseconds(cutover_utc + 1).unwrap(),
                boundary,
            );

            prop_assert_eq!(before.succ(), at);
            prop_assert_eq!(at, after);
        }

        /// Whatever sequence the injected wall clock reports, the guarded
        /// sequence presented to callers never decreases.
        #[test]
        fn prop_monotonic_clock_never_decreases(
            (first_millis, readings) in clock_reading_sequences()
        ) {
            let first = Timestamp::from_epoch_milliseconds(first_millis).unwrap();
            let mut guarded = MonotonicClock::new(FixedClock::new(first));
            let (mut previous, _) = guarded.now();
            for delta in readings {
                let observed = Timestamp::from_epoch_milliseconds(first_millis + delta).unwrap();
                guarded.source_mut().set(observed);
                let (emitted, _) = guarded.now();
                prop_assert!(emitted >= previous, "{emitted} followed {previous}");
                previous = emitted;
            }
        }

        /// Trusted projection, device fallback and every anomaly path all obey
        /// the same persisted high-water floor.
        #[test]
        fn prop_effective_now_never_precedes_high_water(case in effective_now_cases()) {
            let base = Timestamp::from_epoch_milliseconds(BASE_MILLIS).unwrap();
            let device_now = Timestamp::from_epoch_milliseconds(BASE_MILLIS + case.device_delta)
                .unwrap();
            let high_water = Timestamp::from_epoch_milliseconds(
                BASE_MILLIS + case.high_water_delta,
            )
            .unwrap();
            let monotonic_anchor = match case.anchor_kind {
                0 => None,
                1 => Some(0),
                2 => Some(case.monotonic_now_ms.saturating_add(1)),
                _ => Some(case.monotonic_now_ms / 2),
            };
            let anomaly = match case.anomaly_kind {
                0 => None,
                1 => Some(ClockAnomaly::JumpedBack { by_ms: 1, at: device_now }),
                2 => Some(ClockAnomaly::JumpedForward { by_ms: 1, at: device_now }),
                _ => Some(ClockAnomaly::MonotonicReset { at: device_now }),
            };
            let state = ClockState {
                last_trusted_at: (case.anchor_kind != 0).then_some(base),
                device_at_trust: (case.anchor_kind != 0).then_some(base),
                monotonic_since_trust_ms: monotonic_anchor,
                high_water,
                anomaly,
            };

            prop_assert!(effective_now(&state, device_now, case.monotonic_now_ms) >= high_water);
        }

        /// At a fixed authenticated anchor and age, increasing absolute device
        /// skew can preserve or reduce confidence but can never improve it.
        #[test]
        fn prop_clock_confidence_is_monotone_in_skew(
            (age_ms, smaller_skew, larger_skew, negative) in confidence_skew_cases()
        ) {
            let anchor = Timestamp::from_epoch_milliseconds(BASE_MILLIS).unwrap();
            let state = trusted_state(anchor);
            let sign = if negative { -1 } else { 1 };
            let expected_device = BASE_MILLIS + age_ms;
            let smaller_device = Timestamp::from_epoch_milliseconds(
                expected_device + sign * smaller_skew,
            )
            .unwrap();
            let larger_device = Timestamp::from_epoch_milliseconds(
                expected_device + sign * larger_skew,
            )
            .unwrap();
            let policy = ClockPolicy::default();
            let smaller = clock_confidence(&state, smaller_device, 10_000 + age_ms, &policy);
            let larger = clock_confidence(&state, larger_device, 10_000 + age_ms, &policy);

            prop_assert!(
                confidence_severity(smaller) <= confidence_severity(larger),
                "confidence improved from {smaller:?} to {larger:?}",
            );
        }
    }

    fn confidence_severity(confidence: ClockConfidence) -> u8 {
        match confidence {
            ClockConfidence::Trusted => 0,
            ClockConfidence::Stale { .. } => 1,
            ClockConfidence::Suspect { .. } => 2,
            ClockConfidence::Untrusted => 3,
        }
    }
}
