use lol_html::errors::RewritingError;
use lol_html::html_content::UserData;
use lol_html::{element, text, HtmlRewriter, Settings};
use serde::Serialize;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Serialize)]
pub(crate) struct Row {
    series: u16,
    week: u16,
    celebrity: String,
    professional: String,
    dance: String,
    score: u8,
    note: String,
}

pub(crate) fn extract_rows(series: u16, page: String) -> Result<Vec<Row>, RewritingError> {
    let output = Rc::new(RefCell::<Vec<Row>>::new(vec![]));

    #[derive(Debug, PartialEq)]
    enum State {
        ExpectRow,
        ExpectCouple,
        ExpectScore,
        ExpectDance,
        ExpectEnd,
    }
    #[derive(Debug)]
    struct Shared {
        state: State,
        week: u16,
        couple: String,
        couple_uses: u8,
        score: String,
        score_uses: u8,
        dance: String,
        dance_uses: u8,
        note: String,
    }
    impl Default for Shared {
        fn default() -> Self {
            Shared {
                state: State::ExpectRow,
                week: 0,
                couple: String::new(),
                couple_uses: 0,
                score: String::new(),
                score_uses: 0,
                dance: String::new(),
                dance_uses: 0,
                note: String::new(),
            }
        }
    }
    // Cell mutability for shared and mutable access from multiple closures.
    let shared = Rc::new(RefCell::new(Shared::default()));
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                // Find week number
                element!("span.mw-headline", |el| {
                    let mut s = Shared::default();
                    if let Some(id) = el.get_attribute("id") {
                        // "Week_1", "Week_6:_Quarter-final"
                        let mut parts = id.split(&['_', ':'][..]);
                        if let Some("Week") = parts.next() {
                            s.week = parts
                                .next()
                                .ok_or_else(|| format!("Bad parse {}", id))?
                                .parse()?;
                        }
                    }
                    shared.replace(s);
                    Ok(())
                }),
                element!("tr", |tr| {
                    let mut s = shared.borrow_mut();
                    if s.week == 0 {
                        return Ok(());
                    }
                    s.state = match s.state {
                        State::ExpectRow => {
                            if s.couple_uses == 0 {
                                State::ExpectCouple
                            } else if s.score_uses == 0 {
                                State::ExpectScore
                            } else if s.dance_uses == 0 {
                                State::ExpectDance
                            } else {
                                State::ExpectEnd
                            }
                        }
                        _ => {
                            panic!("Unexpected state {:?}", s);
                        }
                    };
                    let output = output.clone();
                    let shared = shared.clone();
                    tr.on_end_tag(move |_| {
                        let mut s = shared.borrow_mut();
                        if s.state != State::ExpectEnd {
                            // This should only occur for the header row that contains no
                            // td elements and where there is no couple set.
                            assert!(s.state == State::ExpectCouple, "{:?}", s);
                            s.state = State::ExpectRow;
                            return Ok(());
                        }
                        assert!(s.state == State::ExpectEnd, "series={} {:?}", series, s);
                        assert!(s.couple.len() > 0);
                        assert!(s.couple_uses > 0);
                        assert!(s.score.len() > 0);
                        assert!(s.score_uses > 0);
                        assert!(s.dance.len() > 0);
                        assert!(s.dance_uses > 0);
                        let dance = html_escape::decode_html_entities(&s.dance)
                            .trim()
                            .to_owned();
                        let couple = html_escape::decode_html_entities(&s.couple)
                            .trim()
                            .to_owned();
                        let mut i = couple.split(" & ");
                        let celebrity = i.next().unwrap().to_owned();
                        let professional = i.next().unwrap().to_owned();
                        match i.next() {
                            Some(_) => {
                                // Dance with multiple couples (e.g. Series 7 week 11)
                                // ignore
                            }
                            None => {
                                match html_escape::decode_html_entities(&s.score)
                                    .trim()
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("N/A")
                                    .parse()
                                {
                                    Ok(score) => {
                                        output.borrow_mut().push(Row {
                                            series,
                                            week: s.week,
                                            celebrity,
                                            professional,
                                            dance,
                                            score,
                                            note: s.note.clone(),
                                        });
                                    }
                                    Err(_error) => {
                                        // e.g. "N/A\n" for unscored show dance
                                        // ignore this row
                                    }
                                }
                            }
                        }
                        s.couple_uses -= 1;
                        s.score_uses -= 1;
                        s.dance_uses -= 1;
                        s.state = State::ExpectRow;
                        Ok(())
                    })?;
                    Ok(())
                }),
                element!("td", |td| {
                    let mut s = shared.borrow_mut();
                    if s.week == 0 {
                        return Ok(());
                    }
                    let rows = match td.get_attribute("rowspan") {
                        Some(rowspan) => rowspan.parse()?,
                        None => 1,
                    };
                    match s.state {
                        State::ExpectCouple => {
                            s.couple.clear();
                            // When couples dance multiple dances in a show, the Couples column will
                            // have a rowspan > 1. Keep the rowspan as the repeat count.
                            s.couple_uses = rows;
                            let shared = shared.clone();
                            td.on_end_tag(move |_| {
                                let mut s = shared.borrow_mut();
                                s.state = if s.score_uses == 0 {
                                    State::ExpectScore
                                } else if s.dance_uses == 0 {
                                    State::ExpectDance
                                } else {
                                    State::ExpectEnd
                                };
                                Ok(())
                            })?;
                        }
                        State::ExpectScore => {
                            s.score.clear();
                            s.score_uses = rows;
                            if rows > 1 {
                                // In Series 10, Week 10 couples danced two styles in one dance. For
                                // this week, the scores have rowspan > 1.
                                let len = s.note.len();
                                s.note.replace_range(..len, "combined dance");
                            } else {
                                s.note.clear();
                            }
                            let shared = shared.clone();
                            td.on_end_tag(move |_| {
                                let mut s = shared.borrow_mut();
                                s.state = if s.dance_uses == 0 {
                                    State::ExpectDance
                                } else {
                                    State::ExpectEnd
                                };
                                Ok(())
                            })?;
                        }
                        State::ExpectDance => {
                            s.dance.clear();
                            s.dance_uses = rows;
                            let shared = shared.clone();
                            td.on_end_tag(move |_| {
                                let mut s = shared.borrow_mut();
                                s.state = State::ExpectEnd;
                                Ok(())
                            })?;
                        }
                        _ => {
                            // ignore other td elements
                        }
                    }
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
                    let mut s = shared.borrow_mut();
                    match s.state {
                        State::ExpectCouple => {
                            s.couple.push_str(t.as_str());
                        }
                        State::ExpectScore => {
                            s.score.push_str(t.as_str());
                        }
                        State::ExpectDance => {
                            s.dance.push_str(t.as_str());
                        }
                        _ => {}
                    }
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |_: &[u8]| (),
    );

    rewriter.write(page.as_ref())?;
    rewriter.end()?;
    let result = output.replace(Vec::new());
    Ok(result)
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
