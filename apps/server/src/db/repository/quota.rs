//! Turning a quota between the domain's `Option<u64>` and the column's
//! `Option<i64>`.
//!
//! Two tables carry the same nullable figure — `roles.quota_bytes` and
//! `users.quota_bytes` — with the same three-way reading:
//!
//! | value     | meaning                       |
//! |-----------|-------------------------------|
//! | `None`    | ask the next layer            |
//! | `Some(0)` | no ceiling                    |
//! | `Some(n)` | `n` bytes                     |
//!
//! `None` and `Some(0)` are different answers, which is why neither table
//! stores a plain number.

use sea_orm::sea_query::{Expr, SimpleExpr};
use tessera_core::{RoleId, UserId};

/// Whose storage a figure is about.
///
/// The tier that supplies the ceiling also decides what counts against it. If a
/// role allows ten gigabytes, "ten gigabytes of what" is that role's shares, not
/// the instance's — measuring the ceiling against the wrong denominator refuses
/// somebody at their first upload because a stranger filled the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// One account's own shares.
    User(UserId),
    /// Everything charged to a role, including the guest role's anonymous work.
    Role(RoleId),
    /// Everything on the instance.
    Instance,
}

/// The column value for a quota.
///
/// A figure above [`i64::MAX`] is an operator asking for more storage than the
/// column can hold, so it becomes the largest the column can hold rather than
/// wrapping into a negative one — which the reader would take as "ask the next
/// layer" and quietly widen the ceiling instead of narrowing it.
pub(crate) fn to_column(bytes: Option<u64>) -> Option<i64> {
    bytes.map(|bytes| i64::try_from(bytes).unwrap_or(i64::MAX))
}

/// The same, as something `col_expr` accepts.
///
/// `Option<i64>` cannot go through `into()` there: `None` would be typed as a
/// null of unknown type, which Postgres refuses on a bigint column.
pub(crate) fn to_expr(bytes: Option<u64>) -> SimpleExpr {
    to_column(bytes).map_or_else(|| Expr::value(sea_orm::Value::BigInt(None)), Expr::value)
}

/// The domain value for a stored quota.
///
/// A negative figure is not a smaller quota, it is a row nothing this crate
/// writes could have produced. Reading it as `None` asks the next layer rather
/// than granting the account no ceiling at all.
pub(crate) fn from_column(bytes: Option<i64>) -> Option<u64> {
    bytes.and_then(|bytes| u64::try_from(bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::{from_column, to_column};

    #[test]
    fn nothing_and_no_ceiling_stay_apart() {
        assert_eq!(to_column(None), None);
        assert_eq!(to_column(Some(0)), Some(0));
        assert_eq!(from_column(None), None);
        assert_eq!(from_column(Some(0)), Some(0));
    }

    /// Wrapping would turn the largest quota an operator can ask for into a
    /// negative one, which reads back as "ask the next layer".
    #[test]
    fn an_impossible_figure_saturates_rather_than_wrapping() {
        assert_eq!(to_column(Some(u64::MAX)), Some(i64::MAX));
        assert_eq!(
            from_column(to_column(Some(u64::MAX))),
            Some(0x7fff_ffff_ffff_ffff)
        );
    }

    /// Only reachable by writing to the database by hand, which is exactly when
    /// the reading matters.
    #[test]
    fn a_negative_row_asks_the_next_layer() {
        assert_eq!(from_column(Some(-1)), None);
    }
}
