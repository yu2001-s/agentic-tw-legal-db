use crate::{retrieved_at, TwlawError, TwlawResult};
use reqwest::blocking::Client;
use reqwest::header;
use reqwest::StatusCode;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process;
use std::thread::sleep;
use std::time::Duration;
use url::Url;
use zip::ZipArchive;

const USER_AGENT: &str = "twlaw/0.1";
const MOJ_NEWS_LIST_URL: &str = "https://law.moj.gov.tw/News/NewsList.aspx";
const MOJ_TREATY_URL: &str = "https://law.moj.gov.tw/Law/LawSearchAgree.aspx";
const MOJ_CROSS_STRAIT_URL: &str = "https://law.moj.gov.tw/Law/LawSearchTwo.aspx";
const MOJ_HOT_WORD_URL: &str = "https://law.moj.gov.tw/Hot/HOT_LAWWORD.ashx";
const RETRY_ATTEMPTS: usize = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, Default)]
pub struct MojDatasetQuery {
    pub dataset: Option<String>,
    pub cache_dir: Option<PathBuf>,
    pub remote: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MojSyncQuery {
    pub dataset: Option<String>,
    pub cache_dir: Option<PathBuf>,
    pub force: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MojSearchQuery {
    pub dataset: String,
    pub keyword: String,
    pub include_articles: bool,
    pub limit: usize,
    pub cache_dir: Option<PathBuf>,
    pub refresh: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MojGetQuery {
    pub dataset: String,
    pub law: String,
    pub article: Option<String>,
    pub include_articles: bool,
    pub include_history: bool,
    pub cache_dir: Option<PathBuf>,
    pub refresh: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MojUpdatesQuery {
    pub kind: Option<String>,
    pub keyword: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MojAgreementsQuery {
    pub kind: Option<String>,
    pub keyword: Option<String>,
    pub category_code: Option<String>,
    pub include_categories: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy)]
struct DatasetSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    url: &'static str,
    zip_filename: &'static str,
    json_filename: &'static str,
    language: &'static str,
    kind: &'static str,
    english: bool,
}

struct LoadedDataset {
    spec: DatasetSpec,
    value: Value,
    cache_path: PathBuf,
    cache_was_populated: bool,
}

const DATASETS: &[DatasetSpec] = &[
    DatasetSpec {
        id: "ch-law",
        name: "中文法規法律資料檔",
        description: "Ministry of Justice Chinese laws from the no-token OpenAPI ZIP endpoint.",
        url: "https://law.moj.gov.tw/api/ch/law/json",
        zip_filename: "ChLaw.json.zip",
        json_filename: "ChLaw.json",
        language: "zh-Hant",
        kind: "law",
        english: false,
    },
    DatasetSpec {
        id: "ch-order",
        name: "中文法規命令資料檔",
        description: "Ministry of Justice Chinese orders from the no-token OpenAPI ZIP endpoint.",
        url: "https://law.moj.gov.tw/api/ch/order/json",
        zip_filename: "ChOrder.json.zip",
        json_filename: "ChOrder.json",
        language: "zh-Hant",
        kind: "order",
        english: false,
    },
    DatasetSpec {
        id: "en-law",
        name: "英文法規法律資料檔",
        description:
            "Ministry of Justice English law translations from the no-token OpenAPI ZIP endpoint.",
        url: "https://law.moj.gov.tw/api/en/law/json",
        zip_filename: "EnLaw.json.zip",
        json_filename: "EnLaw.json",
        language: "en",
        kind: "law",
        english: true,
    },
    DatasetSpec {
        id: "en-order",
        name: "英文法規命令資料檔",
        description:
            "Ministry of Justice English order translations from the no-token OpenAPI ZIP endpoint.",
        url: "https://law.moj.gov.tw/api/en/order/json",
        zip_filename: "EnOrder.json.zip",
        json_filename: "EnOrder.json",
        language: "en",
        kind: "order",
        english: true,
    },
];

pub fn moj_datasets() -> TwlawResult<Value> {
    Ok(json!({
        "success": true,
        "datasets": DATASETS.iter().map(dataset_json).collect::<Vec<_>>(),
        "dataset_count": DATASETS.len(),
        "note": "MOJ OpenAPI JSON endpoints return ZIP files containing JSON plus schema/manifest files. No API token is required.",
        "retrieved_at": retrieved_at()
    }))
}

pub fn moj_status(query: MojDatasetQuery) -> TwlawResult<Value> {
    let cache_dir = resolve_cache_dir(query.cache_dir)?;
    let datasets = dataset_selection(query.dataset.as_deref())?;
    let client = if query.remote { Some(client()?) } else { None };
    let mut out = Vec::new();

    for spec in datasets {
        let cache_path = dataset_cache_path(&cache_dir, spec);
        let metadata_path = metadata_cache_path(&cache_dir, spec);
        let cache = cache_status(&cache_path, &metadata_path)?;
        let remote = if let Some(client) = &client {
            Some(remote_status(client, spec)?)
        } else {
            None
        };

        out.push(json!({
            "dataset": dataset_json(spec),
            "cache": cache,
            "remote": remote
        }));
    }

    Ok(json!({
        "success": true,
        "cache_dir": cache_dir,
        "remote_checked": client.is_some(),
        "datasets": out,
        "retrieved_at": retrieved_at()
    }))
}

pub fn moj_sync(query: MojSyncQuery) -> TwlawResult<Value> {
    let cache_dir = resolve_cache_dir(query.cache_dir)?;
    fs::create_dir_all(&cache_dir)?;
    let client = client()?;
    let datasets = dataset_selection(query.dataset.as_deref())?;
    let mut synced = Vec::new();

    for spec in datasets {
        let cache_path = dataset_cache_path(&cache_dir, spec);
        if cache_path.exists() && !query.force {
            let metadata = read_metadata(&metadata_cache_path(&cache_dir, spec))?;
            synced.push(json!({
                "dataset": spec.id,
                "status": "already_cached",
                "cache_path": cache_path,
                "metadata": metadata
            }));
            continue;
        }
        synced.push(download_and_cache(&client, &cache_dir, spec)?);
    }

    Ok(json!({
        "success": true,
        "cache_dir": cache_dir,
        "synced": synced,
        "retrieved_at": retrieved_at()
    }))
}

pub fn moj_search(query: MojSearchQuery) -> TwlawResult<Value> {
    let spec = dataset_by_id(&query.dataset)?;
    let loaded = load_dataset(spec, query.cache_dir, query.refresh)?;
    let keyword = query.keyword.trim().to_string();
    if keyword.is_empty() {
        return Err(TwlawError::InvalidInput("keyword is required".to_string()));
    }
    let limit = query.limit.clamp(1, 200);
    let laws = laws_array(&loaded.value)?;
    let mut results = Vec::new();

    for law in laws {
        let metadata_match = metadata_matches(spec, law, &keyword);
        let article_matches = if query.include_articles {
            matching_articles(spec, law, &keyword, 3)
        } else {
            Vec::new()
        };
        if !metadata_match && article_matches.is_empty() {
            continue;
        }
        let mut summary = law_summary(spec, law);
        summary["matched_in"] = json!(if metadata_match && !article_matches.is_empty() {
            "metadata_and_articles"
        } else if metadata_match {
            "metadata"
        } else {
            "articles"
        });
        if query.include_articles {
            summary["article_matches"] = json!(article_matches);
        }
        results.push(summary);
    }

    let count = results.len();
    let truncated = count > limit;
    results.truncate(limit);

    Ok(json!({
        "success": true,
        "dataset": dataset_json(&loaded.spec),
        "keyword": keyword,
        "search_fields": if query.include_articles {
            json!(["law_name", "translated_law_name", "category", "level", "article_content"])
        } else {
            json!(["law_name", "translated_law_name", "category", "level"])
        },
        "update_date": update_date(&loaded.value),
        "count": count,
        "truncated": truncated,
        "results": results,
        "cache": cache_json(&loaded),
        "retrieved_at": retrieved_at()
    }))
}

pub fn moj_get(query: MojGetQuery) -> TwlawResult<Value> {
    let spec = dataset_by_id(&query.dataset)?;
    let loaded = load_dataset(spec, query.cache_dir, query.refresh)?;
    let requested = query.law.trim();
    if requested.is_empty() {
        return Err(TwlawError::InvalidInput("law is required".to_string()));
    }

    let law = find_law(spec, laws_array(&loaded.value)?, requested).ok_or_else(|| {
        TwlawError::NotFound(format!(
            "{requested} not found in MOJ OpenAPI dataset {}",
            spec.id
        ))
    })?;
    let mut summary = law_summary(spec, law);
    if query.include_history {
        summary["histories"] = json!(string_field(law, history_key(spec)));
        summary["foreword"] = json!(string_field(law, foreword_key(spec)));
    }

    let articles = article_array(spec, law);
    if let Some(article) = query
        .article
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let matched = articles
            .iter()
            .find(|entry| article_no_matches(spec, entry, article))
            .ok_or_else(|| {
                TwlawError::NotFound(format!(
                    "{requested} article {article} not found in MOJ OpenAPI dataset {}",
                    spec.id
                ))
            })?;
        summary["articles"] = json!([article_json(spec, matched)]);
        summary["article_query"] = json!(article);
    } else if query.include_articles {
        summary["articles"] = json!(articles
            .iter()
            .map(|entry| article_json(spec, entry))
            .collect::<Vec<_>>());
    }

    Ok(json!({
        "success": true,
        "dataset": dataset_json(&loaded.spec),
        "query": {
            "law": requested,
            "article": query.article,
            "include_articles": query.include_articles,
            "include_history": query.include_history
        },
        "update_date": update_date(&loaded.value),
        "law": summary,
        "cache": cache_json(&loaded),
        "retrieved_at": retrieved_at()
    }))
}

pub fn moj_updates(query: MojUpdatesQuery) -> TwlawResult<Value> {
    let update_kind = UpdateKind::parse(query.kind.as_deref())?;
    let limit = query.limit.clamp(1, 100);
    let source_url = update_kind.url()?;
    let client = client()?;
    let html = fetch_text_url(&client, source_url.as_str())?;
    let document = Html::parse_document(&html);
    let row_selector = selector("table.tab-news tbody tr");
    let cell_selector = selector("td");
    let link_selector = selector("a[href]");
    let keyword = query.keyword.unwrap_or_default().trim().to_string();
    let mut results = Vec::new();

    for row in document.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() < 4 {
            continue;
        }
        let date = text_of(cells[1]);
        let category = text_of(cells[2]);
        let Some(link) = cells[3].select(&link_selector).next() else {
            continue;
        };
        let title = text_of(link);
        if !keyword.is_empty()
            && !title.to_lowercase().contains(&keyword.to_lowercase())
            && !category.to_lowercase().contains(&keyword.to_lowercase())
        {
            continue;
        }
        let href = link.value().attr("href").unwrap_or("");
        let source = source_url.join(href)?.to_string();
        results.push(json!({
            "date": date,
            "category": category,
            "title": title,
            "source_url": source
        }));
    }

    if results.is_empty() && keyword.is_empty() {
        return Err(TwlawError::ParseChanged(
            "could not parse MOJ news list table".to_string(),
        ));
    }

    let count = results.len();
    let truncated = count > limit;
    results.truncate(limit);

    Ok(json!({
        "success": true,
        "kind": update_kind.id,
        "kind_label": update_kind.label,
        "keyword": keyword,
        "source_url": source_url.to_string(),
        "count": count,
        "truncated": truncated,
        "results": results,
        "cached": false,
        "retrieved_at": retrieved_at()
    }))
}

pub fn moj_agreements(query: MojAgreementsQuery) -> TwlawResult<Value> {
    let kind = AgreementKind::parse(query.kind.as_deref())?;
    let limit = query.limit.clamp(1, 100);
    let keyword = query.keyword.unwrap_or_default().trim().to_string();
    let category_code = query
        .category_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if !keyword.is_empty() && category_code.is_some() {
        return Err(TwlawError::InvalidInput(
            "use either --keyword or --category-code; MOJ keyword search does not preserve category filters".to_string(),
        ));
    }

    if category_code.is_some() && !kind.supports_categories {
        return Err(TwlawError::InvalidInput(format!(
            "{} agreements do not expose MOJ category codes; omit --category-code",
            kind.id
        )));
    }

    let source_url = agreement_url(kind, &keyword, category_code.as_deref())?;
    let client = client()?;
    let html = fetch_text_url(&client, source_url.as_str())?;
    let document = Html::parse_document(&html);
    let categories = agreement_categories(&document, &source_url)?;
    let include_categories = query.include_categories
        || (kind.supports_categories && keyword.is_empty() && category_code.is_none());
    let category_lookup = categories
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("code")?.as_str()?.to_string(),
                entry.get("label")?.as_str()?.to_string(),
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let inferred_category_code = if keyword.is_empty() && kind.supports_categories {
        category_code
            .clone()
            .or_else(|| first_category_code_from_pagination(&document, &source_url))
            .or_else(|| {
                categories
                    .first()
                    .and_then(|value| value.get("code")?.as_str().map(str::to_string))
            })
    } else {
        category_code.clone()
    };
    let results = agreement_results(
        &document,
        &source_url,
        inferred_category_code.as_deref(),
        &category_lookup,
        &keyword,
    )?;
    let total_count = pagination_total(&document).unwrap_or(results.len());
    let total_pages = pagination_pages(&document).unwrap_or(if results.is_empty() { 0 } else { 1 });
    let visible_count = results.len();
    let mut limited_results = results;
    let truncated_by_limit = limited_results.len() > limit;
    limited_results.truncate(limit);
    let first_page_only = total_pages > 1;
    let truncated = truncated_by_limit || first_page_only;
    let effective_category_code = inferred_category_code
        .or_else(|| first_category_code(&limited_results))
        .or_else(|| first_category_code_from_pagination(&document, &source_url))
        .or_else(|| {
            categories
                .first()
                .and_then(|value| value.get("code")?.as_str().map(str::to_string))
        });
    let output_categories = if include_categories {
        categories
    } else {
        Vec::new()
    };

    Ok(json!({
        "success": true,
        "kind": kind.id,
        "kind_label": kind.label,
        "keyword": keyword,
        "category_code": effective_category_code,
        "category_count": category_lookup.len(),
        "categories_included": include_categories,
        "categories": output_categories,
        "source_url": source_url.to_string(),
        "count": total_count,
        "visible_count": visible_count,
        "returned_count": limited_results.len(),
        "total_pages": total_pages,
        "first_page_only": first_page_only,
        "truncated": truncated,
        "results": limited_results,
        "cached": false,
        "notes": kind.notes,
        "retrieved_at": retrieved_at()
    }))
}

fn client() -> TwlawResult<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

fn resolve_cache_dir(input: Option<PathBuf>) -> TwlawResult<PathBuf> {
    if let Some(path) = input {
        return Ok(path);
    }
    if let Ok(path) = env::var("TWLAW_CACHE_DIR") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path).join("moj-openapi"));
        }
    }
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join(".cache/twlaw/moj-openapi"));
        }
    }
    Ok(env::temp_dir().join("twlaw/moj-openapi"))
}

