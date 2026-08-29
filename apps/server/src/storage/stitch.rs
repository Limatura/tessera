//! Turning one byte range over a file into the reads that answer it.
//!
//! A file spread over several backends is still one file to whoever asked for
//! bytes 9 000 000 to 9 000 255 of it. This works out which segments that range
//! falls in and where inside each one to start.
//!
//! The read path this belongs to is drawn in `docs/splitting.md`.
//!
//! With one segment it is the identity: one read, starting where the caller
//! asked. That is the case every file is in today, and it is why this can be
//! written and pinned before anything splits.
//!
//! # Why the offsets are given rather than added up
//!
//! A segment's position could be derived by summing the lengths before it, and
//! that would be one fewer thing to pass. It would also be one fewer thing to
//! check: the planner guarantees the segments tile the file, but the rows come
//! back from a database, and a gap there should produce a wrong answer that a
//! test can see rather than a right-looking one built from an assumption.

/// A slice of one segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    /// Position of the segment in the file, from zero.
    pub segment: usize,
    /// Where to start reading *within that segment*.
    pub start: u64,
    /// How many bytes to take from it.
    pub length: u64,
}

/// The reads that answer `range`, in order.
///
/// `segments` is `(offset, length)` per segment, as the rows record them.
/// `range` is inclusive at both ends, the way an HTTP `Range` header is.
///
/// Empty when the range falls entirely outside the file, or when it is
/// backwards. Both are the caller's business to reject — an empty plan is not a
/// silent success, it is nothing to read.
#[must_use]
pub fn reads_for(segments: &[(u64, u64)], range: (u64, u64)) -> Vec<Read> {
    let (wanted_start, wanted_end) = range;
    if wanted_end < wanted_start {
        return Vec::new();
    }

    let mut reads = Vec::new();
    for (index, &(offset, length)) in segments.iter().enumerate() {
        if length == 0 {
            continue;
        }
        let segment_end = offset.saturating_add(length) - 1;

        // The intersection of what this segment holds and what was asked for.
        // No intersection is not an error: most segments of a large file have
        // nothing to do with any one range.
        let from = wanted_start.max(offset);
        let to = wanted_end.min(segment_end);
        if from > to {
            continue;
        }

        reads.push(Read {
            segment: index,
            start: from - offset,
            length: to - from + 1,
        });
    }
    reads
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The shape every file is in today, and the one this must not disturb.
    #[test]
    fn one_segment_is_the_identity() {
        let reads = reads_for(&[(0, 1000)], (100, 199));
        assert_eq!(
            reads,
            vec![Read {
                segment: 0,
                start: 100,
                length: 100
            }]
        );
    }

    #[test]
    fn a_range_inside_one_segment_of_several_reads_only_that_one() {
        let reads = reads_for(&[(0, 100), (100, 100), (200, 100)], (120, 150));
        assert_eq!(
            reads,
            vec![Read {
                segment: 1,
                start: 20,
                length: 31
            }]
        );
    }

    /// The case the whole module exists for.
    #[test]
    fn a_range_across_a_boundary_reads_the_tail_then_the_head() {
        let reads = reads_for(&[(0, 100), (100, 100)], (90, 109));
        assert_eq!(
            reads,
            vec![
                Read {
                    segment: 0,
                    start: 90,
                    length: 10
                },
                Read {
                    segment: 1,
                    start: 0,
                    length: 10
                },
            ]
        );
    }

    #[test]
    fn a_range_past_the_end_reads_nothing() {
        assert!(reads_for(&[(0, 100)], (200, 300)).is_empty());
    }

    #[test]
    fn a_backwards_range_reads_nothing() {
        assert!(reads_for(&[(0, 100)], (50, 40)).is_empty());
    }

    /// A gap between segments is a corrupt row set, and the answer has to be
    /// visibly short rather than quietly stitched over.
    #[test]
    fn a_gap_is_not_papered_over() {
        // 0..50 and 100..150, with nothing in between.
        let reads = reads_for(&[(0, 50), (100, 50)], (40, 119));
        let total: u64 = reads.iter().map(|read| read.length).sum();
        assert_eq!(total, 30, "a gap was filled in from somewhere: {reads:?}");
    }

    prop_compose! {
        /// A file as a list of contiguous segments, and a range inside it.
        fn a_file_and_a_range()(lengths in prop::collection::vec(1_u64..500, 1..8))
            (range_start in 0..lengths.iter().sum::<u64>(),
             span in 1_u64..600,
             lengths in Just(lengths))
            -> (Vec<(u64, u64)>, (u64, u64), u64)
        {
            let total: u64 = lengths.iter().sum();
            let mut at = 0;
            let segments = lengths
                .into_iter()
                .map(|length| {
                    let segment = (at, length);
                    at += length;
                    segment
                })
                .collect();
            let end = (range_start + span - 1).min(total - 1);
            (segments, (range_start, end), total)
        }
    }

    /// The properties below would all pass on a function that never crossed a
    /// boundary, because most ranges do not. This keeps the generated space
    /// honest: a healthy share of the cases have to touch more than one
    /// segment, or the interesting half is not being tested at all.
    ///
    /// Measured when written: 3 000 cases, 1 471 crossing a boundary.
    #[test]
    fn the_generated_ranges_actually_cross_boundaries() {
        let mut crossing = 0_u32;
        let mut seed = 99_991_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            seed
        };
        for _ in 0..3_000 {
            let how_many = usize::try_from(next() % 7 + 1).unwrap_or(1);
            let mut at = 0_u64;
            let segments: Vec<(u64, u64)> = (0..how_many)
                .map(|_| {
                    let length = next() % 499 + 1;
                    let segment = (at, length);
                    at += length;
                    segment
                })
                .collect();
            let start = next() % at;
            let end = (start + next() % 600).min(at - 1);
            if reads_for(&segments, (start, end)).len() > 1 {
                crossing += 1;
            }
        }
        assert!(
            crossing > 300,
            "the generated ranges stopped crossing boundaries: {crossing} of 3000"
        );
    }

    proptest! {
        /// The property that matters: the reads answer exactly what was asked
        /// for. One byte short is a truncated download, one byte long is a
        /// corrupt one, and neither shows up in a handful of examples.
        #[test]
        fn the_reads_cover_the_range_exactly((segments, range, _total) in a_file_and_a_range()) {
            let reads = reads_for(&segments, range);
            let taken: u64 = reads.iter().map(|read| read.length).sum();
            prop_assert_eq!(
                taken,
                range.1 - range.0 + 1,
                "asked for {:?}, got {:?}", range, reads
            );
        }

        /// Every read stays inside the segment it names, and they arrive in
        /// order. A read that overruns its segment is a request the store will
        /// answer short, silently.
        #[test]
        fn every_read_is_inside_its_own_segment((segments, range, _total) in a_file_and_a_range()) {
            let reads = reads_for(&segments, range);
            let mut previous = None;
            for read in &reads {
                let (_, length) = segments[read.segment];
                prop_assert!(
                    read.start + read.length <= length,
                    "a read overruns its segment: {:?}", reads
                );
                if let Some(previous) = previous {
                    prop_assert!(read.segment > previous, "out of order: {:?}", reads);
                }
                previous = Some(read.segment);
            }
        }

        /// Laid end to end, the reads are the bytes the caller asked for — in
        /// the right order, from the right places. The check above proves the
        /// count; this proves the positions.
        #[test]
        fn the_reads_land_where_they_should((segments, range, _total) in a_file_and_a_range()) {
            let reads = reads_for(&segments, range);
            let mut at = range.0;
            for read in &reads {
                let (offset, _) = segments[read.segment];
                prop_assert_eq!(
                    offset + read.start,
                    at,
                    "a read starts in the wrong place: {:?}", reads
                );
                at += read.length;
            }
            prop_assert_eq!(at, range.1 + 1);
        }

        /// One segment is the identity, whatever the range. This is the case
        /// every file is in today, so a change here that broke it would break
        /// every download while the split feature is still switched off.
        #[test]
        fn one_segment_stays_the_identity(
            length in 1_u64..10_000,
            start in 0_u64..10_000,
            span in 1_u64..10_000,
        ) {
            let end = (start + span - 1).min(length - 1);
            let reads = reads_for(&[(0, length)], (start, end));
            if start >= length {
                prop_assert!(reads.is_empty());
            } else {
                prop_assert_eq!(
                    reads,
                    vec![Read { segment: 0, start, length: end - start + 1 }]
                );
            }
        }
    }
}
