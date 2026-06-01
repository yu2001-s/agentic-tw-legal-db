use crate::{retrieved_at, TwlawError, TwlawResult};
use percent_encoding::percent_decode_str;
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::thread::sleep;
use std::time::Duration;
use url::Url;

const JUDICIAL_SEARCH_URL: &str = "https://judgment.judicial.gov.tw/FJUD/Default_AD.aspx";
const JUDICIAL_QRYRESULT_URL: &str = "https://judgment.judicial.gov.tw/FJUD/qryresult.aspx";
const JUDICIAL_DATA_URL: &str = "https://judgment.judicial.gov.tw/FJUD/data.aspx";
const JUDICIAL_BASE: &str = "https://judgment.judicial.gov.tw/FJUD/";
const JUDICIAL_SIMPLE_SEARCH_URL: &str = "https://judgment.judicial.gov.tw/FJUD/defaulte_AD.aspx";
const JUDICIAL_DECLARATION_SEARCH_URL: &str =
    "https://judgment.judicial.gov.tw/FJUD/defaultk_AD.aspx?ty=E";
const JUDICIAL_PUBLIC_SUMMONS_SEARCH_URL: &str =
    "https://judgment.judicial.gov.tw/FJUD/defaultk_AD.aspx?ty=V";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";
const RETRY_ATTEMPTS: usize = 3;
const RETRY_BASE_DELAY_MS: u64 = 350;

#[derive(Debug, Clone, Default)]
pub struct JudgmentSearch {
    pub keyword: Option<String>,
    pub court: Option<String>,
    pub case_type: Option<String>,
    pub year_from: Option<u32>,
    pub year_to: Option<u32>,
    pub case_word: Option<String>,
    pub case_number: Option<String>,
    pub main_text: Option<String>,
    pub max_results: usize,
}

#[derive(Debug, Clone, Default)]
pub struct JudgmentGet {
    pub jid: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct JudgmentSpecialSearch {
    pub kind: Option<String>,
    pub keyword: Option<String>,
    pub court: Option<String>,
    pub year: Option<u32>,
    pub case_word: Option<String>,
    pub case_number: Option<String>,
    pub max_results: usize,
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

fn is_waf_blocked(html: &str) -> bool {
    html.contains("Request Rejected") || html.contains("TSPD") || html.contains("bobcmn")
}

fn fetch_text(
    client: &Client,
    url: &str,
    params: &[(&str, String)],
) -> TwlawResult<(String, String)> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.get(url).query(params).send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt + 1 < RETRY_ATTEMPTS {
                    retry_delay(attempt);
                    continue;
                }
                let response = response.error_for_status()?;
                let final_url = response.url().to_string();
                let html = response.text()?;
                if is_waf_blocked(&html) {
                    return Err(TwlawError::UpstreamBlocked(
                        "Judicial Yuan WAF blocked the request; retry later or add an external browser-cookie refresh flow"
                            .to_string(),
                    ));
                }
                return Ok((html, final_url));
            }
            Err(err) => {
                if should_retry_error(&err) && attempt + 1 < RETRY_ATTEMPTS {
                    last_error = Some(err.to_string());
                    retry_delay(attempt);
                    continue;
                }
                return Err(err.into());
            }
        }
    }

    Err(TwlawError::Network(last_error.unwrap_or_else(|| {
        "request failed after retry attempts".to_string()
    })))
}

fn post_form(
    client: &Client,
    url: &str,
    form: &[(String, String)],
) -> TwlawResult<(String, String)> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.post(url).form(form).send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt + 1 < RETRY_ATTEMPTS {
                    retry_delay(attempt);
                    continue;
                }
                let response = response.error_for_status()?;
                let final_url = response.url().to_string();
                let html = response.text()?;
                if is_waf_blocked(&html) {
                    return Err(TwlawError::UpstreamBlocked(
                        "Judicial Yuan WAF blocked the request; retry later or add an external browser-cookie refresh flow"
                            .to_string(),
                    ));
                }
                return Ok((html, final_url));
            }
            Err(err) => {
                if should_retry_error(&err) && attempt + 1 < RETRY_ATTEMPTS {
                    last_error = Some(err.to_string());
                    retry_delay(attempt);
                    continue;
                }
                return Err(err.into());
            }
        }
    }

    Err(TwlawError::Network(last_error.unwrap_or_else(|| {
        "request failed after retry attempts".to_string()
    })))
}

fn should_retry_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn should_retry_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout() || err.is_request()
}

fn retry_delay(attempt: usize) {
    let multiplier = 1_u64 << attempt.min(4);
    sleep(Duration::from_millis(RETRY_BASE_DELAY_MS * multiplier));
}

fn court_code(input: &str) -> String {
    court_codes()
        .iter()
        .find(|(_, name)| *name == input)
        .map(|(code, _)| (*code).to_string())
        .unwrap_or_else(|| input.to_string())
}