fn dataset_selection(input: Option<&str>) -> TwlawResult<Vec<&'static DatasetSpec>> {
    match input.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("all") => Ok(DATASETS.iter().collect()),
        Some(value) => Ok(vec![dataset_by_id(value)?]),
    }
}

fn dataset_by_id(input: &str) -> TwlawResult<&'static DatasetSpec> {
    let normalized = input.trim().to_ascii_lowercase().replace('_', "-");
    DATASETS
        .iter()
        .find(|spec| spec.id == normalized)
        .ok_or_else(|| {
            TwlawError::InvalidInput(format!(
                "unknown MOJ OpenAPI dataset: {input}; expected one of ch-law, ch-order, en-law, en-order, all"
            ))
        })
}

fn dataset_cache_path(cache_dir: &Path, spec: &DatasetSpec) -> PathBuf {
    cache_dir.join(format!("{}.json", spec.id))
}

fn metadata_cache_path(cache_dir: &Path, spec: &DatasetSpec) -> PathBuf {
    cache_dir.join(format!("{}.metadata.json", spec.id))
}

fn dataset_json(spec: &DatasetSpec) -> Value {
    json!({
        "id": spec.id,
        "name": spec.name,
        "description": spec.description,
        "url": spec.url,
        "zip_filename": spec.zip_filename,
        "json_filename": spec.json_filename,
        "language": spec.language,
        "kind": spec.kind,
        "credentials_required": false
    })
}

