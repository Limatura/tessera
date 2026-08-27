//! Choosing which backend a new upload goes to.
//!
//! # Why best fit and not most free space
//!
//! `rclone` ships both policies, and which one is right depends on what the
//! pool is for. Spreading across the emptiest backend (`mfs`) balances load and
//! leaves *every* backend with a gap at the end; filling the fullest one that
//! still fits (`lfs`) concentrates the leftover into one place instead.
//!
//! The audience here is somebody pooling free tiers, where the scarce thing is
//! bytes rather than throughput — so the gaps matter and the balance does not.
//!
//! # What this deliberately does not do
//!
//! It never splits a file across backends. If nothing has room for the whole
//! thing, that is a refusal, not a plan — see #184 for the other answer and why
//! it is not free.

/// One backend, as the placer sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// What a segment will record.
    pub name: String,
    /// How much it may hold, or `None` for no limit.
    pub capacity_bytes: Option<u64>,
    /// How much it already holds.
    pub used_bytes: u64,
    /// Whether it still takes new content.
    pub read_only: bool,
    /// Whether it is the operator's stated preference for new uploads.
    pub is_default: bool,
}

impl Candidate {
    /// Room left, or `None` when nothing limits it.
    #[must_use]
    pub fn free_bytes(&self) -> Option<u64> {
        self.capacity_bytes
            .filter(|capacity| *capacity > 0)
            .map(|capacity| capacity.saturating_sub(self.used_bytes))
    }

    /// Whether `size` fits.
    fn fits(&self, size: u64) -> bool {
        !self.read_only && self.free_bytes().is_none_or(|free| free >= size)
    }
}

/// The backend a file of `size` should go to, or `None` when none has room.
///
/// The default is preferred whenever it fits, because an operator who set one
/// meant it — spreading away from it to balance bytes would quietly undo a
/// choice they made. Only when it cannot take the file does the rest of the
/// pool come into play, and then the tightest fit wins.
#[must_use]
pub fn choose(candidates: &[Candidate], size: u64) -> Option<&Candidate> {
    let usable = || candidates.iter().filter(|candidate| candidate.fits(size));

    if let Some(preferred) = usable().find(|candidate| candidate.is_default) {
        return Some(preferred);
    }
    // Unlimited backends sort last: spending a bounded one first is what keeps
    // the bounded ones usable, and an unlimited one has nothing to preserve.
    usable().min_by_key(|candidate| candidate.free_bytes().unwrap_or(u64::MAX))
}

/// The backend for a file whose size nobody has said.
///
/// A chunked body declares nothing, and asking [`choose`] for zero bytes is not
/// a neutral question: everything fits zero, so the tightest-fit rule picks the
/// *fullest* backend every time. An upload that could not say how large it is
/// would be steered into the one place least able to take it, and then refused
/// part-way by the running check.
///
/// So the rule inverts when the size is unknown: the operator's default first,
/// because they said so, and otherwise the most room — the choice that is
/// wrong in the fewest cases when there is nothing to fit against.
#[must_use]
pub fn choose_unsized(candidates: &[Candidate]) -> Option<&Candidate> {
    let usable = || candidates.iter().filter(|candidate| candidate.fits(0));

    if let Some(preferred) = usable().find(|candidate| candidate.is_default) {
        return Some(preferred);
    }
    usable().max_by_key(|candidate| candidate.free_bytes().unwrap_or(u64::MAX))
}

/// The largest single file the pool can accept right now.
///
/// What a client is told, and what a refusal is measured against. Deliberately
/// **not** the sum of what is free: no file is split, so a pool with five
/// backends each holding 100 MB free cannot take a 200 MB file, and reporting
/// 500 MB would be a number that explains nothing when the upload is refused.
#[must_use]
pub fn largest_acceptable(candidates: &[Candidate]) -> Option<u64> {
    let writable: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| !candidate.read_only)
        .collect();
    if writable
        .iter()
        .any(|candidate| candidate.free_bytes().is_none())
    {
        return None;
    }
    writable
        .iter()
        .filter_map(|candidate| candidate.free_bytes())
        .max()
}

#[cfg(test)]
mod unsized_tests {
    use super::*;

    fn backend(name: &str, free: u64) -> Candidate {
        Candidate {
            name: name.to_owned(),
            capacity_bytes: Some(free),
            used_bytes: 0,
            read_only: false,
            is_default: false,
        }
    }

    /// The bug this exists for: asking `choose` for zero bytes is not a neutral
    /// question. Everything fits zero, so the tightest-fit rule hands back the
    /// fullest backend — the one least able to take a body of unknown size.
    #[test]
    fn asking_for_zero_bytes_picks_the_fullest_backend() {
        let pool = [backend("nearly-full", 1024), backend("roomy", 1024 * 1024)];
        assert_eq!(choose(&pool, 0).expect("a backend").name, "nearly-full");
    }

