use crate::{retrieved_at, TwlawError, TwlawResult};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use scraper::{ElementRef, Html, Selector};
use serde_json::{json, Value};
use std::thread::sleep;
use std::time::Duration;
use url::Url;

const BASE_URL: &str = "https://lis.ly.gov.tw";
const LGLAWKM_URL: &str = "https://lis.ly.gov.tw/lglawc/lglawkm";
const USER_AGENT: &str = "twlaw/0.1";
const RETRY_ATTEMPTS: usize = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, Default)]
pub struct LegislativeHistoryQuery {
    pub law: String,
    pub date: Option<String>,
    pub article: Option<String>,
    pub include_reasons: bool,
    pub all_versions: bool,
    pub limit: usize,
}

#[derive(Debug, Clone)]
struct SearchForm {
    action: Url,
    info: String,
}

#[derive(Debug, Clone)]
struct LawSearchResult {
    title: String,
    source_url: String,
    passed_date: Option<String>,
    promulgated_date: Option<String>,
    related_links: Vec<Value>,
}

#[derive(Debug, Clone)]
struct VersionEntry {
    action: String,
    action_date: Option<String>,
    promulgation: Option<String>,
    effective: Option<String>,
    source_url: String,
}

#[derive(Debug, Clone, Default)]
struct ReasonEntry {
    article: String,
    article_number: Option<String>,
    change_type: Option<String>,
    text: Option<String>,
    reason: Option<String>,
    gazette_links: Vec<Value>,
}

pub fn legislative_history(query: LegislativeHistoryQuery) -> TwlawResult<Value> {
    let law = query.law.trim().to_string();
    if law.is_empty() {
        return Err(TwlawError::InvalidInput("provide --law".to_string()));
    }
    if query.include_reasons
        && query.all_versions
        && query
            .article
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(TwlawError::InvalidInput(
            "--all-versions requires --article to keep live Legislative Yuan queries bounded"
                .to_string(),
        ));
    }

    let client = client()?;
    let limit = query.limit.clamp(1, 100);
    let home_url = Url::parse(LGLAWKM_URL)?;
    let (home_html, _) = fetch_text_url(&client, &home_url)?;
    let form = parse_search_form(&home_html)?;
    let (search_html, search_source_url) = submit_law_search(&client, &form, &law)?;
    let mut search_results = parse_search_results(&search_html, &Url::parse(&search_source_url)?)?;

    if search_results.is_empty() {
        return Err(TwlawError::NotFound(format!(
            "no Legislative Yuan law-history results for: {law}"
        )));
    }

    let selected = choose_search_result(&search_results, &law)
        .cloned()
        .ok_or_else(|| TwlawError::NotFound(format!("no selectable result for: {law}")))?;
    search_results.truncate(limit);

    let selected_url = Url::parse(&selected.source_url)?;
    let (detail_html, detail_source_url) = fetch_text_url(&client, &selected_url)?;
    let detail_url = Url::parse(&detail_source_url)?;
    let law_name = parse_law_name(&detail_html).unwrap_or_else(|| selected.title.clone());
    let versions = parse_versions(&detail_html, &detail_url);
    let selected_version = choose_version(&versions, query.date.as_deref()).cloned();

    let mut result = json!({
        "success": true,
        "source": "立法院法律系統",
        "source_url": LGLAWKM_URL,
        "query": {
            "law": law,
            "date": query.date.clone(),
            "article": query.article.clone(),
            "include_reasons": query.include_reasons,
            "all_versions": query.all_versions,
            "limit": limit
        },
        "selected_law": {
            "name": law_name,
            "matched_title": selected.title,
            "source_url": selected.source_url,
            "passed_date": selected.passed_date,
            "promulgated_date": selected.promulgated_date,
            "related_links": selected.related_links
        },
        "search": {
            "source_url": search_source_url,
            "returned_count": search_results.len(),
            "results": search_results.iter().map(search_result_json).collect::<Vec<_>>()
        },
        "history": {
            "source_url": detail_source_url,
            "version_count": versions.len(),
            "selected_version": selected_version.as_ref().map(version_json),
            "versions": versions.iter().map(version_json).collect::<Vec<_>>()
        },
        "cached": false,
        "retrieved_at": retrieved_at()
    });

    if query.include_reasons {
        if query.all_versions {
            let article = query
                .article
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .expect("validated all-versions article");
            result["article_history"] =
                scan_article_history(&client, &versions, article, query.date.as_deref())?;
        } else {
            let version = selected_version.ok_or_else(|| {
                TwlawError::NotFound(
                    "no matching legislative-history version for requested --date".to_string(),
                )
            })?;
            result["reasons"] = fetch_version_reasons(&client, &version, query.article.as_deref())?;
        }
    }

    Ok(result)
}