fn cache_status(cache_path: &Path, metadata_path: &Path) -> TwlawResult<Value> {
    let exists = cache_path.exists();
    let size_bytes = if exists {
        Some(fs::metadata(cache_path)?.len())
    } else {
        None
    };
    let metadata = read_metadata(metadata_path)?;
    Ok(json!({
        "exists": exists,
        "path": cache_path,
        "size_bytes": size_bytes,
        "metadata": metadata
    }))
}

fn read_metadata(path: &Path) -> TwlawResult<Value> {
    if !path.exists() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn remote_status(client: &Client, spec: &DatasetSpec) -> TwlawResult<Value> {
    let head_response = client.head(spec.url).send()?;
    let response = if head_response.status() == StatusCode::METHOD_NOT_ALLOWED {
        client.get(spec.url).send()?
    } else {
        head_response
    }
    .error_for_status()?;
    let headers = response.headers();
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let content_disposition = headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    Ok(json!({
        "status": response.status().as_u16(),
        "content_length": content_length,
        "content_disposition": content_disposition,
        "source_url": response.url().to_string()
    }))
}

fn load_dataset(
    spec: &DatasetSpec,
    cache_dir: Option<PathBuf>,
    refresh: bool,
) -> TwlawResult<LoadedDataset> {
    let cache_dir = resolve_cache_dir(cache_dir)?;
    fs::create_dir_all(&cache_dir)?;
    let cache_path = dataset_cache_path(&cache_dir, spec);
    let mut populated = false;
    if refresh || !cache_path.exists() {
        let client = client()?;
        download_and_cache(&client, &cache_dir, spec)?;
        populated = true;
    }
    let text = fs::read_to_string(&cache_path)?;
    let value = parse_json_with_bom(&text)?;
    Ok(LoadedDataset {
        spec: *spec,
        value,
        cache_path,
        cache_was_populated: populated,
    })
}

fn download_and_cache(client: &Client, cache_dir: &Path, spec: &DatasetSpec) -> TwlawResult<Value> {
    let bytes = fetch_bytes(client, spec.url)?;
    let json_text = extract_zip_json(bytes.as_ref(), spec.json_filename)?;
    let value = parse_json_with_bom(&json_text)?;
    let law_count = laws_array(&value)?.len();
    let update_date = update_date(&value);
    let cache_path = dataset_cache_path(cache_dir, spec);
    let metadata_path = metadata_cache_path(cache_dir, spec);
    write_atomic(&cache_path, json_text.as_bytes())?;

    let metadata = json!({
        "dataset": spec.id,
        "source_url": spec.url,
        "zip_filename": spec.zip_filename,
        "json_filename": spec.json_filename,
        "update_date": update_date,
        "law_count": law_count,
        "retrieved_at": retrieved_at()
    });
    write_atomic(
        &metadata_path,
        serde_json::to_string_pretty(&metadata)?.as_bytes(),
    )?;

    Ok(json!({
        "dataset": spec.id,
        "status": "synced",
        "cache_path": cache_path,
        "metadata": metadata
    }))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> TwlawResult<()> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let tmp_path = path.with_file_name(format!("{filename}.{}.tmp", process::id()));
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn fetch_bytes(client: &Client, url: &str) -> TwlawResult<Vec<u8>> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.get(url).send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt + 1 < RETRY_ATTEMPTS {
                    retry_delay(attempt);
                    continue;
                }
                return Ok(response.error_for_status()?.bytes()?.to_vec());
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

fn fetch_text_url(client: &Client, url: &str) -> TwlawResult<String> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.get(url).send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt + 1 < RETRY_ATTEMPTS {
                    retry_delay(attempt);
                    continue;
                }
                return Ok(response.error_for_status()?.text()?);
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

fn extract_zip_json(bytes: &[u8], json_filename: &str) -> TwlawResult<String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)?;
    let mut file = archive.by_name(json_filename)?;
    let mut out = String::new();
    file.read_to_string(&mut out)?;
    Ok(out)
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

#[derive(Debug, Clone, Copy)]
struct UpdateKind {
    id: &'static str,
    label: &'static str,
    query_type: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
struct AgreementKind {
    id: &'static str,
    label: &'static str,
    url: &'static str,
    hot_type: &'static str,
    supports_categories: bool,
    notes: &'static str,
}

impl AgreementKind {
    fn parse(input: Option<&str>) -> TwlawResult<Self> {
        let value = input.unwrap_or("treaty").trim().to_ascii_lowercase();
        match value.as_str() {
            "" | "treaty" | "treaties" | "agreement" | "agreements" => Ok(Self {
                id: "treaty",
                label: "條約協定",
                url: MOJ_TREATY_URL,
                hot_type: "CONVENTION",
                supports_categories: true,
                notes: "Treaty browsing without a keyword is scoped to one MOJ category page. Use returned categories with --category-code for broader coverage, or use --keyword for MOJ's public keyword result page.",
            }),
            "cross-strait" | "cross_strait" | "china" | "two" | "兩岸" => Ok(Self {
                id: "cross-strait",
                label: "兩岸協議",
                url: MOJ_CROSS_STRAIT_URL,
                hot_type: "CHINA",
                supports_categories: false,
                notes: "Cross-strait agreements are exposed as a paginated public list. This command returns the first page and reports pagination metadata.",
            }),
            other => Err(TwlawError::InvalidInput(format!(
                "unknown MOJ agreement kind: {other}; expected treaty or cross-strait"
            ))),
        }
    }
}

fn agreement_url(
    kind: AgreementKind,
    keyword: &str,
    category_code: Option<&str>,
) -> TwlawResult<Url> {
    if !keyword.is_empty() {
        let mut url = Url::parse(MOJ_HOT_WORD_URL)?;
        url.query_pairs_mut()
            .append_pair("ty", kind.hot_type)
            .append_pair("kw", keyword);
        return Ok(url);
    }

    let mut url = Url::parse(kind.url)?;
    if let Some(code) = category_code {
        url.query_pairs_mut().append_pair("TY", code);
    }
    Ok(url)
}

fn agreement_categories(document: &Html, source_url: &Url) -> TwlawResult<Vec<Value>> {
    let link_selector = selector("a[href*='TY=']");
    let badge_selector = selector(".badge");
    let mut categories = Vec::new();

    for link in document.select(&link_selector) {
        let count = link
            .select(&badge_selector)
            .next()
            .and_then(|badge| text_of(badge).parse::<usize>().ok());
        if count.is_none() {
            continue;
        }
        let href = link.value().attr("href").unwrap_or("");
        let Some(code) = query_value_from_href(source_url, href, "TY") else {
            continue;
        };
        let mut label = text_of(link);
        if let Some(count) = count {
            let suffix = count.to_string();
            if label.ends_with(&suffix) {
                label.truncate(label.len() - suffix.len());
                label = label.trim().to_string();
            }
        }
        if label.is_empty() {
            continue;
        }
        let url = source_url.join(href)?.to_string();
        categories.push(json!({
            "code": code,
            "label": label,
            "count": count,
            "source_url": url
        }));
    }

    Ok(categories)
}

fn agreement_results(
    document: &Html,
    source_url: &Url,
    selected_category_code: Option<&str>,
    category_lookup: &std::collections::HashMap<String, String>,
    keyword: &str,
) -> TwlawResult<Vec<Value>> {
    let row_selector = selector("table.tab-list tbody tr");
    let cell_selector = selector("td");
    let link_selector = selector("a[href]");
    let mut results = Vec::new();

    for row in document.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() < 2 {
            continue;
        }
        let Some(link) = cells[1].select(&link_selector).next() else {
            continue;
        };
        let title = link
            .value()
            .attr("title")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| text_of(link));
        let href = link.value().attr("href").unwrap_or("");
        let pcode = query_value_from_href(source_url, href, "pcode").unwrap_or_default();
        let source = if pcode.is_empty() {
            source_url.join(href)?.to_string()
        } else {
            format!("https://law.moj.gov.tw/LawClass/LawAll.aspx?pcode={pcode}")
        };
        let date_text = extract_agreement_date_text(&text_of(cells[1]));
        let date = date_text
            .as_deref()
            .and_then(normalize_agreement_date)
            .unwrap_or_default();
        let category_code = selected_category_code.map(str::to_string).or_else(|| {
            query_value_from_href(source_url, href, "cur").and_then(normalize_cur_code)
        });
        let category_label = category_code
            .as_ref()
            .and_then(|code| category_lookup.get(code))
            .cloned();
        let row_number = digits_only(&text_of(cells[0])).parse::<usize>().ok();

        results.push(json!({
            "row_number": row_number,
            "title": title,
            "pcode": pcode,
            "date": date,
            "date_text": date_text.unwrap_or_default(),
            "category_code": category_code,
            "category_label": category_label,
            "source_url": source
        }));
    }

    if results.is_empty() && keyword.is_empty() {
        return Err(TwlawError::ParseChanged(
            "could not parse MOJ agreement result table".to_string(),
        ));
    }

    Ok(results)
}

