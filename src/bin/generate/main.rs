mod extract;

use std::error::Error;

use extract::extract_rows;

fn fetch_page(series: u16) -> Result<String, reqwest::Error> {
    let url = format!(
        "https://en.wikipedia.org/wiki/Strictly_Come_Dancing_(series_{})",
        series
    );
    reqwest::blocking::Client::new().get(url).send()?.text()
}

fn main() -> Result<(), Box<dyn Error>> {
    const LATEST_SERIES: u16 = 20;
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for series in 1..=LATEST_SERIES {
        let page = fetch_page(series)?;
        for row in extract_rows(series, &page)? {
            wtr.serialize(row)?;
        }
    }
    wtr.flush()?;
    Ok(())
}
