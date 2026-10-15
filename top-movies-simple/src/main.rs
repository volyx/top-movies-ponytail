//! top-movies — print the top movies by average IMDB-style rating.
//!
//! Reads a MovieLens-style `ratings` file (`userId,movieId,rating,timestamp`),
//! averages the ratings per movie, and prints the highest rated ones. Movie
//! titles and IMDB ids are resolved through `links` and `movies_metadata`
//! when those files are available.
//!
//! The tool is dependency-free and streams the ratings file, so the full
//! 26-million-row `ratings.csv` works without loading it into memory.

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULT_RATINGS: &str = "archive/ratings.csv";
const DEFAULT_LINKS: &str = "archive/links.csv";
const DEFAULT_METADATA: &str = "archive/movies_metadata.csv";
const DEFAULT_TOP: usize = 10;
/// Ratings below this many votes are excluded by default: without a floor the
/// ranking is dominated by obscure titles with a single 5.0 rating.
const DEFAULT_MIN_VOTES: u64 = 100;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("top-movies: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = match Config::from_args(env::args().skip(1))? {
        Some(config) => config,
        None => return Ok(()), // --help / --version already printed
    };

    // --- 1. Aggregate every rating into (sum, count) per movie id -----------
    let ratings = aggregate_ratings(&config.ratings)?;
    if ratings.is_empty() {
        return Err(format!("no ratings found in {}", config.ratings.display()).into());
    }

    // --- 2. Rank the movies that clear the vote floor -----------------------
    let qualified = ratings
        .values()
        .filter(|&&(_, votes)| votes >= config.min_votes)
        .count();

    let mut ranked: Vec<Ranked> = ratings
        .iter()
        .filter(|(_, (_, votes))| *votes >= config.min_votes)
        .map(|(&id, &(sum, votes))| Ranked {
            movie_id: id,
            average: sum / votes as f64,
            votes,
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.average
            .partial_cmp(&a.average)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.votes.cmp(&a.votes))
            .then(a.movie_id.cmp(&b.movie_id))
    });
    ranked.truncate(config.top);

    if ranked.is_empty() {
        return Err(format!(
            "no movie reaches the {} vote minimum; try --min-votes 0",
            config.min_votes
        )
        .into());
    }

    if config.verbose {
        eprintln!(
            "read {} ratings across {} movies; kept {} with >= {} votes",
            total_ratings(&ratings),
            ratings.len(),
            ranked.len(),
            config.min_votes
        );
    }

    // --- 3. Resolve titles / imdb ids (best effort) -------------------------
    let links = match load_links(&config.links) {
        Ok(links) => links,
        Err(err) => {
            warn(&config.links, &err);
            HashMap::new()
        }
    };
    let titles = match load_titles(&config.metadata) {
        Ok(titles) => titles,
        Err(err) => {
            warn(&config.metadata, &err);
            HashMap::new()
        }
    };

    print_table(
        &config,
        &ranked,
        &links,
        &titles,
        total_ratings(&ratings),
        ratings.len(),
        qualified,
    );
    Ok(())
}

fn warn(path: &Path, err: &io::Error) {
    eprintln!(
        "top-movies: warning: cannot read {} ({err}); continuing without it",
        path.display()
    );
}

fn total_ratings(ratings: &HashMap<u32, (f64, u64)>) -> u64 {
    ratings.values().map(|&(_, votes)| votes).sum()
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Config {
    ratings: PathBuf,
    links: PathBuf,
    metadata: PathBuf,
    top: usize,
    min_votes: u64,
    verbose: bool,
}

impl Config {
    /// Returns `Ok(None)` when help/version was printed and the caller should
    /// exit successfully.
    fn from_args(args: impl Iterator<Item = String>) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let mut config = Config {
            ratings: PathBuf::from(DEFAULT_RATINGS),
            links: PathBuf::from(DEFAULT_LINKS),
            metadata: PathBuf::from(DEFAULT_METADATA),
            top: DEFAULT_TOP,
            min_votes: DEFAULT_MIN_VOTES,
            verbose: false,
        };

        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (arg.clone(), None),
            };

            // Pull the value either from `--flag=value` or from the next arg.
            let mut value = |name: &str| -> Result<String, Box<dyn std::error::Error>> {
                match inline.clone() {
                    Some(value) => Ok(value),
                    None => args
                        .next()
                        .ok_or_else(|| format!("{name} needs a value").into()),
                }
            };

            match name.as_str() {
                "-h" | "--help" => {
                    print_help();
                    return Ok(None);
                }
                "-V" | "--version" => {
                    println!("top-movies {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                "-v" | "--verbose" => config.verbose = true,
                "--ratings" => config.ratings = PathBuf::from(value("--ratings")?),
                "--links" => config.links = PathBuf::from(value("--links")?),
                "--metadata" => config.metadata = PathBuf::from(value("--metadata")?),
                "--top" => {
                    let raw = value("--top")?;
                    config.top = raw
                        .parse()
                        .map_err(|_| format!("--top expects a number, got {raw:?}"))?;
                }
                "--min-votes" => {
                    let raw = value("--min-votes")?;
                    config.min_votes = raw
                        .parse()
                        .map_err(|_| format!("--min-votes expects a number, got {raw:?}"))?;
                }
                other => return Err(format!("unknown option {other:?} (try --help)").into()),
            }
        }

        if config.top == 0 {
            return Err("--top must be at least 1".into());
        }
        config.ratings = resolve(&config.ratings)?;
        config.links = resolve(&config.links)?;
        config.metadata = resolve(&config.metadata)?;
        Ok(Some(config))
    }
}