fn pagination_total(document: &Html) -> Option<usize> {
    pagination_text(document)
        .as_deref()
        .and_then(|text| text.split('筆').next())
        .map(digits_only)
        .and_then(|digits| digits.parse::<usize>().ok())
}

fn pagination_pages(document: &Html) -> Option<usize> {
    let text = pagination_text(document)?;
    let (_, after_slash) = text.split_once('/')?;
    let digits = digits_only(after_slash);
    digits.parse::<usize>().ok()
}

fn pagination_text(document: &Html) -> Option<String> {
    let selector = selector(".pageinfo");
    document
        .select(&selector)
        .map(text_of)
        .find(|text| text.contains('筆') && text.contains('/'))
}

fn first_category_code(results: &[Value]) -> Option<String> {
    results
        .iter()
        .find_map(|value| value.get("category_code")?.as_str().map(str::to_string))
}

fn first_category_code_from_pagination(document: &Html, source_url: &Url) -> Option<String> {
    let selector = selector(".pager a[href*='TY=']");
    document.select(&selector).find_map(|link| {
        query_value_from_href(source_url, link.value().attr("href").unwrap_or(""), "TY")
    })
}

fn query_value_from_href(base_url: &Url, href: &str, key: &str) -> Option<String> {
    base_url.join(href).ok().and_then(|url| {
        url.query_pairs()
            .find(|(query_key, _)| query_key.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.to_string())
    })
}

