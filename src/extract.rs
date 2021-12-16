use lol_html::errors::RewritingError;
use lol_html::html_content::UserData;
use lol_html::{element, text, HtmlRewriter, Settings};
use serde::Serialize;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Serialize)]
pub(crate) struct Row {
    series: u16,
    week: u16,
    celebrity: String,
    professional: String,
    dance: String,
    total_score: u8,
    score_count: u8,
    avg_score: f32,
    note: String,
}

pub(crate) fn extract_rows(series: u16, page: String) -> Result<Vec<Row>, RewritingError> {
    let output = Rc::new(RefCell::<Vec<Row>>::new(vec![]));
    let celeb_moniker_to_name = Rc::new(RefCell::new(HashMap::<String, String>::new()));

    #[derive(Debug, PartialEq)]
    enum CoupleExpect {
        NewRow,
        Celebrity,
        EndRow,
    }
    #[derive(Debug)]
    struct CoupleState {
        state: CoupleExpect,
        celebrity: String,
    }
    impl Default for CoupleState {
        fn default() -> Self {
            CoupleState {
                state: CoupleExpect::NewRow,
                celebrity: String::new(),
            }
        }
    }
    #[derive(Debug, PartialEq)]
    enum WeekExpect {
        NewRow,
        Couple,
        Score,
        Dance,
        EndRow,
    }
    #[derive(Debug)]
    struct WeekState {
        state: WeekExpect,
        week: u16,
        couple: String,
        couple_uses: u8,
        score: String,
        score_uses: u8,
        dance: String,
        dance_uses: u8,
        note: String,
    }
    impl WeekState {
        fn new_for_week(week: u16) -> Self {
            WeekState {
                state: WeekExpect::NewRow,
                week,
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
    #[derive(Debug)]
    enum State {
        Unrecognized,
        CouplesTable(CoupleState),
        WeekTable(WeekState),
    }
    impl Default for State {
        fn default() -> Self {
            State::Unrecognized
        }
    }
    // Cell mutability for shared and mutable access from multiple closures.
    let state = Rc::new(Cell::new(State::Unrecognized));
    let element_content_handlers = vec![
        // Find week number
        element!("span.mw-headline", |el| {
            state.set(State::Unrecognized);
            if let Some(id) = el.get_attribute("id") {
                if id == "Couples" {
                    state.set(State::CouplesTable(CoupleState::default()));
                } else {
                    let mut parts = id.split(&['_', ':'][..]);
                    if let Some("Week") = parts.next() {
                        // "Week_1", "Week_6:_Quarter-final"
                        let week = parts
                            .next()
                            .ok_or_else(|| format!("Bad parse {}", id))?
                            .parse()?;
                        state.set(State::WeekTable(WeekState::new_for_week(week)));
                    }
                }
            }
            Ok(())
        }),
        element!("tr", |tr| {
            match state.take() {
                State::Unrecognized => state.set(State::Unrecognized),
                State::CouplesTable(mut row) => {
                    row.state = match row.state {
                        CoupleExpect::NewRow => CoupleExpect::Celebrity,
                        _ => {
                            panic!("Unexpected state {:?}", row.state);
                        }
                    };
                    state.set(State::CouplesTable(row));
                    let celeb_moniker_to_name = celeb_moniker_to_name.clone();
                    let state = state.clone();
                    tr.on_end_tag(move |_| {
                        match state.take() {
                            State::CouplesTable(row) => {
                                // Create a mapping of celeb monikers (their short name on the show
                                // and in the Week tables) to their full names. Rather than work out
                                // their monikers we just create the common transformations and plug
                                // them in.  If doing this creates duplicates, where the same moniker
                                // could be two celebs (e.g. same first name), map the moniker to an
                                // empty string.
                                let full_name = html_escape::decode_html_entities(&row.celebrity)
                                    .trim()
                                    .to_owned();
                                if full_name == "DJ Spoony" {
                                    // the exception to the rules
                                    celeb_moniker_to_name.borrow_mut().insert("Spoony".to_owned(), full_name);
                                }
                                else {
                                    let add = |moniker: String| {
                                        let mut c = celeb_moniker_to_name.borrow_mut();
                                        match c.get(&moniker) {
                                            Some(_) => {
                                                // Two celebs have the same moniker!
                                                // Replace with empty string
                                                c.insert(moniker, "".to_string());
                                            }
                                            None => {
                                                c.insert(moniker, full_name.clone());
                                            }
                                        }
                                    };
                                    let mut names = full_name.split(" ");
                                    let first_name = names.next().unwrap().to_owned();
                                    if let Some(second_name) = names.next() {
                                        // Some celebs are represented by first name and initial of their surname.
                                        // e.g two Ricky's in series 7, two Emma's in series 17
                                        let initial = second_name.chars().next().unwrap();
                                        let name_initial = format!("{} {}.", first_name, initial);
                                        add(name_initial);
                                        // A few are represented by their 2 first "names":
                                        // Dr. Ranj Singh -> Dr. Ranj
                                        // Judge Rinder -> Judge Rinder
                                        // Rev. Richard Coles -> Rev. Richard
                                        let two_names = format!("{} {}", first_name, second_name);
                                        add(two_names);
                                    }
                                    // Most are represented by their first (or only) name in the Week tables.
                                    add(first_name);
                                }
                            }
                            other => {
                                panic!("Unexpected state {:?}", other);
                            }
                        }
                        state.set(State::CouplesTable(CoupleState::default()));
                        Ok(())
                    })?;
                }
                State::WeekTable(mut row) => {
                    row.state = match row.state {
                        WeekExpect::NewRow => {
                            if row.couple_uses == 0 {
                                WeekExpect::Couple
                            } else if row.score_uses == 0 {
                                WeekExpect::Score
                            } else if row.dance_uses == 0 {
                                WeekExpect::Dance
                            } else {
                                WeekExpect::EndRow
                            }
                        }
                        _ => {
                            panic!("Unexpected state {:?}", row.state);
                        }
                    };
                    state.set(State::WeekTable(row));
                    let output = output.clone();
                    let state = state.clone();
                    let celeb_moniker_to_name = celeb_moniker_to_name.clone();
                    tr.on_end_tag(move |_| {
                        match state.take() {
                            State::WeekTable(mut row) => {
                                if row.state != WeekExpect::EndRow {
                                    // This should only occur for the header row that contains no
                                    // td elements and where there is no couple set.
                                    assert!(row.state == WeekExpect::Couple, "{:?}", row.state);
                                    row.state = WeekExpect::NewRow;
                                    state.set(State::WeekTable(row));
                                    return Ok(());
                                }
                                assert!(
                                    row.state == WeekExpect::EndRow,
                                    "series={} {:?}",
                                    series,
                                    row.state
                                );
                                assert!(row.couple.len() > 0);
                                assert!(row.couple_uses > 0);
                                assert!(row.score.len() > 0);
                                assert!(row.score_uses > 0);
                                assert!(row.dance.len() > 0);
                                assert!(row.dance_uses > 0);
                                let dance = html_escape::decode_html_entities(&row.dance)
                                    .trim()
                                    .to_owned();
                                let couple = html_escape::decode_html_entities(&row.couple)
                                    .trim()
                                    .to_owned();
                                if couple.len() > 0 {
                                    let mut i = couple.split(" & ");
                                    let celeb_moniker = i.next().unwrap();
                                    let professional = i.next().unwrap().to_owned();
                                    match i.next() {
                                        Some(_) => {
                                            // Dance with multiple couples (e.g. Series 7 week 11)
                                            // ignore
                                        }
                                        None => {
                                            // Convert the short celeb name to a full name.
                                            let celebrity = match celeb_moniker_to_name
                                                .borrow()
                                                .get(celeb_moniker)
                                            {
                                                Some(full_name) if !full_name.is_empty() => {
                                                    full_name.clone()
                                                }
                                                _ => celeb_moniker.to_owned(),
                                            };
                                            let scores =
                                                html_escape::decode_html_entities(&row.score);
                                            let mut i = scores.trim().split_whitespace();

                                            match i.next().unwrap_or("N/A").parse() {
                                                Ok(total_score) => {
                                                    // The second word is the individual judges' scores.
                                                    // Count the separating commas and add one to get the
                                                    // number of scores.
                                                    let score_count =
                                                        i.next().unwrap().matches(",").count()
                                                            as u8
                                                            + 1;
                                                    let avg_score =
                                                        total_score as f32 / score_count as f32;
                                                    assert!(avg_score >= 1.0);
                                                    assert!(avg_score <= 10.0);
                                                    output.borrow_mut().push(Row {
                                                        series,
                                                        week: row.week,
                                                        celebrity,
                                                        professional,
                                                        dance,
                                                        total_score,
                                                        score_count,
                                                        avg_score,
                                                        note: row.note.clone(),
                                                    });
                                                }
                                                Err(_error) => {
                                                    // e.g. "N/A\n" for unscored show dance
                                                    // ignore this row
                                                }
                                            }
                                        }
                                    }
                                }
                                row.couple_uses -= 1;
                                row.score_uses -= 1;
                                row.dance_uses -= 1;
                                row.state = WeekExpect::NewRow;
                                state.set(State::WeekTable(row));
                            }
                            other => {
                                panic!("Unexpected state {:?}", other);
                            }
                        }
                        Ok(())
                    })?;
                }
            }
            Ok(())
        }),
        element!("td", |td| {
            match state.take() {
                State::Unrecognized => state.set(State::Unrecognized),
                State::CouplesTable(row) => {
                    match row.state {
                        CoupleExpect::Celebrity => {
                            let state = state.clone();
                            td.on_end_tag(move |_| {
                                match state.take() {
                                    State::CouplesTable(mut row) => {
                                        row.state = match row.state {
                                            CoupleExpect::Celebrity => CoupleExpect::EndRow,
                                            CoupleExpect::EndRow => CoupleExpect::EndRow,
                                            other => {
                                                panic!("Unexpected state {:?}", other);
                                            }
                                        };
                                        state.set(State::CouplesTable(row));
                                    }
                                    other => {
                                        panic!("Unexpected state {:?}", other);
                                    }
                                }
                                Ok(())
                            })?;
                        }
                        _ => {}
                    }
                    state.set(State::CouplesTable(row));
                }
                State::WeekTable(mut row) => {
                    let rows = match td.get_attribute("rowspan") {
                        Some(rowspan) => rowspan.parse()?,
                        None => 1,
                    };
                    match row.state {
                        WeekExpect::Couple => {
                            row.couple.clear();
                            // When couples dance multiple dances in a show, the Couples column will
                            // have a rowspan > 1. Keep the rowspan as the repeat count.
                            row.couple_uses = rows;
                            let state = state.clone();
                            td.on_end_tag(move |_| {
                                match state.take() {
                                    State::WeekTable(mut row) => {
                                        row.state = if row.score_uses == 0 {
                                            WeekExpect::Score
                                        } else if row.dance_uses == 0 {
                                            WeekExpect::Dance
                                        } else {
                                            WeekExpect::EndRow
                                        };
                                        state.set(State::WeekTable(row));
                                    }
                                    other => {
                                        panic!("Unexpected state {:?}", other);
                                    }
                                }
                                Ok(())
                            })?;
                        }
                        WeekExpect::Score => {
                            row.score.clear();
                            row.score_uses = rows;
                            if rows > 1 {
                                // In Series 10, Week 10 couples danced two styles in one dance. For
                                // this week, the scores have rowspan > 1.
                                let len = row.note.len();
                                row.note.replace_range(..len, "combined dance");
                            } else {
                                row.note.clear();
                            }
                            let state = state.clone();
                            td.on_end_tag(move |_| {
                                match state.take() {
                                    State::WeekTable(mut row) => {
                                        row.state = if row.dance_uses == 0 {
                                            WeekExpect::Dance
                                        } else {
                                            WeekExpect::EndRow
                                        };
                                        state.set(State::WeekTable(row));
                                    }
                                    other => {
                                        panic!("Unexpected state {:?}", other);
                                    }
                                }
                                Ok(())
                            })?;
                        }
                        WeekExpect::Dance => {
                            row.dance.clear();
                            row.dance_uses = rows;
                            let state = state.clone();
                            td.on_end_tag(move |_| {
                                match state.take() {
                                    State::WeekTable(mut row) => {
                                        row.state = WeekExpect::EndRow;
                                        state.set(State::WeekTable(row));
                                    }
                                    other => {
                                        panic!("Unexpected state {:?}", other);
                                    }
                                }
                                Ok(())
                            })?;
                        }
                        WeekExpect::EndRow => {
                            // skip remaining columns
                        }
                        other => {
                            panic!("Unexpected state {:?}", other);
                        }
                    };
                    state.set(State::WeekTable(row));
                }
            }
            Ok(())
        }),
        text!("td *", |t| {
            // "<td>Anastacia &amp; Gorka<sup>1</sup>\n</td>"
            // Set the user data for the text to a boolean to skip the sub-element
            // in the extracted text.  We only skip in the Week tables, where
            // subelements are typically footnotes.  In the Couples table most
            // names are inside `a` subelements so we want to keep the text part.
            t.set_user_data(true);
            Ok(())
        }),
        text!("td", |t| {
            let s = state.take();
            match s {
                State::Unrecognized => {
                    state.set(State::Unrecognized);
                }
                State::CouplesTable(mut row) => {
                    match row.state {
                        CoupleExpect::Celebrity => {
                            row.celebrity.push_str(t.as_str());
                        }
                        _ => {}
                    }
                    state.set(State::CouplesTable(row));
                }
                State::WeekTable(mut row) => {
                    if t.user_data().is::<bool>() {
                        // ignore text in sub-elements of td
                        state.set(State::WeekTable(row));
                        return Ok(());
                    }
                    match row.state {
                        WeekExpect::Couple => {
                            row.couple.push_str(t.as_str());
                        }
                        WeekExpect::Score => {
                            row.score.push_str(t.as_str());
                        }
                        WeekExpect::Dance => {
                            row.dance.push_str(t.as_str());
                        }
                        _ => {}
                    }
                    state.set(State::WeekTable(row));
                }
            }
            Ok(())
        }),
    ];

    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers,
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
