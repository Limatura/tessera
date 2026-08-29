//! Where a file's bytes go when no single backend can hold all of them.
//!
//! The case this exists for: five free accounts of 10 GB each, and one 50 GB
//! file to move. Without splitting that is not slow or wasteful, it is
//! impossible. Every competitor surveyed refuses it.
//!
//! The order the pieces run in, and what turning this on costs, are drawn out
//! in `docs/splitting.md`, which can carry a diagram where rustdoc cannot.
//!
//! Off by default. An operator turns it on, and until they do this module only
//! ever produces plans of one segment, exactly what
//! [`placement::choose`](super::placement::choose) already did.
//!
//! # Why the cuts are not aligned to a part
//!
//! Every cut but the last used to land on a multiple of
//! `server.part_size_bytes`. It does not any more, and the reason it did was
//! wrong: it was written down as an S3 requirement, and S3 does not require it.
//!
//! S3's rule, no part below 5 MiB except the last, is scoped to one multipart
//! upload, and each segment is an upload of its own whose part numbering starts
//! again at one. A segment of any length ends in a short final part that S3
//! accepts. The suite proves it on every store in CI by storing 20 MiB in 8 MiB
//! parts: 8, 8 and 4.
//!
//! Alignment cost the remainder of every backend it touched, up to a part each,
//! and those bytes were reachable by nothing. On the free tiers this feature
//! exists for, unreachable space is what is being economised.
//!
//! It will have to come back for **resumable** splitting, not yet built.
//! A `tus` session's chunks are already forced to whole parts, so with aligned
//! segments a chunk boundary always lands on a part boundary and a request can
//! end with nothing buffered. Unaligned, a chunk can end in the middle of a
//! part, and those bytes would have to survive until the next request. That is
//! the work, and it belongs with the work that needs it.
//!
//! # Why a backend with a little room left is skipped entirely
//!
//! Taking 6 MB from a backend with 6 MB free would add a segment, a row, a
//! multipart upload and a place for the reader to stall, in exchange for 6 MB.
//! [`min_segment`] is the floor below which a backend is not worth involving,
//! and why [`capacity`] is not the sum of what is free: a pool of five backends
//! with 1 MB each holds nothing, and reporting 5 MB would explain nothing when
//! the upload is refused.

use super::placement::Candidate;

/// The smallest useful segment.
///
/// `part_size_bytes` because a segment shorter than one part cannot be aligned,
/// and 5 MiB because that is S3's floor for a part that is not the last. The
/// configuration already refuses a `part_size_bytes` below 5 MiB, so the first
/// term always wins today; the `max` keeps that true if it ever does not.
#[must_use]
pub const fn min_segment(part_size: u64) -> u64 {
    const S3_PART_FLOOR: u64 = 5 * 1024 * 1024;
    if part_size > S3_PART_FLOOR {
        part_size
    } else {
        S3_PART_FLOOR
    }
}

/// One backend's share of a file.
///
/// `offset` is where this piece starts within the whole file, exactly what the
/// reader needs to answer a `Range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The backend that holds it.
    pub backend: String,
    /// Where it starts within the file.
    pub offset: u64,
    /// How long it is.
    pub length: u64,
}

/// Whether splitting is allowed, and how large a piece has to be.
///
/// One argument, not two loose numbers, because they are never chosen
/// independently: `part_size` comes from `server.part_size_bytes` and `split`
/// from the operator's switch, so a call site passing one without the other has
/// got something wrong.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    /// `server.part_size_bytes`.
    pub part_size: u64,
    /// Whether a file may be spread across more than one backend.
    pub split: bool,
}

