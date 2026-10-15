# top-movies

A small Rust CLI that prints the top movies by average rating from a
MovieLens-style `ratings` file.

```
Top 10 movies by average rating  ·  source: archive/ratings.csv
26024289 ratings · 45115 movies · 9976 with at least 100 votes

Rank  Rating  Votes  IMDb       Title
----  ------  -----  ---------  ------------------------
   1    4.48    754  tt0795176  Planet Earth
   2    4.43  91082  tt0111161  The Shawshank Redemption
   3    4.39    284  tt0185906  Band of Brothers
   ...
```

## How it works

1. Streams the ratings file (`userId,movieId,rating,timestamp`) and keeps a
   running `sum` and `count` per `movieId`.
2. Keeps only movies with at least `--min-votes` ratings, sorts them by average
   rating (ties broken by vote count), and prints the top `--top`.
3. Enriches the output using the companion files when they are present:
   - `links.csv` gives `movieId -> imdbId, tmdbId`;
   - `movies_metadata.csv` gives `tmdbId -> title`.

The ratings file has no quotes and can be hundreds of megabytes, so it is read
line-by-line with a hand-rolled splitter. The two small companion files go
through a proper RFC 4180 CSV reader because `movies_metadata.csv` contains
quoted fields with embedded commas, escaped quotes, and a few records that span
multiple lines.

If a movie cannot be resolved, its IMDb column shows `—` and its title shows
`movieId <id>`. The tool still works if `links.csv` / `movies_metadata.csv` are
missing — it just prints ids.

## Build

No dependencies, so it builds offline:

```sh
cargo build --release
```

The release binary is `target/release/top-movies`.

## Usage

```sh
cargo run --release                 # top 10 from archive/ratings.csv, >= 100 votes
cargo run --release -- --top 25 --min-votes 1000
cargo run --release -- --ratings archive/ratings_small.csv --min-votes 50
cargo run --release -- --help
```

### Options

| Option | Default | Description |
| --- | --- | --- |
| `--ratings <FILE>` | `archive/ratings.csv` | ratings file to read |
| `--links <FILE>` | `archive/links.csv` | `movieId` → IMDb / TMDb id |
| `--metadata <FILE>` | `archive/movies_metadata.csv` | `tmdbId` → title |
| `--top <N>` | `10` | how many movies to print |
| `--min-votes <N>` | `100` | minimum number of ratings for a movie to qualify |
| `-v`, `--verbose` | off | print progress summary to stderr |
| `-h`, `--help` | | show help |
| `-V`, `--version` | | show version |

Paths may be given as `--flag value` or `--flag=value`. Relative defaults are
resolved against the current directory first, then against the project root, so
the binary also works when run from elsewhere.

### About `--min-votes`

Averaging alone ranks obscure titles with a single `5.0` rating at the top, so
movies need `--min-votes` ratings to qualify (default `100`). Raise it for a
more mainstream list, or pass `--min-votes 0` to rank on the raw average.

## Data layout

Download [The Movies Dataset](https://www.kaggle.com/datasets/rounakbanik/the-movies-dataset)
from Kaggle and unzip it into `archive/` (git-ignored):

```
archive/
├── ratings.csv            # userId,movieId,rating,timestamp   (the input)
├── ratings_small.csv      # smaller sample, handy for quick runs
├── links.csv              # movieId,imdbId,tmdbId
└── movies_metadata.csv    # id,imdb_id,title,...
```

## Tests

```sh
cargo test
```

Covers the CSV parser (quotes, escaped quotes, multi-line records), the ratings
line parser, IMDb id normalisation, and argument parsing.
