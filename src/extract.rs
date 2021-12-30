use lol_html::errors::RewritingError;
use lol_html::html_content::{Element, EndTag, TextChunk, UserData};
use lol_html::{element, text, HtmlRewriter, Settings};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::error::Error;
use std::rc::Rc;
use std::str::FromStr;

trait TableHandler {
    fn tr_begin(&mut self, _tr: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
    fn tr_end(&mut self, _tr: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
    fn td_begin(&mut self, _td: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
    fn td_break(&mut self, _td: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
    fn td_end(&mut self, _td: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
    fn td_text(&mut self, _t: &TextChunk) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

struct UnrecognizedTable {}
impl UnrecognizedTable {
    fn new() -> UnrecognizedTable {
        UnrecognizedTable {}
    }
}
impl TableHandler for UnrecognizedTable {}

#[derive(Debug, PartialEq)]
enum CoupleExpect {
    NewRow,
    Celebrity,
    KnownFor,
    Professional,
    EndRow,
}
#[derive(Debug)]
struct CoupleTable {
    state: CoupleExpect,
    celebrity: String,
    professional: String,
    celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
    pro_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
}
impl CoupleTable {
    fn new(
        celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
        pro_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
    ) -> CoupleTable {
        CoupleTable {
            state: CoupleExpect::NewRow,
            celebrity: String::new(),
            professional: String::new(),
            celeb_moniker_to_name,
            pro_moniker_to_name,
        }
    }
    fn add_celeb_name(&self, moniker: String, full_name: &str) {
        let mut celeb_moniker_to_name = self.celeb_moniker_to_name.borrow_mut();
        match celeb_moniker_to_name.get(&moniker) {
            Some(_) => {
                // Two contestants have the same moniker!
                // Replace with empty string
                celeb_moniker_to_name.insert(moniker, "".to_owned());
            }
            None => {
                celeb_moniker_to_name.insert(moniker, full_name.to_owned());
            }
        }
    }
    fn add_celeb_names(&self, full_name: &str) {
        // Create a mapping of contestant monikers (short name on the show
        // and in the Week tables) to their full names. Rather than work out
        // their monikers we just create the common transformations and plug
        // them in. If doing this creates duplicates, where the same moniker
        // could be two celebs (e.g. same first name), map the moniker to an
        // empty string.
        if full_name == "DJ Spoony" {
            // the exception to the rules
            self.add_celeb_name("Spoony".to_owned(), full_name);
        } else {
            let mut names = full_name.split(' ');
            // Split returns at least one item so this `unwrap` will not panic
            let first_name = names.next().unwrap().to_owned();
            if let Some(second_name) = names.next() {
                // Some celebs are represented by first name and initial of their surname.
                // e.g two Ricky's in series 7, two Emma's in series 17
                if let Some(initial) = second_name.chars().next() {
                    let name_initial = format!("{} {}.", first_name, initial);
                    self.add_celeb_name(name_initial, full_name);
                }
                // A few are represented by their 2 first "names":
                // Dr. Ranj Singh -> Dr. Ranj
                // Judge Rinder -> Judge Rinder
                // Rev. Richard Coles -> Rev. Richard
                let two_names = format!("{} {}", first_name, second_name);
                self.add_celeb_name(two_names, full_name);
            }
            // Most are represented by their first (or only) name in the Week tables.
            self.add_celeb_name(first_name, full_name);
        }
    }
    fn add_pro_names(&self, full_name: &str) {
        let mut names = full_name.split(' ');
        // Split returns at least one item so this `unwrap` will not panic
        let first_name = names.next().unwrap().to_owned();
        self.pro_moniker_to_name
            .borrow_mut()
            .insert(first_name, full_name.to_owned());
    }
}
impl TableHandler for CoupleTable {
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
        self.add_celeb_names(html_escape::decode_html_entities(&self.celebrity).trim());

        // Where a celebrity dances with more than one professional during a series, we will have
        // their names separated by semi-colons. e.g.
        // Robin Windsor;Brendan Cole (Week 9)
        let professional_decoded = html_escape::decode_html_entities(&self.professional);
        for professional in professional_decoded.split(';') {
            // Split returns at least one item so this `unwrap` will not panic
            let name = professional.split('(').next().unwrap().trim();
            self.add_pro_names(name);
        }

        self.state = CoupleExpect::NewRow;
        self.celebrity.clear();
        self.professional.clear();
        Ok(())
    }

    fn td_break(&mut self, _td: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
        match self.state {
            CoupleExpect::Celebrity => {
                self.celebrity.push(';');
            }
            CoupleExpect::Professional => {
                self.professional.push(';');
            }
            _ => {}
        }
        Ok(())
    }
    fn td_end(&mut self, _td: &EndTag) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.state = match self.state {
            CoupleExpect::Celebrity => CoupleExpect::KnownFor,
            CoupleExpect::KnownFor => CoupleExpect::Professional,
            CoupleExpect::Professional => CoupleExpect::EndRow,
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
            CoupleExpect::Professional => {
                self.professional.push_str(t.as_str());
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
struct WeekTable {
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
    pro_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
}
impl WeekTable {
    fn new_for_week(
        output: Rc<RefCell<Vec<Row>>>,
        celeb_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
        pro_moniker_to_name: Rc<RefCell<HashMap<String, String>>>,
        series: u16,
        week: u16,
    ) -> Self {
        WeekTable {
            output,
            celeb_moniker_to_name,
            pro_moniker_to_name,
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
    fn split_couple(&self, couple: &str) -> (String, String, String) {
        // Split a string "Celeb & Professional" into tuple `("Celeb's Fullname", "Professional")`
        let mut names = couple.split(" & ");
        // Split returns at least one item so this `unwrap` will not panic
        let celeb_moniker = names.next().unwrap();
        // Some couples have an asterisk at the end to refer to a footnote.
        // This `unwrap` can panic
        let pro_moniker = names.next().unwrap().trim_end_matches('*');
        assert!(names.next().is_none());
        // Convert the short celeb name to a full name.
        let celebrity = match self.celeb_moniker_to_name.borrow().get(celeb_moniker) {
            Some(name) if !name.is_empty() => name.clone(),
            _ => celeb_moniker.to_owned(),
        };
        let mut note = self.note.clone();
        let professional = match self.pro_moniker_to_name.borrow().get(pro_moniker) {
            Some(name) if !name.is_empty() => {
                if name == "Karen Clifton" {
                    // Karen Hauer danced as Karen Clifton for some series.
                    // For data analysis, use a consistent name for an individual.
                    assert!(note.is_empty());
                    note = "Karen danced as Karen Clifton".to_owned();
                    "Karen Hauer".to_owned()
                } else {
                    name.clone()
                }
            }
            _ => pro_moniker.to_owned(),
        };
        (celebrity, professional, note)
    }
}
impl TableHandler for WeekTable {
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
        assert!(!self.couple.is_empty());
        assert!(self.couple_uses > 0);
        assert!(!self.score.is_empty());
        assert!(self.score_uses > 0);
        assert!(!self.dance.is_empty());
        assert!(self.dance_uses > 0);
        let dance = html_escape::decode_html_entities(&self.dance)
            .trim()
            .to_owned();
        let couple_decoded = html_escape::decode_html_entities(&self.couple);
        let couple = couple_decoded.trim();
        if couple.contains(';') {
            // Group dance with multiple couples (e.g. Series 7 week 11).
            // These are ranked rather than scored, so we ignore them.
        } else {
            let (celebrity, professional, note) = self.split_couple(couple);
            let scores_decoded = html_escape::decode_html_entities(&self.score);
            let scores = scores_decoded.trim();
            match scores.split_once(' ') {
                None => {
                    // No space in scores. Perhaps "N/A" for unscored showdance.
                    const NONSCORED: [&str; 4] = ["Showdance", "N/A", "", "*"];
                    assert!(NONSCORED.contains(&scores), "{}", scores);
                }
                Some((first, remainder)) => {
                    if let Ok(total_score) = u8::from_str(first) {
                        // The remainder is the individual judges' scores.
                        // Count the separating commas and add one to get the
                        // number of scores. This `unwrap` can panic.
                        let comma_count: u8 = remainder.matches(',').count().try_into()?;
                        let score_count = comma_count + 1;
                        let avg_score = f32::from(total_score) / f32::from(score_count);
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
                            note,
                        });
                    } else {
                        assert!(scores == "Not scored", "{}", scores);
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
    fn td_break(&mut self, _td: &Element) -> Result<(), Box<dyn Error + Send + Sync>> {
        match self.state {
            WeekExpect::Couple => {
                self.couple.push(';');
            }
            WeekExpect::Score => {
                self.score.push(';');
            }
            WeekExpect::Dance => {
                self.dance.push(';');
            }
            _ => {}
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

pub(crate) fn extract_rows(series: u16, page: &str) -> Result<Vec<Row>, RewritingError> {
    // Cell mutability for shared and mutable access from multiple closures.
    let rows = Rc::new(RefCell::<Vec<Row>>::new(vec![]));
    let celeb_moniker_to_name = Rc::new(RefCell::new(HashMap::<String, String>::new()));
    let pro_moniker_to_name = Rc::new(RefCell::new(HashMap::<String, String>::new()));
    let current_table = Rc::new(RefCell::new(
        Box::new(UnrecognizedTable::new()) as Box<dyn TableHandler>
    ));
    let mut default_table_retainer: Option<Box<dyn TableHandler>> = None;

    let element_content_handlers = vec![
        // Find week number
        element!("span.mw-headline", |el| {
            if let Some(id) = el.get_attribute("id") {
                if id == "Couples" {
                    assert!(default_table_retainer.is_none());
                    let prev_table = current_table.replace(Box::new(CoupleTable::new(
                        celeb_moniker_to_name.clone(),
                        pro_moniker_to_name.clone(),
                    )));
                    default_table_retainer = Some(prev_table);
                } else {
                    let mut parts = id.split(&['_', ':'][..]);
                    match parts.next() {
                        Some("Week") => {
                            // "Week_1", "Week_6:_Quarter-final"
                            let week = parts
                                .next()
                                .ok_or_else(|| format!("Bad parse {}", id))?
                                .parse()?;
                            let week_table = Box::new(WeekTable::new_for_week(
                                rows.clone(),
                                celeb_moniker_to_name.clone(),
                                pro_moniker_to_name.clone(),
                                series,
                                week,
                            ));
                            let prev = current_table.replace(week_table);
                            match default_table_retainer {
                                Some(_) => {
                                    // default is already in default_table_retainer, so
                                    // previous table must be previous week.
                                }
                                None => {
                                    default_table_retainer = Some(prev);
                                }
                            }
                        }
                        Some("Night" | "Show") => {
                            // "Night_2_–_Latin", "Show_1" - multiple shows within a week,
                            // ignore these headers so we keep the week as the current table.
                        }
                        _ => {
                            // Use the default no-op table for any other sections.
                            match default_table_retainer.take() {
                                None => {
                                    // current_table is already default
                                }
                                Some(default_table) => {
                                    current_table.replace(default_table);
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }),
        element!("tr", |tr| {
            let table = current_table.clone();
            tr.on_end_tag(move |tr| table.borrow_mut().tr_end(tr))?;
            current_table.borrow_mut().tr_begin(tr)
        }),
        element!("td", |td| {
            let table = current_table.clone();
            td.on_end_tag(move |td| table.borrow_mut().td_end(td))?;
            current_table.borrow_mut().td_begin(td)
        }),
        element!("td br", |td| {
            // `<br />` is used to separate group dances and multiple professionals. In this
            // case we replace the values with semi-colons to help parse later
            current_table.borrow_mut().td_break(td)
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
        text!("td", |t| { current_table.borrow_mut().td_text(t) }),
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
    let result = rows.replace(Vec::new());
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
        for row in extract_rows(1, &page)? {
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
        for row in extract_rows(1, &page)? {
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
        for row in extract_rows(1, &page)? {
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
