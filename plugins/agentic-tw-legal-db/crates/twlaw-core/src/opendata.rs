use crate::{retrieved_at, TwlawError, TwlawResult};
use csv::{StringRecord, Trim};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::thread::sleep;
use std::time::Duration;

const USER_AGENT: &str = "twlaw/0.1";
const DATA_GOV_DATASET_EXPORT_URL: &str = "https://data.gov.tw/api/v2/rest/dataset/export";
const RETRY_ATTEMPTS: usize = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;

const LEGAL_TERMS: &[&str] = &[
    "法規",
    "法律",
    "法制",
    "法務",
    "司法",
    "司法院",
    "裁判",
    "判決",
    "判例",
    "憲法",
    "憲法法庭",
    "法院",
    "行政法院",
    "地方法院",
    "高等法院",
    "最高法院",
    "智慧財產",
    "懲戒",
    "檢察",
    "檢察署",
    "矯正",
    "監獄",
    "看守所",
    "公證",
    "調解",
    "仲裁",
    "訴願",
    "訴訟",
    "刑事",
    "民事",
    "家事",
    "少年事件",
];

const LEGAL_AGENCY_TERMS: &[&str] = &[
    "法務部",
    "司法院",
    "最高法院",
    "最高行政法院",
    "臺灣高等法院",
    "高等行政法院",
    "智慧財產及商業法院",
    "懲戒法院",
    "最高檢察署",
    "臺灣高等檢察署",
    "監察院",
];

#[derive(Debug, Clone, Default)]
pub struct OpenDataLegalCatalogQuery {
    pub keyword: Option<String>,
    pub agency: Option<String>,
    pub limit: usize,
    pub cache_dir: Option<PathBuf>,
    pub refresh: bool,
}

struct LoadedCatalog {
    bytes: Vec<u8>,
    cache_path: PathBuf,
    metadata: Value,
    cache_was_populated: bool,
}

#[derive(Debug, Clone, Default)]
struct CatalogRow {
    dataset_id: String,
    title: String,
    service_category: String,
    formats: String,
    download_urls: String,
    description: String,
    fields: String,
    agency: String,
    update_frequency: String,
    license: String,
    related_url: String,
    charge: String,
    published_at: String,
    metadata_updated_at: String,
    notes: String,
    row_count: String,
}

pub fn open_data_legal_catalog(query: OpenDataLegalCatalogQuery) -> TwlawResult<Value> {
    let limit = query.limit.clamp(1, 200);
    let keyword = query.keyword.unwrap_or_default().trim().to_string();
    let agency_filter = query.agency.unwrap_or_default().trim().to_string();
    let cache_dir = resolve_cache_dir(query.cache_dir)?;
    let loaded = load_catalog(&cache_dir, query.refresh)?;
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(Trim::All)
        .from_reader(loaded.bytes.as_slice());
    let headers = reader.headers()?.clone();
    let mut total_catalog_rows = 0usize;
    let mut results = Vec::new();

    for record in reader.records() {
        total_catalog_rows += 1;
        let record = record?;
        let row = catalog_row(&headers, &record);
        let match_reasons = legal_match_reasons(&row);
        if match_reasons.is_empty() {
            continue;
        }
        if !keyword.is_empty() && !contains_folded(&row.search_text(), &keyword) {
            continue;
        }
        if !agency_filter.is_empty() && !contains_folded(&row.agency, &agency_filter) {
            continue;
        }
        results.push(catalog_row_json(&row, match_reasons));
    }

    if total_catalog_rows == 0 {
        return Err(TwlawError::ParseChanged(
            "data.gov.tw dataset export returned no catalog rows".to_string(),
        ));
    }

    let count = results.len();
    let truncated = count > limit;
    results.truncate(limit);

    Ok(json!({
        "success": true,
        "source": {
            "name": "政府資料開放平臺資料集清單",
            "url": DATA_GOV_DATASET_EXPORT_URL,
            "credentials_required": false,
            "license_note": "Dataset metadata is discovery material; inspect each returned dataset's license and source URL before reuse."
        },
        "query": {
            "keyword": keyword,
            "agency": agency_filter,
            "legal_terms": LEGAL_TERMS,
            "legal_agency_terms": LEGAL_AGENCY_TERMS
        },
        "count": count,
        "returned_count": results.len(),
        "truncated": truncated,
        "total_catalog_rows": total_catalog_rows,
        "results": results,
        "cache": {
            "path": loaded.cache_path,
            "metadata": loaded.metadata,
            "cache_was_populated": loaded.cache_was_populated
        },
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
            return Ok(PathBuf::from(path).join("open-data"));
        }
    }
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join(".cache/twlaw/open-data"));
        }
    }
    Ok(env::temp_dir().join("twlaw/open-data"))
}

