use std::collections::HashMap;

use chrono::NaiveDate;
use rand::Rng;
use rayon::prelude::*;

use crate::config::Config;
use crate::types::{AgeCohort, InjMode, PeGroupName, PeMap, PeRange, Person, SpecialtyGroup};

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

pub type EffectMap = HashMap<AgeCohort, HashMap<NaiveDate, f64>>;
pub type CiMap = HashMap<AgeCohort, HashMap<NaiveDate, (f64, f64)>>;

pub struct MatchingResult {
    pub median: EffectMap,
    pub iqr: CiMap,
    pub ci: CiMap,
    pub vax_median: EffectMap,
    pub novax_median: EffectMap,
    pub vax_ci: CiMap,
    pub novax_ci: CiMap,
    /// Per-specialty decomposition (only populated when --specialty-analysis)
    pub specialty: Option<HashMap<SpecialtyGroup, SpecialtyResult>>,
}

pub struct SpecialtyResult {
    pub median: EffectMap,
    pub ci: CiMap,
    pub vax_median: EffectMap,
    pub novax_median: EffectMap,
    pub vax_ci: CiMap,
    pub novax_ci: CiMap,
    /// How many vax persons in each cohort have at least one prescription from this specialty
    pub person_count: HashMap<AgeCohort, u64>,
}

// ---------------------------------------------------------------------------
// Single-run output (collected per thread, then merged)
// ---------------------------------------------------------------------------

type FlatEffects = HashMap<AgeCohort, HashMap<NaiveDate, f64>>;