fn scan_article_history(
    client: &Client,
    versions: &[VersionEntry],
    article: &str,
    date: Option<&str>,
) -> TwlawResult<Value> {
    let matching_versions = if let Some(wanted) = date.and_then(normalize_date_query) {
        versions
            .iter()
            .filter(|version| version.action_date.as_deref() == Some(wanted.as_str()))
            .collect::<Vec<_>>()
    } else {
        versions.iter().collect::<Vec<_>>()
    };
    let mut matches = Vec::new();
    let mut skipped = Vec::new();

    for version in matching_versions {
        let reasons = match fetch_version_reasons(client, version, Some(article)) {
            Ok(reasons) => reasons,
            Err(err) => {
                skipped.push(json!({
                    "version": version_json(version),
                    "error": {
                        "code": err.code(),
                        "message": err.to_string()
                    }
                }));
                continue;
            }
        };
        if reasons["returned_count"].as_u64().unwrap_or(0) == 0 {
            continue;
        }
        matches.push(reasons);
    }

    Ok(json!({
        "article_filter": article,
        "version_matches": matches.len(),
        "matches": matches,
        "scan": {
            "version_count": versions.len(),
            "skipped_count": skipped.len(),
            "skipped": skipped,
            "bounded": true,
            "note": "Sequentially fetched Legislative Yuan article-reason pages for known law-history versions; keep live query concurrency low."
        }
    }))
}

fn fetch_version_reasons(
    client: &Client,
    version: &VersionEntry,
    article: Option<&str>,
) -> TwlawResult<Value> {
    let version_url = Url::parse(&version.source_url)?;
    let (version_html, version_source_url) = fetch_text_url(client, &version_url)?;
    let version_source = Url::parse(&version_source_url)?;
    let reason_url = parse_yellow_button_url(&version_html, &version_source, "yellow_btn01")
        .ok_or_else(|| {
            TwlawError::ParseChanged(
                "could not find Legislative Yuan article-reason link".to_string(),
            )
        })?;
    let (reason_html, reason_source_url) = fetch_text_url(client, &reason_url)?;
    let mut reasons = parse_reason_entries(&reason_html, &Url::parse(&reason_source_url)?);
    if let Some(article) = article.map(str::trim).filter(|value| !value.is_empty()) {
        let wanted = normalize_article_query(article);
        reasons.retain(|entry| {
            entry
                .article_number
                .as_deref()
                .map(|number| number == wanted)
                .unwrap_or_else(|| entry.article.contains(article))
        });
    }
    reasons.retain(has_reason_content);
    Ok(json!({
        "source_url": reason_source_url,
        "version": version_json(version),
        "article_filter": article,
        "returned_count": reasons.len(),
        "entries": reasons.iter().map(reason_json).collect::<Vec<_>>()
    }))
}

fn has_reason_content(entry: &ReasonEntry) -> bool {
    entry
        .text
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || entry
            .reason
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        || !entry.gazette_links.is_empty()
}

fn client() -> TwlawResult<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

fn fetch_text_url(client: &Client, url: &Url) -> TwlawResult<(String, String)> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.get(url.clone()).send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt + 1 < RETRY_ATTEMPTS {
                    retry_delay(attempt);
                    continue;
                }
                match response.error_for_status() {
                    Ok(ok) => {
                        let source_url = ok.url().to_string();
                        match ok.text() {
                            Ok(text) => return Ok((text, source_url)),
                            Err(err) => last_error = Some(err.to_string()),
                        }
                    }
                    Err(err) => last_error = Some(err.to_string()),
                }
            }
            Err(err) => last_error = Some(err.to_string()),
        }
        if attempt + 1 < RETRY_ATTEMPTS {
            retry_delay(attempt);
        }
    }

    Err(TwlawError::Network(format!(
        "failed to fetch {url}: {}",
        last_error.unwrap_or_else(|| "unknown network error".to_string())
    )))
}

