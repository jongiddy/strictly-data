use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub(crate) struct Row {
    series: u16,
    week: u16,
    total_score: u8,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UltimateRow {
    #[serde(rename = "Series")]
    series: String,
    #[serde(rename = "Week")]
    week: String,
    #[serde(rename = "Total")]
    total: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    const TOP_DIR: Option<&str> = option_env!("CARGO_MANIFEST_DIR");
    let top_dir = Path::new(TOP_DIR.unwrap_or("."));

    let csv_file = top_dir.join("output.csv");
    println!("Parsing {}", csv_file.display());
    let mut my_scores = HashMap::<String, Vec<u8>>::new();
    let f = File::open(csv_file)?;
    let reader = BufReader::new(f);
    let mut rdr = csv::Reader::from_reader(reader);
    for result in rdr.deserialize() {
        let record: Row = result?;
        let key = format!("Series {} Week {}", record.series, record.week);
        let entry = my_scores.entry(key).or_insert_with(Vec::new);
        entry.push(record.total_score);
    }

    let csv_file = top_dir.join("ultimate/SCD_Series18.csv");
    println!("Parsing {}", csv_file.display());
    let mut us_scores = HashMap::<String, Vec<u8>>::new();
    let f = File::open(csv_file)?;
    let reader = BufReader::new(f);
    let mut rdr = csv::Reader::from_reader(reader);
    for result in rdr.deserialize() {
        let record: UltimateRow = result?;
        match record.total.parse() {
            Ok(total) => {
                let key = format!("Series {} Week {}", record.series, record.week);
                let entry = us_scores.entry(key).or_insert_with(Vec::new);
                entry.push(total);
            }
            Err(_) => {
                assert!(record.total == "-");
            }
        }
    }

    for (key, mut us_score) in us_scores {
        let my_score = my_scores.get_mut(&key).unwrap();
        us_score.sort();
        my_score.sort();
        if us_score != *my_score {
            println!("{}\n{:?}\n{:?}", key, my_score, us_score);
        }
    }
    Ok(())
}