fn normalize_cur_code(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if let Some(stripped) = trimmed.strip_prefix('C') {
        Some(stripped.to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_agreement_date_text(text: &str) -> Option<String> {
    parenthetical_groups(text, '（', '）')
        .into_iter()
        .chain(parenthetical_groups(text, '(', ')'))
        .rev()
        .find(|value| normalize_agreement_date(value).is_some())
}

fn parenthetical_groups(text: &str, open: char, close: char) -> Vec<String> {
    let mut groups = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after_start = &rest[start + open.len_utf8()..];
        let Some(end) = after_start.find(close) else {
            break;
        };
        let value = after_start[..end].trim();
        if !value.is_empty() {
            groups.push(value.to_string());
        }
        rest = &after_start[end + close.len_utf8()..];
    }
    groups
}

fn normalize_agreement_date(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if is_iso_date(trimmed) {
        return Some(trimmed.to_string());
    }
    if !trimmed.contains("民國") {
        return None;
    }
    let parts = trimmed
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok())
        .collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    Some(format!(
        "{:04}-{:02}-{:02}",
        parts[0] + 1911,
        parts[1],
        parts[2]
    ))
}

fn is_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

impl UpdateKind {
    fn parse(input: Option<&str>) -> TwlawResult<Self> {
        let value = input.unwrap_or("all").trim().to_ascii_lowercase();
        match value.as_str() {
            "" | "all" => Ok(Self {
                id: "all",
                label: "全部",
                query_type: None,
            }),
            "l" | "law" | "laws" => Ok(Self {
                id: "law",
                label: "法律",
                query_type: Some("l"),
            }),
            "m" | "order" | "orders" => Ok(Self {
                id: "order",
                label: "法規命令",
                query_type: Some("m"),
            }),
            "o" | "rule" | "rules" | "administrative-rule" => Ok(Self {
                id: "administrative-rule",
                label: "行政規則",
                query_type: Some("o"),
            }),
            "q" | "local" | "local-law" | "local-laws" => Ok(Self {
                id: "local-law",
                label: "地方法規",
                query_type: Some("q"),
            }),
            "s" | "draft" | "drafts" => Ok(Self {
                id: "draft",
                label: "法規草案",
                query_type: Some("s"),
            }),
            other => Err(TwlawError::InvalidInput(format!(
                "unknown MOJ update kind: {other}; expected all, law, order, rule, local, or draft"
            ))),
        }
    }

    fn url(&self) -> TwlawResult<Url> {
        let mut url = Url::parse(MOJ_NEWS_LIST_URL)?;
        if let Some(kind) = self.query_type {
            url.query_pairs_mut().append_pair("type", kind);
        }
        Ok(url)
    }
}

fn parse_json_with_bom(text: &str) -> TwlawResult<Value> {
    Ok(serde_json::from_str(text.trim_start_matches('\u{feff}'))?)
}

fn laws_array(value: &Value) -> TwlawResult<&[Value]> {
    value
        .get("Laws")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            TwlawError::ParseChanged("MOJ OpenAPI JSON does not contain Laws array".to_string())
        })
}

