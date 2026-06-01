use crate::{retrieved_at, TwlawError, TwlawResult};
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = "twlaw/0.1";
const MOJLAW_RESULT_URL: &str = "https://mojlaw.moj.gov.tw/LawResult.aspx";
const MOJLAW_CHECK_ALL: &str = "law,etype5,etype3,qtype,etype4,ftype,ctype,jtype";
const RETRY_ATTEMPTS: usize = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, Default)]
pub struct MojlawSearchQuery {
    pub kind: Option<String>,
    pub keyword: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy)]
struct MojlawKind {
    id: &'static str,
    label: &'static str,
    aliases: &'static [&'static str],
}

const KINDS: &[MojlawKind] = &[
    MojlawKind {
        id: "etype5",
        label: "行政函釋",
        aliases: &[
            "admin-interpretation",
            "administrative-interpretation",
            "函釋",
            "etype5",
        ],
    },
    MojlawKind {
        id: "etype3",
        label: "法規諮詢意見",
        aliases: &["legal-consultation", "consultation", "諮詢", "etype3"],
    },
    MojlawKind {
        id: "qtype",
        label: "法律問題座談",
        aliases: &["legal-seminar", "seminar", "座談", "qtype"],
    },
    MojlawKind {
        id: "etype4",
        label: "聲明異議決定書",
        aliases: &["objection", "objection-decision", "聲明異議", "etype4"],
    },
    MojlawKind {
        id: "ftype",
        label: "憲法法庭判決",
        aliases: &[
            "constitutional-judgment",
            "constitution",
            "憲法法庭",
            "ftype",
        ],
    },
    MojlawKind {
        id: "ctype",
        label: "大法官解釋",
        aliases: &["grand-justice", "interpretation", "大法官", "ctype"],
    },
    MojlawKind {
        id: "jtype",
        label: "判例",
        aliases: &["precedent", "判例", "jtype"],
    },
];

