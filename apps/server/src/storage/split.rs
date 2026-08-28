//! Where a file's bytes go when no single backend can hold all of them.
//!
//! The case this exists for: five free accounts of 10 GB each, and one 50 GB
//! file to move. Without splitting that is not slow or wasteful, it is
//! impossible — see #184. Every competitor surveyed refuses it.
//!
//! The order the pieces run in, and what turning this on costs, are drawn out in
//! `docs/splitting.md` — `rustdoc` cannot draw a diagram and that is the half
//! this note cannot carry.
//!
//! Off by default. An operator turns it on, and until they do this module only
//! ever produces plans of one segment, which is exactly what
//! [`placement::choose`](super::placement::choose) already did.
//!
//! # Why the cuts are aligned, and what that is not
//!
//! Every cut but the last lands on a multiple of `server.part_size_bytes`.
//!
//! This is **not** an S3 requirement, and an earlier version of this comment
//! said it was. S3's rule — no part below 5 MiB except the last — is scoped to
//! one multipart upload, and each segment is an upload of its own with its part
//! numbering starting again at one. A segment of any length at all therefore
//! ends in a short final part that S3 accepts; the suite already proves it, by
//! storing 20 MiB in 8 MiB parts on every store in CI.
//!
//! What alignment actually buys is that a segment is a whole number of parts, so
//! nothing downstream has to carry a partial part from one segment into the
//! next. [`Writer`](crate::upload::segments::Writer) does not need that today —
//! it flushes its buffer when it closes a segment — but a resumable upload,
//! which must stop between requests, does.
//!
//! What it costs is the remainder: a backend with 12 MiB free and 5 MiB parts
//! contributes 10, and the other 2 are not used by anything. On the free tiers
//! this feature is aimed at, that is a real if small price, and it is the reason
//! [`capacity`] is an upper bound rather than a sum.
//!
//! # Why a backend with a little room left is skipped entirely
//!
//! Taking 6 MB from a backend with 6 MB free would add a segment, a row, a
//! multipart upload and a place for the reader to stall, in exchange for 6 MB.
//! [`min_segment`] is the floor below which a backend is not worth involving,
//! and it is why [`capacity`] is not the sum of what is free — a pool of five
//! backends with 1 MB each can hold nothing at all, and reporting 5 MB would be
//! a number that explains nothing when the upload is refused.

use super::placement::Candidate;

/// The smallest useful segment.
///
/// `part_size_bytes` because a segment shorter than one part cannot be aligned,
/// and 5 MiB because that is S3's floor for a part that is not the last. The
/// configuration already refuses a `part_size_bytes` below 5 MiB, so today the
/// first term always wins — the `max` is here so that stays true if it ever
/// does not.
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
/// `offset` is where this piece starts within the whole file, which is what the
/// reader needs to answer a `Range` — see #184.
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
/// One argument rather than two loose numbers because they are never chosen
/// independently: `part_size` comes from `server.part_size_bytes` and `split`
/// from the operator's switch, and a call site that passes one without the other
/// has got something wrong.
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
/// The order backends are filled in is the order `choose` would pick them — the
/// operator's default first, then the tightest fit. Spending the tightest
/// backend first is what keeps the roomier ones able to take a whole file later.
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

        // The piece that ends the file may be any length; every other piece has
        // to be a whole number of parts, so that no segment's parts have to know
        // about the segment after it.
        let length = if remaining <= free {
            remaining
        } else {
            let aligned = (free / rules.part_size) * rules.part_size;
            if aligned < floor {
                continue;
            }
            aligned
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
/// With splitting off this is the largest single backend, which is what
/// [`largest_acceptable`](super::placement::largest_acceptable) already says.
///
/// With it on, it is an **upper bound** rather than a promise. Everything
/// [`plan`] accepts is within it, which is the direction that matters — a figure
/// that accepted a file the planner then refused would be a `507` after the
/// upload had started. It can be a little generous in an awkward pool, because
/// the one segment allowed to be ragged is whichever backend the planner
/// happens to fill last, and no pool-wide figure knows which that will be. The
/// bound assumes the best case: every backend contributes its whole parts, and
/// the roomiest leftover is spent on the file's tail.
///
/// It is still nothing like the naive sum. A backend below [`min_segment`]
/// contributes nothing at all, so five backends with a megabyte each hold no
/// file — reporting five megabytes would be a number that explains nothing when
/// the upload is refused.
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
    let mut aligned_total = 0_u64;
    let mut best_tail = 0_u64;
    for candidate in candidates.iter().filter(|c| !c.read_only) {
        let free = candidate.free_bytes()?;
        if free < floor {
            continue;
        }
        let aligned = (free / rules.part_size) * rules.part_size;
        aligned_total = aligned_total.saturating_add(aligned);
        best_tail = best_tail.max(free - aligned);
    }

    // A pool whose every backend is below the floor can still take a file small
    // enough to sit whole in one of them, which is what `whole` is doing here.
    Some(aligned_total.saturating_add(best_tail).max(whole))
}

/// The backends worth filling, in the order they should be filled.
///
/// The operator's default first — they said it, and going elsewhere while it has
/// room would quietly undo that. Then the tightest fit, so the roomiest backend
/// is the last one spent and stays able to take a whole file. Unlimited ones
/// sort last for the same reason: they have nothing to preserve.
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
        /// Free space in whole parts, sometimes with a ragged tail, so that both
        /// the aligned and the unaligned case are generated. Named by position
        /// afterwards — two backends with the same name is not a pool that can
        /// exist, and generating one only tests the test.
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
    /// refused everything would pass every one of them. This keeps that from
    /// happening quietly: over a fixed sweep of the same space the generators
    /// draw from, a healthy share of the plans have to be genuinely split.
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

        /// Every piece but the last is a whole number of parts and worth having.
        /// The last is the one S3 lets be short.
        #[test]
        fn every_segment_but_the_last_is_aligned_and_worth_having(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let Some(segments) = plan(&pool, size, RULES) else { return Ok(()) };

            let floor = min_segment(PART);
            for segment in segments.iter().take(segments.len().saturating_sub(1)) {
                prop_assert_eq!(
                    segment.length % PART, 0,
                    "an unaligned cut: {:?}", segments
                );
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

        /// A plan is never produced for a file the pool said it could not take,
        /// and the figure is not so conservative that it refuses what it could
        /// have held.
        #[test]
        fn capacity_agrees_with_what_can_be_planned(
            pool in a_pool(),
            size in 1_u64..600 * 1024 * 1024,
        ) {
            let room = capacity(&pool, RULES).unwrap_or(u64::MAX);
            if plan(&pool, size, RULES).is_some() {
                prop_assert!(
                    size <= room,
                    "planned {size} bytes into a pool that claims {room}"
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
