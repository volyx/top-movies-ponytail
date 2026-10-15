use std::collections::HashMap;

// Kaggle "The Movies Dataset" dump, relative to the working directory.
const RATINGS: &str = "archive/ratings.csv"; // userId,movieId,rating,timestamp
const LINKS: &str = "archive/links.csv"; // movieId,imdbId,tmdbId
const METADATA: &str = "archive/movies_metadata.csv"; // ...imdb_id,...,title,...

// Without a floor the top 10 is whatever has one 5.0 vote.
const MIN_VOTES: u32 = 1000;
const COUNT: usize = 10;

fn reader(path: &str) -> csv::Reader<std::fs::File> {
    csv::ReaderBuilder::new()
        .flexible(true) // a few metadata rows are ragged
        .from_path(path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// (movieId, sum of ratings, number of ratings) for every movie.
fn ratings() -> HashMap<u32, (f64, u32)> {
    let mut totals = HashMap::new();
    for row in reader(RATINGS).records() {
        let row = row.expect("ratings row");
        let movie: u32 = row[1].parse().expect("movieId");
        let rating: f64 = row[2].parse().expect("rating");
        let total = totals.entry(movie).or_insert((0.0, 0));
        total.0 += rating;
        total.1 += 1;
    }
    totals
}

/// movieId -> imdbId, as the digits-only key metadata uses without its "tt" prefix.
fn imdb_ids() -> HashMap<u32, String> {
    let mut ids = HashMap::new();
    for row in reader(LINKS).records() {
        let row = row.expect("links row");
        ids.insert(row[0].parse().expect("movieId"), row[1].to_string());
    }
    ids
}

/// digits-only imdbId -> title.
fn titles() -> HashMap<String, String> {
    let mut reader = reader(METADATA);
    let header = reader.headers().expect("metadata header").clone();
    let column = |name: &str| header.iter().position(|c| c == name).expect(name);
    let (imdb_id, title) = (column("imdb_id"), column("title"));

    let mut titles = HashMap::new();
    for row in reader.records() {
        let Ok(row) = row else { continue }; // skip unparseable rows
        if let (Some(imdb_id), Some(title)) = (row.get(imdb_id), row.get(title))
            && let Some(imdb_id) = imdb_id.strip_prefix("tt")
        {
            titles.insert(imdb_id.to_string(), title.to_string());
        }
    }
    titles
}

/// The `count` best-rated movies: highest average first, more ratings breaking ties.
fn best(totals: &HashMap<u32, (f64, u32)>, count: usize) -> Vec<(u32, f64, u32)> {
    let mut ranked: Vec<_> = totals
        .iter()
        .filter(|(_, (_, votes))| *votes >= MIN_VOTES)
        .map(|(movie, (sum, votes))| (*movie, sum / f64::from(*votes), *votes))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.2.cmp(&a.2)));
    ranked.truncate(count);
    ranked
}

fn main() {
    let (totals, ids, titles) = (ratings(), imdb_ids(), titles());

    for (rank, (movie, average, votes)) in best(&totals, COUNT).iter().enumerate() {
        let imdb_id = ids.get(movie).map(String::as_str).unwrap_or("");
        let title = titles.get(imdb_id).map(String::as_str).unwrap_or("(unknown)");
        // The file rates 0.5-5.0; double it to the familiar 1-10 IMDb scale.
        println!(
            "{:>2}. {title} (tt{imdb_id}) {:.2}/10 — {votes} ratings",
            rank + 1,
            average * 2.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_by_average_but_drops_thin_evidence() {
        let totals = HashMap::from([
            (1, (15.0, 3)),       // 5.00 from 3 ratings: below the floor
            (2, (16000.0, 2000)), // 8.00
            (3, (9.0, 1)),        // 9.00 from 1 rating: below the floor
            (4, (13500.0, 1500)), // 9.00
        ]);
        let ranked = best(&totals, 10);
        assert_eq!(
            ranked.iter().map(|(m, ..)| *m).collect::<Vec<_>>(),
            vec![4, 2]
        );
        assert_eq!(ranked[0].1, 9.0);
    }
}