fn should_retry_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: usize) {
    let multiplier = 1_u64 << attempt.min(4);
    sleep(Duration::from_millis(RETRY_BASE_DELAY_MS * multiplier));
}

fn submit_law_search(
    client: &Client,
    form: &SearchForm,
    law: &str,
) -> TwlawResult<(String, String)> {
    let params = [
        ("INFO", form.info.as_str()),
        ("@_1_6_T", "T_LN/LW"),
        ("_1_6_T", law),
        ("@_1_5_T", "T_LW/AL/AX/AZ/AW/AY/AV/AN"),
        ("_1_5_T", ""),
        ("@_1_7_r", "r6_"),
        ("_1_7_r_1", "0"),
        ("_1_7_r_6", "AD"),
        ("_IMG_檢索.x", "1"),
        ("_IMG_檢索.y", "1"),
    ];

    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.post(form.action.clone()).form(&params).send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt + 1 < RETRY_ATTEMPTS {
                    retry_delay(attempt);
                    continue;
                }
                match response.error_for_status() {
                    Ok(ok) => {
                        let source_url = ok.url().to_string();
                        match ok.text() {
                            Ok(text) => return Ok((text, source_url)),
                            Err(err) => last_error = Some(err.to_string()),
                        }
                    }
                    Err(err) => last_error = Some(err.to_string()),
                }
            }
            Err(err) => last_error = Some(err.to_string()),
        }
        if attempt + 1 < RETRY_ATTEMPTS {
            retry_delay(attempt);
        }
    }

    Err(TwlawError::Network(format!(
        "failed to search Legislative Yuan law system: {}",
        last_error.unwrap_or_else(|| "unknown network error".to_string())
    )))
}

fn parse_search_form(html: &str) -> TwlawResult<SearchForm> {
    let document = Html::parse_document(html);
    let form_selector = selector("form");
    let input_selector = selector("input[name=INFO]");
    let base = Url::parse(BASE_URL)?;

    for form in document.select(&form_selector) {
        let action = form
            .value()
            .attr("action")
            .filter(|value| value.contains("lglawkm"));
        let Some(action) = action else {
            continue;
        };
        let info = form
            .select(&input_selector)
            .next()
            .and_then(|input| input.value().attr("value"))
            .unwrap_or("")
            .trim()
            .to_string();
        if info.is_empty() {
            continue;
        }
        return Ok(SearchForm {
            action: base.join(action)?,
            info,
        });
    }

    Err(TwlawError::ParseChanged(
        "could not parse Legislative Yuan search form".to_string(),
    ))
}

fn parse_search_results(html: &str, base_url: &Url) -> TwlawResult<Vec<LawSearchResult>> {
    let document = Html::parse_document(html);
    let row_selector = selector("tr.sumtr1");
    let title_selector = selector(".sumtd2_TI a[href], .sumtd2002 a[href]");
    let passed_selector = selector(".sumtd2_PD");
    let promulgated_selector = selector(".sumtd2_AD");
    let related_selector = selector(".sumtd2_PR a[href], .sumtd2003 a[href]");
    let mut results = Vec::new();

    for row in document.select(&row_selector) {
        let Some(link) = row.select(&title_selector).next() else {
            continue;
        };
        let href = link.value().attr("href").unwrap_or("");
        let source_url = base_url.join(href)?.to_string();
        let title = inline_text(link);
        if title.is_empty() {
            continue;
        }
        results.push(LawSearchResult {
            title,
            source_url,
            passed_date: row.select(&passed_selector).next().map(inline_text),
            promulgated_date: row.select(&promulgated_selector).next().map(inline_text),
            related_links: links_from(row, base_url, &related_selector),
        });
    }

    Ok(results)
}

