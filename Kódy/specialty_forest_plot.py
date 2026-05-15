"""
Per-specialty forest plots in the same style as forest_plot.py:
5 periods (3y back → 1y forward) per age cohort, one file per
(specialty × PE bucket).

Two plot types per (specialty × bucket):
  1. Treatment effect (Med + 95% CI) — vax−novax PE difference per person
  2. Raw vax (filled) and novax (open) PE change with 95% CI whiskers
"""

import argparse
import json
import multiprocessing as mp
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

DEFAULT_OUT_ROOT = Path("out")
DEFAULT_COMPANIES = ["cpzp"]

COMPANY_LABELS = {
    "cpzp": "Data z ČPZP",
    "ozp": "Data z OZP",
    "both_companies": "Souhrnná data",
}

INJ_LABELS = {
    "non_inj_analysis": "Jen systémově podávané (bez injekcí)",
    "inj_analysis": "Jen injekce",
}

DIL5 = "Výsledky - srovnání s virtuálním očkováním (5. díl)"

PUB_BUCKETS = ["0_PE", "1_to_500_PE", "500_to_5000_PE", "NEVER_PRESCRIBED"]

ANALYSIS_MODES = [
    "non_inj_analysis",
    "inj_analysis",
    "every_prescription_analysis",
]

EFFECT_BASELINES = [
    "different_effect_baseline",
    "unified_effect_baseline",
]

MODE_LABELS = {
    "non_inj_analysis": "non-inj",
    "inj_analysis": "inj",
    "every_prescription_analysis": "every-rx",
}

BASELINE_LABELS = {
    "different_effect_baseline": "diff-baseline",
    "unified_effect_baseline": "uni-baseline",
}

PERIODS = [
    ("3_years_back_matching_analysis", "3 roky zpět"),
    ("2_years_back_matching_analysis", "2 roky zpět"),
    ("1_years_back_matching_analysis", "1 rok zpět"),
    ("0_years_back_matching_analysis", "Očkovací období"),
    ("-1_years_back_matching_analysis", "1 rok dopředu"),
]

PERIOD_STYLES = [
    {"color": "#888888", "marker": "v", "ms": 5},
    {"color": "#888888", "marker": "D", "ms": 4.5},
    {"color": "#888888", "marker": "s", "ms": 4.5},
    {"color": "#2171b5", "marker": "o", "ms": 6},
    {"color": "#888888", "marker": "^", "ms": 5},
]

BUCKETS = [
    "0_PE",
    "1_to_500_PE",
    "500_to_5000_PE",
    "ZERO_PE_SUSPECTIBLE",
    "NEVER_PRESCRIBED",
]

BUCKET_LABELS = {
    "0_PE": "0 PE",
    "1_to_500_PE": "1–500 PE",
    "500_to_5000_PE": "500–5 000 PE",
    "ZERO_PE_SUSPECTIBLE": "0 PE (susceptible)",
    "NEVER_PRESCRIBED": "Nikdy předepsáno",
}

AGE_ORDER = ["12-15", "16-22", "23-29", "30-39", "40-49", "50-59"]
AGE_LABELS = {
    "12-15": "12–15 let",
    "16-22": "16–22 let",
    "23-29": "23–29 let",
    "30-39": "30–39 let",
    "40-49": "40–49 let",
    "50-59": "50–59 let",
}

SPECIALTIES = [
    "ortopedie_chirurgie",
    "revmatologie",
    "neurologie",
    "praktik",
    "gastroenterologie",
    "pneumo_ftizeo",
    "interna",
    "other",
]

SPEC_LABELS = {
    "ortopedie_chirurgie": "Ortopedie + chirurgie",
    "revmatologie": "Revmatologie",
    "neurologie": "Neurologie",
    "praktik": "Praktický lékař",
    "gastroenterologie": "Gastroenterologie",
    "pneumo_ftizeo": "Pneumologie + ftizeo",
    "interna": "Interna",
    "other": "Ostatní",
}

VAX_COLOR = "#2171b5"
NOVAX_COLOR = "#d94801"

N_PERIODS = len(PERIODS)
ROW_HEIGHT = N_PERIODS + 1.5


def _fmt_n(n: int) -> str:
    return f"n={n:,}".replace(",", "\u2009")


def load_specialty_data(directory: Path, bucket: str) -> dict | None:
    path = directory / bucket / "specialty_effects.json"
    if not path.exists():
        return None
    with open(path) as f:
        rows = json.load(f)
    result = {}
    for row in rows:
        spec = row["specializace"]
        age = row["věk"]
        result.setdefault(spec, {})[age] = row
    return result