fn update_date(value: &Value) -> String {
    string_field(value, "UpdateDate")
}

fn cache_json(loaded: &LoadedDataset) -> Value {
    json!({
        "path": loaded.cache_path,
        "was_populated": loaded.cache_was_populated
    })
}

fn field_contains(value: &Value, key: &str, keyword: &str) -> bool {
    let haystack = string_field(value, key).to_lowercase();
    haystack.contains(&keyword.to_lowercase())
}

fn metadata_matches(spec: &DatasetSpec, law: &Value, keyword: &str) -> bool {
    [
        primary_name_key(spec),
        secondary_name_key(spec),
        "LawCategory",
        "LawLevel",
        modified_date_key(spec),
        abandon_note_key(spec),
    ]
    .iter()
    .any(|key| field_contains(law, key, keyword))
}

fn matching_articles(spec: &DatasetSpec, law: &Value, keyword: &str, limit: usize) -> Vec<Value> {
    let keyword_lower = keyword.to_lowercase();
    article_array(spec, law)
        .into_iter()
        .filter(|article| {
            string_field(article, article_no_key(spec))
                .to_lowercase()
                .contains(&keyword_lower)
                || string_field(article, article_content_key(spec))
                    .to_lowercase()
                    .contains(&keyword_lower)
        })
        .take(limit)
        .map(|article| {
            let content = string_field(article, article_content_key(spec));
            json!({
                "article_no": string_field(article, article_no_key(spec)),
                "snippet": snippet(&content, keyword, 120)
            })
        })
        .collect()
}

