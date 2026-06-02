use crate::data::{new_cases, old_cases, pcode_data};
use crate::{retrieved_at, TwlawResult};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct SourceListOptions {
    pub implemented_only: bool,
    pub planned_only: bool,
    pub no_credentials_only: bool,
}

#[derive(Debug, Clone, Copy)]
struct OfficialSource {
    id: &'static str,
    name: &'static str,
    agency: &'static str,
    category: &'static str,
    url: &'static str,
    access_mode: &'static str,
    credentials_required: bool,
    implementation_status: &'static str,
    coverage: &'static str,
    agent_default: bool,
    commands: &'static [&'static str],
    notes: &'static str,
}

const SOURCES: &[OfficialSource] = &[
    OfficialSource {
        id: "moj_laws_current",
        name: "全國法規資料庫中央法規",
        agency: "Ministry of Justice",
        category: "regulations",
        url: "https://law.moj.gov.tw/",
        access_mode: "public_html_and_bundled_metadata",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Law pcode lookup/search and live article fetch are available; full OpenAPI sync is not yet implemented.",
        agent_default: true,
        commands: &[
            "twlaw regulation pcode --law <name> --json",
            "twlaw regulation search --keyword <term> --json",
            "twlaw regulation query --law <name> --article <no> --json",
        ],
        notes: "Use narrow article queries before full-law queries.",
    },
    OfficialSource {
        id: "moj_law_history",
        name: "全國法規資料庫沿革資料",
        agency: "Ministry of Justice",
        category: "regulations",
        url: "https://law.moj.gov.tw/",
        access_mode: "bundled_snapshot",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Bundled law-history strings are available when querying known pcodes.",
        agent_default: true,
        commands: &["twlaw regulation query --law <name> --include-history --json"],
        notes: "Snapshot freshness should be checked with sources status before relying on history completeness.",
    },
    OfficialSource {
        id: "legislative_yuan_law_history",
        name: "立法院法律系統法律沿革與立法理由",
        agency: "Legislative Yuan",
        category: "law_history_and_legislative_reasons",
        url: "https://lis.ly.gov.tw/lglawc/lglawkm",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Law-name search, legislative-history versions, and article-level 條文/理由 pages are available through bounded live queries.",
        agent_default: true,
        commands: &[
            "twlaw legislative history --law <law-name> --json",
            "twlaw legislative history --law <law-name> --include-reasons --json",
            "twlaw legislative history --law <law-name> --date <roc-yyyymmdd> --article <no> --include-reasons --json",
            "twlaw legislative history --law <law-name> --article <no> --include-reasons --all-versions --json",
        ],
        notes: "Use when the user asks for 法律沿革, 立法理由, or Legislative Yuan source URLs. Keep live query concurrency low.",
    },
    OfficialSource {
        id: "moj_openapi_ch_law",
        name: "全國法規資料庫 OpenAPI 中文法規",
        agency: "Ministry of Justice",
        category: "regulations",
        url: "https://law.moj.gov.tw/api/ch/law/json",
        access_mode: "official_openapi_no_token",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "No-token official bulk ZIP can be synced to local cache and searched/fetched through twlaw.",
        agent_default: true,
        commands: &[
            "twlaw moj sync --dataset ch-law --json",
            "twlaw moj search --dataset ch-law --keyword <term> --json",
            "twlaw moj get --dataset ch-law --law <name> --article <no> --json",
        ],
        notes: "Use sync before high-volume work so repeated queries use local cache.",
    },
    OfficialSource {
        id: "moj_openapi_ch_order",
        name: "全國法規資料庫 OpenAPI 中文命令",
        agency: "Ministry of Justice",
        category: "orders",
        url: "https://law.moj.gov.tw/api/ch/order/json",
        access_mode: "official_openapi_no_token",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "No-token official bulk ZIP can be synced to local cache and searched/fetched through twlaw.",
        agent_default: true,
        commands: &[
            "twlaw moj sync --dataset ch-order --json",
            "twlaw moj search --dataset ch-order --keyword <term> --json",
            "twlaw moj get --dataset ch-order --law <name> --article <no> --json",
        ],
        notes: "This expands coverage to orders and administrative rules beyond laws.",
    },
    OfficialSource {
        id: "moj_openapi_en_law",
        name: "全國法規資料庫 OpenAPI 英譯法規",
        agency: "Ministry of Justice",
        category: "english_laws",
        url: "https://law.moj.gov.tw/api/en/law/json",
        access_mode: "official_openapi_no_token",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "No-token official English law ZIP can be synced to local cache and searched/fetched through twlaw.",
        agent_default: true,
        commands: &[
            "twlaw moj sync --dataset en-law --json",
            "twlaw moj search --dataset en-law --keyword <term> --json",
            "twlaw moj get --dataset en-law --law <english-name> --article <no> --json",
        ],
        notes: "Use for English translation research; preserve source URLs because English text can lag Chinese changes.",
    },
    OfficialSource {
        id: "moj_openapi_en_order",
        name: "全國法規資料庫 OpenAPI 英譯命令",
        agency: "Ministry of Justice",
        category: "english_orders",
        url: "https://law.moj.gov.tw/api/en/order/json",
        access_mode: "official_openapi_no_token",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "No-token official English order ZIP can be synced to local cache and searched/fetched through twlaw.",
        agent_default: true,
        commands: &[
            "twlaw moj sync --dataset en-order --json",
            "twlaw moj search --dataset en-order --keyword <term> --json",
            "twlaw moj get --dataset en-order --law <english-name> --article <no> --json",
        ],
        notes: "Use for English order translation research; preserve source URLs and update dates.",
    },
    OfficialSource {
        id: "moj_treaties",
        name: "條約協定",
        agency: "Ministry of Justice",
        category: "treaties",
        url: "https://law.moj.gov.tw/Law/LawSearchAgree.aspx",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Treaty categories, bounded result pages, and public keyword search are available from MOJ pages.",
        agent_default: true,
        commands: &[
            "twlaw moj agreements --kind treaty --include-categories --json",
            "twlaw moj agreements --kind treaty --category-code <code> --json",
            "twlaw moj agreements --kind treaty --keyword <term> --json",
        ],
        notes: "This is a bounded live listing, not a full treaty bulk sync. Use returned pagination metadata and category codes.",
    },
    OfficialSource {
        id: "moj_cross_strait_agreements",
        name: "兩岸協議",
        agency: "Ministry of Justice",
        category: "cross_strait_agreements",
        url: "https://law.moj.gov.tw/Law/LawSearchTwo.aspx",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Cross-strait agreement first-page listings and public keyword search are available from MOJ pages.",
        agent_default: true,
        commands: &[
            "twlaw moj agreements --kind cross-strait --json",
            "twlaw moj agreements --kind cross-strait --keyword <term> --json",
        ],
        notes: "This is a bounded live listing. Use returned pagination metadata before treating it as exhaustive.",
    },
    OfficialSource {
        id: "moj_latest_news",
        name: "全國法規資料庫最新法規訊息",
        agency: "Ministry of Justice",
        category: "legal_updates",
        url: "https://law.moj.gov.tw/",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Recent law/order/rule/local/draft update notices can be fetched from the public MOJ news list.",
        agent_default: true,
        commands: &[
            "twlaw moj updates --kind all --json",
            "twlaw moj updates --kind order --keyword <term> --json",
        ],
        notes: "Use this for freshness checks before relying on cached law/order datasets.",
    },
    OfficialSource {
        id: "official_gazette",
        name: "行政院公報資訊網",
        agency: "Executive Yuan",
        category: "gazette",
        url: "https://gazette.nat.gov.tw/",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Gazette links exposed through MOJ legal update notices are returned by twlaw; full Gazette detail parsing is not yet implemented.",
        agent_default: false,
        commands: &["twlaw moj updates --kind order --json"],
        notes: "Use returned Gazette source URLs as publication-source cross-checks.",
    },
    OfficialSource {
        id: "judicial_public_search",
        name: "司法院裁判書查詢",
        agency: "Judicial Yuan",
        category: "judgments",
        url: "https://judgment.judicial.gov.tw/FJUD/Default_AD.aspx",
        access_mode: "public_html_bounded",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Judgment search and get are implemented through public pages with WAF detection and result caps.",
        agent_default: true,
        commands: &[
            "twlaw judgment search --keyword <term> --max-results <n> --json",
            "twlaw judgment get --jid <jid> --json",
        ],
        notes: "Keep parallelism low for live queries; prefer metadata search before fetching full text.",
    },
    OfficialSource {
        id: "judicial_special_searches",
        name: "司法院裁判書特殊查詢",
        agency: "Judicial Yuan",
        category: "judgments",
        url: "https://judgment.judicial.gov.tw/sitemap.aspx",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "簡易案件, 除權判決, and 公示催告裁定 searches are available through bounded public form searches.",
        agent_default: true,
        commands: &[
            "twlaw judgment special --kind simple --keyword <term> --json",
            "twlaw judgment special --kind declaration --keyword <term> --json",
            "twlaw judgment special --kind public-summons --keyword <term> --json",
        ],
        notes: "Use only for those special search domains. General judgments should still use twlaw judgment search.",
    },
    OfficialSource {
        id: "judicial_jlist_jdoc_api",
        name: "司法院裁判書 JList/JDoc API",
        agency: "Judicial Yuan",
        category: "judgments",
        url: "https://data.judicial.gov.tw/jdg/api/",
        access_mode: "official_api_token_required",
        credentials_required: true,
        implementation_status: "reference_only",
        coverage: "Official change-list and document API requires a Judicial Yuan open-data account, so it is not used by default.",
        agent_default: false,
        commands: &[],
        notes: "Keep optional only; the default plugin must work without an application step.",
    },
    OfficialSource {
        id: "constitutional_bundled",
        name: "憲法法庭解釋與判決快照",
        agency: "Constitutional Court",
        category: "constitutional",
        url: "https://cons.judicial.gov.tw/",
        access_mode: "bundled_snapshot",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Bundled old interpretations and newer constitutional judgments are searchable offline.",
        agent_default: true,
        commands: &[
            "twlaw interpretation search --keyword <term> --json",
            "twlaw interpretation get <case-id> --json",
            "twlaw interpretation citations <case-id> --json",
        ],
        notes: "Fast and parallel-safe, but current-ruling freshness depends on snapshot updates.",
    },
    OfficialSource {
        id: "constitutional_current_judgments",
        name: "憲法法庭最新判決",
        agency: "Constitutional Court",
        category: "constitutional",
        url: "https://cons.judicial.gov.tw/judcurrentNew1.aspx?fid=38",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Live current judgment list is available for freshness checks; full-detail sync still relies on public detail pages or bundled data.",
        agent_default: true,
        commands: &["twlaw interpretation current --json"],
        notes: "Use before claiming the bundled constitutional snapshot is current.",
    },
    OfficialSource {
        id: "constitutional_terminal_cases",
        name: "憲法法庭終結案件查詢",
        agency: "Constitutional Court",
        category: "constitutional",
        url: "https://cons.judicial.gov.tw/judsearch.aspx?fid=46&type=1",
        access_mode: "public_ajax_bounded",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Live terminal-case search covers interpretations, non-acceptance decisions, judgments, substantive rulings, and procedure rulings with bounded pagination.",
        agent_default: true,
        commands: &[
            "twlaw interpretation terminal --kind all --limit 20 --json",
            "twlaw interpretation terminal --kind procedure-ruling --keyword <term> --json",
            "twlaw interpretation terminal --kind non-acceptance --year-from <roc-year> --json",
        ],
        notes: "Use for constitutional coverage beyond bundled merits judgments. Live paging is capped; preserve count/total_pages/truncated.",
    },
    OfficialSource {
        id: "moj_department_retrieval_system",
        name: "法務部主管法規查詢系統",
        agency: "Ministry of Justice",
        category: "administrative_interpretations",
        url: "https://mojlaw.moj.gov.tw/LawQuery.aspx",
        access_mode: "public_html",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Administrative interpretations, legal consultation opinions, legal issue seminars, objection decisions, Constitutional Court/Judicial Yuan references, and precedent materials are searchable through bounded public result pages.",
        agent_default: true,
        commands: &[
            "twlaw mojlaw search --kind admin-interpretation --keyword <term> --json",
            "twlaw mojlaw search --kind legal-consultation --keyword <term> --json",
            "twlaw mojlaw search --kind objection --keyword <term> --json",
        ],
        notes: "Use for MOJ department legal-reference materials. Use MOJ OpenAPI commands for current central law/order text.",
    },
    OfficialSource {
        id: "local_law_portals",
        name: "地方政府法規資料",
        agency: "Local governments",
        category: "local_laws",
        url: "https://law.moj.gov.tw/",
        access_mode: "public_html_and_linked_sources",
        credentials_required: false,
        implementation_status: "planned_medium",
        coverage: "Local-law update notices are visible through official sources, but local-law text is not indexed as a first-class domain.",
        agent_default: false,
        commands: &[],
        notes: "Start with MOJ-linked local-law updates, then add source-specific adapters only where stable.",
    },
    OfficialSource {
        id: "data_gov_catalog_legal",
        name: "政府資料開放平臺法律相關資料集",
        agency: "National Development Council / source agencies",
        category: "open_data_catalog",
        url: "https://data.gov.tw/api/v2/rest/dataset/export",
        access_mode: "public_catalog_no_token",
        credentials_required: false,
        implementation_status: "implemented_partial",
        coverage: "Legal-related data.gov.tw datasets are discoverable from the official no-token catalog export with local CSV caching.",
        agent_default: true,
        commands: &[
            "twlaw open-data legal-catalog --json",
            "twlaw open-data legal-catalog --keyword <term> --json",
            "twlaw open-data legal-catalog --agency <agency> --json",
        ],
        notes: "Use as discovery metadata; each dataset still needs source-specific adapter, licensing, and freshness checks before content reuse.",
    },
];

