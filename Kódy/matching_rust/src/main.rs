mod config;
mod data_loader;
mod matching;
mod pe_windows;
mod results;
mod types;

use clap::Parser;
use std::time::Instant;

use config::{Cli, Config};
use types::PeGroupName;

fn main() {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);

    eprintln!("=== Matching Analysis (Rust) ===");
    eprintln!(
        "  company={}, year_offset={}, runs={}, inj={:?}",
        config.company, config.year_offset, config.num_runs, config.inj_mode,
    );

    let t0 = Instant::now();

    // 1. Load persons from CSV
    let loaded = data_loader::load_persons(&config);
    let persons = &loaded.persons;
    eprintln!("  loaded in {:.1}s", t0.elapsed().as_secs_f64());

    // 2. Split into vax / novax
    let vax_idx = data_loader::vax_people(persons, &config);
    let novax_idx = data_loader::novax_people(persons, &config);
    eprintln!(
        "  vax={}, novax={} (after insurance filter)",
        vax_idx.len(),
        novax_idx.len(),
    );

    // 3. Compute PeMap from novax people
    let t1 = Instant::now();
    let anchor_dates = config.anchor_dates();
    let pe_map = pe_windows::compute_pe_map(persons, &novax_idx, &anchor_dates, &config);
    eprintln!("  pe_map computed in {:.1}s", t1.elapsed().as_secs_f64());

    // 4. Define groups and run matching for each
    let aggregation_days = anchor_dates.len() as i32;

    let groups = [
        PeGroupName::NeverPrescribed,
        PeGroupName::ZeroPeSuspectible,
        PeGroupName::ZeroPe,
        PeGroupName::OneToFiveHundredPe,
        PeGroupName::FiveHundredToFiveThousandPe,
    ];

    for group_name in &groups {
        let t2 = Instant::now();
        let group_idx =
            data_loader::filter_group(persons, &vax_idx, *group_name, config.inj_mode);
        eprintln!(
            "\n  group {} — {} persons",
            group_name.label(),
            group_idx.len(),
        );

        if group_idx.is_empty() {
            eprintln!("    skipping (empty group)");
            continue;
        }

        let result = matching::run_matching_analysis(
            persons,
            &group_idx,
            &pe_map,
            &config,
            *group_name,
            aggregation_days,
        );

        results::write_results(
            &result,
            persons,
            &group_idx,
            &config,
            *group_name,
            aggregation_days,
        );

        eprintln!(
            "    group done in {:.1}s",
            t2.elapsed().as_secs_f64(),
        );
    }

    eprintln!("\n=== total: {:.1}s ===", t0.elapsed().as_secs_f64());
}
