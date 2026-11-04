use crate::TimeGeoError;

/// Nonempty half-open interval of Unix-epoch UTC milliseconds `[start, end)`.
///
/// This timeline convention does not represent leap seconds, uncertain instants,
/// calendar dates or source-clock alignment. No date/time-zone conversion is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    start_utc_ms: i64,
    end_utc_ms: i64,
}

impl TimeWindow {
    pub fn new(start_utc_ms: i64, end_utc_ms: i64) -> Result<Self, TimeGeoError> {
        let tw = Self {
            start_utc_ms,
            end_utc_ms,
        };
        tw.validate()?;
        Ok(tw)
    }

    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if self.start_utc_ms >= self.end_utc_ms {
            return Err(TimeGeoError::InvalidTimeWindow {
                start_utc_ms: self.start_utc_ms,
                end_utc_ms: self.end_utc_ms,
            });
        }
        Ok(())
    }

    #[inline]
    #[must_use]
    pub fn start_utc_ms(&self) -> i64 {
        self.start_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn end_utc_ms(&self) -> i64 {
        self.end_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn contains(&self, instant_utc_ms: i64) -> bool {
        instant_utc_ms >= self.start_utc_ms && instant_utc_ms < self.end_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn overlaps(&self, other: &TimeWindow) -> bool {
        self.start_utc_ms < other.end_utc_ms && other.start_utc_ms < self.end_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn contains_window(&self, other: &TimeWindow) -> bool {
        self.start_utc_ms <= other.start_utc_ms && self.end_utc_ms >= other.end_utc_ms
    }
}