pub fn list_sources(options: SourceListOptions) -> TwlawResult<Value> {
    let sources = filtered_sources(&options)
        .into_iter()
        .map(|source| source_json(&source))
        .collect::<Vec<_>>();
    let source_count = sources.len();

    Ok(json!({
        "success": true,
        "filters": {
            "implemented_only": options.implemented_only,
            "planned_only": options.planned_only,
            "no_credentials_only": options.no_credentials_only
        },
        "sources": sources,
        "source_count": source_count,
        "retrieved_at": retrieved_at()
    }))
}

pub fn sources_status() -> TwlawResult<Value> {
    let pcode = pcode_data()?;
    let old = old_cases()?;
    let new = new_cases()?;
    let implemented = SOURCES
        .iter()
        .filter(|source| source.implementation_status.starts_with("implemented"))
        .count();
    let no_credentials = SOURCES
        .iter()
        .filter(|source| !source.credentials_required)
        .count();
    let agent_defaults = SOURCES.iter().filter(|source| source.agent_default).count();

    Ok(json!({
        "success": true,
        "goal": {
            "agent_friendly": true,
            "requires_external_api_application_by_default": false,
            "coverage_strategy": "Prefer official no-token bulk/open pages synced locally; keep token-required APIs optional and never part of the default agent path."
        },
        "local_snapshot": {
            "law_names": pcode.pcode_map.len(),
            "abolished_laws": pcode.abolished.len(),
            "law_histories": pcode.histories.len(),
            "old_constitutional_interpretations": old.len(),
            "new_constitutional_cases": new.len()
        },
        "coverage": {
            "official_sources_tracked": SOURCES.len(),
            "implemented_sources": implemented,
            "no_credential_sources": no_credentials,
            "agent_default_sources": agent_defaults
        },
        "robustness": {
            "stateless_cli": true,
            "json_stdout_contract": true,
            "bounded_live_judgment_results": true,
            "waf_detection": true,
            "live_http_retry_backoff": true,
            "recommended_parallelism": {
                "offline_snapshot_queries": "safe for high parallelism",
                "live_government_http_queries": "keep per-host concurrency low and prefer sync/cache before repeated queries"
            }
        },
        "next_highest_value_sources": [
            "local_law_portals"
        ],
        "sources": SOURCES.iter().map(source_json).collect::<Vec<_>>(),
        "retrieved_at": retrieved_at()
    }))
}

