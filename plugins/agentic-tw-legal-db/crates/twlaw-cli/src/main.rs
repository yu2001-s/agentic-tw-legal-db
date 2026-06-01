use clap::{Args, Parser, Subcommand};
use twlaw_core::constitutional::{
    current_judgments, get_citations, get_interpretation, search_interpretations, terminal_cases,
    CurrentJudgmentsQuery, InterpretationQuery, InterpretationSearch, TerminalCasesQuery,
};
use twlaw_core::judicial::{
    get_judgment, search_judgments, search_special_judgments, JudgmentGet, JudgmentSearch,
    JudgmentSpecialSearch,
};
use twlaw_core::moj_openapi::{
    moj_agreements, moj_datasets, moj_get, moj_search, moj_status, moj_sync, moj_updates,
    MojAgreementsQuery, MojDatasetQuery, MojGetQuery, MojSearchQuery, MojSyncQuery,
    MojUpdatesQuery,
};
use twlaw_core::mojlaw::{mojlaw_search, MojlawSearchQuery};
use twlaw_core::opendata::{open_data_legal_catalog, OpenDataLegalCatalogQuery};
use twlaw_core::regulations::{
    get_pcode, query_regulation, search_regulations, RegulationQuery, RegulationSearch,
};
use twlaw_core::sources::{
    agent_guide, coverage_gaps, list_sources, sources_status, SourceListOptions,
};

#[derive(Debug, Parser)]
#[command(name = "twlaw")]
#[command(version, about = "CLI-native Taiwan legal database lookup tool")]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Emit stable machine-readable JSON. JSON is the default and only output contract."
    )]
    json: bool,
    #[arg(long, global = true, help = "Pretty-print JSON output.")]
    pretty: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(
        about = "Query Ministry of Justice laws and regulation metadata.",
        alias = "reg"
    )]
    Regulation(RegulationCommand),
    #[command(about = "Search and fetch Judicial Yuan judgments.", alias = "jud")]
    Judgment(JudgmentCommand),
    #[command(
        about = "Search Constitutional Court current judgments and bundled interpretations or rulings.",
        alias = "interp"
    )]
    Interpretation(InterpretationCommand),
    #[command(about = "Inspect official Taiwan legal data source coverage and gaps.")]
    Sources(SourcesCommand),
    #[command(about = "Return agent-oriented command workflows and usage rules.")]
    Agent(AgentCommand),
    #[command(about = "Use Ministry of Justice no-token OpenAPI ZIP datasets.")]
    Moj(MojCommand),
    #[command(
        name = "mojlaw",
        alias = "moj-law",
        about = "Search the Ministry of Justice department law retrieval system."
    )]
    Mojlaw(MojlawCommand),
    #[command(
        name = "open-data",
        alias = "opendata",
        alias = "data-gov",
        about = "Discover legal-related datasets from data.gov.tw without credentials."
    )]
    OpenData(OpenDataCommand),
}

#[derive(Debug, Args)]
struct RegulationCommand {
    #[command(subcommand)]
    command: RegulationSubcommand,
}

#[derive(Debug, Subcommand)]
enum RegulationSubcommand {
    #[command(about = "Resolve a law name or common alias to its Ministry of Justice pcode.")]
    Pcode {
        #[arg(long, help = "Law name or common alias, for example 民法 or 勞基法.")]
        law: String,
    },
    #[command(about = "Search bundled Ministry of Justice law-name metadata.")]
    Search {
        #[arg(long, help = "Keyword to match against law names.")]
        keyword: String,
        #[arg(long, default_value_t = 0, help = "Zero-based result offset.")]
        offset: usize,
        #[arg(
            long,
            default_value_t = 50,
            help = "Maximum number of results to return."
        )]
        limit: usize,
        #[arg(long, help = "Hide abolished laws from search results.")]
        exclude_abolished: bool,
    },
    #[command(about = "Fetch law articles or a full law from the official law.moj.gov.tw pages.")]
    Query {
        #[arg(long, help = "Law name or common alias. Use either --law or --pcode.")]
        law: Option<String>,
        #[arg(long, help = "Ministry of Justice pcode. Use either --law or --pcode.")]
        pcode: Option<String>,
        #[arg(long, help = "Single article number to fetch.")]
        article: Option<String>,
        #[arg(long, help = "First article number for a range query.")]
        from: Option<String>,
        #[arg(long, help = "Last article number for a range query.")]
        to: Option<String>,
        #[arg(long, help = "Include bundled law-history text when available.")]
        include_history: bool,
    },
}