fn parse_law_name(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    for query in [".law_NA", ".law_n"] {
        if let Some(element) = document.select(&selector(query)).next() {
            let text = inline_text(element);
            let name = Regex::new(r"\(\d+\)$")
                .expect("valid regex")
                .replace(text.trim(), "")
                .trim()
                .to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn parse_versions(html: &str, base_url: &Url) -> Vec<VersionEntry> {
    let document = Html::parse_document(html);
    let version_selector = selector(".version_0");
    let version_1_selector = selector(".version_1");
    let version_2_selector = selector(".version_2");
    let link_selector = selector("a[href]");
    let mut versions = Vec::new();

    for version in document.select(&version_selector) {
        let action = inline_text(version);
        let Some(link) = version.select(&link_selector).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Ok(source_url) = base_url.join(href) else {
            continue;
        };
        let action_date = compact_roc_date_from_text(&action)
            .or_else(|| compact_roc_date_from_url(source_url.as_str()));
        let table = nearest_table(version);
        let promulgation =
            table.and_then(|table| table.select(&version_1_selector).next().map(inline_text));
        let effective =
            table.and_then(|table| table.select(&version_2_selector).next().map(inline_text));
        versions.push(VersionEntry {
            action,
            action_date,
            promulgation,
            effective,
            source_url: source_url.to_string(),
        });
    }

    versions
}

fn choose_search_result<'a>(
    results: &'a [LawSearchResult],
    keyword: &str,
) -> Option<&'a LawSearchResult> {
    let wanted = normalize_title(keyword);
    results
        .iter()
        .find(|result| normalize_title(&result.title) == wanted)
        .or_else(|| {
            results
                .iter()
                .find(|result| normalize_title(&result.title).starts_with(&wanted))
        })
        .or_else(|| {
            results
                .iter()
                .find(|result| normalize_title(&result.title).contains(&wanted))
        })
        .or_else(|| results.first())
}

fn choose_version<'a>(
    versions: &'a [VersionEntry],
    date: Option<&str>,
) -> Option<&'a VersionEntry> {
    let wanted = date.and_then(normalize_date_query);
    if let Some(wanted) = wanted {
        versions.iter().find(|version| {
            version
                .action_date
                .as_deref()
                .map(|date| date == wanted)
                .unwrap_or(false)
        })
    } else {
        versions.last()
    }
}

fn parse_yellow_button_url(html: &str, base_url: &Url, button: &str) -> Option<Url> {
    let document = Html::parse_document(html);
    let link_selector = selector("a[href]");
    let img_selector = selector("img[src]");
    for link in document.select(&link_selector) {
        let has_button = link.select(&img_selector).any(|img| {
            img.value()
                .attr("src")
                .map(|src| src.contains(button))
                .unwrap_or(false)
        });
        if !has_button {
            continue;
        }
        let href = link.value().attr("href")?;
        if let Ok(url) = base_url.join(href) {
            return Some(url);
        }
    }
    None
}

fn parse_reason_entries(html: &str, base_url: &Url) -> Vec<ReasonEntry> {
    let document = Html::parse_document(html);
    let row_selector = selector("tr");
    let text_label_selector = selector(".artiupd_TH_1");
    let text_content_selector = selector(".artiupd_TH_2");
    let reason_label_selector = selector(".artipud_RS_1");
    let reason_content_selector = selector(".artipud_RS_2");
    let mut entries = Vec::new();
    let mut current: Option<ReasonEntry> = None;

    for row in document.select(&row_selector) {
        if let Some((article, change_type)) = article_header(row) {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(ReasonEntry {
                article_number: article_number_from_title(&article),
                article,
                change_type,
                ..Default::default()
            });
            continue;
        }

        if row.select(&text_label_selector).next().is_some() {
            if let Some(entry) = current.as_mut() {
                entry.text = row.select(&text_content_selector).next().map(block_text);
            }
            continue;
        }

        if row.select(&reason_label_selector).next().is_some() {
            if let Some(entry) = current.as_mut() {
                if let Some(content) = row.select(&reason_content_selector).next() {
                    entry.reason = Some(block_text(content));
                    entry.gazette_links = links_from(content, base_url, &selector("a[href]"));
                }
            }
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }
    entries
}

fn article_header(row: ElementRef<'_>) -> Option<(String, Option<String>)> {
    let font_selector = selector("font");
    let mut article = None;
    let mut change_type = None;
    for font in row.select(&font_selector) {
        let color = font
            .value()
            .attr("color")
            .unwrap_or("")
            .to_ascii_lowercase();
        let text = inline_text(font);
        if color == "#8600b3" && text.contains('第') && text.contains('條') {
            article = Some(text);
        } else if color == "seagreen" {
            change_type = Some(text.trim_matches(|ch| ch == '(' || ch == ')').to_string());
        }
    }
    article.map(|article| (article, change_type))
}

fn article_number_from_title(title: &str) -> Option<String> {
    let re = Regex::new(
        r"第\s*([0-9０-９零〇一二三四五六七八九十百千]+)\s*條\s*(?:之\s*([0-9０-９零〇一二三四五六七八九十百千]+))?",
    )
    .expect("valid regex");
    let caps = re.captures(title)?;
    let main = number_text_to_u32(caps.get(1)?.as_str())?;
    let suffix = caps
        .get(2)
        .and_then(|value| number_text_to_u32(value.as_str()));
    Some(match suffix {
        Some(suffix) => format!("{main}-{suffix}"),
        None => main.to_string(),
    })
}

fn normalize_article_query(input: &str) -> String {
    article_number_from_title(input).unwrap_or_else(|| {
        let normalized = input
            .trim()
            .trim_start_matches('第')
            .replace('條', "")
            .replace('之', "-")
            .replace(' ', "");
        let parts = normalized
            .split('-')
            .filter_map(number_text_to_u32)
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            normalized
        } else {
            parts.join("-")
        }
    })
}

