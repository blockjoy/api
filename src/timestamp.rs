use std::ops::{Add, Sub};

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use derive_more::{Deref, From, Into};
use displaydoc::Display;
use prost_types::Timestamp;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse NanosUtc from: (seconds: {0}, nanos: {1})
    ParseNanos(i64, u32),
    /// Failed to parse SecondsUtc from seconds: {0}
    ParseSeconds(i64),
    /// Invalid number of nanoseconds from protobuf: {0}
    ProtoNanos(i32),
}

/// A wrapper around a `chrono::DateTime<Utc>` with second precision.
#[derive(Clone, Copy, Debug, Deref, PartialEq, Eq, PartialOrd, Ord, Into)]
pub struct SecondsUtc(DateTime<Utc>);

impl SecondsUtc {
    fn new(seconds: i64) -> Result<Self, Error> {
        NaiveDateTime::from_timestamp_opt(seconds, 0)
            .ok_or(Error::ParseSeconds(seconds))
            .map(|dt| SecondsUtc(Utc.from_utc_datetime(&dt)))
    }

    pub fn now() -> Self {
        let now = Utc::now().timestamp();
        let naive = NaiveDateTime::from_timestamp_opt(now, 0).expect("valid timestamp");
        SecondsUtc(Utc.from_utc_datetime(&naive))
    }
}

impl Serialize for SecondsUtc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.timestamp())
    }
}

impl<'de> Deserialize<'de> for SecondsUtc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = i64::deserialize(deserializer)?;
        SecondsUtc::new(seconds).map_err(serde::de::Error::custom)
    }
}

impl From<SecondsUtc> for Timestamp {
    fn from(seconds: SecondsUtc) -> Self {
        Timestamp {
            seconds: seconds.timestamp(),
            nanos: 0,
        }
    }
}

impl TryFrom<Timestamp> for SecondsUtc {
    type Error = Error;

    fn try_from(ts: Timestamp) -> Result<Self, Self::Error> {
        SecondsUtc::new(ts.seconds)
    }
}

impl Add<chrono::Duration> for SecondsUtc {
    type Output = Self;

    fn add(self, duration: chrono::Duration) -> Self {
        SecondsUtc(self.0 + duration)
    }
}

impl Sub for SecondsUtc {
    type Output = chrono::Duration;

    fn sub(self, other: Self) -> Self::Output {
        self.0 - other.0
    }
}

/// A wrapper around a `chrono::DateTime<Utc>` with nanosecond precision.
#[derive(Clone, Copy, Debug, Deref, PartialEq, Eq, PartialOrd, Ord, From, Into)]
pub struct NanosUtc(DateTime<Utc>);

impl NanosUtc {
    pub fn new(seconds: i64, nanos: u32) -> Result<Self, Error> {
        NaiveDateTime::from_timestamp_opt(seconds, nanos)
            .ok_or(Error::ParseNanos(seconds, nanos))
            .map(|dt| NanosUtc(Utc.from_utc_datetime(&dt)))
    }

    pub fn now() -> Self {
        NanosUtc(Utc::now())
    }
}

impl From<NanosUtc> for Timestamp {
    fn from(nanos: NanosUtc) -> Self {
        Timestamp {
            seconds: nanos.timestamp(),
            nanos: i32::try_from(nanos.timestamp_subsec_nanos())
                .expect("nanos + leap second < 2e9 < i32::MAX"),
        }
    }
}

impl TryFrom<Timestamp> for NanosUtc {
    type Error = Error;

    fn try_from(ts: Timestamp) -> Result<Self, Self::Error> {
        let nanos = u32::try_from(ts.nanos).map_err(|_| Error::ProtoNanos(ts.nanos))?;
        NanosUtc::new(ts.seconds, nanos)
    }
}
