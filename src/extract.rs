use lol_html::errors::RewritingError;
use lol_html::{element, text, HtmlRewriter, Settings};
use serde::Serialize;
use std::cell::Cell;

#[derive(Debug, Serialize)]
pub(crate) struct Row {
    series: u16,
    week: u16,
    celebrity: String,
    professional: String,
    dance: String,
    score: u8,
}

pub(crate) fn extract_rows(series: u16, page: String) -> Result<Vec<Row>, RewritingError> {
    let mut output: Vec<Row> = vec![];

    {
        #[derive(PartialEq)]
        enum State {
            ExpectNone,
            ExpectRow,
            ExpectCouple(i32, String),
            ExpectScore(i32, String, String),
            ExpectDance(i32, String, u8, String),
            ExpectEnd(i32, String),
        }
        struct Shared {
            state: State,
            week: u16,
        }
        impl Default for Shared {
            fn default() -> Self {
                Shared {
                    state: State::ExpectNone,
                    week: 0,
                }
            }
        }
        // Cell mutability for shared and mutable access from multiple closures.
        let shared = Cell::new(Shared::default());
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    // Find week number
                    element!("span.mw-headline", |el| {
                        shared.set(Shared::default());
                        if let Some(id) = el.get_attribute("id") {
                            // "Week_1", "Week_6:_Quarter-final"
                            let mut parts = id.split(&['_', ':'][..]);
                            if let Some("Week") = parts.next() {
                                let week = parts
                                    .next()
                                    .ok_or_else(|| format!("Bad parse {}", id))?
                                    .parse()?;
                                shared.set(Shared {
                                    state: State::ExpectRow,
                                    week,
                                });
                            }
                        }
                        Ok(())
                    }),
                    // When couples dance multiple dances in a show, the Couples column will
                    // have a rowspan > 1. Pass the rowspan through the states and reuse the
                    // couple for the next row.
                    element!("td", |el| {
                        let mut s = shared.take();
                        if let State::ExpectRow = s.state {
                            let rowspan = match el.get_attribute("rowspan") {
                                Some(rowspan) => rowspan.parse()?,
                                None => 1,
                            };
                            s.state = State::ExpectCouple(rowspan, String::new());
                        }
                        shared.set(s);
                        Ok(())
                    }),
                    text!("td", |t| {
                        let mut s = shared.take();
                        if s.week == 0 {
                            assert!(s.state == State::ExpectNone);
                            return Ok(());
                        }
                        s.state = match s.state {
                            State::ExpectCouple(rows, mut buffer) => {
                                buffer.push_str(t.as_str());
                                if t.last_in_text_node() {
                                    let couple = html_escape::decode_html_entities(&buffer)
                                        .trim()
                                        .to_owned();
                                    State::ExpectScore(rows, couple, String::new())
                                } else {
                                    State::ExpectCouple(rows, buffer)
                                }
                            }
                            State::ExpectScore(rows, couple, mut buffer) => {
                                buffer.push_str(t.as_str());
                                if t.last_in_text_node() {
                                    // "27 (7,7,8,5)\n"
                                    match html_escape::decode_html_entities(&buffer)
                                        .trim()
                                        .split_whitespace()
                                        .next()
                                        .unwrap()
                                        .parse()
                                    {
                                        Ok(score) => {
                                            State::ExpectDance(rows, couple, score, String::new())
                                        }
                                        Err(_error) => {
                                            // e.g. "N/A\n" for unscored show dance
                                            // ignore this row
                                            State::ExpectEnd(rows - 1, couple)
                                        }
                                    }
                                } else {
                                    State::ExpectScore(rows, couple, buffer)
                                }
                            }
                            State::ExpectDance(rows, couple, score, mut buffer) => {
                                buffer.push_str(t.as_str());
                                if t.last_in_text_node() {
                                    let dance = html_escape::decode_html_entities(&buffer)
                                        .trim()
                                        .to_owned();
                                    let mut i = couple.split(" & ");
                                    let celebrity = i.next().unwrap().to_owned();
                                    let professional = i.next().unwrap().to_owned();
                                    assert!(i.next().is_none());
                                    let row = Row {
                                        series,
                                        week: s.week,
                                        celebrity,
                                        professional,
                                        dance,
                                        score,
                                    };
                                    output.push(row);
                                    State::ExpectEnd(rows - 1, couple)
                                } else {
                                    State::ExpectDance(rows, couple, score, buffer)
                                }
                            }
                            State::ExpectEnd(rows, couple) => {
                                // ignore columns until we see the end of row
                                State::ExpectEnd(rows, couple)
                            }
                            _ => {
                                panic!("Unexpected state");
                            }
                        };
                        shared.set(s);
                        Ok(())
                    }),
                    // New table row - reset search for td elements
                    element!("tr", |_el| {
                        let mut s = shared.take();
                        if let State::ExpectEnd(rows, couple) = s.state {
                            s.state = if rows == 0 {
                                State::ExpectRow
                            } else {
                                State::ExpectScore(rows, couple, String::new())
                            }
                        }
                        shared.set(s);
                        Ok(())
                    }),
                ],
                ..Settings::default()
            },
            |_: &[u8]| (),
        );

        rewriter.write(page.as_ref())?;
        rewriter.end()?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::format;

    use super::extract_rows;

    #[derive(Debug)]
    struct TestError {}

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestError")
        }
    }

    impl Error for TestError {}

    #[test]
    fn test_extract_single_dance_per_couple() -> Result<(), Box<dyn Error>> {
        let top = env!("CARGO_MANIFEST_DIR");
        let page = std::fs::read_to_string(format!("{}/test-data/test1.html", top))?;
        let expected_output = std::fs::read_to_string(format!("{}/test-data/test1.out", top))?;

        let mut wtr = csv::Writer::from_writer(vec![]);
        for row in extract_rows(1, page)? {
            wtr.serialize(row)?;
        }
        let actual_output = String::from_utf8(wtr.into_inner()?)?;
        if expected_output == actual_output {
            Ok(())
        } else {
            dbg!(expected_output);
            dbg!(actual_output);
            Err(Box::new(TestError {}))
        }
    }

    #[test]
    fn test_extract_multiple_dances_per_couple() -> Result<(), Box<dyn Error>> {
        let top = env!("CARGO_MANIFEST_DIR");
        let page = std::fs::read_to_string(format!("{}/test-data/test2.html", top))?;
        let expected_output = std::fs::read_to_string(format!("{}/test-data/test2.out", top))?;

        let mut wtr = csv::Writer::from_writer(vec![]);
        for row in extract_rows(1, page)? {
            wtr.serialize(row)?;
        }
        let actual_output = String::from_utf8(wtr.into_inner()?)?;
        if expected_output == actual_output {
            Ok(())
        } else {
            dbg!(expected_output);
            dbg!(actual_output);
            Err(Box::new(TestError {}))
        }
    }

    #[test]
    #[ignore] // https://github.com/jongiddy/strictly-data/issues/2
    fn test_extract_footnote() -> Result<(), Box<dyn Error>> {
        let top = env!("CARGO_MANIFEST_DIR");
        let page = std::fs::read_to_string(format!("{}/test-data/test3.html", top))?;
        let expected_output = std::fs::read_to_string(format!("{}/test-data/test3.out", top))?;

        let mut wtr = csv::Writer::from_writer(vec![]);
        for row in extract_rows(1, page)? {
            wtr.serialize(row)?;
        }
        let actual_output = String::from_utf8(wtr.into_inner()?)?;
        if expected_output == actual_output {
            Ok(())
        } else {
            dbg!(expected_output);
            dbg!(actual_output);
            Err(Box::new(TestError {}))
        }
    }
}