def make_spec_treatment_effect_plot(
    bucket: str, spec: str, ax: plt.Axes, *, base: Path
):
    """Treatment-effect forest plot for one specialty: Med + 95% CI across periods."""
    all_data = []
    for dir_name, _ in PERIODS:
        d = load_specialty_data(base / dir_name / "whole_period", bucket)
        all_data.append(d)

    for age_idx, age in enumerate(AGE_ORDER):
        group_base = age_idx * ROW_HEIGHT

        for p_idx, (data, (_, plabel), style) in enumerate(
            zip(all_data, PERIODS, PERIOD_STYLES)
        ):
            if data is None or spec not in data or age not in data[spec]:
                continue
            entry = data[spec][age]
            med = entry.get("Med")
            ci = entry.get("95% CI")
            if med is None or ci is None:
                continue
            ci_lo, ci_hi = ci
            y = group_base + p_idx

            ax.errorbar(
                med,
                y,
                xerr=[[max(0, med - ci_lo)], [max(0, ci_hi - med)]],
                fmt=style["marker"],
                color=style["color"],
                markersize=style["ms"],
                capsize=3,
                linewidth=1.5,
                markeredgewidth=1.5,
            )

            vax_count = entry.get("počet očko", 0)
            spec_count = entry.get("počet u spec.", 0)
            if vax_count:
                label = f"n={vax_count:,} ({spec_count:,} u spec.)".replace(
                    ",", "\u2009"
                )
                ax.annotate(
                    label,
                    (ci_hi, y),
                    textcoords="offset points",
                    xytext=(5, 0),
                    fontsize=5.5,
                    color=style["color"],
                    va="center",
                )

    yticks = []
    ylabels = []
    for age_idx, age in enumerate(AGE_ORDER):
        mid = age_idx * ROW_HEIGHT + (N_PERIODS - 1) / 2
        yticks.append(mid)
        ylabels.append(AGE_LABELS[age])

    ax.set_yticks(yticks)
    ax.set_yticklabels(ylabels, fontsize=9)
    ax.invert_yaxis()
    ax.axvline(0, color="grey", linestyle="--", linewidth=0.8, zorder=0)

    for age_idx in range(len(AGE_ORDER) - 1):
        sep_y = (age_idx + 1) * ROW_HEIGHT - 1
        ax.axhline(sep_y, color="#dddddd", linestyle="-", linewidth=0.5, zorder=0)

    ax.set_title(
        f"{BUCKET_LABELS[bucket]} — {SPEC_LABELS[spec]} (treatment effect)",
        fontsize=12,
        fontweight="bold",
        pad=10,
    )
    ax.set_xlabel("Medián efektu (95% CI) — PE na osobu (vax−novax)", fontsize=9)
    ax.grid(axis="x", alpha=0.2, linewidth=0.5)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.margins(x=0.15)


