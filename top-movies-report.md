# Report — `top-movies` (ponytail) vs `top-movies-simple` (baseline)

## 0. What was compared

| | top-movies (ponytail arm) | top-movies-simple (baseline arm) |
|---|---|---|
| Prompt (identical) | `Write a CLI which prints top 10 movies by IMDB rating, read ratings from arvhive/ratings file , use Rust language` | same |
| Model | `opencode-go/deepseek-v4.1-flash` (variant `high`) | same |
| Agent | `build` | same |
| Seed difference | `AGENTS.md` present (ponytail "lazy senior dev" rules) | no `AGENTS.md` |
| Outcome | succeeded | succeeded |

Both sessions contain a second `/export` user turn; token numbers below are split into **TASK** and **EXPORT** phases so the "real work" is comparable.

---

## 1. Token usage

**Session totals** (authoritative, from `info.tokens` / `info.cost`):

| Metric | ponytail | simple | Ratio (simple ÷ ponytail) |
|---|---:|---:|---:|
| Input tokens | 53,972 | 75,820 | **1.40×** |
| Output tokens | 4,670 | 12,291 | **2.63×** |
| Reasoning tokens | 8,585 | 8,742 | 1.02× |
| Cache read | 596,352 | 832,896 | **1.40×** |
| Cache write | 0 | 0 | — |
| New tokens (in+out+reasoning) | 67,227 | 96,853 | 1.44× |
| Effective tokens (incl. cache read) | 663,579 | 928,309 | 1.40× |
| **Cost (USD)** | **$0.017809** | **$0.026463** | **1.49×** |

**Phase split** (sum of per-message `tokens`):

| Phase | ponytail in/out/reason | simple in/out/reason |
|---|---|---|
| TASK | 46,725 / 4,199 / 7,268 | 55,979 / 11,680 / 7,435 |
| EXPORT | 6,705 / 454 / 1,317 | 19,299 / 594 / 1,307 |
| TASK total new tokens | **58,192** | **75,094** (1.29×) |
| TASK + cache read | 529,744 | 750,791 (1.42×) |

**Key observations**
- **Reasoning is essentially identical** (7.3k vs 7.4k on the task) — the model "thought" the same amount. The entire token gap is **output** and **input/cache**.
- **Output tokens are 2.8× higher** for the baseline, matching the ~6× larger deliverable (it wrote a full CLI + 8 tests + README in one long generation, peaking at 5,392 output tokens in a single turn).
- **Input/cache is 1.2–1.4× higher** for the baseline: it fetched more/earlier and additionally loaded the `opencode` skill (1 `skill` call) and did more data-inspection (`awk` over the 709 MB file).
- **Ponytail cost ~33% less** overall (~$0.0178 vs $0.0265).
- Cache is tiny relative to context: despite ~596k/833k cumulative cache reads, there were no cache *writes* (provider-side prompt caching, read-only here).

Per-assistant-message averages:

| | messages | avg input | avg output | avg reasoning | avg cache read |
|---|---:|---:|---:|---:|---:|
| ponytail | 25 | 2,137 | 187 | 343 | 23,854 |
| simple | 30 | 2,509 | 410 | 291 | 27,763 |

---

## 2. Code metrics (`src/main.rs`)

| Metric | ponytail | simple | Ratio |
|---|---:|---:|---:|
| Physical lines | 108 | 647 | **5.99×** |
| Code lines (non-blank, non-comment) | 89 | 528 | 5.93× |
| — non-test code | 70 | 465 | 6.64× |
| Comment lines | 7 | 52 | 7.4× |
| Blank lines | 12 | 67 | — |
| Inline trailing comments | 9 | 3 | — |
| File size | 3,843 B | 20,368 B | 5.30× |
| Functions | 7 | 26 | 3.7× |
| Structs / impls | 0 / 0 | 4 / 1 | — |
| `#[test]` cases | 1 | 8 | 8× |
| Test-module lines | 20 | 71 | 3.6× |
| `match` expressions | 0 | 8 | — |
| Loops (`for`/`while`) | 5 / 0 | 2 / 7 | — |
| `Result<…>` signatures | 0 | 8 | — |
| Panicking calls in production (`expect!`/`panic!`) | 8 | 0 | — |
| `unwrap()` (all in tests for both) | 0 | 9 | — |
| `assert*` | 2 | 20 | 10× |
| Language edition | 2024 | 2021 | — |
| Direct dependencies | **1** (`csv`) | **0** | — |
| Transitive deps | 8 (csv-core, serde_core, syn, …) | 0 | — |
| Release binary size | 504,320 B | 448,448 B | — |
| Runtime on full 26M-row `ratings.csv` | **1.85 s** | **0.81 s** | 2.3× faster |