struct RunResult {
    effects: FlatEffects,
    vax_effects: FlatEffects,
    novax_effects: FlatEffects,
    /// per-specialty vax/novax raw PE (only Some when specialty_analysis=true)
    spec_vax: Option<HashMap<SpecialtyGroup, FlatEffects>>,
    spec_novax: Option<HashMap<SpecialtyGroup, FlatEffects>>,
    spec_effects: Option<HashMap<SpecialtyGroup, FlatEffects>>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn run_matching_analysis(
    persons: &[Person],
    group_indices: &[usize],
    pe_map: &PeMap,
    config: &Config,
    group_name: PeGroupName,
    aggregation_days: i32,
) -> MatchingResult {
    let ref_year = config.year_for_age();
    let inj_mode = config.inj_mode;
    let unified = config.unified_effect_baseline;
    let epoch = config.start_date();
    let num_runs = config.num_runs;
    let spec = config.specialty_analysis;

    // Precompute per-vax-person invariants (avoids recalculating 100 times).
    let vax_data: Vec<VaxPersonData> = group_indices
        .iter()
        .filter_map(|&idx| {
            let p = &persons[idx];
            let vax_date = p.first_vax_date()?;
            let before_pe = p.pe_before(vax_date, inj_mode);
            if before_pe > 5000.0 {
                return None;
            }

            let pe_range = match group_name {
                PeGroupName::NeverPrescribed => PeRange::ZeroNoPre,
                PeGroupName::ZeroPeSuspectible => PeRange::ZeroPeSuspectible,
                _ => PeRange::from_pe(before_pe),
            };

            let ac = p.age_cohort(ref_year);
            let gender = p.gender;

            let candidates = pe_map
                .get(&vax_date)
                .and_then(|r| r.get(&pe_range))
                .and_then(|a| a.get(&ac))
                .and_then(|g| g.get(&gender));

            let candidates = match candidates {
                Some(c) if !c.is_empty() => c,
                _ => return None,
            };

            let after_pe = p.pe_after(vax_date, inj_mode);
            let agg_date = aggregate_date(vax_date, aggregation_days, epoch);

            Some(VaxPersonData {
                person_idx: idx,
                vax_date,
                before_pe,
                after_pe,
                ac,
                agg_date,
                candidate_indices: candidates.as_slice(),
            })
        })
        .collect();

    // Per-specialty person counts: how many vax persons have ≥1 prescription
    // from each specialty (deterministic, computed once).
    let spec_person_counts: HashMap<SpecialtyGroup, HashMap<AgeCohort, u64>> = if spec {
        let mut counts: HashMap<SpecialtyGroup, HashMap<AgeCohort, u64>> = HashMap::new();
        for vd in &vax_data {
            let p = &persons[vd.person_idx];
            let vax_date = vd.vax_date;
            let start = vax_date - chrono::Duration::days(365);
            let end = vax_date + chrono::Duration::days(365);
            for &sg in &SpecialtyGroup::ALL {
                let has_any = p.prescriptions.iter().any(|pr| {
                    inj_mode.matches(pr)
                        && pr.specialty == sg
                        && pr.date > start
                        && pr.date < end
                });
                if has_any {
                    *counts.entry(sg).or_default().entry(vd.ac).or_default() += 1;
                }
            }
        }
        counts
    } else {
        HashMap::new()
    };

    eprintln!(
        "    {} vax persons ready for matching ({} runs × {} threads)",
        vax_data.len(),
        num_runs,
        rayon::current_num_threads(),
    );

    // Run all iterations in parallel via Rayon
    let all_runs: Vec<RunResult> = (0..num_runs)
        .into_par_iter()
        .map(|_| single_run(persons, &vax_data, inj_mode, group_name, unified, spec))
        .collect();

    eprintln!("    all {num_runs} runs finished — computing statistics");

    // Merge into per-(cohort, date) value lists
    type ValueLists = HashMap<AgeCohort, HashMap<NaiveDate, Vec<f64>>>;
    let mut effects_lists: ValueLists = HashMap::new();
    let mut vax_lists: ValueLists = HashMap::new();
    let mut novax_lists: ValueLists = HashMap::new();

    let mut spec_vax_lists: HashMap<SpecialtyGroup, ValueLists> = HashMap::new();
    let mut spec_novax_lists: HashMap<SpecialtyGroup, ValueLists> = HashMap::new();
    let mut spec_effects_lists: HashMap<SpecialtyGroup, ValueLists> = HashMap::new();

    for run in &all_runs {
        merge_into(&mut effects_lists, &run.effects);
        merge_into(&mut vax_lists, &run.vax_effects);
        merge_into(&mut novax_lists, &run.novax_effects);

        if let Some(sv) = &run.spec_vax {
            for (&sg, eff) in sv {
                merge_into(spec_vax_lists.entry(sg).or_default(), eff);
            }
        }
        if let Some(sn) = &run.spec_novax {
            for (&sg, eff) in sn {
                merge_into(spec_novax_lists.entry(sg).or_default(), eff);
            }
        }
        if let Some(se) = &run.spec_effects {
            for (&sg, eff) in se {
                merge_into(spec_effects_lists.entry(sg).or_default(), eff);
            }
        }
    }

    let specialty = if spec {
        let mut map = HashMap::new();
        let empty: ValueLists = HashMap::new();
        for &sg in &SpecialtyGroup::ALL {
            let sv = spec_vax_lists.get(&sg).unwrap_or(&empty);
            let sn = spec_novax_lists.get(&sg).unwrap_or(&empty);
            let se = spec_effects_lists.get(&sg).unwrap_or(&empty);
            let pc = spec_person_counts.get(&sg).cloned().unwrap_or_default();
            map.insert(sg, SpecialtyResult {
                median: compute_median(se),
                ci: compute_percentiles(se, 2.5, 97.5),
                vax_median: compute_median(sv),
                novax_median: compute_median(sn),
                vax_ci: compute_percentiles(sv, 2.5, 97.5),
                novax_ci: compute_percentiles(sn, 2.5, 97.5),
                person_count: pc,
            });
        }
        Some(map)
    } else {
        None
    };

    MatchingResult {
        median: compute_median(&effects_lists),
        iqr: compute_percentiles(&effects_lists, 25.0, 75.0),
        ci: compute_percentiles(&effects_lists, 2.5, 97.5),
        vax_median: compute_median(&vax_lists),
        novax_median: compute_median(&novax_lists),
        vax_ci: compute_percentiles(&vax_lists, 2.5, 97.5),
        novax_ci: compute_percentiles(&novax_lists, 2.5, 97.5),
        specialty,
    }
}

// ---------------------------------------------------------------------------
// Precomputed vax-person data (references into pe_map candidate slices)
// ---------------------------------------------------------------------------

struct VaxPersonData<'a> {
    person_idx: usize,
    vax_date: NaiveDate,
    before_pe: f64,
    after_pe: f64,
    ac: AgeCohort,
    agg_date: NaiveDate,
    candidate_indices: &'a [usize],
}