#[derive(Debug, Args)]
struct JudgmentCommand {
    #[command(subcommand)]
    command: JudgmentSubcommand,
}

#[derive(Debug, Subcommand)]
enum JudgmentSubcommand {
    #[command(about = "Search public Judicial Yuan judgment pages with bounded result caps.")]
    Search {
        #[arg(long, help = "Full-text keyword query.")]
        keyword: Option<String>,
        #[arg(long, help = "Court name or court code, for example 最高法院.")]
        court: Option<String>,
        #[arg(
            long,
            help = "Case type: 民事, 刑事, 行政, 懲戒, or a raw Judicial Yuan code."
        )]
        case_type: Option<String>,
        #[arg(long, help = "Lower ROC year bound.")]
        year_from: Option<u32>,
        #[arg(long, help = "Upper ROC year bound.")]
        year_to: Option<u32>,
        #[arg(long, help = "Case word, for example 台上.")]
        case_word: Option<String>,
        #[arg(long, help = "Case number.")]
        case_number: Option<String>,
        #[arg(long, help = "Main-text search term.")]
        main_text: Option<String>,
        #[arg(
            long,
            default_value_t = 10,
            help = "Maximum results to return; core caps very large requests."
        )]
        max_results: usize,
    },
    #[command(about = "Fetch a specific public Judicial Yuan judgment by jid or source URL.")]
    Get {
        #[arg(long, help = "Judicial Yuan judgment id returned by search.")]
        jid: Option<String>,
        #[arg(long, help = "Official judgment source URL.")]
        url: Option<String>,
    },
    #[command(about = "Search Judicial Yuan special public judgment pages.")]
    Special {
        #[arg(
            long,
            help = "Special search kind: simple, declaration, or public-summons."
        )]
        kind: Option<String>,
        #[arg(long, help = "Full-text keyword query.")]
        keyword: Option<String>,
        #[arg(long, help = "Court name or raw special-court code.")]
        court: Option<String>,
        #[arg(long, help = "ROC case year.")]
        year: Option<u32>,
        #[arg(long, help = "Case word.")]
        case_word: Option<String>,
        #[arg(long, help = "Case number.")]
        case_number: Option<String>,
        #[arg(
            long,
            default_value_t = 10,
            help = "Maximum results to return; core caps very large requests."
        )]
        max_results: usize,
    },
}

#[derive(Debug, Args)]
struct InterpretationCommand {
    #[command(subcommand)]
    command: InterpretationSubcommand,
}

#[derive(Debug, Subcommand)]
enum InterpretationSubcommand {
    #[command(about = "Fetch a bundled Constitutional Court interpretation or ruling.")]
    Get {
        #[arg(help = "Case id, for example 釋字748 or 111年憲判字第1號.")]
        case_id: String,
        #[arg(long, help = "Include full reasoning text when available.")]
        include_reasoning: bool,
        #[arg(
            long,
            help = "Return reasoning snippets around this keyword instead of full reasoning."
        )]
        reasoning_keyword: Option<String>,
        #[arg(long, help = "Include separate opinions when available.")]
        include_opinions: bool,
        #[arg(
            long,
            help = "Return opinion snippets around this keyword instead of full opinions."
        )]
        opinions_keyword: Option<String>,
    },
    #[command(about = "Search bundled Constitutional Court interpretations and newer cases.")]
    Search {
        #[arg(
            long,
            help = "Keyword to search titles, issues, summaries, and reasoning."
        )]
        keyword: Option<String>,
        #[arg(long, help = "ROC year filter.")]
        year: Option<u32>,
        #[arg(long, help = "Lower number bound.")]
        number_from: Option<u32>,
        #[arg(long, help = "Upper number bound.")]
        number_to: Option<u32>,
        #[arg(
            long,
            action = clap::ArgAction::Set,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            help = "Include old 釋字 interpretations. Accepts --include-old=false."
        )]
        include_old: bool,
        #[arg(
            long,
            action = clap::ArgAction::Set,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            help = "Include newer constitutional cases. Accepts --include-new=false."
        )]
        include_new: bool,
        #[arg(long, help = "Exclude old 釋字 interpretations.")]
        no_old: bool,
        #[arg(long, help = "Exclude newer constitutional cases.")]
        no_new: bool,
        #[arg(long, default_value_t = 30, help = "Maximum results to return.")]
        limit: usize,
    },
    #[command(about = "Find citations for a bundled Constitutional Court case.")]
    Citations {
        #[arg(help = "Case id, for example 釋字748 or 111年憲判字第1號.")]
        case_id: String,
        #[arg(long, help = "Include matched citation context snippets.")]
        include_context: bool,
    },
    #[command(about = "Fetch the live public Constitutional Court current judgment list.")]
    Current {
        #[arg(long, help = "ROC year filter, for example 115.")]
        year: Option<u32>,
        #[arg(long, default_value_t = 20, help = "Maximum results to return.")]
        limit: usize,
    },
    #[command(about = "Search live public Constitutional Court terminal cases.")]
    Terminal {
        #[arg(long, help = "Keyword to search across terminal case fields.")]
        keyword: Option<String>,
        #[arg(
            long,
            help = "Case kind: all, interpretation, non-acceptance, judgment, substantive-ruling, or procedure-ruling."
        )]
        kind: Option<String>,
        #[arg(long, help = "Lower ROC decision-year bound.")]
        year_from: Option<u32>,
        #[arg(long, help = "Upper ROC decision-year bound.")]
        year_to: Option<u32>,
        #[arg(long, default_value_t = 20, help = "Maximum results to return.")]
        limit: usize,
    },
}