**Complexity differences**
- Ponytail: flat, functional — 5 tiny pure functions (`reader`, `ratings`, `imdb_ids`, `titles`, `best`) plus `main`. No structs, no CLI layer, no error type. It leans on the `csv` crate for parsing and on `panic!`/`expect` for failure (data assumed well-formed except the one `let Ok(row) = row else { continue }` guard for ragged metadata rows).
- Simple: a full CLI — `Config` + argument parser (`--flag` / `--flag=value`, `--help`, `--version`, `-v`), a hand-rolled RFC 4180 `CsvReader` (quotes, escaped `""`, multi-line records), streaming aggregator, graceful degradation when `links.csv`/`metadata.csv` are missing, width-aligned table output, and 8 unit tests. Error handling is explicit `Result`/`ExitCode` throughout; zero panics in production paths.

**Verbosity/density**
- Ponytail: 35.6 bytes/line, ~43 output tokens per final source line.
- Simple: 31.5 bytes/line, ~19 output tokens per final source line.
- Comment ratio is actually similar (ponytail 7/89 ≈ 8%; simple 52/528 ≈ 10%), but ponytail's comments are terse `why` lines (`// Without a floor the top 10 is whatever has one 5.0 vote.`), while simple uses section banners + `//!` module docs.

---

## 3. Process / interaction metrics

| Metric | ponytail | simple |
|---|---:|---:|
| Total messages | 29 | 34 |
| Assistant messages | 25 | 30 |
| Tool calls | 33 | 41 |
| — shell | 25 | 23 |
| — edit / write | 5 / 1 | 9 / 4 |
| — read / glob | 2 / 0 | 3 / 1 |
| — skill | 0 | 1 |
| File-write operations | **6** | **13** |
| Wall-clock duration | **3.05 min** | **5.05 min** |
| Build/test/re-run cycles | several `cargo build/test/clippy/run` | several `cargo build/test/clippy/run` |

- Ponytail explored less and **wrote once, then patched** (1 `write` + 5 targeted `edit`s). Baseline wrote a large file and **refactored repeatedly** (4 `write` + 9 `edit`s), including renaming a helper and restructuring `print_table`.
- Both verified with `cargo build --release`, `cargo test`, `cargo clippy`, and timed runs; baseline also tested invocation from `/tmp` and the small dataset.
- Neither asked a clarifying question (the AGENTS.md "smallest complete change / don't ask for extra" guidance was followed by the ponytail arm; baseline also chose to proceed).

---

## 4. Behavioural / output differences

- **Vote floor:** ponytail hardcodes `MIN_VOTES = 1000`; baseline defaults to `100` (`--min-votes`, configurable). This alone changes the two top-10 lists.
- **Rating scale:** ponytail prints `average × 2.0` (0.5–5.0 → 1–10 "IMDb scale"); baseline prints the raw MovieLens average (out of 5).
- **Ponytail output (top 3):** `The Shawshank Redemption 8.86/10`, `The Godfather 8.68/10`, `The Usual Suspects 8.60/10`.
- **Baseline output (top 3):** `Planet Earth 4.48`, `The Shawshank Redemption 4.43`, `Band of Brothers 4.39`.
- Both correctly resolve titles through `links.csv` + `movies_metadata.csv` despite embedded commas/quotes/newlines; the baseline additionally reports one genuine data gap (`movieId 100044` → "movieId 100044"), where ponytail would silently emit `(unknown)`.
- Ponytail's own closing note flags the real trade-offs: no CLI args, hardcoded paths, `~2 s` per run, and **panic on a malformed `movieId`/`rating`**. Baseline's is robust (`Result`, warnings, `--min-votes 0` escape hatch, 8 tests) but ~6× more code and ~1.5× the tokens/cost.

---

## 5. Headline conclusions

1. **Tokens per delivered line invert the "less code = fewer tokens" intuition.** Fixed exploration/reasoning overhead dominates, so a 6× smaller deliverable cut total tokens only ~1.3–1.4× (reasoning was flat). Ponytail used ~4.5× *more* tokens per final source line — it bought brevity, not token savings per line.
2. **Output tokens are the only real lever**: 4,670 vs 12,291 (2.6×), exactly tracking the size of what was written.
3. **Ponytail won on budget/effort**: ~33% cheaper ($0.0178 vs $0.0265), 3.05 vs 5.05 min, 33 vs 41 tool calls, and one dependency instead of a hand-rolled parser.
4. **Baseline won on robustness and completeness**: zero dependencies (offline build), no production panics, 8 tests vs 1, configurable CLI, graceful degradation — at ~6× the LOC and ~2.3× faster runtime (helped by `lto`/`codegen-units=1`).

**Skipped / risk:** I derived cost from whole-session totals and inferred the TASK/EXPORT split from message rows, so phase-level dollar figures are approximate; I did not byte-diff the generated code against a spec or run `cargo test` fresh (I relied on the built binaries I re-ran). Risk: the two arms also differ in seed (`AGENTS.md` present only in the ponytail arm), so some of the gap is prompt guidance, not the plugin runtime itself.