/// Accept a path as given, or fall back to the crate directory so the binary
/// works when invoked from outside the project root.
fn resolve(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(path.to_path_buf());
    }
    let from_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    if from_crate.exists() {
        return Ok(from_crate);
    }
    // Only the ratings file is required; report it clearly.
    if path == Path::new(DEFAULT_RATINGS) {
        return Err(format!("ratings file not found: {}", path.display()).into());
    }
    Ok(path.to_path_buf())
}

fn print_help() {
    println!(
        "\
top-movies — print the top movies by average rating

Reads a MovieLens-style ratings file, averages the rating per movie, and prints
the highest rated movies. Titles and IMDb ids are added from links.csv and
movies_metadata.csv when present.

USAGE:
    top-movies [OPTIONS]

OPTIONS:
    --ratings <FILE>     ratings file        [default: {DEFAULT_RATINGS}]
    --links <FILE>       movieId -> imdbId   [default: {DEFAULT_LINKS}]
    --metadata <FILE>    movieId/tmdb titles [default: {DEFAULT_METADATA}]
    --top <N>            how many to print   [default: {DEFAULT_TOP}]
    --min-votes <N>      minimum votes       [default: {DEFAULT_MIN_VOTES}]
    -v, --verbose        print progress to stderr
    -h, --help           show this help
    -V, --version        show the version

EXAMPLES:
    top-movies
    top-movies --min-votes 1000 --top 25
    top-movies --ratings archive/ratings_small.csv --min-votes 50"
    );
}

// ---------------------------------------------------------------------------
// CSV reading
// ---------------------------------------------------------------------------

/// A minimal RFC 4180 CSV reader. Handles quoted fields, escaped quotes ("")
/// and quoted fields that span multiple physical lines — all of which occur in
/// `movies_metadata.csv`.
struct CsvReader<R: BufRead> {
    inner: R,
    record: Vec<u8>,
    scratch: Vec<u8>,
}

impl<R: BufRead> CsvReader<R> {
    fn new(inner: R) -> Self {
        CsvReader {
            inner,
            record: Vec::with_capacity(512),
            scratch: Vec::with_capacity(512),
        }
    }

    /// Read one record, or `None` at end of input. A record is one logical CSV
    /// row: physical lines are glued together while a quoted field is open.
    fn read_record(&mut self) -> io::Result<Option<Vec<String>>> {
        self.record.clear();
        loop {
            self.scratch.clear();
            if self.inner.read_until(b'\n', &mut self.scratch)? == 0 {
                break;
            }
            self.record.extend_from_slice(&self.scratch);
            if !has_open_quote(&self.record) {
                break;
            }
        }
        if self.record.is_empty() {
            return Ok(None);
        }
        while matches!(self.record.last(), Some(b'\n') | Some(b'\r')) {
            self.record.pop();
        }
        Ok(Some(parse_fields(&self.record)))
    }
}

/// True when the record ends inside a quoted field.
fn has_open_quote(record: &[u8]) -> bool {
    let mut open = false;
    let mut i = 0;
    while i < record.len() {
        if record[i] == b'"' {
            if open && record.get(i + 1) == Some(&b'"') {
                i += 1; // escaped quote
            } else {
                open = !open;
            }
        }
        i += 1;
    }
    open
}

