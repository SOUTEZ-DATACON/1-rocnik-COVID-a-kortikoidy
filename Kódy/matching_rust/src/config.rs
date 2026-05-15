use chrono::NaiveDate;
use clap::Parser;

use crate::types::InjMode;

#[derive(Parser, Debug)]
#[command(name = "matching-analysis", about = "Matching analysis in Rust")]
pub struct Cli {
    /// Insurance company: "cpzp", "ozp", or "both_companies"
    #[arg(long, default_value = "cpzp")]
    pub company: String,

    /// Start of insurance coverage filter
    #[arg(long, default_value = "2015-01-01")]
    pub insurance_start: NaiveDate,

    /// End of insurance coverage filter
    #[arg(long, default_value = "2023-12-31")]
    pub insurance_end: NaiveDate,

    /// How many years to shift the analysis window back
    #[arg(long, default_value_t = 3, allow_negative_numbers = true)]
    pub year_offset: i32,

    /// Use unified effect baseline (ratio mode)
    #[arg(long, default_value_t = false)]
    pub unified_effect_baseline: bool,

    /// Only analyse INJ prescriptions
    #[arg(long, default_value_t = false)]
    pub inj_analysis: bool,

    /// Analyse every prescription regardless of form
    #[arg(long, default_value_t = false)]
    pub every_prescription: bool,

    /// Treat every L04 (immunosuppressive) prescription as a corticoid prescription
    /// with prednison-equivalent 500 mg. Not strictly correct, but gives a rough
    /// signal of what the cohorts look like once immunosuppressives are folded in.
    #[arg(long, default_value_t = false)]
    pub immuno_as_corticoid_500pe: bool,

    /// Number of matching runs
    #[arg(long, default_value_t = 100)]
    pub num_runs: usize,

    /// Path to the data directory containing CSV files
    #[arg(long, default_value = "./DATACON_data")]
    pub data_dir: String,

    /// Output directory for results
    #[arg(long, default_value = "./out")]
    pub out_dir: String,

    /// Enable per-specialty PE decomposition
    #[arg(long, default_value_t = false)]
    pub specialty_analysis: bool,
}

/// Derived configuration used throughout the pipeline.
pub struct Config {
    pub company: String,
    pub insurance_start: NaiveDate,
    pub insurance_end: NaiveDate,
    pub year_offset: i32,
    pub unified_effect_baseline: bool,
    pub inj_mode: InjMode,
    pub immuno_as_corticoid_500pe: bool,
    pub num_runs: usize,
    pub data_dir: String,
    pub out_dir: String,
    pub specialty_analysis: bool,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Self {
        let inj_mode = if cli.every_prescription {
            InjMode::All
        } else if cli.inj_analysis {
            InjMode::InjOnly
        } else {
            InjMode::NonInj
        };

        Config {
            company: cli.company.clone(),
            insurance_start: cli.insurance_start,
            insurance_end: cli.insurance_end,
            year_offset: cli.year_offset,
            unified_effect_baseline: cli.unified_effect_baseline,
            inj_mode,
            immuno_as_corticoid_500pe: cli.immuno_as_corticoid_500pe,
            num_runs: cli.num_runs,
            data_dir: cli.data_dir.clone(),
            out_dir: cli.out_dir.clone(),
            specialty_analysis: cli.specialty_analysis,
        }
    }

    pub fn day_offset(&self) -> chrono::Duration {
        chrono::Duration::days(self.year_offset as i64 * 365)
    }

    /// First day of the analysis window (shifted back by year_offset).
    pub fn start_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(2021, 1, 1).unwrap() - self.day_offset()
    }

    /// Last day of the analysis window.
    pub fn end_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(2022, 2, 28).unwrap() - self.day_offset()
    }

    pub fn year_for_age(&self) -> i32 {
        2021 - self.year_offset
    }

    /// All dates between start_date and end_date (inclusive).
    pub fn anchor_dates(&self) -> Vec<NaiveDate> {
        let start = self.start_date();
        let end = self.end_date();
        let n = (end - start).num_days() + 1;
        (0..n).map(|i| start + chrono::Duration::days(i)).collect()
    }

    pub fn inj_subfolder(&self) -> &str {
        match self.inj_mode {
            InjMode::InjOnly => "inj_analysis",
            InjMode::All => "every_prescription_analysis",
            InjMode::NonInj => "non_inj_analysis",
        }
    }

    pub fn effect_baseline_folder(&self) -> &str {
        if self.unified_effect_baseline {
            "unified_effect_baseline"
        } else {
            "different_effect_baseline"
        }
    }

    /// Optional extra subfolder describing the immunosuppressive override.
    /// When `--immuno-as-corticoid-500pe` is on, results live under this
    /// dedicated branch so they never collide with the standard baselines.
    pub fn immuno_subfolder(&self) -> Option<&'static str> {
        if self.immuno_as_corticoid_500pe {
            Some("immuno_as_corticoid_500pe")
        } else {
            None
        }
    }
}