fn load_catalog(cache_dir: &Path, refresh: bool) -> TwlawResult<LoadedCatalog> {
    fs::create_dir_all(cache_dir)?;
    let cache_path = cache_dir.join("data-gov-dataset-export.csv");
    let metadata_path = cache_dir.join("data-gov-dataset-export.metadata.json");

    if cache_path.exists() && !refresh {
        let bytes = fs::read(&cache_path)?;
        return Ok(LoadedCatalog {
            bytes,
            cache_path,
            metadata: read_metadata(&metadata_path)?,
            cache_was_populated: false,
        });
    }

    let bytes = fetch_bytes(&client()?, DATA_GOV_DATASET_EXPORT_URL)?;
    write_atomic(&cache_path, &bytes)?;
    let metadata = json!({
        "source_url": DATA_GOV_DATASET_EXPORT_URL,
        "downloaded_at": retrieved_at(),
        "size_bytes": bytes.len()
    });
    write_atomic(&metadata_path, &serde_json::to_vec_pretty(&metadata)?)?;

    Ok(LoadedCatalog {
        bytes,
        cache_path,
        metadata,
        cache_was_populated: true,
    })
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

fn read_metadata(path: &Path) -> TwlawResult<Value> {
    if !path.exists() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn fetch_bytes(client: &Client, url: &str) -> TwlawResult<Vec<u8>> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match client.get(url).send() {
            Ok(response) => match response.error_for_status() {
                Ok(ok) => match ok.bytes() {
                    Ok(bytes) => return Ok(bytes.to_vec()),
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

fn catalog_row(headers: &StringRecord, record: &StringRecord) -> CatalogRow {
    CatalogRow {
        dataset_id: field(headers, record, &["資料集識別碼"]),
        title: field(headers, record, &["資料集名稱"]),
        service_category: field(headers, record, &["服務分類"]),
        formats: field(headers, record, &["檔案格式"]),
        download_urls: field(headers, record, &["資料下載網址"]),
        description: field(headers, record, &["資料集描述"]),
        fields: field(headers, record, &["主要欄位說明"]),
        agency: field(headers, record, &["提供機關"]),
        update_frequency: field(headers, record, &["更新頻率"]),
        license: field(headers, record, &["授權方式"]),
        related_url: field(headers, record, &["相關網址"]),
        charge: field(headers, record, &["計費方式"]),
        published_at: field(headers, record, &["上架日期"]),
        metadata_updated_at: field(headers, record, &["詮釋資料更新時間"]),
        notes: field(headers, record, &["備註"]),
        row_count: field(headers, record, &["資料量"]),
    }
}

fn field(headers: &StringRecord, record: &StringRecord, names: &[&str]) -> String {
    headers
        .iter()
        .position(|header| {
            let header = header.trim_start_matches('\u{feff}').trim();
            names.iter().any(|name| header == *name)
        })
        .and_then(|index| record.get(index))
        .unwrap_or("")
        .trim()
        .to_string()
}

fn legal_match_reasons(row: &CatalogRow) -> Vec<String> {
    let mut reasons = Vec::new();
    for term in LEGAL_AGENCY_TERMS {
        if contains_folded(&row.agency, term) {
            reasons.push(format!("agency:{term}"));
        }
    }
    let text = row.search_text();
    for term in LEGAL_TERMS {
        if contains_folded(&text, term) {
            reasons.push(format!("term:{term}"));
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn catalog_row_json(row: &CatalogRow, match_reasons: Vec<String>) -> Value {
    json!({
        "dataset_id": row.dataset_id,
        "title": row.title,
        "agency": row.agency,
        "service_category": row.service_category,
        "formats": split_multi_value(&row.formats),
        "download_urls": split_multi_value(&row.download_urls),
        "dataset_page_url": if row.dataset_id.is_empty() {
            Value::Null
        } else {
            json!(format!("https://data.gov.tw/dataset/{}", row.dataset_id))
        },
        "related_url": empty_to_null(&row.related_url),
        "description": row.description,
        "fields": row.fields,
        "update_frequency": row.update_frequency,
        "license": row.license,
        "charge": row.charge,
        "published_at": row.published_at,
        "metadata_updated_at": row.metadata_updated_at,
        "notes": row.notes,
        "row_count": row.row_count,
        "match_reasons": match_reasons
    })
}

fn split_multi_value(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn empty_to_null(value: &str) -> Value {
    if value.trim().is_empty() {
        Value::Null
    } else {
        json!(value.trim())
    }
}

fn contains_folded(haystack: &str, needle: &str) -> bool {
    haystack
        .to_lowercase()
        .contains(&needle.trim().to_lowercase())
}

impl CatalogRow {
    fn search_text(&self) -> String {
        [
            self.dataset_id.as_str(),
            self.title.as_str(),
            self.service_category.as_str(),
            self.formats.as_str(),
            self.download_urls.as_str(),
            self.description.as_str(),
            self.fields.as_str(),
            self.agency.as_str(),
            self.related_url.as_str(),
            self.notes.as_str(),
        ]
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bom_header_field() {
        let headers = StringRecord::from(vec!["\u{feff}資料集識別碼", "資料集名稱"]);
        let record = StringRecord::from(vec!["172819", "法務部Open Data資料集清單"]);

        assert_eq!(field(&headers, &record, &["資料集識別碼"]), "172819");
    }

    #[test]
    fn detects_legal_catalog_matches() {
        let row = CatalogRow {
            title: "裁判書資料集".to_string(),
            agency: "司法院".to_string(),
            description: "判決資料".to_string(),
            ..CatalogRow::default()
        };
        let reasons = legal_match_reasons(&row);

        assert!(reasons.iter().any(|reason| reason == "agency:司法院"));
        assert!(reasons.iter().any(|reason| reason == "term:裁判"));
    }

    #[test]
    fn splits_semicolon_values() {
        assert_eq!(
            split_multi_value("CSV;JSON; "),
            vec!["CSV".to_string(), "JSON".to_string()]
        );
    }
}