/// Split one CSV record into its fields, honouring quotes.
fn parse_fields(record: &[u8]) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut in_quotes = false;
    let mut i = 0;

    while i < record.len() {
        let byte = record[i];
        if in_quotes {
            if byte == b'"' {
                if record.get(i + 1) == Some(&b'"') {
                    field.push(b'"');
                    i += 1;
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(byte);
            }
        } else {
            match byte {
                b'"' => in_quotes = true,
                b',' => {
                    fields.push(String::from_utf8_lossy(&field).into_owned());
                    field.clear();
                }
                _ => field.push(byte),
            }
        }
        i += 1;
    }

    fields.push(String::from_utf8_lossy(&field).into_owned());
    fields
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Sum and count the ratings per movie id. This streams the file, and uses a
/// hand-rolled splitter because `ratings.csv` has no quoting but may contain
/// tens of millions of rows.
fn aggregate_ratings(path: &Path) -> Result<HashMap<u32, (f64, u64)>, Box<dyn std::error::Error>> {
    let file = File::open(path)
        .map_err(|err| format!("cannot open ratings file {}: {err}", path.display()))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line = Vec::with_capacity(128);
    let mut aggregate: HashMap<u32, (f64, u64)> = HashMap::with_capacity(1 << 16);
    let mut first_line = true;

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if first_line {
            first_line = false;
            // Tolerate a missing header, but skip it when present.
            if line.starts_with(b"userId") {
                continue;
            }
        }
        if let Some((movie_id, rating)) = parse_rating_line(&line) {
            let entry = aggregate.entry(movie_id).or_insert((0.0, 0));
            entry.0 += rating;
            entry.1 += 1;
        }
    }

    Ok(aggregate)
}

/// Parse `userId,movieId,rating,timestamp`, ignoring everything but the id and
/// the rating. Returns `None` for blank or malformed lines.
fn parse_rating_line(line: &[u8]) -> Option<(u32, f64)> {
    let mut fields = line.split(|&byte| byte == b',');
    fields.next()?; // userId
    let movie_id: u32 = parse_field(fields.next()?)?.parse().ok()?;
    let rating: f64 = parse_field(fields.next()?)?.parse().ok()?;
    if !(0.0..=10.0).contains(&rating) {
        return None;
    }
    Some((movie_id, rating))
}

fn parse_field(raw: &[u8]) -> Option<&str> {
    std::str::from_utf8(raw).ok().map(str::trim)
}

/// `movieId -> (imdbId, tmdbId)` from `links.csv`.
fn load_links(path: &Path) -> io::Result<HashMap<u32, Link>> {
    let mut links = HashMap::new();
    let mut csv = CsvReader::new(BufReader::new(File::open(path)?));
    let mut first = true;

    while let Some(fields) = csv.read_record()? {
        if first {
            first = false;
            if fields.first().map(String::as_str) == Some("movieId") {
                continue;
            }
        }
        if fields.len() < 3 {
            continue;
        }
        let Some(movie_id) = fields[0].trim().parse::<u32>().ok() else {
            continue;
        };
        let imdb = normalize_imdb_id(fields[1].trim());
        let tmdb = fields[2].trim().parse::<u32>().ok();
        links.insert(movie_id, Link { imdb, tmdb });
    }

    Ok(links)
}

/// `tmdbId -> title` from `movies_metadata.csv`. Rows that do not parse (the
/// file contains a handful of malformed records) are skipped.
fn load_titles(path: &Path) -> io::Result<HashMap<u32, String>> {
    const ID_COLUMN: usize = 5;
    const TITLE_COLUMN: usize = 20;

    let mut titles = HashMap::new();
    let mut csv = CsvReader::new(BufReader::new(File::open(path)?));
    let mut first = true;

    while let Some(fields) = csv.read_record()? {
        if first {
            first = false;
            if fields.get(ID_COLUMN).map(String::as_str) == Some("id") {
                continue;
            }
        }
        if fields.len() <= TITLE_COLUMN {
            continue;
        }
        let Some(id) = fields[ID_COLUMN].trim().parse::<u32>().ok() else {
            continue;
        };
        let title = fields[TITLE_COLUMN].trim();
        if !title.is_empty() {
            titles.insert(id, title.to_string());
        }
    }

    Ok(titles)
}