fn case_type_code(input: &str) -> String {
    match input {
        "民事" => "V".to_string(),
        "刑事" => "M".to_string(),
        "行政" => "A".to_string(),
        "懲戒" => "P".to_string(),
        other => other.to_string(),
    }
}

fn court_codes() -> &'static [(&'static str, &'static str)] {
    &[
        ("JCC", "憲法法庭"),
        ("TPC", "司法院刑事補償法庭"),
        ("TPS", "最高法院"),
        ("TPA", "最高行政法院"),
        ("TPPD", "懲戒法院懲戒法庭"),
        ("TPP", "懲戒法院"),
        ("TPJ", "懲戒法院職務法庭"),
        ("TPH", "臺灣高等法院"),
        ("TCH", "臺灣高等法院臺中分院"),
        ("TNH", "臺灣高等法院臺南分院"),
        ("KSH", "臺灣高等法院高雄分院"),
        ("HLH", "臺灣高等法院花蓮分院"),
        ("KMH", "福建高等法院金門分院"),
        ("IPC", "智慧財產及商業法院"),
        ("TPB", "臺北高等行政法院"),
        ("TCB", "臺中高等行政法院"),
        ("KSB", "高雄高等行政法院"),
        ("TPD", "臺灣臺北地方法院"),
        ("SLD", "臺灣士林地方法院"),
        ("PCD", "臺灣新北地方法院"),
        ("ILD", "臺灣宜蘭地方法院"),
        ("KLD", "臺灣基隆地方法院"),
        ("TYD", "臺灣桃園地方法院"),
        ("SCD", "臺灣新竹地方法院"),
        ("MLD", "臺灣苗栗地方法院"),
        ("TCD", "臺灣臺中地方法院"),
        ("CHD", "臺灣彰化地方法院"),
        ("NTD", "臺灣南投地方法院"),
        ("ULD", "臺灣雲林地方法院"),
        ("CYD", "臺灣嘉義地方法院"),
        ("TND", "臺灣臺南地方法院"),
        ("KSD", "臺灣高雄地方法院"),
        ("CTD", "臺灣橋頭地方法院"),
        ("HLD", "臺灣花蓮地方法院"),
        ("TTD", "臺灣臺東地方法院"),
        ("PTD", "臺灣屏東地方法院"),
        ("PHD", "臺灣澎湖地方法院"),
        ("KMD", "福建金門地方法院"),
        ("LCD", "福建連江地方法院"),
        ("KSY", "臺灣高雄少年及家事法院"),
        ("TPE", "臺北簡易庭"),
        ("STE", "新店簡易庭"),
        ("SLE", "士林簡易庭"),
        ("NHE", "內湖簡易庭"),
        ("PCE", "板橋簡易庭"),
        ("SJE", "三重簡易庭"),
        ("TYE", "桃園簡易庭"),
        ("CLE", "中壢簡易庭"),
        ("CPE", "竹北簡易庭"),
        ("TCE", "臺中簡易庭"),
        ("SDE", "沙鹿簡易庭"),
        ("FYE", "豐原簡易庭"),
        ("CHE", "彰化簡易庭"),
        ("OLE", "員林簡易庭"),
        ("PDE", "北斗簡易庭"),
        ("NTE", "南投簡易庭"),
        ("TLE", "斗六簡易庭"),
        ("HUE", "虎尾簡易庭"),
        ("CYE", "嘉義簡易庭"),
        ("PKE", "北港簡易庭"),
        ("TNE", "臺南簡易庭"),
        ("SYE", "柳營簡易庭"),
        ("SSE", "新市簡易庭"),
        ("KSE", "高雄簡易庭"),
        ("FSE", "鳳山簡易庭"),
        ("CDE", "橋頭簡易庭"),
        ("GSE", "岡山簡易庭"),
        ("CSE", "旗山簡易庭"),
        ("PTE", "屏東簡易庭"),
        ("CCE", "潮州簡易庭"),
        ("TTE", "臺東、成功簡易庭"),
        ("HLE", "花蓮簡易庭"),
        ("ILE", "宜蘭簡易庭"),
        ("LTE", "羅東簡易庭"),
        ("MKE", "馬公簡易庭"),
        ("KME", "金城簡易庭"),
    ]
}

fn court_level(code: &str) -> u8 {
    match code {
        "JCC" | "TPS" | "TPA" => 1,
        "TPH" | "TCH" | "TNH" | "KSH" | "HLH" | "KMH" | "IPC" | "TPB" | "TCB" | "KSB" | "TPP"
        | "TPPD" | "TPJ" | "TPC" => 2,
        _ => 3,
    }
}

fn case_type_name(code: &str) -> &str {
    match code {
        "V" => "民事",
        "M" => "刑事",
        "A" => "行政",
        "P" => "懲戒",
        _ => "",
    }
}

