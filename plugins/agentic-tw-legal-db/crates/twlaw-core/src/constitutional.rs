use crate::data::{new_cases, old_cases, value_bool, value_str};
use crate::{retrieved_at, TwlawError, TwlawResult};
use regex::Regex;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::{json, Map, Value};
use std::time::Duration;
use url::Url;

const HARD_SAFETY_VALVE: usize = 15_000;
const SNIPPET_CONTEXT: usize = 200;
const SNIPPET_MAX_MATCHES: usize = 10;
const SUBSTANTIVE_THRESHOLD: usize = 50;
const CURRENT_JUDGMENTS_URL: &str = "https://cons.judicial.gov.tw/judcurrentNew1.aspx?fid=38";
const TERMINAL_CASES_URL: &str = "https://cons.judicial.gov.tw/judsearch.aspx?fid=46&type=1";
const TERMINAL_CASES_AJAX_URL: &str = "https://cons.judicial.gov.tw/Ajax/judsearch_src.aspx";
const CONSTITUTIONAL_BASE: &str = "https://cons.judicial.gov.tw/";
const USER_AGENT: &str = "twlaw/0.1";

#[derive(Debug, Clone, Default)]
pub struct InterpretationQuery {
    pub case_id: String,
    pub include_reasoning: bool,
    pub reasoning_keyword: Option<String>,
    pub include_opinions: bool,
    pub opinions_keyword: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InterpretationSearch {
    pub keyword: Option<String>,
    pub year: Option<u32>,
    pub number_from: Option<u32>,
    pub number_to: Option<u32>,
    pub include_old: bool,
    pub include_new: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, Default)]
