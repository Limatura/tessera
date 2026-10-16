//! What the malware scanner has reported, and what it still has to check.
//!
//! Verdicts live next to the content they describe, on `file_shares` and
//! `text_shares`, so reading a share's content reads them too. This is the
//! other side: the scanner's work queue and where it writes back.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _, QueryFilter as _,
    QueryOrder as _, QuerySelect as _, sea_query::Expr,
};
use time::OffsetDateTime;

use crate::db::entity::{
    file_share::{self, Entity as FileShareEntity},
    share::{self, Entity as ShareEntity},
    text_share::{self, Entity as TextShareEntity},
};

/// What the scanner has said about one file or note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    /// Waiting for a verdict. Its share is not served until there is one.
    Pending,
    /// The scanner could not be reached. Served anyway, and rescanned once the
    /// scanner is back.
    Deferred,
    /// Scanned, and nothing was found.
    Clean,
    /// Scanned, and the scanner named a threat. The share is switched off.
    Infected(String),
    /// Not scanned, because there was nothing a scanner could inspect.
    Skipped(Skip),
}

/// Why something was not scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Sealed on the uploader's machine; the server holds only ciphertext.
    Encrypted,
    /// Larger than the scanner accepts.
    TooLarge,
}

impl Scan {
    /// Whether this holds its share back from being served.
    #[must_use]
    pub const fn holds(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// The two columns: `scan_state` and `scan_detail`.
    #[must_use]
    pub fn columns(&self) -> (&'static str, Option<String>) {
        match self {
            Self::Pending => ("pending", None),
            Self::Deferred => ("deferred", None),
            Self::Clean => ("clean", None),
            Self::Infected(threat) => ("infected", Some(threat.clone())),
            Self::Skipped(Skip::Encrypted) => ("skipped", Some("encrypted".to_owned())),
            Self::Skipped(Skip::TooLarge) => ("skipped", Some("too_large".to_owned())),
        }
    }

    /// Reads the two columns back. `None` for content never sent to the
    /// scanner, and for unknown values, which are treated the same instead of
    /// holding a share back forever over a bad row.
    #[must_use]
    pub fn from_columns(state: Option<&str>, detail: Option<&str>) -> Option<Self> {
        Some(match (state?, detail) {
            ("pending", _) => Self::Pending,
            ("deferred", _) => Self::Deferred,
            ("clean", _) => Self::Clean,
            ("infected", threat) => Self::Infected(threat.unwrap_or("unnamed").to_owned()),
            ("skipped", Some("encrypted")) => Self::Skipped(Skip::Encrypted),
            ("skipped", _) => Self::Skipped(Skip::TooLarge),
            _ => return None,
        })
    }
}

/// One item the scanner has yet to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// A file, by its own id, in the share `share_id`.
    File {
        /// The `file_shares` row.
        id: String,
        /// The share it belongs to, which is disabled if it is infected.
        share_id: String,
        /// Its position in that share, which is how its content is found.
        position: usize,
    },
    /// A note, which is its share's only content.
    Text {
        /// The share, which is also the `text_shares` key.
        share_id: String,
    },
}

impl Job {
    /// The share this belongs to.
    #[must_use]
    pub fn share_id(&self) -> &str {
        match self {
            Self::File { share_id, .. } | Self::Text { share_id } => share_id,
        }
    }
}

/// Anything that can go wrong reading or writing the scanner's queue.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ScanRepositoryError(#[from] sea_orm::DbErr);

impl From<ScanRepositoryError> for crate::error::Error {
    fn from(error: ScanRepositoryError) -> Self {
        Self::Database(error.0)
    }
}

/// Access to the scanner's queue.
#[derive(Debug, Clone)]
pub struct ScanRepository {
    connection: DatabaseConnection,
}