pub fn mojlaw_search(query: MojlawSearchQuery) -> TwlawResult<Value> {
    let kind = MojlawKind::parse(query.kind.as_deref())?;
    let keyword = query.keyword.trim().to_string();
    if keyword.is_empty() {
        return Err(TwlawError::InvalidInput("keyword is required".to_string()));
    }
    let limit = query.limit.clamp(1, 40);
    let source_url = search_url(kind, &keyword, limit)?;
    let html = fetch_text_url(&client()?, source_url.as_str())?;
    let document = Html::parse_document(&html);
    let counts = result_counts(&document, &source_url)?;
    let mut results = result_rows(&document, &source_url, kind)?;
    let total_count = counts
        .iter()
        .find_map(|entry| {
            if entry.get("kind")?.as_str()? == kind.id {
                entry.get("count")?.as_u64()
            } else {
                None
            }
        })
        .unwrap_or(results.len() as u64);
    let truncated = total_count as usize > results.len();
    results.truncate(limit);

    if results.is_empty() && total_count > 0 {
        return Err(TwlawError::ParseChanged(
            "could not parse MOJ law retrieval result rows".to_string(),
        ));
    }

    Ok(json!({
        "success": true,
        "source": "法務部主管法規查詢系統",
        "source_url": source_url.to_string(),
        "kind": kind.id,
        "kind_label": kind.label,
        "keyword": keyword,
        "count": total_count,
        "returned_count": results.len(),
        "truncated": truncated,
        "category_counts": counts,
        "results": results,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

impl MojlawKind {
    fn parse(input: Option<&str>) -> TwlawResult<Self> {
        let value = input
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("admin-interpretation")
            .to_ascii_lowercase()
            .replace('_', "-");
        KINDS
            .iter()
            .copied()
            .find(|kind| kind.id == value || kind.aliases.iter().any(|alias| *alias == value))
            .ok_or_else(|| {
                TwlawError::InvalidInput(format!(
                    "unknown MOJ law retrieval kind: {}; expected one of admin-interpretation, legal-consultation, legal-seminar, objection, constitutional-judgment, grand-justice, precedent",
                    input.unwrap_or("")
                ))
            })
    }
}

fn search_url(kind: MojlawKind, keyword: &str, limit: usize) -> TwlawResult<Url> {
    let mut url = Url::parse(MOJLAW_RESULT_URL)?;
    url.query_pairs_mut()
        .append_pair("id", "A0000")
        .append_pair("check", MOJLAW_CHECK_ALL)
        .append_pair("search", "3")
        .append_pair("valid", "3")
        .append_pair("star", "")
        .append_pair("end", "")
        .append_pair("number", "")
        .append_pair("kw", keyword)
        .append_pair("sort", "")
        .append_pair("LawType", kind.id)
        .append_pair("CategoryID", "")
        .append_pair("iPageSize", &limit.to_string())
        .append_pair("page", "1");
    Ok(url)
}

fn client() -> TwlawResult<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

fn fetch_text_url(client: &Client, url: &str) -> TwlawResult<String> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.get(url).send() {
            Ok(response) => match response.error_for_status() {
                Ok(ok) => match ok.text() {
                    Ok(text) => return Ok(text),
                    Err(err) => last_error = Some(err.to_string()),
                },
                Err(err) => last_error = Some(err.to_string()),
            },
            Err(err) => last_error = Some(err.to_string()),
        }
        if attempt + 1 < RETRY_ATTEMPTS {
            sleep(Duration::from_millis(
                RETRY_BASE_DELAY_MS * 2u64.pow(attempt as u32),
            ));
        }
    }
    Err(TwlawError::Network(format!(
        "failed to fetch {url}: {}",
        last_error.unwrap_or_else(|| "unknown network error".to_string())
    )))
}

fn result_counts(document: &Html, base_url: &Url) -> TwlawResult<Vec<Value>> {
    let link_selector = selector(".law-type-list a[href], ul a[href]");
    let badge_selector = selector(".badge");
    let mut counts_by_kind = HashMap::<String, u64>::new();

    for link in document.select(&link_selector) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Ok(url) = base_url.join(href) else {
            continue;
        };
        let Some(kind_id) = query_param(&url, "LawType") else {
            continue;
        };
        if !KINDS.iter().any(|kind| kind.id == kind_id) {
            continue;
        }
        let count = link
            .select(&badge_selector)
            .next()
            .map(text_of)
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(0);
        counts_by_kind
            .entry(kind_id)
            .and_modify(|existing| *existing = (*existing).max(count))
            .or_insert(count);
    }

    let counts = KINDS
        .iter()
        .filter_map(|kind| {
            counts_by_kind.get(kind.id).map(|count| {
                json!({
                    "kind": kind.id,
                    "kind_label": kind.label,
                    "count": count
                })
            })
        })
        .collect();
    Ok(counts)
}

fn result_rows(document: &Html, base_url: &Url, kind: MojlawKind) -> TwlawResult<Vec<Value>> {
    let row_selector = selector("table.law-content tbody tr");
    let link_selector = selector("a[href]");
    let div_selector = selector("div");
    let span_selector = selector("span");
    let pre_selector = selector("pre");
    let mut results = Vec::new();

    for row in document.select(&row_selector) {
        let Some(link) = row.select(&link_selector).next() else {
            continue;
        };
        let href = link.value().attr("href").unwrap_or("");
        let source_url = base_url.join(href)?.to_string();
        let mut date = String::new();
        for div in row.select(&div_selector) {
            if text_of(div).contains("發文日期") {
                date = div
                    .select(&span_selector)
                    .next()
                    .map(text_of)
                    .unwrap_or_default();
                break;
            }
        }
        let summary = row
            .select(&pre_selector)
            .next()
            .map(text_of)
            .unwrap_or_default();
        results.push(json!({
            "kind": kind.id,
            "kind_label": kind.label,
            "reference": text_of(link),
            "date": date,
            "summary": summary,
            "source_url": source_url,
            "document_id": query_param(&base_url.join(href)?, "id")
        }));
    }

    Ok(results)
}

fn selector(input: &str) -> Selector {
    Selector::parse(input).expect("valid selector")
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find_map(|(name, value)| (name == key).then(|| value.to_string()))
}

fn text_of(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mojlaw_kind_aliases() {
        assert_eq!(
            MojlawKind::parse(Some("legal-consultation")).unwrap().id,
            "etype3"
        );
        assert!(MojlawKind::parse(Some("not-a-kind")).is_err());
    }

    #[test]
    fn parses_mojlaw_result_rows() {
        let html = Html::parse_document(
            r#"
            <ul><li><a href="LawResult.aspx?LawType=etype5">行政函釋<span class="badge">373</span></a></li></ul>
            <table class="table law-content"><tbody>
              <tr><td>1.</td><td>
                <div><b>發文字號：</b><a href="LawContentExShow.aspx?id=FE393967&type=E">法務部 法律決字第 11403513710 號</a></div>
                <div><b>發文日期：</b><span>114.11.25</span></div>
                <div><b>要　　旨：</b><pre>個資相關摘要</pre></div>
              </td></tr>
            </tbody></table>
            "#,
        );
        let base = Url::parse(MOJLAW_RESULT_URL).unwrap();

        let counts = result_counts(&html, &base).unwrap();
        let rows = result_rows(&html, &base, MojlawKind::parse(Some("etype5")).unwrap()).unwrap();

        assert_eq!(counts[0]["count"], 373);
        assert_eq!(rows[0]["document_id"], "FE393967");
        assert_eq!(rows[0]["date"], "114.11.25");
    }
}
