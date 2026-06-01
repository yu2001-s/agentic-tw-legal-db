use crate::data::{law_history, law_name_for_pcode, law_status, pcode_data, resolve_pcode};
use crate::{retrieved_at, TwlawError, TwlawResult};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::thread::sleep;
use std::time::Duration;

const REGULATION_SINGLE_URL: &str = "https://law.moj.gov.tw/LawClass/LawSingle.aspx";
const REGULATION_ALL_URL: &str = "https://law.moj.gov.tw/LawClass/LawAll.aspx";
const USER_AGENT: &str = "twlaw/0.1";
const RETRY_ATTEMPTS: usize = 3;
const RETRY_BASE_DELAY_MS: u64 = 250;

#[derive(Debug, Clone, Default)]
pub struct RegulationQuery {
    pub law_name: Option<String>,
    pub pcode: Option<String>,
    pub article_no: Option<String>,
    pub from_no: Option<String>,
    pub to_no: Option<String>,
    pub include_history: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RegulationSearch {
    pub keyword: String,
    pub offset: usize,
    pub limit: usize,
    pub exclude_abolished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Article {
    number: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StructureEntry {
    title: String,
    level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_article: Option<String>,
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

fn get_text(client: &Client, url: &str, params: &[(&str, &str)]) -> TwlawResult<(String, String)> {
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
                let source_url = response.url().to_string();
                let html = response.text()?;
                return Ok((html, source_url));
            }
            Err(err) => {
                let retryable = err.is_connect() || err.is_timeout() || err.is_request();
                if retryable && attempt + 1 < RETRY_ATTEMPTS {
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

fn retry_delay(attempt: usize) {
    let multiplier = 1_u64 << attempt.min(4);
    sleep(Duration::from_millis(RETRY_BASE_DELAY_MS * multiplier));
}

fn looks_like_article(text: &str) -> bool {
    const GARBAGE: &[&str] = &[
        "本網站係提供法規之最新動態資訊",
        "若有任何法律上的疑義",
        "著作權聲明",
        "隱私權保護",
        "網站安全政策",
        "瀏覽人次總計",
        "法規整編資料截止日",
        "本站所提供資料僅供參考",
        "電子報訂閱",
    ];
    !GARBAGE.iter().any(|needle| text.contains(needle))
}

fn article_sort_key(number: &str) -> f64 {
    let normalized = number.replace('之', "-");
    let mut parts = normalized.split('-');
    let base = parts
        .next()
        .and_then(|p| p.parse::<f64>().ok())
        .unwrap_or(0.0);
    let suffix = parts
        .next()
        .and_then(|p| p.parse::<f64>().ok())
        .map(|n| n / 10.0)
        .unwrap_or(0.0);
    base + suffix
}

fn parse_law_name(document: &Html) -> String {
    for query in ["h2", ".law-title", "title"] {
        if let Some(el) = document.select(&selector(query)).next() {
            let text = text_of(el);
            let name = text.split('-').next().unwrap_or("").trim();
            if !matches!(
                name,
                "" | "條文內容" | "法規內容" | "全國法規資料庫" | "歷史法規"
            ) {
                return name.to_string();
            }
        }
    }
    String::new()
}

fn parse_single_article(html: &str, requested_article: &str) -> Option<Article> {
    let document = Html::parse_document(html);
    for query in [".law-article", "#pnlContent", ".content-law", "pre"] {
        if let Some(el) = document.select(&selector(query)).next() {
            let text = text_of(el);
            if !text.is_empty() && looks_like_article(&text) {
                return Some(Article {
                    number: requested_article.to_string(),
                    content: text,
                });
            }
        }
    }

    let mut longest = String::new();
    for el in document.select(&selector("p, div, td")) {
        let text = text_of(el);
        if text.len() > longest.len() {
            longest = text;
        }
    }
    if longest.len() > 20 && looks_like_article(&longest) {
        Some(Article {
            number: requested_article.to_string(),
            content: longest,
        })
    } else {
        None
    }
}

fn parse_all_articles(html: &str) -> (String, Vec<Article>, Vec<StructureEntry>) {
    let document = Html::parse_document(html);
    let law_name = parse_law_name(&document);
    let mut articles = Vec::new();
    let mut structure = Vec::new();
    let mut pending_chapters: Vec<StructureEntry> = Vec::new();

    let content_root = document.select(&selector(".law-reg-content")).next();
    if let Some(root) = content_root {
        for child in root.children() {
            let Some(el) = scraper::ElementRef::wrap(child) else {
                continue;
            };
            let class = el.value().attr("class").unwrap_or("");
            let classes: Vec<&str> = class.split_whitespace().collect();

            if classes.contains(&"h3") {
                let title = text_of(el).split_whitespace().collect::<String>();
                if !title.is_empty() {
                    let level = if classes.contains(&"char-1") {
                        1
                    } else if classes.contains(&"char-2") {
                        2
                    } else {
                        3
                    };
                    pending_chapters.push(StructureEntry {
                        title,
                        level,
                        first_article: None,
                    });
                }
                continue;
            }

            if classes.contains(&"row") {
                let no = el.select(&selector(".col-no")).next();
                let data = el.select(&selector(".col-data")).next();
                if let (Some(no), Some(data)) = (no, data) {
                    let number_text = text_of(no);
                    let content_text = text_of(data);
                    if let Some(number) = extract_article_number(&number_text) {
                        if !content_text.is_empty() {
                            for mut chapter in pending_chapters.drain(..) {
                                chapter.first_article = Some(number.clone());
                                structure.push(chapter);
                            }
                            articles.push(Article {
                                number,
                                content: content_text,
                            });
                        }
                    }
                }
            }
        }
        structure.extend(pending_chapters);
    }

    if articles.is_empty() {
        for row in document.select(&selector("div.row")) {
            let no = row.select(&selector(".col-no")).next();
            let data = row.select(&selector(".col-data")).next();
            if let (Some(no), Some(data)) = (no, data) {
                if let Some(number) = extract_article_number(&text_of(no)) {
                    let content = text_of(data);
                    if !content.is_empty() {
                        articles.push(Article { number, content });
                    }
                }
            }
        }
    }

    if articles.is_empty() {
        for row in document.select(&selector("tr")) {
            let cells: Vec<_> = row.select(&selector("td")).collect();
            if cells.len() >= 2 {
                if let Some(number) = extract_article_number(&text_of(cells[0])) {
                    let content = text_of(cells[1]);
                    if !content.is_empty() {
                        articles.push(Article { number, content });
                    }
                }
            }
        }
    }

    (law_name, articles, structure)
}

fn extract_article_number(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"第\s*(\S+?)\s*條").expect("valid regex");
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn resolve_query_pcode(query: &RegulationQuery) -> TwlawResult<(String, String, String)> {
    if let Some(pcode) = query
        .pcode
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let law_name = law_name_for_pcode(pcode)?;
        let status = law_status(pcode)?;
        return Ok((pcode.to_string(), law_name, status));
    }

    let law_name = query
        .law_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| TwlawError::InvalidInput("provide --law or --pcode".to_string()))?;
    let resolved = resolve_pcode(law_name)?
        .ok_or_else(|| TwlawError::NotFound(format!("unknown regulation: {law_name}")))?;
    Ok((resolved.pcode, resolved.law_name, resolved.status))
}

pub fn get_pcode(law_name: &str) -> TwlawResult<Value> {
    let resolved = resolve_pcode(law_name)?
        .ok_or_else(|| TwlawError::NotFound(format!("unknown regulation: {law_name}")))?;
    Ok(json!({
        "success": true,
        "law_name": resolved.law_name,
        "pcode": resolved.pcode,
        "status": resolved.status,
        "match_type": resolved.match_type,
        "retrieved_at": retrieved_at()
    }))
}

pub fn search_regulations(search: RegulationSearch) -> TwlawResult<Value> {
    let keyword = search.keyword.trim();
    if keyword.is_empty() {
        return Err(TwlawError::InvalidInput("provide --keyword".to_string()));
    }
    let limit = search.limit.clamp(1, 200);
    let data = pcode_data()?;
    let mut matches = Vec::new();
    for (name, pcode) in &data.pcode_map {
        if !name.contains(keyword) && !pcode.contains(keyword) {
            continue;
        }
        let status = law_status(pcode)?;
        if search.exclude_abolished && status == "已廢止" {
            continue;
        }
        matches.push(json!({
            "law_name": name,
            "pcode": pcode,
            "status": status
        }));
    }

    matches.sort_by(|a, b| {
        let a_status = a["status"].as_str().unwrap_or("");
        let b_status = b["status"].as_str().unwrap_or("");
        a_status.cmp(b_status).then_with(|| {
            a["law_name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["law_name"].as_str().unwrap_or(""))
        })
    });

    let total_count = matches.len();
    let results = matches
        .into_iter()
        .skip(search.offset)
        .take(limit)
        .collect::<Vec<_>>();

    Ok(json!({
        "success": true,
        "keyword": keyword,
        "offset": search.offset,
        "limit": limit,
        "total_count": total_count,
        "has_more": search.offset + limit < total_count,
        "results": results,
        "retrieved_at": retrieved_at()
    }))
}

pub fn query_regulation(query: RegulationQuery) -> TwlawResult<Value> {
    let (pcode, resolved_name, status) = resolve_query_pcode(&query)?;
    let client = client()?;

    let mut result = if let Some(article_no) = query
        .article_no
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        query_single_article(&client, &pcode, &resolved_name, &status, article_no)?
    } else {
        let mut all = query_all_articles(&client, &pcode, &resolved_name, &status)?;
        if let (Some(from_no), Some(to_no)) = (
            query
                .from_no
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
            query
                .to_no
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
        ) {
            let from_key = article_sort_key(from_no);
            let to_key = article_sort_key(to_no);
            if let Some(articles) = all.get_mut("articles").and_then(Value::as_array_mut) {
                articles.retain(|article| {
                    article
                        .get("number")
                        .and_then(Value::as_str)
                        .map(|n| {
                            let key = article_sort_key(n);
                            from_key <= key && key <= to_key
                        })
                        .unwrap_or(false)
                });
            }
            all["range"] = json!({"from": from_no, "to": to_no});
        }
        all
    };

    if query.include_history {
        result["history"] = law_history(&pcode)?
            .map(Value::String)
            .unwrap_or(Value::Null);
    }

    Ok(result)
}

fn query_single_article(
    client: &Client,
    pcode: &str,
    resolved_name: &str,
    status: &str,
    article_no: &str,
) -> TwlawResult<Value> {
    let (html, source_url) = get_text(
        client,
        REGULATION_SINGLE_URL,
        &[("pcode", pcode), ("flno", article_no)],
    )?;
    let article = parse_single_article(&html, article_no).ok_or_else(|| {
        TwlawError::NotFound(format!(
            "{resolved_name} 第 {article_no} 條不存在或無法解析"
        ))
    })?;

    Ok(json!({
        "success": true,
        "law": {
            "pcode": pcode,
            "name": resolved_name,
            "status": status
        },
        "articles": [article],
        "source_url": source_url,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

fn query_all_articles(
    client: &Client,
    pcode: &str,
    resolved_name: &str,
    status: &str,
) -> TwlawResult<Value> {
    let (html, source_url) = get_text(client, REGULATION_ALL_URL, &[("pcode", pcode)])?;
    let (parsed_name, articles, structure) = parse_all_articles(&html);
    let name = if resolved_name.is_empty() {
        parsed_name
    } else {
        resolved_name.to_string()
    };

    let mut value = json!({
        "success": true,
        "law": {
            "pcode": pcode,
            "name": name,
            "status": status
        },
        "articles": articles,
        "structure": structure,
        "source_url": source_url,
        "cached": false,
        "retrieved_at": retrieved_at()
    });

    if value["articles"].as_array().map_or(true, Vec::is_empty) && status == "已廢止" {
        value["note"] = json!("該法規已廢止，全國法規資料庫可能不再提供條文全文。");
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_alias_to_pcode() {
        let value = get_pcode("勞基法").unwrap();
        assert_eq!(value["pcode"], "N0030001");
        assert_eq!(value["match_type"], "alias");
    }

    #[test]
    fn searches_regulations_offline() {
        let value = search_regulations(RegulationSearch {
            keyword: "勞動".to_string(),
            limit: 5,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(value["success"], true);
        assert!(value["total_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn parses_law_all_fixture() {
        let html = r#"
        <html><body><h2>民法-全國法規資料庫</h2>
        <div class="law-reg-content">
          <div class="h3 char-1">第 一 編 總則</div>
          <div class="row"><div class="col-no">第 184 條</div><div class="col-data">因故意或過失，不法侵害他人之權利者，負損害賠償責任。</div></div>
        </div></body></html>"#;
        let (name, articles, structure) = parse_all_articles(html);
        assert_eq!(name, "民法");
        assert_eq!(articles[0].number, "184");
        assert_eq!(structure[0].first_article.as_deref(), Some("184"));
    }
}