/// How the bytes of a file of `size` should be laid out, or `None` when the pool
/// cannot take it.
///
/// With `split` off this is [`placement::choose`](super::placement::choose)
/// wearing a different return type: one segment or nothing.
///
/// Backends are filled in the order `choose` would pick them: the operator's
/// default first, then the tightest fit. Spending the tightest first keeps the
/// roomier ones able to take a whole file later.
#[must_use]
pub fn plan(candidates: &[Candidate], size: u64, rules: Rules) -> Option<Vec<Segment>> {
    if !rules.split {
        return super::placement::choose(candidates, size).map(|chosen| {
            vec![Segment {
                backend: chosen.name.clone(),
                offset: 0,
                length: size,
            }]
        });
    }

    // A file that fits somewhere whole goes there whole, even with splitting on.
    // Splitting is the answer to "nowhere has room", not a way to spread load:
    // two segments cost a second multipart upload, a second row, and a reader
    // that has to stitch.
    if let Some(chosen) = super::placement::choose(candidates, size) {
        return Some(vec![Segment {
            backend: chosen.name.clone(),
            offset: 0,
            length: size,
        }]);
    }

    let floor = min_segment(rules.part_size);
    let mut segments = Vec::new();
    let mut placed = 0_u64;

    for candidate in order(candidates, floor) {
        let remaining = size - placed;
        let free = candidate.free_bytes().unwrap_or(u64::MAX);

        // A backend that is not finishing the file gives everything it has.
        // Rounding down to a whole part used to happen here; see the module
        // docs for why it does not any more.
        let length = if remaining <= free {
            remaining
        } else {
            if free < floor {
                continue;
            }
            free
        };

        segments.push(Segment {
            backend: candidate.name.clone(),
            offset: placed,
            length,
        });
        placed += length;
        if placed == size {
            return Some(segments);
        }
    }

    // Ran out of backends with the file unfinished. A partial plan is worse than
    // none: it would spend real space in several places and then refuse.
    None
}

/// The largest file this pool can take, given the rules.
///
/// With splitting off this is the largest single backend, what
/// [`largest_acceptable`](super::placement::largest_acceptable) already says.
///
/// With it on, it is the sum of what every backend worth involving has free.
/// That is exact, not a bound. A segment takes everything its backend
/// holds, so a file of exactly this many bytes plans and one byte more does
/// not. It was a generous upper bound while cuts were aligned to a part.
///
/// Still nothing like the naive sum. A backend below [`min_segment`]
/// contributes nothing, so five backends with a megabyte each hold no file, and
/// reporting five megabytes would explain nothing when the upload is refused.
///
/// `None` means nothing limits it.
#[must_use]
pub fn capacity(candidates: &[Candidate], rules: Rules) -> Option<u64> {
    let whole = super::placement::largest_acceptable(candidates);
    if !rules.split {
        return whole;
    }
    // An unlimited backend anywhere means the pool is unlimited, and
    // `largest_acceptable` has already said so.
    let whole = whole?;

    let floor = min_segment(rules.part_size);
    let mut total = 0_u64;
    for candidate in candidates.iter().filter(|c| !c.read_only) {
        let free = candidate.free_bytes()?;
        if free < floor {
            continue;
        }
        total = total.saturating_add(free);
    }

    // A pool whose every backend is below the floor can still take a file small
    // enough to sit whole in one of them, what `whole` does here.
    Some(total.max(whole))
}