// ---------------------------------------------------------------------------
// One matching run
// ---------------------------------------------------------------------------

fn single_run(
    persons: &[Person],
    vax_data: &[VaxPersonData],
    inj_mode: InjMode,
    group_name: PeGroupName,
    unified: bool,
    specialty_analysis: bool,
) -> RunResult {
    let mut rng = rand::thread_rng();

    let mut vax_before: HashMap<(AgeCohort, NaiveDate), f64> = HashMap::new();
    let mut vax_after: HashMap<(AgeCohort, NaiveDate), f64> = HashMap::new();
    let mut novax_before: HashMap<(AgeCohort, NaiveDate), f64> = HashMap::new();
    let mut novax_after: HashMap<(AgeCohort, NaiveDate), f64> = HashMap::new();
    let mut count: HashMap<(AgeCohort, NaiveDate), f64> = HashMap::new();

    // Per-specialty accumulators: spec -> (cohort, date) -> sum
    type SpecAcc = HashMap<SpecialtyGroup, HashMap<(AgeCohort, NaiveDate), f64>>;
    let mut spec_vax_before: SpecAcc = HashMap::new();
    let mut spec_vax_after: SpecAcc = HashMap::new();
    let mut spec_novax_before: SpecAcc = HashMap::new();
    let mut spec_novax_after: SpecAcc = HashMap::new();

    for vd in vax_data {
        let matched_idx = vd.candidate_indices[rng.gen_range(0..vd.candidate_indices.len())];
        let matched = &persons[matched_idx];
        let vax_person = &persons[vd.person_idx];

        let nb = matched.pe_before(vd.vax_date, inj_mode);
        let na = matched.pe_after(vd.vax_date, inj_mode);

        let key = (vd.ac, vd.agg_date);
        *vax_before.entry(key).or_default() += vd.before_pe;
        *vax_after.entry(key).or_default() += vd.after_pe;
        *novax_before.entry(key).or_default() += nb;
        *novax_after.entry(key).or_default() += na;
        *count.entry(key).or_default() += 1.0;

        if specialty_analysis {
            let vax_spec_b = vax_person.pe_before_by_specialty(vd.vax_date, inj_mode);
            let vax_spec_a = vax_person.pe_after_by_specialty(vd.vax_date, inj_mode);
            let novax_spec_b = matched.pe_before_by_specialty(vd.vax_date, inj_mode);
            let novax_spec_a = matched.pe_after_by_specialty(vd.vax_date, inj_mode);

            for &sg in &SpecialtyGroup::ALL {
                if let Some(&v) = vax_spec_b.get(&sg) {
                    *spec_vax_before.entry(sg).or_default().entry(key).or_default() += v;
                }
                if let Some(&v) = vax_spec_a.get(&sg) {
                    *spec_vax_after.entry(sg).or_default().entry(key).or_default() += v;
                }
                if let Some(&v) = novax_spec_b.get(&sg) {
                    *spec_novax_before.entry(sg).or_default().entry(key).or_default() += v;
                }
                if let Some(&v) = novax_spec_a.get(&sg) {
                    *spec_novax_after.entry(sg).or_default().entry(key).or_default() += v;
                }
            }
        }
    }

    // Compute total effects (unchanged logic)
    let mut effects: FlatEffects = HashMap::new();
    let mut vax_eff: FlatEffects = HashMap::new();
    let mut novax_eff: FlatEffects = HashMap::new();

    let all_keys: Vec<_> = vax_before.keys().chain(novax_before.keys()).copied().collect();
    for key in all_keys {
        let (ac, dt) = key;
        let vb = *vax_before.get(&key).unwrap_or(&0.0);
        let va = *vax_after.get(&key).unwrap_or(&0.0);
        let nb = *novax_before.get(&key).unwrap_or(&0.0);
        let na = *novax_after.get(&key).unwrap_or(&0.0);
        let n = *count.get(&key).unwrap_or(&1.0);

        if !unified && !group_name.is_zero_pe_group() {
            if vb != 0.0 && nb != 0.0 {
                let ve = va / vb;
                let ne = na / nb;
                vax_eff.entry(ac).or_default().insert(dt, ve);
                novax_eff.entry(ac).or_default().insert(dt, ne);
                effects.entry(ac).or_default().insert(dt, ve - ne);
            }
        } else {
            let ve = (va - vb) / n;
            let ne = (na - nb) / n;
            vax_eff.entry(ac).or_default().insert(dt, ve);
            novax_eff.entry(ac).or_default().insert(dt, ne);
            if ne != 0.0 {
                effects.entry(ac).or_default().insert(dt, ve / ne);
            }
        }
    }

    // Compute per-specialty raw PE averages (per-person average = sum / count)
    // and per-specialty treatment effects (vax_diff - novax_diff)
    let (sv, sn, se) = if specialty_analysis {
        let mut sv_out: HashMap<SpecialtyGroup, FlatEffects> = HashMap::new();
        let mut sn_out: HashMap<SpecialtyGroup, FlatEffects> = HashMap::new();
        let mut se_out: HashMap<SpecialtyGroup, FlatEffects> = HashMap::new();
        for &sg in &SpecialtyGroup::ALL {
            let svb = spec_vax_before.get(&sg);
            let sva = spec_vax_after.get(&sg);
            let snb = spec_novax_before.get(&sg);
            let sna = spec_novax_after.get(&sg);

            for &key in count.keys() {
                let (ac, dt) = key;
                let n = *count.get(&key).unwrap_or(&1.0);

                let vb = svb.and_then(|m| m.get(&key)).copied().unwrap_or(0.0);
                let va = sva.and_then(|m| m.get(&key)).copied().unwrap_or(0.0);
                let nb = snb.and_then(|m| m.get(&key)).copied().unwrap_or(0.0);
                let na = sna.and_then(|m| m.get(&key)).copied().unwrap_or(0.0);

                let vax_diff = (va - vb) / n;
                let novax_diff = (na - nb) / n;
                sv_out.entry(sg).or_default().entry(ac).or_default().insert(dt, vax_diff);
                sn_out.entry(sg).or_default().entry(ac).or_default().insert(dt, novax_diff);
                se_out.entry(sg).or_default().entry(ac).or_default().insert(dt, vax_diff - novax_diff);
            }
        }
        (Some(sv_out), Some(sn_out), Some(se_out))
    } else {
        (None, None, None)
    };

    RunResult {
        effects,
        vax_effects: vax_eff,
        novax_effects: novax_eff,
        spec_vax: sv,
        spec_novax: sn,
        spec_effects: se,
    }
}

