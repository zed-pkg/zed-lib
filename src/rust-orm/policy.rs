//! Registry domain policies that must agree across every transport.

use std::fmt;
use std::time::Duration;

/// A private package may become public through the end of its tenth day.
pub const MAX_PRIVATE_AGE: Duration = Duration::from_secs(10 * 24 * 60 * 60);

/// A private package may become public while it has at most fifty completed
/// downloads. The fifty-first completed download closes the transition window.
pub const MAX_PRIVATE_DOWNLOADS: i64 = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicTransitionDenied {
    InvalidDownloadCount(i64),
    OlderThanTenDays { age: Duration },
    MoreThanFiftyDownloads { completed_downloads: i64 },
}

impl fmt::Display for PublicTransitionDenied {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDownloadCount(count) => {
                write!(
                    formatter,
                    "completed download count cannot be negative: {count}"
                )
            }
            Self::OlderThanTenDays { age } => write!(
                formatter,
                "package is older than the ten-day private-to-public window: {} seconds",
                age.as_secs_f64()
            ),
            Self::MoreThanFiftyDownloads {
                completed_downloads,
            } => write!(
                formatter,
                "package has more than fifty completed downloads: {completed_downloads}"
            ),
        }
    }
}

impl std::error::Error for PublicTransitionDenied {}

/// Validate the one-way private-to-public transition.
///
/// Exact boundaries are allowed: an age of exactly ten days and exactly fifty
/// completed downloads both pass. Callers must hold a lock on the package row
/// while reading these facts and writing visibility so a concurrent download
/// cannot race the decision.
pub fn validate_private_to_public(
    age: Duration,
    completed_downloads: i64,
) -> Result<(), PublicTransitionDenied> {
    if completed_downloads < 0 {
        return Err(PublicTransitionDenied::InvalidDownloadCount(
            completed_downloads,
        ));
    }
    if age > MAX_PRIVATE_AGE {
        return Err(PublicTransitionDenied::OlderThanTenDays { age });
    }
    if completed_downloads > MAX_PRIVATE_DOWNLOADS {
        return Err(PublicTransitionDenied::MoreThanFiftyDownloads {
            completed_downloads,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_age_and_download_boundaries_are_allowed() {
        assert!(validate_private_to_public(MAX_PRIVATE_AGE, 50).is_ok());
    }

    #[test]
    fn one_microsecond_after_ten_days_is_rejected() {
        let age = MAX_PRIVATE_AGE + Duration::from_micros(1);
        assert!(matches!(
            validate_private_to_public(age, 0),
            Err(PublicTransitionDenied::OlderThanTenDays { .. })
        ));
    }

    #[test]
    fn fifty_first_download_is_rejected() {
        assert_eq!(
            validate_private_to_public(Duration::ZERO, 51),
            Err(PublicTransitionDenied::MoreThanFiftyDownloads {
                completed_downloads: 51,
            })
        );
    }

    #[test]
    fn corrupt_negative_counts_fail_closed() {
        assert_eq!(
            validate_private_to_public(Duration::ZERO, -1),
            Err(PublicTransitionDenied::InvalidDownloadCount(-1))
        );
    }
}