fn compact_roc_date_from_text(text: &str) -> Option<String> {
    let re = Regex::new(
        r"中華民國\s*([0-9０-９零〇一二三四五六七八九十百千]+)\s*年\s*([0-9０-９零〇一二三四五六七八九十百千]+)\s*月\s*([0-9０-９零〇一二三四五六七八九十百千]+)\s*日",
    )
    .expect("valid regex");
    let caps = re.captures(text)?;
    let year = number_text_to_u32(caps.get(1)?.as_str())?;
    let month = number_text_to_u32(caps.get(2)?.as_str())?;
    let day = number_text_to_u32(caps.get(3)?.as_str())?;
    Some(format!("{year:03}{month:02}{day:02}"))
}

fn compact_roc_date_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"\d{5}(\d{7})00").expect("valid regex");
    re.captures(url)
        .and_then(|caps| caps.get(1).map(|value| value.as_str().to_string()))
}

fn normalize_date_query(input: &str) -> Option<String> {
    if let Some(date) = compact_roc_date_from_text(input) {
        return Some(date);
    }
    let digits = input.chars().filter_map(ascii_digit).collect::<String>();
    (digits.len() == 7).then_some(digits)
}

fn number_text_to_u32(input: &str) -> Option<u32> {
    let digits = input.chars().filter_map(ascii_digit).collect::<String>();
    if !digits.is_empty() {
        return digits.parse::<u32>().ok();
    }
    chinese_number_to_u32(input)
}

fn chinese_number_to_u32(input: &str) -> Option<u32> {
    let mut total = 0;
    let mut current = 0;
    let mut saw = false;
    for ch in input.chars().filter(|ch| !ch.is_whitespace()) {
        if let Some(value) = chinese_digit(ch) {
            current = value;
            saw = true;
            continue;
        }
        let unit = match ch {
            '十' | '拾' => 10,
            '百' | '佰' => 100,
            '千' | '仟' => 1000,
            _ => continue,
        };
        let value = if current == 0 { 1 } else { current };
        total += value * unit;
        current = 0;
        saw = true;
    }
    if saw {
        Some(total + current)
    } else {
        None
    }
}

fn chinese_digit(ch: char) -> Option<u32> {
    match ch {
        '零' | '〇' => Some(0),
        '一' | '壹' => Some(1),
        '二' | '貳' | '兩' => Some(2),
        '三' | '參' => Some(3),
        '四' | '肆' => Some(4),
        '五' | '伍' => Some(5),
        '六' | '陸' => Some(6),
        '七' | '柒' => Some(7),
        '八' | '捌' => Some(8),
        '九' | '玖' => Some(9),
        _ => None,
    }
}

fn ascii_digit(ch: char) -> Option<char> {
    match ch {
        '0'..='9' => Some(ch),
        '０'..='９' => char::from_u32('0' as u32 + ch as u32 - '０' as u32),
        _ => None,
    }
}