fn law_summary(spec: &DatasetSpec, law: &Value) -> Value {
    let articles = article_array(spec, law);
    let source_url = string_field(law, source_url_key(spec));
    json!({
        "law_name": string_field(law, "LawName"),
        "primary_name": string_field(law, primary_name_key(spec)),
        "translated_name": string_field(law, secondary_name_key(spec)),
        "pcode": extract_pcode(&source_url),
        "level": string_field(law, "LawLevel"),
        "category": string_field(law, "LawCategory"),
        "modified_date": string_field(law, modified_date_key(spec)),
        "effective_date": string_field(law, "LawEffectiveDate"),
        "abandon_note": string_field(law, abandon_note_key(spec)),
        "has_english_version": string_field(law, "LawHasEngVersion"),
        "article_count": articles.len(),
        "attachment_count": law.get(attachment_key(spec)).and_then(Value::as_array).map_or(0, Vec::len),
        "source_url": source_url
    })
}

fn find_law<'a>(spec: &DatasetSpec, laws: &'a [Value], requested: &str) -> Option<&'a Value> {
    let requested_norm = normalize_name(requested);
    laws.iter()
        .find(|law| {
            normalize_name(&string_field(law, primary_name_key(spec))) == requested_norm
                || normalize_name(&string_field(law, "LawName")) == requested_norm
                || normalize_name(&string_field(law, secondary_name_key(spec))) == requested_norm
        })
        .or_else(|| {
            laws.iter().find(|law| {
                normalize_name(&string_field(law, primary_name_key(spec))).contains(&requested_norm)
                    || normalize_name(&string_field(law, "LawName")).contains(&requested_norm)
                    || normalize_name(&string_field(law, secondary_name_key(spec)))
                        .contains(&requested_norm)
            })
        })
}