pub fn search_judgments(search: JudgmentSearch) -> TwlawResult<Value> {
    let max_results = search.max_results.clamp(1, 200);
    let has_keyword = search
        .keyword
        .as_deref()
        .map_or(false, |v| !v.trim().is_empty());
    let has_case_number = search
        .case_number
        .as_deref()
        .map_or(false, |v| !v.trim().is_empty());
    let has_main_text = search
        .main_text
        .as_deref()
        .map_or(false, |v| !v.trim().is_empty());
    if !has_keyword && !has_case_number && !has_main_text {
        return Err(TwlawError::InvalidInput(
            "provide --keyword, --case-number, or --main-text".to_string(),
        ));
    }

    let client = client()?;
    let results = if has_case_number
        && !has_keyword
        && !has_main_text
        && search
            .case_word
            .as_deref()
            .map_or(false, |v| !v.trim().is_empty())
    {
        precise_search(&client, &search, max_results)?
    } else {
        keyword_search(&client, &search, max_results)?
    };

    Ok(json!({
        "success": true,
        "query": {
            "keyword": search.keyword.unwrap_or_default(),
            "court": search.court.unwrap_or_default(),
            "case_type": search.case_type.unwrap_or_default(),
            "year_from": search.year_from.unwrap_or_default(),
            "year_to": search.year_to.unwrap_or_default(),
            "case_word": search.case_word.unwrap_or_default(),
            "case_number": search.case_number.unwrap_or_default(),
            "main_text": search.main_text.unwrap_or_default(),
            "max_results": max_results
        },
        "total_count": results.len(),
        "results": results,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

pub fn search_special_judgments(search: JudgmentSpecialSearch) -> TwlawResult<Value> {
    let kind = SpecialJudgmentKind::parse(search.kind.as_deref())?;
    let max_results = search.max_results.clamp(1, 100);
    let has_keyword = search
        .keyword
        .as_deref()
        .map_or(false, |value| !value.trim().is_empty());
    let has_case_number = search
        .case_number
        .as_deref()
        .map_or(false, |value| !value.trim().is_empty());
    if !has_keyword && !has_case_number {
        return Err(TwlawError::InvalidInput(
            "provide --keyword or --case-number for special judgment search".to_string(),
        ));
    }

    let client = client()?;
    let (form_html, form_url) = fetch_text(&client, kind.url, &[])?;
    let document = Html::parse_document(&form_html);
    let mut form = hidden_form_values(&document);
    form.push((
        "ctl00$cp_content$btnQry".to_string(),
        "送出查詢".to_string(),
    ));
    push_form_replace(&mut form, "jud_kw", search.keyword.as_deref());
    push_form_replace(&mut form, "jud_court", search.court.as_deref());
    if let Some(year) = search.year {
        form.retain(|(key, _)| key != "jud_year");
        form.push(("jud_year".to_string(), year.to_string()));
    }
    push_form_replace(&mut form, "jud_case", search.case_word.as_deref());
    push_form_replace(&mut form, "jud_no", search.case_number.as_deref());

    let (posted, posted_url) = post_form(&client, kind.url, &form)?;
    let Some(mut page_url) = extract_iframe_url(&posted)? else {
        return Ok(json!({
            "success": true,
            "source": kind.label,
            "source_url": form_url,
            "posted_url": posted_url,
            "query": special_query_json(&search, kind, max_results),
            "total_count": 0,
            "results": [],
            "cached": false,
            "retrieved_at": retrieved_at()
        }));
    };

    let mut all = Vec::new();
    let mut seen = HashSet::new();
    for _ in 0..50 {
        if all.len() >= max_results {
            break;
        }
        let (html, _) = fetch_text(&client, &page_url, &[])?;
        let page_results = parse_search_results(&html);
        if page_results.is_empty() {
            break;
        }
        let mut added = 0usize;
        for item in page_results {
            let jid = item
                .get("jid")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !jid.is_empty() && seen.insert(jid) {
                all.push(item);
                added += 1;
            }
            if all.len() >= max_results {
                break;
            }
        }
        if added == 0 {
            break;
        }
        let Some(next) = extract_next_page_url(&html)? else {
            break;
        };
        page_url = next;
    }

    Ok(json!({
        "success": true,
        "source": kind.label,
        "source_url": form_url,
        "posted_url": posted_url,
        "query": special_query_json(&search, kind, max_results),
        "total_count": all.len(),
        "results": all,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

#[derive(Debug, Clone, Copy)]
struct SpecialJudgmentKind {
    id: &'static str,
    label: &'static str,
    url: &'static str,
}

impl SpecialJudgmentKind {
    fn parse(input: Option<&str>) -> TwlawResult<Self> {
        let value = input.unwrap_or("simple").trim().to_ascii_lowercase();
        match value.as_str() {
            "" | "simple" | "summary" | "簡易" => Ok(Self {
                id: "simple",
                label: "簡易案件查詢",
                url: JUDICIAL_SIMPLE_SEARCH_URL,
            }),
            "declaration" | "ex-rights" | "ex_rights" | "除權" | "除權判決" => Ok(Self {
                id: "declaration",
                label: "除權判決查詢",
                url: JUDICIAL_DECLARATION_SEARCH_URL,
            }),
            "public-summons" | "public_summons" | "summons" | "公示催告" | "公示催告裁定" => {
                Ok(Self {
                    id: "public-summons",
                    label: "公示催告裁定查詢",
                    url: JUDICIAL_PUBLIC_SUMMONS_SEARCH_URL,
                })
            }
            other => Err(TwlawError::InvalidInput(format!(
                "unknown Judicial Yuan special search kind: {other}; expected simple, declaration, or public-summons"
            ))),
        }
    }
}

fn hidden_form_values(document: &Html) -> Vec<(String, String)> {
    document
        .select(&selector(r#"input[type="hidden"][name]"#))
        .filter_map(|input| {
            Some((
                input.value().attr("name")?.to_string(),
                input.value().attr("value").unwrap_or("").to_string(),
            ))
        })
        .collect()
}

fn push_form_replace(form: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        form.retain(|(existing, _)| existing != key);
        form.push((key.to_string(), value.to_string()));
    }
}

fn special_query_json(
    search: &JudgmentSpecialSearch,
    kind: SpecialJudgmentKind,
    max_results: usize,
) -> Value {
    json!({
        "kind": kind.id,
        "kind_label": kind.label,
        "keyword": search.keyword.as_deref().unwrap_or_default(),
        "court": search.court.as_deref().unwrap_or_default(),
        "year": search.year,
        "case_word": search.case_word.as_deref().unwrap_or_default(),
        "case_number": search.case_number.as_deref().unwrap_or_default(),
        "max_results": max_results
    })
}

fn precise_search(
    client: &Client,
    search: &JudgmentSearch,
    max_results: usize,
) -> TwlawResult<Vec<Value>> {
    let mut base_params = vec![
        (
            "jud_case",
            search
                .case_word
                .as_deref()
                .unwrap_or_default()
                .replace('臺', "台"),
        ),
        (
            "jud_no",
            search
                .case_number
                .as_deref()
                .unwrap_or_default()
                .to_string(),
        ),
        ("judtype", "JUDBOOK".to_string()),
    ];
    if let Some(year) = search.year_from.or(search.year_to) {
        base_params.push(("jud_year", year.to_string()));
    }
    if let Some(court) = search.court.as_deref().filter(|v| !v.trim().is_empty()) {
        base_params.push(("jud_court", court_code(court)));
    }

    let sys_codes =
        if let Some(case_type) = search.case_type.as_deref().filter(|v| !v.trim().is_empty()) {
            vec![case_type_code(case_type)]
        } else {
            vec!["V".to_string(), "M".to_string(), "A".to_string()]
        };

    let mut all = Vec::new();
    let mut seen = HashSet::new();
    for sys in sys_codes {
        let mut params = base_params.clone();
        params.push(("sys", sys));
        let (html, _) = fetch_text(client, JUDICIAL_QRYRESULT_URL, &params)?;
        let Some(iframe) = extract_iframe_url(&html)? else {
            continue;
        };
        let (list_html, _) = fetch_text(client, &iframe, &[])?;
        for item in parse_search_results(&list_html) {
            let jid = item
                .get("jid")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !jid.is_empty() && seen.insert(jid) {
                all.push(item);
            }
        }
    }

    all.sort_by(|a, b| {
        a.get("court_level")
            .and_then(Value::as_u64)
            .unwrap_or(99)
            .cmp(&b.get("court_level").and_then(Value::as_u64).unwrap_or(99))
    });
    all.truncate(max_results);
    Ok(all)
}

fn keyword_search(
    client: &Client,
    search: &JudgmentSearch,
    max_results: usize,
) -> TwlawResult<Vec<Value>> {
    let (form_html, _) = fetch_text(client, JUDICIAL_SEARCH_URL, &[])?;
    let document = Html::parse_document(&form_html);
    let mut form = vec![
        (
            "__VIEWSTATE".to_string(),
            input_value(&document, "__VIEWSTATE")?,
        ),
        (
            "__EVENTVALIDATION".to_string(),
            input_value(&document, "__EVENTVALIDATION")?,
        ),
        (
            "__VIEWSTATEGENERATOR".to_string(),
            input_value_optional(&document, "__VIEWSTATEGENERATOR").unwrap_or_default(),
        ),
        ("__VIEWSTATEENCRYPTED".to_string(), "".to_string()),
        ("judtype".to_string(), "JUDBOOK".to_string()),
        ("whosub".to_string(), "0".to_string()),
        (
            "ctl00$cp_content$btnQry".to_string(),
            "送出查詢".to_string(),
        ),
    ];

    push_form(&mut form, "jud_kw", search.keyword.as_deref());
    push_form(&mut form, "jud_jmain", search.main_text.as_deref());
    if let Some(court) = search.court.as_deref().filter(|v| !v.trim().is_empty()) {
        form.push(("jud_court".to_string(), court_code(court)));
    }
    if let Some(case_type) = search.case_type.as_deref().filter(|v| !v.trim().is_empty()) {
        form.push(("jud_sys".to_string(), case_type_code(case_type)));
    }
    if let Some(year) = search.year_from {
        form.push(("dy1".to_string(), year.to_string()));
    }
    if let Some(year) = search.year_to {
        form.push(("dy2".to_string(), year.to_string()));
    }
    push_form(&mut form, "jud_case", search.case_word.as_deref());
    push_form(&mut form, "jud_no", search.case_number.as_deref());

    let (posted, _) = post_form(client, JUDICIAL_SEARCH_URL, &form)?;
    let Some(mut page_url) = extract_iframe_url(&posted)? else {
        return Ok(Vec::new());
    };

    let mut all = Vec::new();
    let mut seen = HashSet::new();
    for _ in 0..100 {
        if all.len() >= max_results {
            break;
        }
        let (html, _) = fetch_text(client, &page_url, &[])?;
        let page_results = parse_search_results(&html);
        if page_results.is_empty() {
            break;
        }
        let mut added = 0usize;
        for item in page_results {
            let jid = item
                .get("jid")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !jid.is_empty() && seen.insert(jid) {
                all.push(item);
                added += 1;
            }
            if all.len() >= max_results {
                break;
            }
        }
        if added == 0 {
            break;
        }
        let Some(next) = extract_next_page_url(&html)? else {
            break;
        };
        page_url = next;
    }

    all.sort_by(|a, b| {
        a.get("court_level")
            .and_then(Value::as_u64)
            .unwrap_or(99)
            .cmp(&b.get("court_level").and_then(Value::as_u64).unwrap_or(99))
    });
    all.truncate(max_results);
    Ok(all)
}

fn push_form(form: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        form.push((key.to_string(), value.to_string()));
    }
}

fn input_value(document: &Html, name: &str) -> TwlawResult<String> {
    input_value_optional(document, name).ok_or_else(|| {
        TwlawError::ParseChanged(format!(
            "cannot find ASP.NET hidden input {name}; Judicial Yuan form may have changed"
        ))
    })
}

fn input_value_optional(document: &Html, name: &str) -> Option<String> {
    let query = format!(r#"input[name="{name}"]"#);
    document
        .select(&selector(&query))
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(ToString::to_string)
}

fn extract_iframe_url(html: &str) -> TwlawResult<Option<String>> {
    let document = Html::parse_document(html);
    let Some(iframe) = document.select(&selector("iframe")).next() else {
        return Ok(None);
    };
    let Some(src) = iframe.value().attr("src") else {
        return Ok(None);
    };
    Ok(Some(join_judicial_url(src)?))
}

fn extract_next_page_url(html: &str) -> TwlawResult<Option<String>> {
    let document = Html::parse_document(html);
    let Some(link) = document.select(&selector("a#hlNext")).next() else {
        return Ok(None);
    };
    let Some(href) = link.value().attr("href") else {
        return Ok(None);
    };
    Ok(Some(join_judicial_url(href)?))
}

fn join_judicial_url(href: &str) -> TwlawResult<String> {
    if href.starts_with("http://") || href.starts_with("https://") {
        validate_judicial_url(href)?;
        return Ok(href.replace("&amp;", "&"));
    }
    let base = Url::parse(JUDICIAL_BASE)?;
    let joined = base.join(&href.replace("&amp;", "&"))?;
    validate_judicial_url(joined.as_str())?;
    Ok(joined.to_string())
}

fn validate_judicial_url(url: &str) -> TwlawResult<()> {
    let parsed = Url::parse(url)?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(TwlawError::InvalidInput(
            "URL must use http or https".to_string(),
        ));
    }
    if parsed.host_str() != Some("judgment.judicial.gov.tw") {
        return Err(TwlawError::InvalidInput(
            "URL host must be judgment.judicial.gov.tw".to_string(),
        ));
    }
    Ok(())
}

fn parse_search_results(html: &str) -> Vec<Value> {
    let document = Html::parse_document(html);
    let table = document
        .select(&selector("table#jud"))
        .next()
        .or_else(|| document.select(&selector("table.jub-table")).next());
    let Some(table) = table else {
        return Vec::new();
    };

    let rows: Vec<_> = table.select(&selector("tr")).collect();
    let mut results = Vec::new();
    let mut i = 0usize;
    while i < rows.len() {
        let row = rows[i];
        let class = row.value().attr("class").unwrap_or("");
        if class.split_whitespace().any(|c| c == "summary")
            || row.select(&selector("th")).next().is_some()
        {
            i += 1;
            continue;
        }
        let cells: Vec<_> = row.select(&selector("td")).collect();
        if cells.len() < 3 {
            i += 1;
            continue;
        }
        let mut entry = parse_result_row(&cells);

        if i + 1 < rows.len() {
            let next_class = rows[i + 1].value().attr("class").unwrap_or("");
            if next_class.split_whitespace().any(|c| c == "summary") {
                if let Some(summary) = rows[i + 1].select(&selector("span.tdCut")).next() {
                    entry["summary"] = json!(text_of(summary));
                }
                i += 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }

        if entry
            .get("case_id")
            .and_then(Value::as_str)
            .map_or(false, |v| !v.is_empty())
        {
            results.push(entry);
        }
    }
    results
}

fn parse_result_row(cells: &[scraper::ElementRef<'_>]) -> Value {
    let mut entry = json!({
        "case_id": "",
        "court": "",
        "case_type": "",
        "court_level": 0,
        "date": "",
        "cause": "",
        "summary": "",
        "url": "",
        "jid": ""
    });

    if let Some(link) = cells
        .get(1)
        .and_then(|cell| cell.select(&selector("a")).next())
    {
        entry["case_id"] = json!(text_of(link));
        if let Some(href) = link.value().attr("href") {
            if let Ok(url) = join_judicial_url(href) {
                entry["url"] = json!(url);
            }
            if let Some(jid) = extract_query_id(href) {
                entry["jid"] = json!(jid);
            }
        }
    }

    if let Some(date_cell) = cells.get(2) {
        let text = text_of(*date_cell);
        let re = Regex::new(r"(\d{2,3})[./](\d{1,2})[./](\d{1,2})").expect("valid regex");
        if let Some(caps) = re.captures(&text) {
            entry["date"] = json!(format!(
                "{}-{}-{}",
                &caps[1],
                caps[2]
                    .parse::<u32>()
                    .unwrap_or(0)
                    .to_string()
                    .pad_left_zero(2),
                caps[3]
                    .parse::<u32>()
                    .unwrap_or(0)
                    .to_string()
                    .pad_left_zero(2)
            ));
        }
    }

    if let Some(cause_cell) = cells.get(3) {
        entry["cause"] = json!(text_of(*cause_cell));
    }

    enrich_from_jid(&mut entry);
    entry
}

trait PadLeftZero {
    fn pad_left_zero(&self, width: usize) -> String;
}

impl PadLeftZero for str {
    fn pad_left_zero(&self, width: usize) -> String {
        if self.len() >= width {
            self.to_string()
        } else {
            format!("{}{}", "0".repeat(width - self.len()), self)
        }
    }
}

fn extract_query_id(href: &str) -> Option<String> {
    let re = Regex::new(r"id=([^&]+)").expect("valid regex");
    re.captures(href).and_then(|caps| caps.get(1)).map(|m| {
        percent_decode_str(m.as_str())
            .decode_utf8_lossy()
            .to_string()
    })
}

fn enrich_from_jid(entry: &mut Value) {
    let Some(jid) = entry.get("jid").and_then(Value::as_str).map(str::to_string) else {
        return;
    };
    let Some(prefix) = jid.split(',').next() else {
        return;
    };
    let mut codes = court_codes().to_vec();
    codes.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (code, court) in codes {
        if let Some(remaining) = prefix.strip_prefix(code) {
            entry["court"] = json!(court);
            entry["court_level"] = json!(court_level(code));
            entry["case_type"] = json!(case_type_name(remaining));
            break;
        }
    }
}

pub fn get_judgment(query: JudgmentGet) -> TwlawResult<Value> {
    let client = client()?;
    let (html, source_url) = if let Some(jid) = query
        .jid
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        fetch_text(
            &client,
            JUDICIAL_DATA_URL,
            &[("ty", "JD".to_string()), ("id", jid.to_string())],
        )?
    } else if let Some(url) = query
        .url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        validate_judicial_url(url)?;
        fetch_text(&client, url, &[])?
    } else {
        return Err(TwlawError::InvalidInput(
            "provide --jid or --url".to_string(),
        ));
    };

    let parsed = parse_judgment_page(&html, &source_url)?;
    Ok(parsed)
}

fn parse_judgment_page(html: &str, source_url: &str) -> TwlawResult<Value> {
    let document = Html::parse_document(html);
    let content = [
        "#jud",
        "#jud_content",
        ".jud-content",
        "pre",
        "#MainContent",
        "body",
    ]
    .iter()
    .find_map(|query| document.select(&selector(query)).next())
    .ok_or_else(|| TwlawError::ParseChanged("cannot find judgment content element".to_string()))?;

    let raw_text = content.text().collect::<Vec<_>>().join("\n");
    let full_text = clean_judgment_text(&raw_text);
    if full_text.chars().count() < 50 {
        return Err(TwlawError::ParseChanged(
            "judgment text is unexpectedly short".to_string(),
        ));
    }

    let lines = full_text.lines().map(str::trim).collect::<Vec<_>>();
    let case_id = extract_case_id(&lines);
    let court = extract_court(&lines);
    let date = extract_date(&lines);
    let judges = extract_judges(&lines);
    let parties = extract_parties(&lines, &case_id);
    let cause = extract_cause(&lines);
    let (main_text, facts, reasoning) = extract_sections(&lines);
    let cited_statutes = extract_cited_statutes(&full_text);
    let cited_cases = extract_cited_cases(&full_text);

    Ok(json!({
        "success": true,
        "source": "judicial_http",
        "source_url": source_url,
        "case_id": case_id,
        "court": court,
        "date": date,
        "judges": judges,
        "parties": parties,
        "cause": cause,
        "main_text": main_text,
        "facts": facts,
        "reasoning": reasoning,
        "cited_statutes": cited_statutes,
        "cited_cases": cited_cases,
        "full_text": full_text,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

fn clean_judgment_text(text: &str) -> String {
    let re_header = Regex::new(r"^\s*版面大小[\s\d%]*").expect("valid regex");
    let re_blank = Regex::new(r"\n{3,}").expect("valid regex");
    let text = re_header.replace(text, "");
    let text = text.replace('\u{00a0}', " ");
    let lines = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    re_blank.replace_all(&lines, "\n\n").trim().to_string()
}

fn extract_case_id(lines: &[&str]) -> String {
    let re = Regex::new(r"\d+\s*年度?\s*.*字\s*第?\s*\d+\s*號").expect("valid regex");
    lines
        .iter()
        .take(20)
        .find(|line| re.is_match(line))
        .map(|s| (*s).to_string())
        .unwrap_or_default()
}

fn extract_court(lines: &[&str]) -> String {
    let re = Regex::new(
        r"((?:最高(?:行政)?法院|(?:臺北|臺中|高雄)高等行政法院|臺灣高等法院(?:\S{2,3}分院)?|臺灣\S+?(?:地方|少年及家事)法院|智慧財產(?:及商業)?法院|懲戒法院|福建\S*?(?:地方|高等)法院(?:\S{2,3}分院)?))",
    )
    .expect("valid regex");
    lines
        .iter()
        .take(20)
        .find_map(|line| {
            re.captures(line)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string())
        })
        .unwrap_or_default()
}

fn extract_date(lines: &[&str]) -> String {
    let re = Regex::new(r"中\s*華\s*民\s*國\s*(\d{2,3})\s*年\s*(\d{1,2})\s*月\s*(\d{1,2})\s*日")
        .expect("valid regex");
    for line in lines {
        if let Some(caps) = re.captures(line) {
            return format!(
                "{}-{}-{}",
                &caps[1],
                caps[2]
                    .parse::<u32>()
                    .unwrap_or(0)
                    .to_string()
                    .pad_left_zero(2),
                caps[3]
                    .parse::<u32>()
                    .unwrap_or(0)
                    .to_string()
                    .pad_left_zero(2)
            );
        }
    }
    String::new()
}

fn extract_judges(lines: &[&str]) -> Vec<String> {
    let re = Regex::new(r"(?:審判長)?法\s*官\s+(.+?)$").expect("valid regex");
    lines
        .iter()
        .rev()
        .take(30)
        .filter_map(|line| {
            re.captures(line).and_then(|caps| {
                caps.get(1).map(|m| {
                    m.as_str()
                        .chars()
                        .filter(|c| !c.is_whitespace() && *c != '\u{3000}')
                        .collect::<String>()
                })
            })
        })
        .filter(|name| name.chars().count() >= 2)
        .collect()
}

fn extract_parties(lines: &[&str], case_id: &str) -> Value {
    let role_re = Regex::new(
        r"^\s*((?:共同|上訴人|被上訴人|原告|被告|抗告人|相對人|聲請人|再抗告人|再審原告|再審被告|法定代理人|訴訟代理人))\s+(.+?)$",
    )
    .expect("valid regex");
    let mut parties = serde_json::Map::new();
    let mut in_party_section = false;
    for line in lines {
        let normalized = line.split_whitespace().collect::<String>();
        if normalized == "主文" || normalized == "據上論結" {
            break;
        }
        if !in_party_section {
            if !case_id.is_empty() && line.contains(case_id) {
                in_party_section = true;
            }
            continue;
        }
        if let Some(caps) = role_re.captures(line) {
            let role = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let name = caps
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !role.is_empty() && !name.is_empty() {
                parties
                    .entry(role)
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("array")
                    .push(json!(name));
            }
        }
    }
    Value::Object(parties)
}

fn extract_cause(lines: &[&str]) -> String {
    let re = Regex::new(r"(?:間|因)(?:請求)?(.{2,20}?)事件").expect("valid regex");
    lines
        .iter()
        .find_map(|line| {
            re.captures(line)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().trim().to_string())
        })
        .unwrap_or_default()
}

fn extract_sections(lines: &[&str]) -> (String, String, String) {
    let mut section = "";
    let mut main_text = Vec::new();
    let mut facts = Vec::new();
    let mut reasoning = Vec::new();
    for line in lines {
        let normalized = line.split_whitespace().collect::<String>();
        match normalized.as_str() {
            "主文" => {
                section = "main";
                continue;
            }
            "事實" | "事實及理由" | "事實與理由" | "犯罪事實" | "犯罪事實及理由" =>
            {
                section = "facts";
                continue;
            }
            "理由" => {
                section = "reasoning";
                continue;
            }
            _ => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        match section {
            "main" => main_text.push((*line).to_string()),
            "facts" => facts.push((*line).to_string()),
            "reasoning" => reasoning.push((*line).to_string()),
            _ => {}
        }
    }
    (main_text.join("\n"), facts.join("\n"), reasoning.join("\n"))
}

fn extract_cited_statutes(text: &str) -> Vec<String> {
    let re = Regex::new(
        r"([\u4e00-\u9fff]{2,20}(?:法|條例|規則|辦法))\s*第\s*(\d{1,4}(?:[-之]\d{1,2})?)\s*條",
    )
    .expect("valid regex");
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        let law_name = clean_statute_name(&caps[1]);
        let entry = format!("{}第{}條", law_name, &caps[2].replace('之', "-"));
        if seen.insert(entry.clone()) {
            out.push(entry);
        }
    }
    out
}

fn clean_statute_name(raw: &str) -> String {
    let mut name = raw.trim();
    for marker in ["依", "按", "據", "違反", "適用", "準用", "及", "與", "、"] {
        if let Some(idx) = name.rfind(marker) {
            let candidate = &name[idx + marker.len()..];
            if candidate.chars().count() >= 2 {
                name = candidate;
                break;
            }
        }
    }
    name.to_string()
}

fn extract_cited_cases(text: &str) -> Vec<String> {
    let re = Regex::new(r"((?:最高(?:行政)?法院|(?:臺灣|台灣)高等法院(?:\S{2,3}分院)?|(?:臺灣|台灣)\S+?(?:地方|少年及家事)法院|(?:臺北|臺中|高雄)高等行政法院|智慧財產(?:及商業)?法院|懲戒法院|福建\S*?(?:地方|高等)法院(?:\S{2,3}分院)?)\s*\d+\s*年度?\s*\S+字\s*第?\s*\d+\s*號)")
        .expect("valid regex");
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        let entry = caps[1].to_string();
        if seen.insert(entry.clone()) {
            out.push(entry);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_results_fixture() {
        let html = r#"
        <table id="jud" class="jub-table">
          <tr><th>序</th></tr>
          <tr>
            <td>1</td>
            <td><a class="hlTitle_scroll" href="data.aspx?ty=JD&id=TPSV%2C114%2C台上%2C3753%2C20251112%2C1">最高法院 114 年度台上字第3753號</a></td>
            <td>114.11.12</td>
            <td>損害賠償</td>
          </tr>
          <tr class="summary"><td colspan="4"><span class="tdCut">摘要文字</span></td></tr>
        </table>"#;
        let results = parse_search_results(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["court"], "最高法院");
        assert_eq!(results[0]["case_type"], "民事");
        assert_eq!(results[0]["summary"], "摘要文字");
    }

    #[test]
    fn parses_judgment_fixture() {
        let html = r#"
        <div id="jud">
        臺灣臺北地方法院民事判決
        114年度訴字第123號
        原告 王小明
        被告 陳小華
        主文
        被告應給付原告新臺幣壹萬元。
        事實及理由
        一、原告主張依民法第184條請求。
        中  華  民  國  114  年  5  月  1  日
        法官  張三
        </div>"#;
        let value =
            parse_judgment_page(html, "https://judgment.judicial.gov.tw/FJUD/data.aspx").unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["court"], "臺灣臺北地方法院");
        assert_eq!(value["date"], "114-05-01");
        assert!(value["cited_statutes"]
            .as_array()
            .unwrap()
            .contains(&json!("民法第184條")));
    }

    #[test]
    fn parses_special_judgment_kind_aliases() {
        let simple = SpecialJudgmentKind::parse(Some("simple")).unwrap();
        assert_eq!(simple.id, "simple");
        let summons = SpecialJudgmentKind::parse(Some("public-summons")).unwrap();
        assert_eq!(summons.url, JUDICIAL_PUBLIC_SUMMONS_SEARCH_URL);
    }
}