/// Turn `0114709` from `links.csv` into `tt0114709`.
fn normalize_imdb_id(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("tt") {
        return Some(raw.to_string());
    }
    Some(format!("tt{raw:0>7}"))
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Link {
    imdb: Option<String>,
    tmdb: Option<u32>,
}

#[derive(Debug)]
struct Ranked {
    movie_id: u32,
    average: f64,
    votes: u64,
}

fn print_table(
    config: &Config,
    ranked: &[Ranked],
    links: &HashMap<u32, Link>,
    titles: &HashMap<u32, String>,
    total_ratings: u64,
    total_movies: usize,
    qualified: usize,
) {
    let rank_width = ranked.len().to_string().len().max(4);
    let width = |f: &dyn Fn(&Ranked) -> usize| ranked.iter().map(f).max().unwrap_or(0);

    let rating_width = width(&|r| format!("{:.2}", r.average).len()).max(6);
    let votes_width = width(&|r| r.votes.to_string().len()).max(5);
    let imdb_width = width(&|r| {
        links
            .get(&r.movie_id)
            .and_then(|l| l.imdb.as_deref())
            .map_or(1, str::len)
    })
    .max(4);

    let mut out = io::stdout().lock();
    let _ = writeln!(
        out,
        "Top {} movies by average rating  ·  source: {}",
        ranked.len(),
        config.ratings.display()
    );
    let _ = writeln!(
        out,
        "{} ratings · {} movies · {} with at least {} votes\n",
        total_ratings, total_movies, qualified, config.min_votes
    );
    let _ = writeln!(
        out,
        "{:>rank_width$}  {:>rating_width$}  {:>votes_width$}  {:<imdb_width$}  Title",
        "Rank", "Rating", "Votes", "IMDb"
    );
    let _ = writeln!(
        out,
        "{:>rank_width$}  {:>rating_width$}  {:>votes_width$}  {:<imdb_width$}  {}",
        "-".repeat(rank_width),
        "-".repeat(rating_width),
        "-".repeat(votes_width),
        "-".repeat(imdb_width),
        "-".repeat(24)
    );

    for (index, movie) in ranked.iter().enumerate() {
        let link = links.get(&movie.movie_id);
        let imdb = link
            .and_then(|l| l.imdb.as_deref())
            .unwrap_or("—")
            .to_string();
        let title = link
            .and_then(|l| l.tmdb)
            .and_then(|tmdb| titles.get(&tmdb))
            .cloned()
            .unwrap_or_else(|| format!("movieId {}", movie.movie_id));

        let _ = writeln!(
            out,
            "{:>rank_width$}  {:>rating_width$.2}  {:>votes_width$}  {:<imdb_width$}  {}",
            index + 1,
            movie.average,
            movie.votes,
            imdb,
            title
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_fields() {
        assert_eq!(parse_fields(b"a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn parses_quoted_fields_with_commas() {
        assert_eq!(
            parse_fields(br#"1,"Toy Story, 3",x"#),
            vec!["1", "Toy Story, 3", "x"]
        );
    }

    #[test]
    fn parses_escaped_quotes() {
        assert_eq!(parse_fields(br#""a""b",c"#), vec![r#"a"b"#, "c"]);
    }

    #[test]
    fn detects_open_quote_across_lines() {
        assert!(has_open_quote(b"1,\"never closed"));
        assert!(!has_open_quote(b"1,\"closed\""));
        assert!(!has_open_quote(b"1,\"quote\"\"here\""));
    }

    #[test]
    fn reads_multiline_records() {
        let input = "a,b\n1,\"line one\nline two\"\n2,c\n";
        let mut csv = CsvReader::new(io::Cursor::new(input));
        assert_eq!(csv.read_record().unwrap().unwrap(), vec!["a", "b"]);
        assert_eq!(
            csv.read_record().unwrap().unwrap(),
            vec!["1", "line one\nline two"]
        );
        assert_eq!(csv.read_record().unwrap().unwrap(), vec!["2", "c"]);
        assert!(csv.read_record().unwrap().is_none());
    }

    #[test]
    fn parses_rating_lines() {
        assert_eq!(parse_rating_line(b"1,31,2.5,1260759144\r"), Some((31, 2.5)));
        assert_eq!(parse_rating_line(b"1,31,9.0,1"), Some((31, 9.0)));
        assert_eq!(parse_rating_line(b"garbage"), None);
        assert_eq!(parse_rating_line(b"1,31,42.0,1"), None);
    }

    #[test]
    fn normalizes_imdb_ids() {
        assert_eq!(normalize_imdb_id("0114709"), Some("tt0114709".into()));
        assert_eq!(normalize_imdb_id("tt0114709"), Some("tt0114709".into()));
        assert_eq!(normalize_imdb_id(""), None);
    }

    #[test]
    fn parses_arguments() {
        let config = Config::from_args(
            ["--top", "3", "--min-votes=7", "--verbose"]
                .iter()
                .map(|s| s.to_string()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(config.top, 3);
        assert_eq!(config.min_votes, 7);
        assert!(config.verbose);
    }
}