// ---------------------------------------------------------------------------
// Statistics helpers
// ---------------------------------------------------------------------------

fn merge_into(
    dst: &mut HashMap<AgeCohort, HashMap<NaiveDate, Vec<f64>>>,
    src: &HashMap<AgeCohort, HashMap<NaiveDate, f64>>,
) {
    for (&ac, date_map) in src {
        for (&dt, &val) in date_map {
            dst.entry(ac).or_default().entry(dt).or_default().push(val);
        }
    }
}

fn compute_median(
    data: &HashMap<AgeCohort, HashMap<NaiveDate, Vec<f64>>>,
) -> EffectMap {
    let mut out = HashMap::new();
    for (&ac, date_map) in data {
        for (&dt, vals) in date_map {
            let median = percentile(vals, 50.0);
            out.entry(ac).or_insert_with(HashMap::new).insert(dt, median);
        }
    }
    out
}

fn compute_percentiles(
    data: &HashMap<AgeCohort, HashMap<NaiveDate, Vec<f64>>>,
    lo: f64,
    hi: f64,
) -> CiMap {
    let mut out = HashMap::new();
    for (&ac, date_map) in data {
        for (&dt, vals) in date_map {
            let p_lo = percentile(vals, lo);
            let p_hi = percentile(vals, hi);
            out.entry(ac)
                .or_insert_with(HashMap::new)
                .insert(dt, (p_lo, p_hi));
        }
    }
    out
}

fn percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let idx = (pct / 100.0) * (n as f64 - 1.0);
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi || hi >= n {
        sorted[lo.min(n - 1)]
    } else {
        let frac = idx - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

fn aggregate_date(date: NaiveDate, window_days: i32, epoch: NaiveDate) -> NaiveDate {
    if window_days <= 1 {
        return date;
    }
    let days_since = (date - epoch).num_days() as i32;
    let start = (days_since / window_days) * window_days;
    epoch + chrono::Duration::days(start as i64)
}