#[derive(Debug, Args)]
struct SourcesCommand {
    #[command(subcommand)]
    command: SourcesSubcommand,
}

#[derive(Debug, Subcommand)]
enum SourcesSubcommand {
    #[command(
        about = "Show implemented coverage, no-credential policy, and local snapshot counts."
    )]
    Status,
    #[command(about = "List tracked official legal data sources.")]
    List {
        #[arg(long, help = "Only show sources with implemented command coverage.")]
        implemented: bool,
        #[arg(long, help = "Only show planned sources.")]
        planned: bool,
        #[arg(
            long,
            help = "Only show sources that do not require an external API application."
        )]
        no_credentials: bool,
    },
    #[command(about = "Show missing official-source coverage and recommended next steps.")]
    Gaps,
}

#[derive(Debug, Args)]
struct AgentCommand {
    #[command(subcommand)]
    command: AgentSubcommand,
}

#[derive(Debug, Subcommand)]
enum AgentSubcommand {
    #[command(about = "Return machine-readable usage rules and recommended workflows for agents.")]
    Guide,
}

#[derive(Debug, Args)]
struct MojCommand {
    #[command(subcommand)]
    command: MojSubcommand,
}

#[derive(Debug, Args)]
struct MojlawCommand {
    #[command(subcommand)]
    command: MojlawSubcommand,
}

#[derive(Debug, Subcommand)]
enum MojlawSubcommand {
    #[command(
        about = "Search MOJ administrative interpretations and related legal-reference materials."
    )]
    Search {
        #[arg(
            long,
            help = "Kind: admin-interpretation, legal-consultation, legal-seminar, objection, constitutional-judgment, grand-justice, or precedent."
        )]
        kind: Option<String>,
        #[arg(long, help = "Keyword to search.")]
        keyword: String,
        #[arg(long, default_value_t = 10, help = "Maximum results to return.")]
        limit: usize,
    },
}

#[derive(Debug, Args)]
struct OpenDataCommand {
    #[command(subcommand)]
    command: OpenDataSubcommand,
}

#[derive(Debug, Subcommand)]
enum OpenDataSubcommand {
    #[command(about = "Search the official data.gov.tw catalog for legal-related datasets.")]
    LegalCatalog {
        #[arg(long, help = "Filter legal-related catalog rows by keyword.")]
        keyword: Option<String>,
        #[arg(long, help = "Filter legal-related catalog rows by providing agency.")]
        agency: Option<String>,
        #[arg(long, default_value_t = 30, help = "Maximum results to return.")]
        limit: usize,
        #[arg(long, help = "Directory for cached data.gov.tw catalog CSV.")]
        cache_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Redownload the data.gov.tw catalog before searching.")]
        refresh: bool,
    },
}