def make_spec_period_plot(bucket: str, spec: str, ax: plt.Axes, *, base: Path):
    """Forest plot for one specialty: 5 periods × age cohorts, vax + novax."""
    all_data = []
    for dir_name, _ in PERIODS:
        d = load_specialty_data(base / dir_name / "whole_period", bucket)
        all_data.append(d)

    for age_idx, age in enumerate(AGE_ORDER):
        group_base = age_idx * ROW_HEIGHT

        for p_idx, (data, (_, plabel), style) in enumerate(
            zip(all_data, PERIODS, PERIOD_STYLES)
        ):
            if data is None or spec not in data or age not in data[spec]:
                continue
            entry = data[spec][age]
            vax_val = entry.get("očko PE po-před")
            novax_val = entry.get("neočko PE po-před")
            y = group_base + p_idx
            marker = style["marker"]

            rightmost = float("-inf")
            has_point = False

            if vax_val is not None:
                vax_ci = entry.get("očko 95% CI")
                if vax_ci is not None:
                    lo, hi = vax_ci
                    ax.errorbar(
                        vax_val,
                        y,
                        xerr=[[max(0, vax_val - lo)], [max(0, hi - vax_val)]],
                        fmt=marker,
                        color=VAX_COLOR,
                        markersize=style["ms"],
                        capsize=2.5,
                        linewidth=1,
                        markeredgewidth=1.2,
                    )
                    rightmost = max(rightmost, hi)
                    has_point = True
                else:
                    ax.plot(
                        vax_val,
                        y,
                        marker=marker,
                        color=VAX_COLOR,
                        markersize=style["ms"],
                        linestyle="None",
                        markeredgewidth=1.2,
                    )
                    rightmost = max(rightmost, vax_val)
                    has_point = True

            if novax_val is not None:
                novax_ci = entry.get("neočko 95% CI")
                if novax_ci is not None:
                    lo, hi = novax_ci
                    ax.errorbar(
                        novax_val,
                        y,
                        xerr=[[max(0, novax_val - lo)], [max(0, hi - novax_val)]],
                        fmt=marker,
                        color=NOVAX_COLOR,
                        markersize=style["ms"],
                        capsize=2.5,
                        linewidth=1,
                        markeredgewidth=1.2,
                        fillstyle="none",
                    )
                    rightmost = max(rightmost, hi)
                    has_point = True
                else:
                    ax.plot(
                        novax_val,
                        y,
                        marker=marker,
                        color=NOVAX_COLOR,
                        markersize=style["ms"],
                        linestyle="None",
                        markeredgewidth=1.2,
                        fillstyle="none",
                    )
                    rightmost = max(rightmost, novax_val)
                    has_point = True

            vax_count = entry.get("počet očko", 0)
            spec_count = entry.get("počet u spec.", 0)
            if vax_count and has_point:
                label = f"n={vax_count:,} ({spec_count:,} u spec.)".replace(
                    ",", "\u2009"
                )
                ax.annotate(
                    label,
                    (rightmost, y),
                    textcoords="offset points",
                    xytext=(5, 0),
                    fontsize=5.5,
                    color=style["color"],
                    va="center",
                )

    yticks = []
    ylabels = []
    for age_idx, age in enumerate(AGE_ORDER):
        mid = age_idx * ROW_HEIGHT + (N_PERIODS - 1) / 2
        yticks.append(mid)
        ylabels.append(AGE_LABELS[age])

    ax.set_yticks(yticks)
    ax.set_yticklabels(ylabels, fontsize=9)
    ax.invert_yaxis()
    ax.axvline(0, color="grey", linestyle="--", linewidth=0.8, zorder=0)

    for age_idx in range(len(AGE_ORDER) - 1):
        sep_y = (age_idx + 1) * ROW_HEIGHT - 1
        ax.axhline(sep_y, color="#dddddd", linestyle="-", linewidth=0.5, zorder=0)

    ax.set_title(
        f"{BUCKET_LABELS[bucket]} — {SPEC_LABELS[spec]}",
        fontsize=12,
        fontweight="bold",
        pad=10,
    )
    ax.set_xlabel("Průměrné PE na osobu (po – před)", fontsize=9)
    ax.grid(axis="x", alpha=0.2, linewidth=0.5)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.margins(x=0.15)


def _period_legend():
    return [
        plt.Line2D(
            [0],
            [0],
            marker=s["marker"],
            color=s["color"],
            linestyle="None",
            markersize=s["ms"],
            label=label,
        )
        for (_, label), s in zip(PERIODS, PERIOD_STYLES)
    ]


def _type_legend():
    return [
        plt.Line2D(
            [0],
            [0],
            marker="o",
            color=VAX_COLOR,
            linestyle="None",
            markersize=5,
            label="Očkovaní (po–před)",
        ),
        plt.Line2D(
            [0],
            [0],
            marker="o",
            color=NOVAX_COLOR,
            linestyle="None",
            markersize=4,
            fillstyle="none",
            label="Neočkovaní (po–před)",
        ),
    ]


