# top-movies-ponytail

Two Rust CLIs that print the top movies by average rating, written by the same
model from the same prompt, plus a report comparing the two runs:

- [`top-movies/`](top-movies) — ponytail arm (seeded with `AGENTS.md`)
- [`top-movies-simple/`](top-movies-simple) — baseline arm (no `AGENTS.md`)
- [`top-movies-report.md`](top-movies-report.md) — token, cost and code comparison

## Dataset

The data is not in this repo (`archive/` is git-ignored; `ratings.csv` alone is
~700 MB). Download **The Movies Dataset** from Kaggle:

https://www.kaggle.com/datasets/rounakbanik/the-movies-dataset

Unzip it into an `archive/` folder inside each project:

```sh
unzip archive.zip -d top-movies/archive
unzip archive.zip -d top-movies-simple/archive
```

Or with the [Kaggle CLI](https://github.com/Kaggle/kaggle-api):

```sh
kaggle datasets download rounakbanik/the-movies-dataset
unzip the-movies-dataset.zip -d top-movies/archive
unzip the-movies-dataset.zip -d top-movies-simple/archive
```

Both CLIs read `archive/ratings.csv`, `archive/links.csv` and
`archive/movies_metadata.csv` by default.