impl ScanRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Up to `limit` items in `state`, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`ScanRepositoryError`] if a query fails.
    pub async fn in_state(
        &self,
        state: &Scan,
        limit: u64,
    ) -> Result<Vec<Job>, ScanRepositoryError> {
        let (spelling, _) = state.columns();
        let files = FileShareEntity::find()
            .filter(file_share::Column::ScanState.eq(spelling))
            .order_by_asc(file_share::Column::CreatedAt)
            .limit(limit)
            .all(&self.connection)
            .await?;
        let texts = TextShareEntity::find()
            .select_only()
            .column(text_share::Column::ShareId)
            .filter(text_share::Column::ScanState.eq(spelling))
            .limit(limit)
            .into_tuple::<String>()
            .all(&self.connection)
            .await?;
        Ok(files
            .into_iter()
            .map(|file| Job::File {
                id: file.id,
                share_id: file.share_id,
                position: usize::try_from(file.position).unwrap_or(usize::MAX),
            })
            .chain(texts.into_iter().map(|share_id| Job::Text { share_id }))
            .collect())
    }

    /// How many files and notes are in `state`.
    ///
    /// # Errors
    ///
    /// Returns [`ScanRepositoryError`] if a query fails.
    pub async fn count_in_state(&self, state: &Scan) -> Result<u64, ScanRepositoryError> {
        let (spelling, _) = state.columns();
        let files = FileShareEntity::find()
            .filter(file_share::Column::ScanState.eq(spelling))
            .count(&self.connection)
            .await?;
        let texts = TextShareEntity::find()
            .filter(text_share::Column::ScanState.eq(spelling))
            .count(&self.connection)
            .await?;
        Ok(files.saturating_add(texts))
    }

    /// Records the scanner's verdict.
    ///
    /// # Errors
    ///
    /// Returns [`ScanRepositoryError`] if the update fails.
    pub async fn record(&self, job: &Job, verdict: &Scan) -> Result<(), ScanRepositoryError> {
        let (state, detail) = verdict.columns();
        match job {
            Job::File { id, .. } => {
                FileShareEntity::update_many()
                    .col_expr(file_share::Column::ScanState, Expr::value(state))
                    .col_expr(file_share::Column::ScanDetail, Expr::value(detail))
                    .filter(file_share::Column::Id.eq(id.as_str()))
                    .exec(&self.connection)
                    .await?;
            }
            Job::Text { share_id } => {
                TextShareEntity::update_many()
                    .col_expr(text_share::Column::ScanState, Expr::value(state))
                    .col_expr(text_share::Column::ScanDetail, Expr::value(detail))
                    .filter(text_share::Column::ShareId.eq(share_id.as_str()))
                    .exec(&self.connection)
                    .await?;
            }
        }
        Ok(())
    }

    /// Moves everything still `pending` to `deferred`, so it is served while
    /// the scanner is unreachable. Returns how many moved.
    ///
    /// # Errors
    ///
    /// Returns [`ScanRepositoryError`] if an update fails.
    pub async fn defer_all_pending(&self) -> Result<u64, ScanRepositoryError> {
        let (pending, _) = Scan::Pending.columns();
        let (deferred, _) = Scan::Deferred.columns();
        let files = FileShareEntity::update_many()
            .col_expr(file_share::Column::ScanState, Expr::value(deferred))
            .filter(file_share::Column::ScanState.eq(pending))
            .exec(&self.connection)
            .await?
            .rows_affected;
        let texts = TextShareEntity::update_many()
            .col_expr(text_share::Column::ScanState, Expr::value(deferred))
            .filter(text_share::Column::ScanState.eq(pending))
            .exec(&self.connection)
            .await?
            .rows_affected;
        Ok(files.saturating_add(texts))
    }

    /// Disables a share, as an administrator would, unless it already is.
    ///
    /// # Errors
    ///
    /// Returns [`ScanRepositoryError`] if the update fails.
    pub async fn disable_share(
        &self,
        share_id: &str,
        now: OffsetDateTime,
    ) -> Result<(), ScanRepositoryError> {
        ShareEntity::update_many()
            .col_expr(share::Column::DisabledAt, Expr::value(now))
            .col_expr(share::Column::UpdatedAt, Expr::value(now))
            .filter(share::Column::Id.eq(share_id))
            .filter(share::Column::DisabledAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::pending(Scan::Pending)]
    #[case::deferred(Scan::Deferred)]
    #[case::clean(Scan::Clean)]
    #[case::infected(Scan::Infected("Eicar-Signature".to_owned()))]
    #[case::encrypted(Scan::Skipped(Skip::Encrypted))]
    #[case::too_large(Scan::Skipped(Skip::TooLarge))]
    fn every_verdict_survives_its_columns(#[case] verdict: Scan) {
        let (state, detail) = verdict.columns();
        assert_eq!(
            Scan::from_columns(Some(state), detail.as_deref()),
            Some(verdict)
        );
    }

    /// An unreadable row must not hold its share back forever.
    #[test]
    fn an_unknown_spelling_holds_nothing() {
        assert_eq!(Scan::from_columns(Some("mystery"), None), None);
        assert_eq!(Scan::from_columns(None, None), None);
    }

    #[test]
    fn only_a_pending_verdict_holds_a_share() {
        assert!(Scan::Pending.holds());
        assert!(!Scan::Deferred.holds());
        assert!(!Scan::Infected("x".to_owned()).holds());
    }
}