pub fn coverage_gaps() -> TwlawResult<Value> {
    let gaps = SOURCES
        .iter()
        .filter(|source| !source.implementation_status.starts_with("implemented"))
        .map(|source| {
            json!({
                "id": source.id,
                "name": source.name,
                "agency": source.agency,
                "category": source.category,
                "priority": priority(source.implementation_status),
                "credentials_required": source.credentials_required,
                "recommended_default": !source.credentials_required && source.implementation_status != "reference_only",
                "coverage_gap": source.coverage,
                "next_step": next_step(source),
                "url": source.url
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "success": true,
        "gaps": gaps,
        "gap_count": gaps.len(),
        "policy": "Do not make token-required sources mandatory. For high-volume use, sync no-token official sources to a local index first.",
        "retrieved_at": retrieved_at()
    }))
}

pub fn agent_guide() -> TwlawResult<Value> {
    Ok(json!({
        "success": true,
        "contract": {
            "interface": "CLI",
            "json_flag": "--json",
            "stdout": "Always JSON on success and error.",
            "external_api_application_required": false
        },
        "first_calls": [
            "twlaw sources status --json",
            "twlaw sources gaps --json"
        ],
        "workflows": [
            {
                "task": "Known law article",
                "commands": [
                    "twlaw regulation pcode --law <law-name> --json",
                    "twlaw moj get --dataset ch-law --law <law-name> --article <article-no> --json",
                    "twlaw regulation query --law <law-name> --article <article-no> --json"
                ]
            },
            {
                "task": "Law history and legislative reasons",
                "commands": [
                    "twlaw legislative history --law <law-name> --json",
                    "twlaw legislative history --law <law-name> --include-reasons --json",
                    "twlaw legislative history --law <law-name> --date <roc-yyyymmdd> --article <article-no> --include-reasons --json",
                    "twlaw legislative history --law <law-name> --article <article-no> --include-reasons --all-versions --json"
                ]
            },
            {
                "task": "Bulk MOJ law/order research",
                "commands": [
                    "twlaw moj datasets --json",
                    "twlaw moj sync --dataset ch-law --json",
                    "twlaw moj search --dataset ch-order --keyword <terms> --include-articles --limit 20 --json"
                ]
            },
            {
                "task": "Recent legal updates",
                "commands": [
                    "twlaw moj updates --kind all --limit 20 --json",
                    "twlaw moj updates --kind order --keyword <terms> --json"
                ]
            },
            {
                "task": "Treaties and cross-strait agreements",
                "commands": [
                    "twlaw moj agreements --kind treaty --include-categories --json",
                    "twlaw moj agreements --kind treaty --keyword <terms> --json",
                    "twlaw moj agreements --kind treaty --category-code <code> --limit 20 --json",
                    "twlaw moj agreements --kind cross-strait --keyword <terms> --json"
                ]
            },
            {
                "task": "MOJ administrative interpretations and legal-reference materials",
                "commands": [
                    "twlaw mojlaw search --kind admin-interpretation --keyword <terms> --limit 10 --json",
                    "twlaw mojlaw search --kind legal-consultation --keyword <terms> --json",
                    "twlaw mojlaw search --kind legal-seminar --keyword <terms> --json"
                ]
            },
            {
                "task": "Judgment research",
                "commands": [
                    "twlaw judgment search --keyword <terms> --case-type <民事|刑事|行政|懲戒> --max-results 10 --json",
                    "twlaw judgment special --kind simple --keyword <terms> --max-results 10 --json",
                    "twlaw judgment get --jid <jid> --json"
                ]
            },
            {
                "task": "Constitutional research",
                "commands": [
                    "twlaw interpretation current --limit 10 --json",
                    "twlaw interpretation terminal --kind procedure-ruling --limit 10 --json",
                    "twlaw interpretation search --keyword <terms> --limit 10 --json",
                    "twlaw interpretation get <case-id> --reasoning-keyword <term> --json"
                ]
            },
            {
                "task": "Coverage audit",
                "commands": [
                    "twlaw sources list --no-credentials --json",
                    "twlaw sources gaps --json"
                ]
            },
            {
                "task": "Find additional official legal open-data datasets",
                "commands": [
                    "twlaw open-data legal-catalog --limit 30 --json",
                    "twlaw open-data legal-catalog --keyword <terms> --json",
                    "twlaw open-data legal-catalog --agency <agency> --json"
                ]
            }
        ],
        "selection_rules": [
            "Use source status before claiming exhaustive coverage.",
            "Prefer no-credential official sources and bundled snapshots.",
            "Fetch long text only after narrowing with metadata search.",
            "Preserve source_url, cached, and retrieved_at in citations.",
            "For repeated or parallel work, sync/index first instead of repeatedly hitting government pages."
        ],
        "retrieved_at": retrieved_at()
    }))
}

fn filtered_sources(options: &SourceListOptions) -> Vec<OfficialSource> {
    SOURCES
        .iter()
        .copied()
        .filter(|source| {
            (!options.implemented_only || source.implementation_status.starts_with("implemented"))
                && (!options.planned_only || source.implementation_status.starts_with("planned"))
                && (!options.no_credentials_only || !source.credentials_required)
        })
        .collect()
}

fn source_json(source: &OfficialSource) -> Value {
    json!({
        "id": source.id,
        "name": source.name,
        "agency": source.agency,
        "category": source.category,
        "url": source.url,
        "access_mode": source.access_mode,
        "credentials_required": source.credentials_required,
        "implementation_status": source.implementation_status,
        "coverage": source.coverage,
        "agent_default": source.agent_default,
        "commands": source.commands,
        "notes": source.notes
    })
}

fn priority(status: &str) -> &'static str {
    if status.ends_with("_high") {
        "high"
    } else if status.ends_with("_medium") {
        "medium"
    } else if status.ends_with("_low") {
        "low"
    } else {
        "reference"
    }
}

fn next_step(source: &OfficialSource) -> &'static str {
    match source.id {
        "moj_openapi_ch_law" | "moj_openapi_ch_order" => {
            "Add no-token OpenAPI sync into a local SQLite/FTS index."
        }
        "constitutional_terminal_cases" => {
            "Add a Constitutional Court sync/freshness check before relying on bundled snapshots."
        }
        "moj_latest_news" => "Track legal update notices and expose freshness warnings in status output.",
        "judicial_special_searches" => {
            "Add separate commands for simple cases, public summons, and declaration-of-right searches."
        }
        "judicial_jlist_jdoc_api" => {
            "Keep optional only because it requires a Judicial Yuan account/token."
        }
        _ => "Add a source adapter, local cache, source URL, and freshness metadata.",
    }
}