pub struct CurrentJudgmentsQuery {
    pub year: Option<u32>,
    pub limit: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalCasesQuery {
    pub keyword: Option<String>,
    pub kind: Option<String>,
    pub year_from: Option<u32>,
    pub year_to: Option<u32>,
    pub limit: usize,
}

impl Default for InterpretationSearch {
    fn default() -> Self {
        Self {
            keyword: None,
            year: None,
            number_from: None,
            number_to: None,
            include_old: true,
            include_new: true,
            limit: 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaseSystem {
    Old,
    New,
}

#[derive(Debug, Clone, Copy)]
struct ParsedCaseId {
    system: CaseSystem,
    number: u32,
    year: Option<u32>,
}

fn parse_case_id(case_id: &str) -> TwlawResult<ParsedCaseId> {
    let s = case_id.trim();
    if s.is_empty() {
        return Err(TwlawError::InvalidInput("case_id is required".to_string()));
    }

    if s.contains("憲判") {
        let year_re = Regex::new(r"(\d+)\s*年").expect("valid regex");
        let compact_year_re = Regex::new(r"^\s*(\d+)\s*憲判").expect("valid regex");
        let num_re = Regex::new(r"憲判[^\d]*(\d+)").expect("valid regex");
        let year = year_re
            .captures(s)
            .or_else(|| compact_year_re.captures(s))
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .ok_or_else(|| {
                TwlawError::InvalidInput(
                    "new Constitutional Court rulings require a year, e.g. 111年憲判字第1號"
                        .to_string(),
                )
            })?;
        let number = num_re
            .captures(s)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .ok_or_else(|| {
                TwlawError::InvalidInput(format!("cannot parse ruling number: {case_id}"))
            })?;
        return Ok(ParsedCaseId {
            system: CaseSystem::New,
            number,
            year: Some(year),
        });
    }

    let old_re = Regex::new(r"(?:釋字|解釋)[^\d]*(\d+)").expect("valid regex");
    if s.contains("釋字") || s.contains("解釋") {
        let number = old_re
            .captures(s)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .ok_or_else(|| {
                TwlawError::InvalidInput(format!("cannot parse interpretation number: {case_id}"))
            })?;
        return Ok(ParsedCaseId {
            system: CaseSystem::Old,
            number,
            year: None,
        });
    }

    if let Ok(number) = s.parse::<u32>() {
        return Ok(ParsedCaseId {
            system: CaseSystem::Old,
            number,
            year: None,
        });
    }

    Err(TwlawError::InvalidInput(format!(
        "unsupported case_id format: {case_id}"
    )))
}

fn is_substantive(text: &str) -> bool {
    text.trim().len() >= SUBSTANTIVE_THRESHOLD
}

fn client() -> TwlawResult<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

fn selector(input: &str) -> Selector {
    Selector::parse(input).expect("valid selector")
}

fn text_of(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

fn apply_safety_valve(text: &str) -> (String, bool) {
    if text.chars().count() <= HARD_SAFETY_VALVE {
        return (text.to_string(), false);
    }
    let truncated = text.chars().take(HARD_SAFETY_VALVE).collect::<String>();
    (
        format!(
            "{truncated}\n\n[System Warning: this field was truncated at {HARD_SAFETY_VALVE} characters; do not infer absence from omitted text.]"
        ),
        true,
    )
}

fn snippets(text: &str, keyword: &str) -> (Vec<Value>, usize) {
    if text.is_empty() || keyword.is_empty() {
        return (Vec::new(), 0);
    }

    let mut out = Vec::new();
    let mut total = 0usize;
    let mut start_at = 0usize;
    while let Some(relative) = text[start_at..].find(keyword) {
        let idx = start_at + relative;
        total += 1;
        if out.len() < SNIPPET_MAX_MATCHES {
            let start = text
                .char_indices()
                .map(|(i, _)| i)
                .filter(|i| *i <= idx)
                .rev()
                .nth(SNIPPET_CONTEXT)
                .unwrap_or(0);
            let end_target = idx + keyword.len();
            let end = text
                .char_indices()
                .map(|(i, _)| i)
                .find(|i| *i >= end_target + SNIPPET_CONTEXT)
                .unwrap_or(text.len());
            let mut snippet = text[start..end].to_string();
            if start > 0 {
                snippet.insert_str(0, "...");
            }
            if end < text.len() {
                snippet.push_str("...");
            }
            out.push(json!({"snippet": snippet, "position": idx}));
        }
        start_at = idx + keyword.len();
    }
    (out, total)
}

fn insert_long_field(
    result: &mut Map<String, Value>,
    raw_text: &str,
    field_name: &str,
    include_full: bool,
    keyword: Option<&str>,
) {
    let keyword = keyword.unwrap_or("").trim();
    if keyword.is_empty() && !include_full {
        return;
    }

    let raw_len = raw_text.chars().count();
    if !is_substantive(raw_text) {
        result.insert(format!("{field_name}_unavailable"), json!(true));
        result.insert(format!("{field_name}_full_length"), json!(raw_len));
        result.insert(
            format!("{field_name}_hint"),
            json!("field is empty or appears to be an OCR placeholder; do not infer that the source contains no discussion"),
        );
        return;
    }

    if !keyword.is_empty() {
        let (matches, count) = snippets(raw_text, keyword);
        result.insert(format!("{field_name}_matches"), json!(matches));
        result.insert(format!("{field_name}_match_count"), json!(count));
        result.insert(format!("{field_name}_keyword"), json!(keyword));
        result.insert(format!("{field_name}_full_length"), json!(raw_len));
        if count == 0 {
            result.insert(
                format!("{field_name}_hint"),
                json!("0 matches; use the full field only if this issue is central"),
            );
        }
        return;
    }

    let (text, truncated) = apply_safety_valve(raw_text);
    result.insert(field_name.to_string(), json!(text));
    result.insert(format!("{field_name}_truncated"), json!(truncated));
}

pub fn get_interpretation(query: InterpretationQuery) -> TwlawResult<Value> {
    let parsed = parse_case_id(&query.case_id)?;
    match parsed.system {
        CaseSystem::Old => get_old(parsed.number, query),
        CaseSystem::New => get_new(parsed.year.unwrap_or_default(), parsed.number, query),
    }
}

fn get_old(number: u32, query: InterpretationQuery) -> TwlawResult<Value> {
    let cases = old_cases()?;
    let cached = cases.get(&number.to_string()).ok_or_else(|| {
        TwlawError::NotFound(format!("釋字第 {number} 號 not found in bundled data"))
    })?;

    let mut result = Map::new();
    result.insert("success".to_string(), json!(true));
    result.insert("type".to_string(), json!("釋字"));
    result.insert("case_id".to_string(), json!(format!("釋字第{number}號")));
    result.insert(
        "case_number".to_string(),
        json!(value_str(cached, "case_number")),
    );
    result.insert("date".to_string(), json!(value_str(cached, "date")));
    result.insert("issues".to_string(), json!(value_str(cached, "issues")));
    result.insert(
        "main_text".to_string(),
        json!(value_str(cached, "main_text")),
    );
    result.insert(
        "main_text_truncated".to_string(),
        json!(value_bool(cached, "main_text_truncated")),
    );
    result.insert(
        "related_statutes".to_string(),
        json!(value_str(cached, "related_statutes")),
    );
    result.insert(
        "has_reasoning".to_string(),
        json!(value_bool(cached, "has_reasoning")),
    );
    result.insert(
        "has_opinions".to_string(),
        json!(value_bool(cached, "has_opinions")),
    );
    result.insert(
        "source_url".to_string(),
        json!(value_str(cached, "source_url")),
    );
    result.insert("cached".to_string(), json!(true));
    result.insert("retrieved_at".to_string(), json!(retrieved_at()));

    insert_long_field(
        &mut result,
        value_str(cached, "reasoning"),
        "reasoning",
        query.include_reasoning,
        query.reasoning_keyword.as_deref(),
    );
    insert_long_field(
        &mut result,
        value_str(cached, "opinions"),
        "opinions",
        query.include_opinions,
        query.opinions_keyword.as_deref(),
    );

    Ok(Value::Object(result))
}

fn get_new(year: u32, number: u32, query: InterpretationQuery) -> TwlawResult<Value> {
    let cases = new_cases()?;
    let key = format!("{year}_{number}");
    let cached = cases.get(&key).ok_or_else(|| {
        TwlawError::NotFound(format!(
            "{year}年憲判字第 {number} 號 not found in bundled data"
        ))
    })?;

    let mut result = Map::new();
    result.insert("success".to_string(), json!(true));
    result.insert("type".to_string(), json!("憲判字"));
    result.insert(
        "case_id".to_string(),
        json!(format!("{year}年憲判字第{number}號")),
    );
    result.insert(
        "case_number".to_string(),
        json!(value_str(cached, "case_number")),
    );
    result.insert("date".to_string(), json!(value_str(cached, "date")));
    result.insert(
        "petitioner".to_string(),
        json!(value_str(cached, "petitioner")),
    );
    result.insert(
        "issue_summary".to_string(),
        json!(value_str(cached, "issue_summary")),
    );
    result.insert(
        "main_text".to_string(),
        json!(value_str(cached, "main_text")),
    );
    result.insert(
        "main_text_truncated".to_string(),
        json!(value_bool(cached, "main_text_truncated")),
    );
    result.insert("summary".to_string(), json!(value_str(cached, "summary")));
    result.insert(
        "summary_truncated".to_string(),
        json!(value_bool(cached, "summary_truncated")),
    );
    result.insert(
        "related_statutes".to_string(),
        json!(value_str(cached, "related_statutes")),
    );
    result.insert(
        "has_reasoning".to_string(),
        json!(value_bool(cached, "has_reasoning")),
    );
    result.insert(
        "has_opinions".to_string(),
        json!(value_bool(cached, "has_opinions")),
    );
    result.insert(
        "source_url".to_string(),
        json!(value_str(cached, "source_url")),
    );
    result.insert("cached".to_string(), json!(true));
    result.insert("retrieved_at".to_string(), json!(retrieved_at()));

    insert_long_field(
        &mut result,
        value_str(cached, "reasoning"),
        "reasoning",
        query.include_reasoning,
        query.reasoning_keyword.as_deref(),
    );
    insert_long_field(
        &mut result,
        value_str(cached, "opinions"),
        "opinions",
        query.include_opinions,
        query.opinions_keyword.as_deref(),
    );

    Ok(Value::Object(result))
}

fn in_range(number: u32, from: Option<u32>, to: Option<u32>) -> bool {
    from.map_or(true, |min| number >= min) && to.map_or(true, |max| number <= max)
}

pub fn search_interpretations(search: InterpretationSearch) -> TwlawResult<Value> {
    let keyword = search.keyword.unwrap_or_default();
    let keyword = keyword.trim().to_string();
    let limit = search.limit.clamp(1, 200);
    let mut results = Vec::new();

    if search.include_new {
        for (key, value) in new_cases()? {
            let Some((year, number)) = parse_new_key(key) else {
                continue;
            };
            if search.year.map_or(false, |wanted| wanted != year) {
                continue;
            }
            if !in_range(number, search.number_from, search.number_to) {
                continue;
            }
            let title = format!("{year}年憲判字第{number}號");
            if !keyword.is_empty()
                && !title.contains(&keyword)
                && !value_str(value, "issue_summary").contains(&keyword)
                && !value_str(value, "reasoning").contains(&keyword)
            {
                continue;
            }
            results.push(json!({
                "type": "憲判字",
                "case_id": title,
                "year": year,
                "number": number,
                "title": value_str(value, "case_number"),
                "issues": if keyword.is_empty() { "" } else { value_str(value, "issue_summary") }
            }));
        }
    }

    if search.include_old && search.year.is_none() {
        for (key, value) in old_cases()? {
            let Ok(number) = key.parse::<u32>() else {
                continue;
            };
            if !in_range(number, search.number_from, search.number_to) {
                continue;
            }
            let title = format!("釋字第{number}號");
            if !keyword.is_empty()
                && !title.contains(&keyword)
                && !value_str(value, "issues").contains(&keyword)
                && !value_str(value, "reasoning").contains(&keyword)
            {
                continue;
            }
            results.push(json!({
                "type": "釋字",
                "case_id": title,
                "number": number,
                "title": value_str(value, "case_number"),
                "issues": if keyword.is_empty() { "" } else { value_str(value, "issues") }
            }));
        }
    }

    results.sort_by(|a, b| {
        let ay = a.get("year").and_then(Value::as_u64).unwrap_or(0);
        let by = b.get("year").and_then(Value::as_u64).unwrap_or(0);
        let an = a.get("number").and_then(Value::as_u64).unwrap_or(0);
        let bn = b.get("number").and_then(Value::as_u64).unwrap_or(0);
        by.cmp(&ay).then_with(|| bn.cmp(&an))
    });

    let count = results.len();
    let truncated = count > limit;
    results.truncate(limit);

    Ok(json!({
        "success": true,
        "keyword": keyword,
        "count": count,
        "truncated": truncated,
        "results": results,
        "note": "Bundled offline index supports title, issue summary, and reasoning keyword search. Use interpretation get for full details.",
        "cached": true,
        "retrieved_at": retrieved_at()
    }))
}

pub fn current_judgments(query: CurrentJudgmentsQuery) -> TwlawResult<Value> {
    let limit = query.limit.clamp(1, 200);
    let client = client()?;
    let response = client
        .get(CURRENT_JUDGMENTS_URL)
        .send()?
        .error_for_status()?;
    let source_url = response.url().to_string();
    let html = response.text()?;
    let document = Html::parse_document(&html);
    let row_selector = selector("ul.tcont");
    let cont_selector = selector(".cont");
    let link_selector = selector(r#"a[href*="docdata.aspx?fid=38"]"#);
    let case_re = Regex::new(
        r"(?P<year>\d{3})年憲判字第(?P<number>\d+)號(?:[【〖](?P<caption>[^】〗]+)[】〗])?",
    )
    .expect("valid regex");
    let base = Url::parse(CONSTITUTIONAL_BASE)?;
    let mut results = Vec::new();

    for row in document.select(&row_selector) {
        let mut conts = row.select(&cont_selector);
        let date = conts.next().map(text_of).unwrap_or_default();
        let Some(link) = row.select(&link_selector).next() else {
            continue;
        };
        let title = link
            .value()
            .attr("title")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| text_of(link));
        let normalized = title.replace([' ', '\n', '\t'], "");
        let Some(caps) = case_re.captures(&normalized) else {
            continue;
        };
        let year = caps
            .name("year")
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        if query.year.map_or(false, |wanted| wanted != year) {
            continue;
        }
        let number = caps
            .name("number")
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let case_id = format!("{year}年憲判字第{number}號");
        let caption = caps
            .name("caption")
            .map(|value| value.as_str().to_string())
            .unwrap_or_default();
        let href = link.value().attr("href").unwrap_or("");
        let judgment_url = base.join(href)?.to_string();

        results.push(json!({
            "type": "憲判字",
            "case_id": case_id,
            "year": year,
            "number": number,
            "title": title,
            "caption": caption,
            "date": date,
            "source_url": judgment_url
        }));
    }

    if results.is_empty() {
        return Err(TwlawError::ParseChanged(
            "could not parse Constitutional Court current judgment list".to_string(),
        ));
    }

    let count = results.len();
    let truncated = count > limit;
    results.truncate(limit);

    Ok(json!({
        "success": true,
        "source": "Constitutional Court current judgments",
        "source_url": source_url,
        "query": {
            "year": query.year
        },
        "count": count,
        "truncated": truncated,
        "results": results,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

pub fn terminal_cases(query: TerminalCasesQuery) -> TwlawResult<Value> {
    let kind = TerminalCaseKind::parse(query.kind.as_deref())?;
    if let (Some(from), Some(to)) = (query.year_from, query.year_to) {
        if from > to {
            return Err(TwlawError::InvalidInput(
                "year-from cannot be greater than year-to".to_string(),
            ));
        }
    }

    let limit = query.limit.clamp(1, 100);
    let keyword = query
        .keyword
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    let client = client()?;
    let mut page = 1usize;
    let mut pages_fetched = 0usize;
    let mut total_count = 0usize;
    let mut total_pages = 0usize;
    let mut results = Vec::new();
    let mut source_url = TERMINAL_CASES_AJAX_URL.to_string();

    while results.len() < limit {
        let response = terminal_cases_page(&client, &query, &kind, &keyword, page)?;
        pages_fetched += 1;
        source_url = response.source_url;
        total_count = response.count;
        total_pages = response.total_pages.max(1);
        results.extend(response.results);
        if page >= total_pages || response.result_count == 0 {
            break;
        }
        page += 1;
    }

    let retrieved_count = results.len();
    results.truncate(limit);
    let truncated = total_count > results.len();

    Ok(json!({
        "success": true,
        "source": "Constitutional Court terminal case search",
        "source_url": TERMINAL_CASES_URL,
        "ajax_url": source_url,
        "query": {
            "keyword": keyword,
            "kind": kind.id,
            "kind_label": kind.label,
            "year_from": query.year_from,
            "year_to": query.year_to
        },
        "count": total_count,
        "total_pages": total_pages,
        "pages_fetched": pages_fetched,
        "returned_count": results.len(),
        "retrieved_count_before_limit": retrieved_count,
        "truncated": truncated,
        "results": results,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

struct TerminalCasesPage {
    source_url: String,
    count: usize,
    total_pages: usize,
    result_count: usize,
    results: Vec<Value>,
}

#[derive(Debug, Clone, Copy)]
struct TerminalCaseKind {
    id: &'static str,
    label: &'static str,
    form_value: Option<&'static str>,
}

impl TerminalCaseKind {
    fn parse(input: Option<&str>) -> TwlawResult<Self> {
        let value = input.unwrap_or("all").trim().to_ascii_lowercase();
        match value.as_str() {
            "" | "all" => Ok(Self {
                id: "all",
                label: "全部",
                form_value: None,
            }),
            "interpretation" | "interpretations" | "explanation" | "解釋" => Ok(Self {
                id: "interpretation",
                label: "解釋",
                form_value: Some("'解釋'"),
            }),
            "non-acceptance" | "non_acceptance" | "decision" | "不受理決議" => Ok(Self {
                id: "non-acceptance-decision",
                label: "不受理決議",
                form_value: Some("'不受理決議'"),
            }),
            "judgment" | "judgments" | "判決" => Ok(Self {
                id: "judgment",
                label: "判決",
                form_value: Some("'判決'"),
            }),
            "substantive-ruling" | "substantive_ruling" | "實體裁定" => Ok(Self {
                id: "substantive-ruling",
                label: "實體裁定",
                form_value: Some("'實體裁定'"),
            }),
            "procedure-ruling" | "procedure_ruling" | "procedural-ruling" | "procedural_ruling"
            | "程序裁定" => Ok(Self {
                id: "procedure-ruling",
                label: "程序裁定",
                form_value: Some("'程序裁定'"),
            }),
            other => Err(TwlawError::InvalidInput(format!(
                "unknown Constitutional Court terminal case kind: {other}; expected all, interpretation, non-acceptance, judgment, substantive-ruling, or procedure-ruling"
            ))),
        }
    }
}

fn terminal_cases_page(
    client: &Client,
    query: &TerminalCasesQuery,
    kind: &TerminalCaseKind,
    keyword: &str,
    page: usize,
) -> TwlawResult<TerminalCasesPage> {
    let mut form = vec![
        ("pageid".to_string(), page.to_string()),
        ("s_doc_date".to_string(), "1".to_string()),
    ];
    if !keyword.is_empty() {
        form.push(("search_cond_str[]".to_string(), keyword.to_string()));
        form.push(("search_cond_col[]".to_string(), "0".to_string()));
    }
    if let Some(value) = kind.form_value {
        form.push(("advsearch_cond_kind[]".to_string(), value.to_string()));
    }
    if let Some(year) = query.year_from {
        form.push(("advsearch_start_year".to_string(), year.to_string()));
        form.push(("advsearch_start_month".to_string(), "1".to_string()));
        form.push(("advsearch_start_day".to_string(), "1".to_string()));
    }
    if let Some(year) = query.year_to {
        form.push(("advsearch_end_year".to_string(), year.to_string()));
        form.push(("advsearch_end_month".to_string(), "12".to_string()));
        form.push(("advsearch_end_day".to_string(), "31".to_string()));
    }

    let response = client
        .post(TERMINAL_CASES_AJAX_URL)
        .header("X-Requested-With", "XMLHttpRequest")
        .form(&form)
        .send()?
        .error_for_status()?;
    let source_url = response.url().to_string();
    let value: Value = response.json()?;
    if value.get("res").and_then(Value::as_str) != Some("1") {
        return Err(TwlawError::ParseChanged(
            "Constitutional Court terminal case search returned an unexpected response".to_string(),
        ));
    }

    let count = value
        .get("count")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(0);
    let total_pages = value
        .get("total")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<f64>().ok())
        .map(|raw| raw.ceil() as usize)
        .unwrap_or(0);
    let html = value.get("html").and_then(Value::as_str).unwrap_or("");
    let results = parse_terminal_case_rows(html)?;
    let result_count = results.len();

    Ok(TerminalCasesPage {
        source_url,
        count,
        total_pages,
        result_count,
        results,
    })
}

fn parse_terminal_case_rows(html: &str) -> TwlawResult<Vec<Value>> {
    let document = Html::parse_fragment(html);
    let row_selector = selector("ul.tcont");
    let cell_selector = selector("li");
    let label_selector = selector("span");
    let cont_selector = selector(".cont");
    let link_selector = selector("a[href]");
    let base = Url::parse(CONSTITUTIONAL_BASE)?;
    let mut results = Vec::new();

    for row in document.select(&row_selector) {
        let mut item = Map::new();
        for cell in row.select(&cell_selector) {
            let label = cell
                .select(&label_selector)
                .next()
                .map(text_of)
                .unwrap_or_default();
            let value = cell
                .select(&cont_selector)
                .next()
                .map(text_of)
                .unwrap_or_default();
            if !label.is_empty() {
                item.insert(label, json!(value));
            }
        }
        let Some(link) = row.select(&link_selector).next() else {
            continue;
        };
        let href = link.value().attr("href").unwrap_or("");
        let source_url = base.join(href)?.to_string();
        let case_id = text_of(link);
        let detail_id = Url::parse(&source_url)
            .ok()
            .and_then(|url| {
                url.query_pairs()
                    .find(|(key, _)| key.eq_ignore_ascii_case("id"))
                    .map(|(_, value)| value.to_string())
            })
            .unwrap_or_default();

        results.push(json!({
            "row_number": item.get("項次").and_then(Value::as_str).and_then(|raw| raw.parse::<usize>().ok()),
            "year": item.get("年度").and_then(Value::as_str).and_then(|raw| raw.parse::<u32>().ok()),
            "case_id": case_id,
            "case_type": item.get("類別").and_then(Value::as_str).unwrap_or(""),
            "issue_summary": item.get("案由").and_then(Value::as_str).unwrap_or(""),
            "detail_id": detail_id,
            "source_url": source_url
        }));
    }

    Ok(results)
}

fn parse_new_key(key: &str) -> Option<(u32, u32)> {
    let mut parts = key.split('_');
    let year = parts.next()?.parse::<u32>().ok()?;
    let number = parts.next()?.parse::<u32>().ok()?;
    Some((year, number))
}

pub fn get_citations(case_id: &str, include_context: bool) -> TwlawResult<Value> {
    let parsed = parse_case_id(case_id)?;
    let (source_case_id, reasoning) = match parsed.system {
        CaseSystem::Old => {
            let number = parsed.number;
            let cases = old_cases()?;
            let value = cases
                .get(&number.to_string())
                .ok_or_else(|| TwlawError::NotFound(format!("釋字第 {number} 號 not found")))?;
            (
                format!("釋字第{number}號"),
                value_str(value, "reasoning").to_string(),
            )
        }
        CaseSystem::New => {
            let year = parsed.year.unwrap_or_default();
            let number = parsed.number;
            let cases = new_cases()?;
            let key = format!("{year}_{number}");
            let value = cases.get(&key).ok_or_else(|| {
                TwlawError::NotFound(format!("{year}年憲判字第 {number} 號 not found"))
            })?;
            (
                format!("{year}年憲判字第{number}號"),
                value_str(value, "reasoning").to_string(),
            )
        }
    };

    let (reasoning, truncated) = apply_safety_valve(&reasoning);
    let mut citations = extract_citations(&reasoning);
    if include_context {
        for citation in &mut citations {
            attach_context(citation, &reasoning);
        }
    }

    Ok(json!({
        "success": true,
        "source_case_id": source_case_id,
        "citations": citations,
        "citation_count": citations.len(),
        "reasoning_truncated": truncated,
        "cached": true,
        "retrieved_at": retrieved_at()
    }))
}

fn extract_citations(text: &str) -> Vec<Value> {
    let old_re = Regex::new(r"釋字第\s*(\d+)\s*號").expect("valid regex");
    let new_re = Regex::new(r"(\d{3,4})\s*年\s*憲判字第\s*(\d+)\s*號").expect("valid regex");
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for caps in old_re.captures_iter(text) {
        let number = caps
            .get(1)
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let id = format!("釋字第{number}號");
        if seen.insert(id.clone()) {
            out.push(json!({"type": "釋字", "case_id": id, "number": number}));
        }
    }
    for caps in new_re.captures_iter(text) {
        let year = caps
            .get(1)
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let number = caps
            .get(2)
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let id = format!("{year}年憲判字第{number}號");
        if seen.insert(id.clone()) {
            out.push(json!({"type": "憲判字", "case_id": id, "year": year, "number": number}));
        }
    }

    out.sort_by(|a, b| {
        let at = a.get("type").and_then(Value::as_str).unwrap_or("");
        let bt = b.get("type").and_then(Value::as_str).unwrap_or("");
        let ay = a.get("year").and_then(Value::as_u64).unwrap_or(0);
        let by = b.get("year").and_then(Value::as_u64).unwrap_or(0);
        let an = a.get("number").and_then(Value::as_u64).unwrap_or(0);
        let bn = b.get("number").and_then(Value::as_u64).unwrap_or(0);
        at.cmp(bt)
            .then_with(|| ay.cmp(&by))
            .then_with(|| an.cmp(&bn))
    });
    out
}

fn attach_context(citation: &mut Value, text: &str) {
    let Some(case_id) = citation.get("case_id").and_then(Value::as_str) else {
        return;
    };
    let mut snippets_out = Vec::new();
    let mut start_at = 0usize;
    while let Some(relative) = text[start_at..].find(case_id) {
        let idx = start_at + relative;
        let start = text[..idx]
            .char_indices()
            .rev()
            .nth(80)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let match_end = idx + case_id.len();
        let end = text[match_end..]
            .char_indices()
            .nth(80)
            .map(|(i, _)| match_end + i)
            .unwrap_or(text.len());
        snippets_out.push(text[start..end].to_string());
        start_at = idx + case_id.len();
    }
    citation["context_snippets"] = json!(snippets_out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_old_interpretation_from_bundle() {
        let value = get_interpretation(InterpretationQuery {
            case_id: "釋字748".to_string(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["type"], "釋字");
        assert!(value["main_text"].as_str().unwrap().contains("婚姻"));
    }

    #[test]
    fn keyword_mode_returns_snippets() {
        let value = get_interpretation(InterpretationQuery {
            case_id: "釋字748".to_string(),
            reasoning_keyword: Some("婚姻".to_string()),
            ..Default::default()
        })
        .unwrap();
        assert!(value["reasoning_match_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn searches_interpretations() {
        let value = search_interpretations(InterpretationSearch {
            keyword: Some("集會自由".to_string()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(value["success"], true);
        assert!(value["count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn extracts_citations() {
        let value = get_citations("釋字748", false).unwrap();
        assert_eq!(value["success"], true);
        assert!(value["citation_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn parses_terminal_case_ajax_rows() {
        let html = r#"
        <ul class="tcont flex">
            <li><span>項次</span><div class="cont">1</div></li>
            <li><span>年度</span><div class="cont">115</div></li>
            <li><span>字號</span><div class="cont"><a target="_blank" href="/docdata.aspx?fid=97&id=351300">115年憲判字第4號</a></div></li>
            <li><span>類別</span><div class="cont">判決</div></li>
            <li><span>案由</span><div class="cont">聲請法規範及裁判憲法審查。</div></li>
        </ul>
        "#;
        let rows = parse_terminal_case_rows(html).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["case_id"], "115年憲判字第4號");
        assert_eq!(rows[0]["year"], 115);
        assert_eq!(rows[0]["detail_id"], "351300");
    }

    #[test]
    fn parses_terminal_case_kind_aliases() {
        let kind = TerminalCaseKind::parse(Some("procedure-ruling")).unwrap();
        assert_eq!(kind.id, "procedure-ruling");
        assert_eq!(kind.form_value, Some("'程序裁定'"));
    }
}
