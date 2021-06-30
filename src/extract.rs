use lol_html::errors::RewritingError;
use lol_html::html_content::UserData;
use lol_html::{element, text, HtmlRewriter, Settings};
use serde::Serialize;
use std::cell::{Cell, RefCell};

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
    let output: RefCell<Vec<Row>> = RefCell::new(vec![]);

    #[derive(PartialEq)]
    enum State {
        ExpectNone,
        ExpectRow,
        ExpectCouple(i32),
        SkipCouple(i32, String),
        ExpectScore(i32, String),
        ExpectDance(i32, String, u8),
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
    let buffer = RefCell::new(String::new());
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
                            if series == 10 && week == 10 {
                                // Tricky table - handle specially
                                let mut vector = output.borrow_mut();
                                vector.push(Row {
                                    series,
                                    week,
                                    celebrity: "Denise".to_owned(),
                                    professional: "James".to_owned(),
                                    dance: "Jive/Quickstep".to_owned(),
                                    score: 35,
                                });
                                vector.push(Row {
                                    series,
                                    week,
                                    celebrity: "Lisa".to_owned(),
                                    professional: "Robin".to_owned(),
                                    dance: "Cha-Cha-Cha/Tango".to_owned(),
                                    score: 30,
                                });
                                vector.push(Row {
                                    series,
                                    week,
                                    celebrity: "Nicky".to_owned(),
                                    professional: "Karen".to_owned(),
                                    dance: "American Smooth/Samba".to_owned(),
                                    score: 27,
                                });
                                vector.push(Row {
                                    series,
                                    week,
                                    celebrity: "Dani".to_owned(),
                                    professional: "Vincent".to_owned(),
                                    dance: "Charleston/Quickstep".to_owned(),
                                    score: 38,
                                });
                                vector.push(Row {
                                    series,
                                    week,
                                    celebrity: "Louis".to_owned(),
                                    professional: "Flavia".to_owned(),
                                    dance: "Tango/Rumba".to_owned(),
                                    score: 37,
                                });
                                vector.push(Row {
                                    series,
                                    week,
                                    celebrity: "Kimberley".to_owned(),
                                    professional: "Pasha".to_owned(),
                                    dance: "Cha-Cha-Cha/Tango".to_owned(),
                                    score: 40,
                                });
                                return Ok(());
                            }
                            shared.set(Shared {
                                state: State::ExpectRow,
                                week,
                            });
                        }
                    }
                    Ok(())
                }),
                element!("td", |el| {
                    let text = buffer.replace(String::new());
                    let mut s = shared.take();
                    if s.week == 0 {
                        return Ok(());
                    }
                    s.state = match s.state {
                        State::ExpectRow => {
                            // When couples dance multiple dances in a show, the Couples column will
                            // have a rowspan > 1. Pass the rowspan through the states and reuse the
                            // couple for the next row.
                            let rows = match el.get_attribute("rowspan") {
                                Some(rowspan) => rowspan.parse()?,
                                None => 1,
                            };
                            State::ExpectCouple(rows)
                        }
                        State::ExpectCouple(rows) => {
                            let couple = html_escape::decode_html_entities(&text).trim().to_owned();
                            State::ExpectScore(rows, couple)
                        }
                        State::SkipCouple(rows, couple) => State::ExpectScore(rows, couple),
                        State::ExpectScore(rows, couple) => {
                            // "27 (7,7,8,5)\n"
                            match html_escape::decode_html_entities(&text)
                                .trim()
                                .split_whitespace()
                                .next()
                                .unwrap_or("N/A")
                                .parse()
                            {
                                Ok(score) => State::ExpectDance(rows, couple, score),
                                Err(_error) => {
                                    // e.g. "N/A\n" for unscored show dance
                                    // ignore this row
                                    State::ExpectEnd(rows - 1, couple)
                                }
                            }
                        }
                        State::ExpectDance(rows, couple, score) => {
                            let dance = html_escape::decode_html_entities(&text).trim().to_owned();
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
                            output.borrow_mut().push(row);
                            State::ExpectEnd(rows - 1, couple)
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
                text!("td *", |t| {
                    // "<td>Anastacia &amp; Gorka<sup>1</sup>\n</td>"
                    // Set the user data for the text to a boolean to skip the sub-element
                    // in the extracted text.
                    t.set_user_data(true);
                    Ok(())
                }),
                text!("td", |t| {
                    if t.user_data().is::<bool>() {
                        // ignore text in sub-elements of td
                        return Ok(());
                    }
                    let s = shared.take();
                    match s.state {
                        State::ExpectCouple(..)
                        | State::ExpectScore(..)
                        | State::ExpectDance(..) => {
                            buffer.borrow_mut().push_str(t.as_str());
                        }
                        _ => {}
                    }
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
                            State::SkipCouple(rows, couple)
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
    Ok(output.into_inner())
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
