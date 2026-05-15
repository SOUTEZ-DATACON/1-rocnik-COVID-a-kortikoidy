# 1. ročník — COVID vakcíny a kortikoidy

Repozitář pro soutěžní projekt DATACON zaměřený na analýzu vztahu mezi
očkováním proti COVID-19 a spotřebou kortikoidů na dvou populacích pojištěnců
(ČPZP a OZP) v letech 2015–2023.

## Struktura repa

```
1-rocnik-COVID-a-kortikoidy/
├── README.md                            ← tento soubor
├── Kódy/                                ← matching code (Rust + Python plotting)
└── Finální matchingová analýza/         ← finální PNG forest ploty
    ├── Data z ČPZP/
    ├── Data z OZP/
    └── Souhrnná data/                   ← obě pojišťovny dohromady
```

## Hierarchie výsledků

Pro každou pojišťovnu (ČPZP / OZP / souhrnná data) jsou výsledky rozděleny:

- **Jen systémově podávané (bez injekcí)** — kortikoidy podávané systémově
  (tablety, perorálně, …), bez injekčních forem.
- **Jen injekce** — pouze injekční podání kortikoidů.

Uvnitř každé z těchto větví:

- **Výsledky - jen rok očkování (4. díl)** — forest plot pouze pro očkovací
  období (rok 2021 – únor 2022). Jeden řádek na věkovou kohortu.
- **Výsledky - srovnání s virtuálním očkováním (5. díl)** — porovnání
  očkovacího období se 4 referenčními „virtuálními očkováními" o 1, 2 a 3 roky
  zpět a 1 rok dopředu. Slouží k oddělení reálného efektu očkování od trendu
  v čase.
  - **Souhrnné výsledky** — *treatment effect*: medián rozdílu (resp. poměru)
    mezi očkovanými a párovanými neočkovanými v každém z 5 období, s 95% CI.
    Pokud má bod CI mimo referenční čáru, je rozdíl statisticky významný.
  - **Zvlášť pro očkované a neočkované** — *raw effects*: pro každé období
    zvlášť průměrné PE na osobu „po – před" pro očkované (plné značky) a
    neočkované (prázdné značky). Umožňuje vidět, kde efekt vzniká.
  - **Po specializacích** — totéž rozložené podle specializace lékaře, který
    kortikoid předepsal (revmatologie, neurologie, ortopedie, praktik atd.).
    Pouze pro ČPZP — OZP data nemají sloupec `Specializace`.

PE = „prednison-equivalent", normalizovaná dávka kortikoidu na osobu.
Skupiny pacientů jsou rozděleny dle PE před vakcinačním obdobím (`0 PE`,
`1–500 PE`, `500–5 000 PE`, `Nikdy předepsáno`).

## Reprodukce výsledků

V `Kódy/` jsou všechny skripty potřebné k reprodukci PNG plotů z této složky.

```bash
cd Kódy
just all              # full pipeline: simulate → plots
                      # PNGs se přepíšou v ../Finální matchingová analýza/
```

Závislosti:
- Rust + cargo (matching simulace),
- Python ≥3.13 + matplotlib + numpy (plotování) — `uv sync` z `Kódy/`,
- vstupní CSV `CPZP_preskladane.csv`, `OZP_preskladane.csv`