fn nearest_table<'a>(element: ElementRef<'a>) -> Option<ElementRef<'a>> {
    element.ancestors().skip(1).find_map(|node| {
        let ancestor = ElementRef::wrap(node)?;
        (ancestor.value().name() == "table").then_some(ancestor)
    })
}

fn selector(input: &str) -> Selector {
    Selector::parse(input).expect("valid selector")
}

fn inline_text(element: ElementRef<'_>) -> String {
    cleanup_text(
        &element
            .text()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(""),
    )
}

fn block_text(element: ElementRef<'_>) -> String {
    cleanup_text(
        &element
            .text()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn cleanup_text(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_title(text: &str) -> String {
    inline_cleanup(text).replace(' ', "")
}

fn inline_cleanup(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

fn links_from(element: ElementRef<'_>, base_url: &Url, link_selector: &Selector) -> Vec<Value> {
    element
        .select(link_selector)
        .filter_map(|link| {
            let href = link.value().attr("href")?;
            let url = base_url.join(href).ok()?;
            Some(json!({
                "url": url.to_string(),
                "label": inline_text(link),
                "title": link.value().attr("title")
            }))
        })
        .collect()
}

fn search_result_json(result: &LawSearchResult) -> Value {
    json!({
        "title": result.title,
        "source_url": result.source_url,
        "passed_date": result.passed_date,
        "promulgated_date": result.promulgated_date,
        "related_links": result.related_links
    })
}

fn version_json(version: &VersionEntry) -> Value {
    json!({
        "action": version.action,
        "action_date": version.action_date,
        "promulgation": version.promulgation,
        "effective": version.effective,
        "source_url": version.source_url
    })
}

fn reason_json(entry: &ReasonEntry) -> Value {
    json!({
        "article": entry.article,
        "article_number": entry.article_number,
        "change_type": entry.change_type,
        "text": entry.text,
        "reason": entry.reason,
        "gazette_links": entry.gazette_links
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chinese_roc_dates() {
        assert_eq!(
            compact_roc_date_from_text("中華民國一百十二年五月三十日修正").as_deref(),
            Some("1120530")
        );
        assert_eq!(
            compact_roc_date_from_text("中華民國88年5月14日制定").as_deref(),
            Some("0880514")
        );
    }

    #[test]
    fn parses_article_numbers() {
        assert_eq!(
            article_number_from_title("第七條 之一").as_deref(),
            Some("7-1")
        );
        assert_eq!(normalize_article_query("第184條"), "184");
    }

    #[test]
    fn parses_legislative_versions() {
        let html = r#"
        <table class=law_ch>
          <tr><td><table>
            <tr><td class=version_0><a href=/lglawc/lawsingle?x^01203112053000^y>中華民國112年5月30日修正</a></td></tr>
            <tr><td class=version_1>中華民國112年6月28日公布</td></tr>
          </table></td></tr>
        </table>"#;
        let base = Url::parse(LGLAWKM_URL).unwrap();
        let versions = parse_versions(html, &base);
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].action_date.as_deref(), Some("1120530"));
        assert_eq!(
            versions[0].promulgation.as_deref(),
            Some("中華民國112年6月28日公布")
        );
    }

    #[test]
    fn parses_reason_entries() {
        let html = r#"
        <table><tr><td colspan=5><font color=#8600B3 size=4>第三條</font><font size=2 color=seagreen>(修正)</font></td></tr>
        <tr><td class=artiupd_TH_1><nobr>條文</nobr></td><td class=artiupd_TH_2>條文內容</td></tr>
        <tr><td class=artipud_RS_1><nobr>理由</nobr></td><td class=artipud_RS_2>理由內容<a href=https://lis.ly.gov.tw/lgcgi/lypdftxt?x title="公報紀錄"></a></td></tr></table>"#;
        let base = Url::parse(LGLAWKM_URL).unwrap();
        let entries = parse_reason_entries(html, &base);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].article_number.as_deref(), Some("3"));
        assert_eq!(entries[0].reason.as_deref(), Some("理由內容"));
        assert_eq!(entries[0].gazette_links.len(), 1);
    }
}