#[derive(Debug, Subcommand)]
enum MojSubcommand {
    #[command(about = "List supported no-token Ministry of Justice OpenAPI datasets.")]
    Datasets,
    #[command(about = "Show local cache status, optionally checking remote ZIP metadata.")]
    Status {
        #[arg(long, help = "Dataset id: all, ch-law, ch-order, en-law, or en-order.")]
        dataset: Option<String>,
        #[arg(long, help = "Directory for cached extracted JSON files.")]
        cache_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Check remote endpoint headers with HTTP HEAD.")]
        remote: bool,
    },
    #[command(about = "Download and cache extracted MOJ OpenAPI JSON for offline reuse.")]
    Sync {
        #[arg(long, help = "Dataset id: all, ch-law, ch-order, en-law, or en-order.")]
        dataset: Option<String>,
        #[arg(long, help = "Directory for cached extracted JSON files.")]
        cache_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Redownload even when the dataset is already cached.")]
        force: bool,
    },
    #[command(about = "Search cached MOJ OpenAPI laws/orders, auto-syncing if missing.")]
    Search {
        #[arg(
            long,
            default_value = "ch-law",
            help = "Dataset id: ch-law, ch-order, en-law, or en-order."
        )]
        dataset: String,
        #[arg(long, help = "Keyword to search.")]
        keyword: String,
        #[arg(
            long,
            help = "Also search article text and return short article snippets."
        )]
        include_articles: bool,
        #[arg(long, default_value_t = 20, help = "Maximum results to return.")]
        limit: usize,
        #[arg(long, help = "Directory for cached extracted JSON files.")]
        cache_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Refresh the dataset from MOJ before searching.")]
        refresh: bool,
    },
    #[command(about = "Fetch one MOJ OpenAPI law/order by name, optionally one article.")]
    Get {
        #[arg(
            long,
            default_value = "ch-law",
            help = "Dataset id: ch-law, ch-order, en-law, or en-order."
        )]
        dataset: String,
        #[arg(long, help = "Law/order name to fetch.")]
        law: String,
        #[arg(long, help = "Article number to return, for example 184.")]
        article: Option<String>,
        #[arg(long, help = "Return all articles when --article is not supplied.")]
        include_articles: bool,
        #[arg(long, help = "Include full history and foreword fields.")]
        include_history: bool,
        #[arg(long, help = "Directory for cached extracted JSON files.")]
        cache_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Refresh the dataset from MOJ before fetching.")]
        refresh: bool,
    },
    #[command(about = "Fetch recent MOJ legal update notices without credentials.")]
    Updates {
        #[arg(long, help = "Update kind: all, law, order, rule, local, or draft.")]
        kind: Option<String>,
        #[arg(long, help = "Filter update titles/categories by keyword.")]
        keyword: Option<String>,
        #[arg(long, default_value_t = 20, help = "Maximum results to return.")]
        limit: usize,
    },
    #[command(about = "Fetch MOJ treaty or cross-strait agreement listings without credentials.")]
    Agreements {
        #[arg(long, help = "Agreement kind: treaty or cross-strait.")]
        kind: Option<String>,
        #[arg(
            long,
            help = "Use MOJ public keyword search for treaty/agreement titles."
        )]
        keyword: Option<String>,
        #[arg(
            long,
            help = "Treaty category code returned by this command, for example D1900500000000."
        )]
        category_code: Option<String>,
        #[arg(long, help = "Include treaty category metadata in the response.")]
        include_categories: bool,
        #[arg(long, default_value_t = 20, help = "Maximum results to return.")]
        limit: usize,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Regulation(command) => match command.command {
            RegulationSubcommand::Pcode { law } => get_pcode(&law),
            RegulationSubcommand::Search {
                keyword,
                offset,
                limit,
                exclude_abolished,
            } => search_regulations(RegulationSearch {
                keyword,
                offset,
                limit,
                exclude_abolished,
            }),
            RegulationSubcommand::Query {
                law,
                pcode,
                article,
                from,
                to,
                include_history,
            } => query_regulation(RegulationQuery {
                law_name: law,
                pcode,
                article_no: article,
                from_no: from,
                to_no: to,
                include_history,
            }),
        },
        Command::Judgment(command) => match command.command {
            JudgmentSubcommand::Search {
                keyword,
                court,
                case_type,
                year_from,
                year_to,
                case_word,
                case_number,
                main_text,
                max_results,
            } => search_judgments(JudgmentSearch {
                keyword,
                court,
                case_type,
                year_from,
                year_to,
                case_word,
                case_number,
                main_text,
                max_results,
            }),
            JudgmentSubcommand::Get { jid, url } => get_judgment(JudgmentGet { jid, url }),
            JudgmentSubcommand::Special {
                kind,
                keyword,
                court,
                year,
                case_word,
                case_number,
                max_results,
            } => search_special_judgments(JudgmentSpecialSearch {
                kind,
                keyword,
                court,
                year,
                case_word,
                case_number,
                max_results,
            }),
        },
        Command::Interpretation(command) => match command.command {
            InterpretationSubcommand::Get {
                case_id,
                include_reasoning,
                reasoning_keyword,
                include_opinions,
                opinions_keyword,
            } => get_interpretation(InterpretationQuery {
                case_id,
                include_reasoning,
                reasoning_keyword,
                include_opinions,
                opinions_keyword,
            }),
            InterpretationSubcommand::Search {
                keyword,
                year,
                number_from,
                number_to,
                include_old,
                include_new,
                no_old,
                no_new,
                limit,
            } => search_interpretations(InterpretationSearch {
                keyword,
                year,
                number_from,
                number_to,
                include_old: include_old && !no_old,
                include_new: include_new && !no_new,
                limit,
            }),
            InterpretationSubcommand::Citations {
                case_id,
                include_context,
            } => get_citations(&case_id, include_context),
            InterpretationSubcommand::Current { year, limit } => {
                current_judgments(CurrentJudgmentsQuery { year, limit })
            }
            InterpretationSubcommand::Terminal {
                keyword,
                kind,
                year_from,
                year_to,
                limit,
            } => terminal_cases(TerminalCasesQuery {
                keyword,
                kind,
                year_from,
                year_to,
                limit,
            }),
        },
        Command::Sources(command) => match command.command {
            SourcesSubcommand::Status => sources_status(),
            SourcesSubcommand::List {
                implemented,
                planned,
                no_credentials,
            } => list_sources(SourceListOptions {
                implemented_only: implemented,
                planned_only: planned,
                no_credentials_only: no_credentials,
            }),
            SourcesSubcommand::Gaps => coverage_gaps(),
        },
        Command::Agent(command) => match command.command {
            AgentSubcommand::Guide => agent_guide(),
        },
        Command::Moj(command) => match command.command {
            MojSubcommand::Datasets => moj_datasets(),
            MojSubcommand::Status {
                dataset,
                cache_dir,
                remote,
            } => moj_status(MojDatasetQuery {
                dataset,
                cache_dir,
                remote,
            }),
            MojSubcommand::Sync {
                dataset,
                cache_dir,
                force,
            } => moj_sync(MojSyncQuery {
                dataset,
                cache_dir,
                force,
            }),
            MojSubcommand::Search {
                dataset,
                keyword,
                include_articles,
                limit,
                cache_dir,
                refresh,
            } => moj_search(MojSearchQuery {
                dataset,
                keyword,
                include_articles,
                limit,
                cache_dir,
                refresh,
            }),
            MojSubcommand::Get {
                dataset,
                law,
                article,
                include_articles,
                include_history,
                cache_dir,
                refresh,
            } => moj_get(MojGetQuery {
                dataset,
                law,
                article,
                include_articles,
                include_history,
                cache_dir,
                refresh,
            }),
            MojSubcommand::Updates {
                kind,
                keyword,
                limit,
            } => moj_updates(MojUpdatesQuery {
                kind,
                keyword,
                limit,
            }),
            MojSubcommand::Agreements {
                kind,
                keyword,
                category_code,
                include_categories,
                limit,
            } => moj_agreements(MojAgreementsQuery {
                kind,
                keyword,
                category_code,
                include_categories,
                limit,
            }),
        },
        Command::Mojlaw(command) => match command.command {
            MojlawSubcommand::Search {
                kind,
                keyword,
                limit,
            } => mojlaw_search(MojlawSearchQuery {
                kind,
                keyword,
                limit,
            }),
        },
        Command::OpenData(command) => match command.command {
            OpenDataSubcommand::LegalCatalog {
                keyword,
                agency,
                limit,
                cache_dir,
                refresh,
            } => open_data_legal_catalog(OpenDataLegalCatalogQuery {
                keyword,
                agency,
                limit,
                cache_dir,
                refresh,
            }),
        },
    };

    match result {
        Ok(value) => {
            print_json(&value, cli.pretty);
        }
        Err(err) => {
            print_json(&err.to_json(), cli.pretty);
            std::process::exit(err.exit_code());
        }
    }
}

fn print_json(value: &serde_json::Value, pretty: bool) {
    let rendered = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .expect("JSON rendering should not fail");
    println!("{rendered}");
}
