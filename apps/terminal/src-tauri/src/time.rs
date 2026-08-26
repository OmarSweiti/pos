//! IANA time-zone resolution at the impure shell boundary.
//!
//! The store keeps a zone identifier. For each instant, this module asks the
//! shipped time-zone database for the offset then passes only integer minutes
//! to `pos-domain`; time-zone rules and I/O never enter the pure crate.

use jiff::{Timestamp as JiffTimestamp, tz::TimeZoneDatabase};
use pos_domain::Timestamp;

/// A typed failure to turn a stored IANA zone into domain offset minutes.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZoneOffsetError {
    #[error("IANA time zone id {zone_id:?} is unavailable")]
    UnknownZoneId { zone_id: String },
    #[error("timestamp {epoch_milliseconds} is outside the time-zone resolver range")]
    TimestampOutOfRange { epoch_milliseconds: i64 },
    #[error(
        "IANA time zone {zone_id:?} resolved to a non-minute offset of {offset_seconds} seconds at {epoch_milliseconds}"
    )]
    OffsetNotWholeMinutes {
        zone_id: String,
        epoch_milliseconds: i64,
        offset_seconds: i32,
    },
    #[error(
        "IANA time zone {zone_id:?} resolved to an unrepresentable offset of {offset_minutes} minutes"
    )]
    OffsetOutOfRange {
        zone_id: String,
        offset_minutes: i32,
    },
}

/// Resolve the UTC offset in force in `zone_id` at `instant`.
///
/// This deliberately uses the bundled database directly. Jiff's global
/// database prefers the host database on Unix, which would make two register
/// operating systems resolve against different tzdata releases.
pub fn resolve_utc_offset_minutes(
    zone_id: &str,
    instant: Timestamp,
) -> Result<i16, ZoneOffsetError> {
    let epoch_milliseconds = instant.epoch_milliseconds();
    let jiff_instant = JiffTimestamp::from_millisecond(epoch_milliseconds)
        .map_err(|_| ZoneOffsetError::TimestampOutOfRange { epoch_milliseconds })?;

    let zone =
        TimeZoneDatabase::bundled()
            .get(zone_id)
            .map_err(|_| ZoneOffsetError::UnknownZoneId {
                zone_id: zone_id.to_owned(),
            })?;

    // `Etc/Unknown` is a special Jiff fallback whose offset is UTC. It is not
    // an IANA zone and must not silently turn corrupted settings into offset 0.
    if zone.is_unknown() {
        return Err(ZoneOffsetError::UnknownZoneId {
            zone_id: zone_id.to_owned(),
        });
    }

    let offset_seconds = zone.to_offset(jiff_instant).seconds();
    if offset_seconds % 60 != 0 {
        return Err(ZoneOffsetError::OffsetNotWholeMinutes {
            zone_id: zone_id.to_owned(),
            epoch_milliseconds,
            offset_seconds,
        });
    }

    let offset_minutes = offset_seconds / 60;
    i16::try_from(offset_minutes).map_err(|_| ZoneOffsetError::OffsetOutOfRange {
        zone_id: zone_id.to_owned(),
        offset_minutes,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use pos_domain::{BusinessDate, DayBoundary, business_date_of};

    use super::*;

    fn timestamp(value: &str) -> Timestamp {
        Timestamp::parse_iso8601(value).unwrap()
    }

    fn business_date(value: &str) -> BusinessDate {
        BusinessDate::parse(value).unwrap()
    }

    #[test]
    fn business_date_uses_the_offset_in_force_at_the_instant() {
        let january = timestamp("2026-01-15T08:30:00.000Z");
        let july = timestamp("2026-07-15T08:30:00.000Z");

        let january_offset = resolve_utc_offset_minutes("America/New_York", january).unwrap();
        let july_offset = resolve_utc_offset_minutes("America/New_York", july).unwrap();

        assert_eq!(january_offset, -300);
        assert_eq!(july_offset, -240);
        assert_eq!(
            business_date_of(january, DayBoundary::new(january_offset, 240).unwrap()),
            business_date("2026-01-14")
        );
        assert_eq!(
            business_date_of(july, DayBoundary::new(july_offset, 240).unwrap()),
            business_date("2026-07-15")
        );
    }

    #[test]
    fn a_january_sale_and_a_july_sale_agree_in_asia_amman() {
        // Both instants are 04:30 local with Jordan's current rules. An old
        // seasonal rule would resolve January to +02:00 and put it before the
        // 04:00 cutover, while July would remain after it.
        let january = timestamp("2026-01-15T01:30:00.000Z");
        let july = timestamp("2026-07-15T01:30:00.000Z");

        let january_offset = resolve_utc_offset_minutes("Asia/Amman", january).unwrap();
        let july_offset = resolve_utc_offset_minutes("Asia/Amman", july).unwrap();

        assert_eq!(january_offset, 180);
        assert_eq!(july_offset, 180);
        assert_eq!(
            business_date_of(january, DayBoundary::new(january_offset, 240).unwrap()),
            business_date("2026-01-15")
        );
        assert_eq!(
            business_date_of(july, DayBoundary::new(july_offset, 240).unwrap()),
            business_date("2026-07-15")
        );
    }

    #[test]
    fn resolving_an_unknown_zone_id_is_a_named_error_not_a_default_offset() {
        let instant = timestamp("2026-01-15T01:30:00.000Z");

        for zone_id in ["Mars/Olympus_Mons", "Etc/Unknown"] {
            assert_eq!(
                resolve_utc_offset_minutes(zone_id, instant),
                Err(ZoneOffsetError::UnknownZoneId {
                    zone_id: zone_id.to_owned(),
                })
            );
        }
    }

    #[test]
    fn a_historical_subminute_offset_is_refused() {
        let instant = timestamp("1900-01-01T00:00:00.000Z");

        assert_eq!(
            resolve_utc_offset_minutes("Europe/Paris", instant),
            Err(ZoneOffsetError::OffsetNotWholeMinutes {
                zone_id: "Europe/Paris".to_owned(),
                epoch_milliseconds: instant.epoch_milliseconds(),
                offset_seconds: 561,
            })
        );
    }
}
