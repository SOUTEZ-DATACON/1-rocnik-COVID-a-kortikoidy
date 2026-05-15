use chrono::NaiveDate;
use rayon::prelude::*;

use crate::config::Config;
use crate::types::{AgeCohort, Gender, InjMode, PeMap, PeRange, Person};

/// Build the PeMap: for every anchor date, bucket each novax person by
/// (PeRange, AgeCohort, Gender) so the matching step can randomly pick one.
///
/// Mirrors `PrednisonWindowsComputer.get_prednison_windows()` in Python.
pub fn compute_pe_map(
    persons: &[Person],
    novax_indices: &[usize],
    anchor_dates: &[NaiveDate],
    config: &Config,
) -> PeMap {
    let ref_year = config.year_for_age();
    let inj_mode = config.inj_mode;
    let total = novax_indices.len();

    eprintln!(
        "  pe_map: processing {total} persons across {} threads...",
        rayon::current_num_threads(),
    );

    let map = novax_indices
        .par_iter()
        .fold(
            PeMap::new,
            |mut map, &pidx| {
                process_person(&mut map, persons, pidx, anchor_dates, ref_year, inj_mode);
                map
            },
        )
        .reduce(PeMap::new, |mut a, b| {
            merge_maps(&mut a, b);
            a
        });

    eprintln!("  pe_map: done ({total} persons)");
    map
}

fn process_person(
    map: &mut PeMap,
    persons: &[Person],
    pidx: usize,
    anchor_dates: &[NaiveDate],
    ref_year: i32,
    inj_mode: InjMode,
) {
    let person = &persons[pidx];
    let ac = person.age_cohort(ref_year);
    let gender = person.gender;

    // Precompute earliest prescription date once (across ALL forms, not just
    // inj-filtered ones) – used to decide ZeroNoPre vs ZeroPeSuspectible.
    let earliest_rx = person.prescriptions.iter().map(|p| p.date).min();

    let valid_rx: Vec<_> = person
        .prescriptions
        .iter()
        .filter(|pr| inj_mode.matches(pr))
        .collect();

    if valid_rx.is_empty() {
        for &anchor in anchor_dates {
            insert(map, anchor, PeRange::ZeroPe, ac, gender, pidx);
            if earliest_rx.is_some_and(|d| d < anchor) {
                insert(map, anchor, PeRange::ZeroPeSuspectible, ac, gender, pidx);
            } else {
                insert(map, anchor, PeRange::ZeroNoPre, ac, gender, pidx);
            }
        }
        return;
    }

    // For each anchor date, sum PE in the 365 days before it.
    let mut per_anchor = vec![0.0f64; anchor_dates.len()];

    for pr in &valid_rx {
        let start = pr.date;
        let end = pr.date + chrono::Duration::days(365);
        let lo = anchor_dates.partition_point(|d| *d <= start);
        let hi = anchor_dates.partition_point(|d| *d <= end);
        for idx in lo..hi {
            per_anchor[idx] += pr.prednison_equiv;
        }
    }

    for (aidx, &anchor) in anchor_dates.iter().enumerate() {
        let pe_val = per_anchor[aidx];
        if pe_val == 0.0 {
            insert(map, anchor, PeRange::ZeroPe, ac, gender, pidx);
            if earliest_rx.is_some_and(|d| d < anchor) {
                insert(map, anchor, PeRange::ZeroPeSuspectible, ac, gender, pidx);
            } else {
                insert(map, anchor, PeRange::ZeroNoPre, ac, gender, pidx);
            }
        } else {
            let range = PeRange::from_pe(pe_val);
            insert(map, anchor, range, ac, gender, pidx);
        }
    }
}

fn insert(
    map: &mut PeMap,
    anchor: NaiveDate,
    range: PeRange,
    ac: AgeCohort,
    gender: Gender,
    person_idx: usize,
) {
    map.entry(anchor)
        .or_default()
        .entry(range)
        .or_default()
        .entry(ac)
        .or_default()
        .entry(gender)
        .or_default()
        .push(person_idx);
}

fn merge_maps(dst: &mut PeMap, src: PeMap) {
    for (date, ranges) in src {
        let dst_ranges = dst.entry(date).or_default();
        for (range, cohorts) in ranges {
            let dst_cohorts = dst_ranges.entry(range).or_default();
            for (cohort, genders) in cohorts {
                let dst_genders = dst_cohorts.entry(cohort).or_default();
                for (gender, indices) in genders {
                    dst_genders.entry(gender).or_default().extend(indices);
                }
            }
        }
    }
}
