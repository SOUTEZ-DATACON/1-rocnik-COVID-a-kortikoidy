use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use crate::matching::MatchingResult;
use crate::types::{AgeCohort, PeGroupName, Person, SpecialtyGroup};

#[derive(Serialize)]
struct SummaryRow {
    #[serde(rename = "věk")]
    age: String,
    #[serde(rename = "Med")]
    median: Option<f64>,
    #[serde(rename = "IQR")]
    iqr: Option<(f64, f64)>,
    #[serde(rename = "95% CI")]
    ci: Option<(f64, f64)>,
    #[serde(rename = "počet očko")]
    vax_count: u64,
    #[serde(rename = "počet s PE po")]
    vax_with_pe_after: u64,
    #[serde(rename = "očko po-před")]
    vax_effect: Option<f64>,
    #[serde(rename = "očko 95% CI")]
    vax_effect_ci: Option<(f64, f64)>,
    #[serde(rename = "neočko po-před")]
    novax_effect: Option<f64>,
    #[serde(rename = "neočko 95% CI")]
    novax_effect_ci: Option<(f64, f64)>,
}

#[derive(Serialize)]
struct SpecialtyRow {
    #[serde(rename = "specializace")]
    specialty: String,
    #[serde(rename = "věk")]
    age: String,
    #[serde(rename = "počet očko")]
    vax_count: u64,
    #[serde(rename = "počet u spec.")]
    spec_count: u64,
    #[serde(rename = "Med")]
    median: Option<f64>,
    #[serde(rename = "95% CI")]
    ci: Option<(f64, f64)>,
    #[serde(rename = "očko PE po-před")]
    vax_pe: Option<f64>,
    #[serde(rename = "očko 95% CI")]
    vax_ci: Option<(f64, f64)>,
    #[serde(rename = "neočko PE po-před")]
    novax_pe: Option<f64>,
    #[serde(rename = "neočko 95% CI")]
    novax_ci: Option<(f64, f64)>,
}

pub fn write_results(
    result: &MatchingResult,
    persons: &[Person],
    group_indices: &[usize],
    config: &Config,
    group_name: PeGroupName,
    aggregation_days: i32,
) {
    let inj_sub = config.inj_subfolder();
    let effect_sub = config.effect_baseline_folder();
    let immuno_sub = config.immuno_subfolder();
    let year_back = format!("{}_years_back_matching_analysis", config.year_offset);

    let out = &config.out_dir;
    let immuno_segment = match immuno_sub {
        Some(s) => format!("{s}/"),
        None => String::new(),
    };
    let inj_segment = if inj_sub.is_empty() {
        String::new()
    } else {
        format!("{inj_sub}/")
    };
    let folder = format!(
        "{out}/{company}/matching_analysis/{immuno_segment}{inj_segment}{effect_sub}/{year_back}/whole_period/{group}/{agg}_days_aggregation",
        company = config.company,
        group = group_name.label(),
        agg = aggregation_days,
    );

    fs::create_dir_all(&folder).expect("cannot create output dir");

    // Vax-date counts per cohort
    let ref_year = config.year_for_age();
    let inj_mode = config.inj_mode;
    let mut vax_counts: std::collections::HashMap<AgeCohort, u64> = std::collections::HashMap::new();
    let mut vax_pe_after: std::collections::HashMap<AgeCohort, u64> = std::collections::HashMap::new();
    for &idx in group_indices {
        let p = &persons[idx];
        let ac = p.age_cohort(ref_year);
        *vax_counts.entry(ac).or_default() += 1;
        if let Some(vax_date) = p.first_vax_date() {
            if p.pe_after(vax_date, inj_mode) > 0.0 {
                *vax_pe_after.entry(ac).or_default() += 1;
            }
        }
    }

    let rows: Vec<SummaryRow> = AgeCohort::ALL
        .iter()
        .map(|&ac| {
            let get_first = |map: &crate::matching::EffectMap| -> Option<f64> {
                map.get(&ac)
                    .and_then(|dm| dm.values().next())
                    .copied()
            };
            let get_first_ci = |map: &crate::matching::CiMap| -> Option<(f64, f64)> {
                map.get(&ac)
                    .and_then(|dm| dm.values().next())
                    .copied()
            };

            SummaryRow {
                age: ac.label().to_string(),
                median: get_first(&result.median),
                iqr: get_first_ci(&result.iqr),
                ci: get_first_ci(&result.ci),
                vax_count: *vax_counts.get(&ac).unwrap_or(&0),
                vax_with_pe_after: *vax_pe_after.get(&ac).unwrap_or(&0),
                vax_effect: get_first(&result.vax_median),
                vax_effect_ci: get_first_ci(&result.vax_ci),
                novax_effect: get_first(&result.novax_median),
                novax_effect_ci: get_first_ci(&result.novax_ci),
            }
        })
        .collect();

    let json_path = Path::new(&folder).join("../effects_summary.json");
    let parent = json_path.parent().unwrap();
    fs::create_dir_all(parent).ok();
    let json = serde_json::to_string(&rows).expect("JSON serialization failed");
    fs::write(&json_path, &json).expect("cannot write JSON");

    eprintln!("  wrote {}", json_path.display());

    // Per-specialty JSON
    if let Some(ref spec_map) = result.specialty {
        let mut spec_rows: Vec<SpecialtyRow> = Vec::new();
        for &sg in &SpecialtyGroup::ALL {
            let sr = spec_map.get(&sg);
            for &ac in &AgeCohort::ALL {
                let get_first = |map: &crate::matching::EffectMap| -> Option<f64> {
                    map.get(&ac).and_then(|dm| dm.values().next()).copied()
                };
                let get_first_ci = |map: &crate::matching::CiMap| -> Option<(f64, f64)> {
                    map.get(&ac).and_then(|dm| dm.values().next()).copied()
                };
                let sc = sr
                    .map(|s| *s.person_count.get(&ac).unwrap_or(&0))
                    .unwrap_or(0);
                spec_rows.push(SpecialtyRow {
                    specialty: sg.label().to_string(),
                    age: ac.label().to_string(),
                    vax_count: *vax_counts.get(&ac).unwrap_or(&0),
                    spec_count: sc,
                    median: sr.and_then(|s| get_first(&s.median)),
                    ci: sr.and_then(|s| get_first_ci(&s.ci)),
                    vax_pe: sr.and_then(|s| get_first(&s.vax_median)),
                    vax_ci: sr.and_then(|s| get_first_ci(&s.vax_ci)),
                    novax_pe: sr.and_then(|s| get_first(&s.novax_median)),
                    novax_ci: sr.and_then(|s| get_first_ci(&s.novax_ci)),
                });
            }
        }

        let spec_path = Path::new(&folder).join("../specialty_effects.json");
        let spec_json = serde_json::to_string_pretty(&spec_rows).expect("JSON failed");
        fs::write(&spec_path, &spec_json).expect("cannot write specialty JSON");
        eprintln!("  wrote {}", spec_path.display());
    }
}