/// The backends worth filling, in the order they should be filled.
///
/// The operator's default first, since going elsewhere while it has room would
/// quietly undo what they said. Then the tightest fit, so the roomiest backend
/// is spent last and stays able to take a whole file. Unlimited ones sort last
/// for the same reason: they have nothing to preserve.
fn order(candidates: &[Candidate], floor: u64) -> Vec<&Candidate> {
    let mut usable: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| !candidate.read_only)
        .filter(|candidate| candidate.free_bytes().is_none_or(|free| free >= floor))
        .collect();
    usable.sort_by_key(|candidate| {
        (
            !candidate.is_default,
            candidate.free_bytes().unwrap_or(u64::MAX),
        )
    });
    usable
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const PART: u64 = 8 * 1024 * 1024;
    const RULES: Rules = Rules {
        part_size: PART,
        split: true,
    };

    fn backend(name: &str, free: u64) -> Candidate {
        Candidate {
            name: name.to_owned(),
            capacity_bytes: Some(free),
            used_bytes: 0,
            read_only: false,
            is_default: false,
        }
    }

    #[test]
    fn a_file_that_fits_somewhere_whole_is_not_split() {
        let pool = [backend("a", 10 * PART), backend("b", 100 * PART)];
        let plan = plan(&pool, 9 * PART, RULES).expect("a plan");
        assert_eq!(plan.len(), 1, "split a file that fitted whole: {plan:?}");
    }

    #[test]
    fn a_file_too_large_for_any_one_backend_is_spread() {
        let pool = [backend("a", 10 * PART), backend("b", 10 * PART)];
        let plan = plan(&pool, 15 * PART, RULES).expect("a plan");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].length, 10 * PART);
        assert_eq!(plan[1].length, 5 * PART);
        assert_eq!(plan[1].offset, 10 * PART);
    }

    #[test]
    fn a_backend_below_the_floor_is_not_involved() {
        // Enough in total, but the second has less than one part free, so
        // involving it would add a segment for a scrap.
        let pool = [backend("a", 10 * PART), backend("b", PART / 2)];
        assert!(plan(&pool, 10 * PART + 1, RULES).is_none());
    }

    #[test]
    fn splitting_off_means_one_segment_or_nothing() {
        let pool = [backend("a", 10 * PART), backend("b", 10 * PART)];
        let rules = Rules {
            part_size: PART,
            split: false,
        };
        assert!(plan(&pool, 15 * PART, rules).is_none());
        assert_eq!(plan(&pool, 9 * PART, rules).expect("a plan").len(), 1);
    }

    #[test]
    fn capacity_is_not_the_naive_sum() {
        // Five backends with a scrap each cannot be combined at all: the most
        // they hold is whatever fits whole in one of them.
        let pool: Vec<Candidate> = (0..5).map(|i| backend(&i.to_string(), PART / 2)).collect();
        assert_eq!(capacity(&pool, RULES), Some(PART / 2));
    }

    #[test]
    fn capacity_spends_the_ragged_tail_on_the_last_segment() {
        // One backend, so the file's last piece is on it and may use everything.
        let pool = [backend("a", 10 * PART + 1234)];
        assert_eq!(capacity(&pool, RULES), Some(10 * PART + 1234));
    }

    /// Every backend's ragged tail counts, not just the roomiest one's.
    ///
    /// Three backends a part and a bit each used to report three parts, because
    /// only one tail could be spent. The bytes were there the whole time.
    #[test]
    fn capacity_counts_every_ragged_tail() {
        let pool: Vec<Candidate> = (0..3)
            .map(|i| backend(&i.to_string(), 2 * PART + 4096))
            .collect();
        assert_eq!(capacity(&pool, RULES), Some(3 * (2 * PART + 4096)));
        assert!(plan(&pool, 3 * (2 * PART + 4096), RULES).is_some());
    }

    /// A pool where at least one backend has no ceiling can take anything.
    #[test]
    fn an_unlimited_backend_makes_the_pool_unlimited() {
        let unlimited = Candidate {
            capacity_bytes: None,
            ..backend("cloud", 0)
        };
        assert_eq!(capacity(&[backend("a", PART), unlimited], RULES), None);
    }

    prop_compose! {
        /// Free space in whole parts, sometimes with a ragged tail, so both the
        /// aligned and the unaligned case are generated. Named by position
        /// afterwards, since two backends with the same name is not a pool that
        /// can exist and generating one only tests the test.
        fn a_free_space()(parts in 0_u64..40, tail in 0_u64..PART) -> u64 {
            parts * PART + tail
        }
    }

    fn a_pool() -> impl Strategy<Value = Vec<Candidate>> {
        prop::collection::vec(a_free_space(), 1..6).prop_map(|spaces| {
            spaces
                .into_iter()
                .enumerate()
                .map(|(i, free)| backend(&format!("b{i}"), free))
                .collect()
        })
    }

    /// The properties below all begin "if a plan came back", so a planner that
    /// refused everything would pass all of them. Over a fixed sweep of the
    /// space the generators draw from, a healthy share of the plans have to be
    /// genuinely split.
    ///
    /// Measured when written: 908 refused, 1182 whole, 910 split.
    #[test]
    fn the_generated_pools_actually_produce_split_plans() {
        let (mut refused, mut whole, mut split) = (0_u32, 0_u32, 0_u32);
        let mut seed = 12_345_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            seed
        };
        for _ in 0..3_000 {
            let how_many = usize::try_from(next() % 5 + 1).unwrap_or(1);
            let pool: Vec<Candidate> = (0..how_many)
                .map(|i| {
                    let free = (next() % 40) * PART + (next() % PART);
                    backend(&format!("b{i}"), free)
                })
                .collect();
            let size = next() % (600 * 1024 * 1024) + 1;
            match plan(&pool, size, RULES) {
                None => refused += 1,
                Some(segments) if segments.len() == 1 => whole += 1,
                Some(_) => split += 1,
            }
        }
        assert!(
            split > 300 && whole > 300 && refused > 300,
            "the generated space stopped covering all three outcomes: \
             refused={refused} whole={whole} split={split}"
        );
    }

    proptest! {
        /// The property everything else rests on: a plan covers the file exactly
        /// once. A gap loses bytes, an overlap writes them twice, and either one
        /// is a corrupt download that no single test case would find.
        #[test]
        fn a_plan_tiles_the_file_exactly(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let Some(segments) = plan(&pool, size, RULES) else { return Ok(()) };

            let mut at = 0_u64;
            for segment in &segments {
                prop_assert_eq!(segment.offset, at, "a gap or an overlap: {:?}", segments);
                prop_assert!(segment.length > 0, "an empty segment: {:?}", segments);
                at += segment.length;
            }
            prop_assert_eq!(at, size, "the plan does not cover the file: {:?}", segments);
        }

        /// Every piece is worth having.
        ///
        /// Only the floor is asserted. Cuts used to have to be a whole number
        /// of parts as well; the module docs say why that went away. The last
        /// segment is exempt, being whatever the file has left.
        #[test]
        fn every_segment_but_the_last_is_worth_having(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let Some(segments) = plan(&pool, size, RULES) else { return Ok(()) };

            let floor = min_segment(PART);
            for segment in segments.iter().take(segments.len().saturating_sub(1)) {
                prop_assert!(
                    segment.length >= floor,
                    "a segment below the floor: {:?}", segments
                );
            }
        }

        /// No segment is larger than the backend it is on, and no backend is
        /// asked to hold two of them.
        #[test]
        fn a_plan_respects_what_each_backend_has(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let Some(segments) = plan(&pool, size, RULES) else { return Ok(()) };

            let mut used: Vec<&str> = Vec::new();
            for segment in &segments {
                prop_assert!(
                    !used.contains(&segment.backend.as_str()),
                    "a backend holds two segments: {:?}", segments
                );
                used.push(&segment.backend);

                let free = pool
                    .iter()
                    .find(|candidate| candidate.name == segment.backend)
                    .and_then(Candidate::free_bytes)
                    .unwrap_or(u64::MAX);
                prop_assert!(
                    segment.length <= free,
                    "a segment overruns its backend: {:?}", segments
                );
            }
        }

        /// The figure and the planner agree, in both directions.
        ///
        /// The first half was all that could be asserted while cuts were
        /// aligned, since the figure was then an upper bound and a file inside
        /// it might still be refused. Now a segment takes its whole backend, so
        /// the figure is exact and everything inside it really does plan.
        #[test]
        fn capacity_agrees_with_what_can_be_planned(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let room = capacity(&pool, RULES).unwrap_or(u64::MAX);
            let planned = plan(&pool, size, RULES).is_some();
            if planned {
                prop_assert!(
                    size <= room,
                    "planned {size} bytes into a pool that claims {room}"
                );
            } else {
                prop_assert!(
                    size > room,
                    "refused {size} bytes from a pool that claims {room}"
                );
            }
        }

        /// With the switch off nothing is ever spread, whatever the pool looks
        /// like. This is the guarantee an operator who left it alone is relying
        /// on.
        #[test]
        fn splitting_off_never_produces_two_segments(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let rules = Rules { part_size: PART, split: false };
            if let Some(segments) = plan(&pool, size, rules) {
                prop_assert_eq!(segments.len(), 1);
            }
        }
    }
}