fn article_array<'a>(spec: &DatasetSpec, law: &'a Value) -> Vec<&'a Value> {
    law.get(article_array_key(spec))
        .and_then(Value::as_array)
        .map(|values| values.iter().collect())
        .unwrap_or_default()
}

fn article_json(spec: &DatasetSpec, article: &Value) -> Value {
    json!({
        "type": string_field(article, article_type_key(spec)),
        "article_no": string_field(article, article_no_key(spec)),
        "content": string_field(article, article_content_key(spec))
    })
}

fn article_no_matches(spec: &DatasetSpec, article: &Value, requested: &str) -> bool {
    let article_no = string_field(article, article_no_key(spec));
    if article_no.trim() == requested.trim() {
        return true;
    }
    let req_digits = digits_only(requested);
    if req_digits.is_empty() {
        article_no.contains(requested)
    } else {
        digits_only(&article_no) == req_digits
    }
}

fn primary_name_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawName"
    } else {
        "LawName"
    }
}

fn secondary_name_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "LawName"
    } else {
        "EngLawName"
    }
}

fn source_url_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawURL"
    } else {
        "LawURL"
    }
}

fn modified_date_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawModifiedDate"
    } else {
        "LawModifiedDate"
    }
}

fn abandon_note_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawAbandonNote"
    } else {
        "LawAbandonNote"
    }
}

fn attachment_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawAttachements"
    } else {
        "LawAttachements"
    }
}

fn history_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawHistories"
    } else {
        "LawHistories"
    }
}

fn foreword_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawForeword"
    } else {
        "LawForeword"
    }
}

fn article_array_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngLawArticles"
    } else {
        "LawArticles"
    }
}

fn article_type_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngArticleType"
    } else {
        "ArticleType"
    }
}

fn article_no_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngArticleNo"
    } else {
        "ArticleNo"
    }
}

fn article_content_key(spec: &DatasetSpec) -> &'static str {
    if spec.english {
        "EngArticleContent"
    } else {
        "ArticleContent"
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn normalize_name(input: &str) -> String {
    input
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

fn digits_only(input: &str) -> String {
    input.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

fn extract_pcode(source_url: &str) -> String {
    Url::parse(source_url)
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key.eq_ignore_ascii_case("pcode"))
                .map(|(_, value)| value.to_string())
        })
        .unwrap_or_default()
}

fn snippet(text: &str, keyword: &str, context: usize) -> String {
    let lower_text = text.to_lowercase();
    let lower_keyword = keyword.to_lowercase();
    let Some(byte_pos) = lower_text.find(&lower_keyword) else {
        return text.chars().take(context * 2).collect();
    };
    let char_pos = text[..byte_pos].chars().count();
    let start = char_pos.saturating_sub(context);
    let len = keyword.chars().count() + context * 2;
    let body = text.chars().skip(start).take(len).collect::<String>();
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if start + len < text.chars().count() {
        "..."
    } else {
        ""
    };
    format!("{prefix}{body}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_pcode_from_law_url() {
        assert_eq!(
            extract_pcode("https://law.moj.gov.tw/LawClass/LawAll.aspx?pcode=B0000001"),
            "B0000001"
        );
    }

    #[test]
    fn matches_chinese_article_number_by_digits() {
        let spec = dataset_by_id("ch-law").unwrap();
        let article = json!({
            "ArticleType": "A",
            "ArticleNo": "第 184 條",
            "ArticleContent": "因故意或過失，不法侵害他人之權利者，負損害賠償責任。"
        });
        assert!(article_no_matches(spec, &article, "184"));
        assert!(!article_no_matches(spec, &article, "184-1"));
    }

    #[test]
    fn parses_update_kind_aliases() {
        let order = UpdateKind::parse(Some("order")).unwrap();
        assert_eq!(order.id, "order");
        assert_eq!(order.query_type, Some("m"));

        let all = UpdateKind::parse(None).unwrap();
        assert_eq!(all.id, "all");
        assert_eq!(all.query_type, None);
    }

    #[test]
    fn normalizes_moj_agreement_dates() {
        assert_eq!(
            normalize_agreement_date("民國 96 年 01 月 23 日").unwrap(),
            "2007-01-23"
        );
        assert_eq!(
            normalize_agreement_date("2018-11-30").unwrap(),
            "2018-11-30"
        );
    }

    #[test]
    fn extracts_last_date_like_parenthetical_group() {
        let text = "CONVENTION (CEDAW) (民國 68 年 12 月 18 日 )";
        assert_eq!(
            extract_agreement_date_text(text).unwrap(),
            "民國 68 年 12 月 18 日"
        );
    }

    #[test]
    fn normalizes_hot_search_category_codes() {
        assert_eq!(
            normalize_cur_code("CD0100100000000".to_string()).unwrap(),
            "D0100100000000"
        );
    }
}
