use lol_html::errors::RewritingError;
use lol_html::html_content::{Element, EndTag, TextChunk, UserData};
use lol_html::{element, text, HtmlRewriter, Settings};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
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
        celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
    }
    impl CoupleState {
        fn new(celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>) -> CoupleState {
            CoupleState {
                state: CoupleExpect::NewRow,
                celebrity: String::new(),
                celeb_moniker_to_name,
            }
        }
        fn tr_begin(&mut self, _tr: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.state = match self.state {
                CoupleExpect::NewRow => CoupleExpect::Celebrity,
                _ => {
                    panic!("Unexpected state {:?}", self.state);
                }
            };
            Ok(())
        }
        fn tr_end(&mut self, _tr: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
            // Create a mapping of celeb monikers (their short name on the show
            // and in the Week tables) to their full names. Rather than work out
            // their monikers we just create the common transformations and plug
            // them in.  If doing this creates duplicates, where the same moniker
            // could be two celebs (e.g. same first name), map the moniker to an
            // empty string.
            let full_name = html_escape::decode_html_entities(&self.celebrity)
                .trim()
                .to_owned();
            if full_name == "DJ Spoony" {
                // the exception to the rules
                self.celeb_moniker_to_name
                    .borrow_mut()
                    .insert("Spoony".to_owned(), full_name);
            } else {
                let add = |moniker: String| {
                    let mut c = self.celeb_moniker_to_name.borrow_mut();
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
            self.state = CoupleExpect::NewRow;
            self.celebrity.clear();
            Ok(())
        }

        fn td_begin(&mut self, _td: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
        fn td_end(&mut self, _td: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.state = match self.state {
                CoupleExpect::Celebrity => CoupleExpect::EndRow,
                CoupleExpect::EndRow => CoupleExpect::EndRow,
                ref other => {
                    panic!("Unexpected state {:?}", other);
                }
            };
            Ok(())
        }
        fn td_text(&mut self, t: &TextChunk) -> Result<(), Box<dyn Error + Send + Sync>> {
            match self.state {
                CoupleExpect::Celebrity => {
                    self.celebrity.push_str(t.as_str());
                }
                _ => {}
            }
            Ok(())
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
        series: u16,
        week: u16,
        couple: String,
        couple_uses: u8,
        score: String,
        score_uses: u8,
        dance: String,
        dance_uses: u8,
        note: String,
        output: Rc<RefCell<Vec<Row>>>,
        celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
    }
    impl WeekState {
        fn new_for_week(
            output: Rc<RefCell<Vec<Row>>>,
            celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
            series: u16,
            week: u16,
        ) -> Self {
            WeekState {
                output,
                celeb_moniker_to_name,
                state: WeekExpect::NewRow,
                series,
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
        fn tr_begin(&mut self, _tr: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.state = match self.state {
                WeekExpect::NewRow => {
                    if self.couple_uses == 0 {
                        WeekExpect::Couple
                    } else if self.score_uses == 0 {
                        WeekExpect::Score
                    } else if self.dance_uses == 0 {
                        WeekExpect::Dance
                    } else {
                        WeekExpect::EndRow
                    }
                }
                _ => {
                    panic!("Unexpected state {:?}", self.state);
                }
            };
            Ok(())
        }
        fn tr_end(&mut self, _tr: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
            if self.state != WeekExpect::EndRow {
                // This should only occur for the header row that contains no
                // td elements and where there is no couple set.
                assert!(self.state == WeekExpect::Couple, "{:?}", self.state);
                self.state = WeekExpect::NewRow;
                return Ok(());
            }
            assert!(
                self.state == WeekExpect::EndRow,
                "series={} {:?}",
                self.series,
                self.state
            );
            assert!(self.couple.len() > 0);
            assert!(self.couple_uses > 0);
            assert!(self.score.len() > 0);
            assert!(self.score_uses > 0);
            assert!(self.dance.len() > 0);
            assert!(self.dance_uses > 0);
            let dance = html_escape::decode_html_entities(&self.dance)
                .trim()
                .to_owned();
            let couple = html_escape::decode_html_entities(&self.couple)
                .trim()
                .to_owned();
            if couple.len() > 0 {
                let mut i = couple.split(" & ");
                let celeb_moniker = i.next().unwrap();
                // Some couples have an asterisk at the end to refer to a footnote.
                let professional = i.next().unwrap().trim_end_matches('*').to_owned();
                match i.next() {
                    Some(_) => {
                        // Dance with multiple couples (e.g. Series 7 week 11)
                        // ignore
                    }
                    None => {
                        // Convert the short celeb name to a full name.
                        let celebrity = match self.celeb_moniker_to_name.borrow().get(celeb_moniker)
                        {
                            Some(full_name) if !full_name.is_empty() => full_name.clone(),
                            _ => celeb_moniker.to_owned(),
                        };
                        let scores = html_escape::decode_html_entities(&self.score);
                        let mut i = scores.trim().split_whitespace();

                        match i.next().unwrap_or("N/A").parse() {
                            Ok(total_score) => {
                                // The second word is the individual judges' scores.
                                // Count the separating commas and add one to get the
                                // number of scores.
                                let score_count = i.next().unwrap().matches(",").count() as u8 + 1;
                                let avg_score = total_score as f32 / score_count as f32;
                                assert!(avg_score >= 1.0);
                                assert!(avg_score <= 10.0);
                                self.output.borrow_mut().push(Row {
                                    series: self.series,
                                    week: self.week,
                                    celebrity,
                                    professional,
                                    dance,
                                    total_score,
                                    score_count,
                                    avg_score,
                                    note: self.note.clone(),
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
            self.couple_uses -= 1;
            self.score_uses -= 1;
            self.dance_uses -= 1;
            self.state = WeekExpect::NewRow;
            Ok(())
        }
        fn td_begin(&mut self, td: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
            let rows = match td.get_attribute("rowspan") {
                Some(rowspan) => rowspan.parse()?,
                None => 1,
            };
            match self.state {
                WeekExpect::Couple => {
                    self.couple.clear();
                    // When couples dance multiple dances in a show, the Couples column will
                    // have a rowspan > 1. Keep the rowspan as the repeat count.
                    self.couple_uses = rows;
                }
                WeekExpect::Score => {
                    self.score.clear();
                    self.score_uses = rows;
                    if rows > 1 {
                        // In Series 10, Week 10 couples danced two styles in one dance. For
                        // this week, the scores have rowspan > 1.
                        let len = self.note.len();
                        self.note.replace_range(..len, "combined dance");
                    } else {
                        self.note.clear();
                    }
                }
                WeekExpect::Dance => {
                    self.dance.clear();
                    self.dance_uses = rows;
                }
                WeekExpect::EndRow => {
                    // skip remaining columns
                }
                ref other => {
                    panic!("Unexpected state {:?}", other);
                }
            }
            Ok(())
        }
        fn td_end(&mut self, _td: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
            match self.state {
                WeekExpect::Couple => {
                    self.state = if self.score_uses == 0 {
                        WeekExpect::Score
                    } else if self.dance_uses == 0 {
                        WeekExpect::Dance
                    } else {
                        WeekExpect::EndRow
                    };
                }
                WeekExpect::Score => {
                    self.state = if self.dance_uses == 0 {
                        WeekExpect::Dance
                    } else {
                        WeekExpect::EndRow
                    };
                }
                WeekExpect::Dance => {
                    self.state = WeekExpect::EndRow;
                }
                WeekExpect::EndRow => {
                    // skip remaining columns
                }
                ref other => {
                    panic!("Unexpected state {:?}", other);
                }
            }
            Ok(())
        }
        fn td_text(&mut self, t: &TextChunk) -> Result<(), Box<dyn Error + Send + Sync>> {
            if t.user_data().is::<bool>() {
                // ignore text in sub-elements of td
                return Ok(());
            }
            match self.state {
                WeekExpect::Couple => {
                    self.couple.push_str(t.as_str());
                }
                WeekExpect::Score => {
                    self.score.push_str(t.as_str());
                }
                WeekExpect::Dance => {
                    self.dance.push_str(t.as_str());
                }
                _ => {}
            }
            Ok(())
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
    let state = Rc::new(RefCell::new(State::Unrecognized));
    let element_content_handlers = vec![
        // Find week number
        element!("span.mw-headline", |el| {
            if let Some(id) = el.get_attribute("id") {
                if id == "Couples" {
                    *state.borrow_mut() =
                        State::CouplesTable(CoupleState::new(celeb_moniker_to_name.clone()));
                } else {
                    let mut parts = id.split(&['_', ':'][..]);
                    match parts.next() {
                        Some("Week") => {
                            // "Week_1", "Week_6:_Quarter-final"
                            let week = parts
                                .next()
                                .ok_or_else(|| format!("Bad parse {}", id))?
                                .parse()?;
                            *state.borrow_mut() = State::WeekTable(WeekState::new_for_week(
                                output.clone(),
                                celeb_moniker_to_name.clone(),
                                series,
                                week,
                            ));
                        }
                        Some("Night" | "Show") => {
                            // "Night_2_–_Latin", "Show_1" - multiple nights within a week,
                            // ignore so we keep the state in the Week.
                        }
                        _ => {
                            *state.borrow_mut() = State::Unrecognized;
                        }
                    }
                }
            }
            Ok(())
        }),
        element!("tr", |tr| {
            match *state.borrow_mut() {
                State::Unrecognized => {}
                State::CouplesTable(ref mut table) => {
                    let state = state.clone();
                    tr.on_end_tag(move |tr| match *state.borrow_mut() {
                        State::CouplesTable(ref mut table) => table.tr_end(tr),
                        ref other => {
                            panic!("Unexpected state {:?}", other);
                        }
                    })?;
                    return table.tr_begin(tr);
                }
                State::WeekTable(ref mut table) => {
                    let state = state.clone();
                    tr.on_end_tag(move |tr| match *state.borrow_mut() {
                        State::WeekTable(ref mut table) => table.tr_end(tr),
                        ref other => {
                            panic!("Unexpected state {:?}", other);
                        }
                    })?;
                    return table.tr_begin(tr);
                }
            }
            Ok(())
        }),
        element!("td", |td| {
            match *state.borrow_mut() {
                State::Unrecognized => Ok(()),
                State::CouplesTable(ref mut table) => {
                    let state = state.clone();
                    td.on_end_tag(move |td| match *state.borrow_mut() {
                        State::CouplesTable(ref mut table) => table.td_end(td),
                        ref other => {
                            panic!("Unexpected state {:?}", other);
                        }
                    })?;
                    return table.td_begin(td);
                }
                State::WeekTable(ref mut table) => {
                    let state = state.clone();
                    td.on_end_tag(move |td| match *state.borrow_mut() {
                        State::WeekTable(ref mut table) => table.td_end(td),
                        ref other => {
                            panic!("Unexpected state {:?}", other);
                        }
                    })?;
                    return table.td_begin(td);
                }
            }
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
            match *state.borrow_mut() {
                State::Unrecognized => Ok(()),
                State::CouplesTable(ref mut table) => table.td_text(t),
                State::WeekTable(ref mut table) => table.td_text(t),
            }
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