    #[test]
    fn an_unknown_size_picks_the_roomiest_instead() {
        let pool = [backend("nearly-full", 1024), backend("roomy", 1024 * 1024)];
        assert_eq!(choose_unsized(&pool).expect("a backend").name, "roomy");
    }

    /// The operator's choice still wins. They said which one they wanted, and
    /// not knowing the size is no reason to overrule them.
    #[test]
    fn the_default_is_still_preferred() {
        let pool = [
            Candidate {
                is_default: true,
                ..backend("theirs", 1024)
            },
            backend("roomy", 1024 * 1024),
        ];
        assert_eq!(choose_unsized(&pool).expect("a backend").name, "theirs");
    }

    #[test]
    fn a_read_only_backend_is_not_offered() {
        let pool = [
            Candidate {
                read_only: true,
                ..backend("archive", 1024 * 1024)
            },
            backend("live", 1024),
        ];
        assert_eq!(choose_unsized(&pool).expect("a backend").name, "live");
    }
}

#[cfg(test)]
mod tests {
    use super::{Candidate, choose, largest_acceptable};

    fn backend(name: &str, capacity: Option<u64>, used: u64) -> Candidate {
        Candidate {
            name: name.to_owned(),
            capacity_bytes: capacity,
            used_bytes: used,
            read_only: false,
            is_default: false,
        }
    }

    /// An operator who names a default meant it. Spreading away from it to
    /// balance bytes would quietly undo a choice they made.
    #[test]
    fn the_default_wins_whenever_it_fits() {
        let mut hot = backend("hot", Some(10_000), 0);
        hot.is_default = true;
        let pool = vec![backend("cold", Some(1_000_000), 0), hot];
        assert_eq!(choose(&pool, 5_000).map(|c| c.name.as_str()), Some("hot"));
    }

    /// Once the default cannot take it, the tightest fit does — that is what
    /// leaves one gap at the end instead of one per backend.
    #[test]
    fn otherwise_the_tightest_fit_wins() {
        let mut hot = backend("hot", Some(10_000), 9_000);
        hot.is_default = true;
        let pool = vec![
            hot,
            backend("roomy", Some(1_000_000), 0),
            backend("snug", Some(20_000), 0),
        ];
        assert_eq!(choose(&pool, 5_000).map(|c| c.name.as_str()), Some("snug"));
    }

    /// Spending a bounded backend before an unlimited one is what keeps the
    /// bounded ones usable; the unlimited one has nothing to preserve.
    #[test]
    fn a_bounded_backend_is_spent_before_an_unlimited_one() {
        let pool = vec![
            backend("unbounded", None, 0),
            backend("bounded", Some(50_000), 0),
        ];
        assert_eq!(
            choose(&pool, 1_000).map(|c| c.name.as_str()),
            Some("bounded")
        );
    }

    /// Read-only is what an operator sets while draining, and what Azure's free
    /// tier does to them without asking.
    #[test]
    fn a_read_only_backend_takes_nothing() {
        let mut retired = backend("retired", None, 0);
        retired.read_only = true;
        retired.is_default = true;
        assert_eq!(choose(&[retired], 1).map(|c| c.name.as_str()), None);
    }

    /// Nothing is split, so the answer is the largest single gap — not the sum.
    /// Five backends with 100 MB each cannot take 200 MB, and saying "500 MB
    /// free" explains nothing when the upload is refused.
    #[test]
    fn the_largest_acceptable_file_is_not_the_sum() {
        let pool: Vec<Candidate> = (0..5)
            .map(|index| backend(&format!("b{index}"), Some(100), 0))
            .collect();
        assert_eq!(largest_acceptable(&pool), Some(100));
        assert!(choose(&pool, 200).is_none());
    }

    /// One unlimited backend means there is no figure to report, which is
    /// different from reporting a very large one.
    #[test]
    fn an_unlimited_backend_has_no_figure() {
        let pool = vec![
            backend("bounded", Some(100), 0),
            backend("unbounded", None, 0),
        ];
        assert_eq!(largest_acceptable(&pool), None);
    }

    /// `0` is how "no limit" is spelled in the column as well as `NULL`, and
    /// the two have to behave the same — there is no next layer to ask here.
    #[test]
    fn zero_capacity_means_unlimited() {
        let pool = vec![backend("zero", Some(0), 999_999)];
        assert_eq!(
            choose(&pool, 1_000_000).map(|c| c.name.as_str()),
            Some("zero")
        );
        assert_eq!(largest_acceptable(&pool), None);
    }
}