def _worker(job: tuple) -> str:
    kind, bucket, spec, base_str, out_str = job
    base = Path(base_str)
    out_path = Path(out_str)
    fig_height = max(5, len(AGE_ORDER) * 1.8)

    if kind == "forest":
        fig, ax = plt.subplots(figsize=(10, fig_height))
        make_spec_treatment_effect_plot(bucket, spec, ax, base=base)
        fig.legend(
            handles=_period_legend(),
            loc="upper right",
            fontsize=7,
            frameon=True,
            fancybox=True,
            edgecolor="#cccccc",
        )
    elif kind == "raw":
        fig, ax = plt.subplots(figsize=(10, fig_height))
        make_spec_period_plot(bucket, spec, ax, base=base)
        fig.legend(
            handles=_period_legend() + _type_legend(),
            loc="upper right",
            fontsize=7,
            frameon=True,
            fancybox=True,
            edgecolor="#cccccc",
        )

    fig.tight_layout()
    fig.savefig(out_path, dpi=200, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    return str(out_path)


def _publish_per_spec_dirs(
    publish_root: Path, company: str, mode: str
) -> tuple[Path, Path]:
    """Return (per_spec/treatment_effect, per_spec/raw_effects) leaf dirs in the Czech hierarchy."""
    inj_dir = (
        publish_root
        / COMPANY_LABELS[company]
        / INJ_LABELS[mode]
        / DIL5
        / "Po specializacích"
    )
    return (
        inj_dir / "Souhrnné výsledky",
        inj_dir / "Zvlášť pro očkované a neočkované",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--variant",
        default="",
        help=(
            "Optional subfolder under matching_analysis/ to plot from "
            "(e.g. 'immuno_as_corticoid_500pe'). Plots land inside the same "
            "variant subtree so they never overwrite the standard ones."
        ),
    )
    parser.add_argument(
        "--companies",
        nargs="+",
        default=DEFAULT_COMPANIES,
        help="Insurance companies to plot for (only companies with specialty data).",
    )
    parser.add_argument(
        "--out-root",
        default=str(DEFAULT_OUT_ROOT),
        help="Root output directory containing <company>/matching_analysis/... (default: out).",
    )
    parser.add_argument(
        "--publish-dir",
        default=None,
        help=(
            "If set, write plots directly into a Czech-named hierarchy under this dir "
            "(e.g. 'Finální matchingová analýza'). Only non_inj/inj × different_effect_baseline "
            "are published; susceptible buckets and immuno/every-rx variants are skipped."
        ),
    )
    args = parser.parse_args()

    out_root = Path(args.out_root)
    publish_root = Path(args.publish_dir) if args.publish_dir else None
    publish_mode = publish_root is not None

    if publish_mode and args.variant:
        print(
            f"Skipping publish for variant '{args.variant}' (only main results are published)."
        )
        return

    jobs = []

    for company in args.companies:
        company_root = out_root / company / "matching_analysis"
        root = company_root / args.variant if args.variant else company_root
        if not root.exists():
            print(f"Skipping {company}: {root} does not exist")
            continue
        if args.variant:
            print(f"Variant root for {company}: {root}")

        for mode in ANALYSIS_MODES:
            if publish_mode and mode not in INJ_LABELS:
                continue
            for eb in EFFECT_BASELINES:
                if publish_mode and eb != "different_effect_baseline":
                    continue

                base = root / mode / eb
                if not base.exists():
                    continue

                has_specialty = any(
                    (
                        base
                        / period_dir
                        / "whole_period"
                        / bucket
                        / "specialty_effects.json"
                    ).exists()
                    for period_dir, _ in PERIODS
                    for bucket in BUCKETS
                )
                if not has_specialty:
                    print(f"Skipping {base}: no specialty_effects.json found")
                    continue

                if publish_mode:
                    te_dir, raw_dir = _publish_per_spec_dirs(
                        publish_root, company, mode
                    )
                else:
                    plots_root = base / "forest_plots" / "per_spec"
                    te_dir = plots_root / "treatment_effect"
                    raw_dir = plots_root / "raw_effects"

                for d in (te_dir, raw_dir):
                    d.mkdir(parents=True, exist_ok=True)

                bs = str(base)
                buckets = PUB_BUCKETS if publish_mode else BUCKETS

                for bucket in buckets:
                    for spec in SPECIALTIES:
                        if publish_mode:
                            te_name = (
                                f"{SPEC_LABELS[spec]} - {BUCKET_LABELS[bucket]}.png"
                            )
                            raw_name = (
                                f"{SPEC_LABELS[spec]} - {BUCKET_LABELS[bucket]}.png"
                            )
                        else:
                            te_name = f"spec_{spec}_forest_{bucket.lower()}.png"
                            raw_name = f"spec_{spec}_raw_effects_{bucket.lower()}.png"

                        jobs.append(("forest", bucket, spec, bs, str(te_dir / te_name)))
                        jobs.append(("raw", bucket, spec, bs, str(raw_dir / raw_name)))

    print(f"Generating {len(jobs)} plots across {mp.cpu_count()} cores...")
    with mp.Pool() as pool:
        for path in pool.imap_unordered(_worker, jobs):
            print(f"  Saved → {path}")


if __name__ == "__main__":
    main()
