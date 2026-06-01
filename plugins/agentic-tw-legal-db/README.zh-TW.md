# agentic tw legal db

`agentic-tw-legal-db` 是給 agent 使用的台灣公開法律資料 CLI plugin。安裝後，agent 主要透過本機指令 `twlaw` 查資料。本專案不內建 MCP server；預設就是 shell 指令 + JSON 輸出，減少使用者要處理的設定。

`twlaw` 的介面是為 agent 設計：成功與錯誤都輸出 JSON、預設不需要政府 API key、可先查資料來源狀態；即時連到政府網站的查詢會限制查詢量，常用資料則可走快取或內建資料，方便大量查詢時降低對官方網站的壓力。

英文版文件：[README.md](README.md)

## 用提示詞安裝

多數使用者不需要手動安裝。把下面這一句完整貼給你正在使用的 agent 即可：

```text
Install the public Taiwan legal research CLI plugin from https://github.com/yu2001-s/agentic-tw-legal-db.
```

若要手動安裝：

```bash
git clone https://github.com/yu2001-s/agentic-tw-legal-db.git
cd agentic-tw-legal-db/plugins/agentic-tw-legal-db
scripts/install.sh
```

安裝後可用下列指令確認：

```bash
twlaw --version
twlaw sources status --json
twlaw agent guide --json
```

## 可查詢的資料

| 範圍 | 指令 | 內容 |
| --- | --- | --- |
| 資料來源盤點與 agent 指引 | `twlaw sources ...`、`twlaw agent guide` | 查目前涵蓋哪些官方來源、哪些資料不需 API key、還有哪些明確缺口，以及建議的 agent 工作流程。 |
| 全國法規資料庫資料 | `twlaw regulation ...`、`twlaw moj ...` | 查法規名稱、pcode、條文、沿革資料，以及法務部 OpenAPI ZIP 檔中的中文/英文法規與命令資料。這些 ZIP 檔不需申請 API key。 |
| 最新法規訊息 | `twlaw moj updates ...` | 查近期法律、命令、行政規則、地方法規、草案預告等異動訊息，並保留官方來源連結。 |
| 條約協定 | `twlaw moj agreements ...` | 查條約協定與兩岸協議，可依分類瀏覽或用關鍵字查詢。 |
| 法務部主管法規查詢系統 | `twlaw mojlaw search ...` | 查行政函釋、法規諮詢意見、法律問題座談、聲明異議決定書，以及相關法規與裁判參考資料。 |
| 司法院裁判書 | `twlaw judgment search/get/special ...` | 查公開裁判書與特殊查詢頁面，包含簡易案件、除權判決、公示催告裁定；也可用 `jid` 取得全文。 |
| 憲法法庭與大法官資料 | `twlaw interpretation ...` | 查內建的大法官解釋與憲法法庭裁判資料，也可查最新判決、終結案件、引用關係、理由書與意見書片段。 |
| 政府資料開放平臺 | `twlaw open-data legal-catalog ...` | 查 `data.gov.tw` 中的法律相關資料集，包含來源連結與授權資訊。 |

## 給 agent 的使用規則

- 查詢時一律加上 `--json`。
- 成功與失敗都要讀 JSON，不要只解析一般文字輸出。
- 回答使用者時保留 `source_url`、`retrieved_at`、快取狀態、分頁與截斷資訊。
- 先查目錄或中繼資料，再抓全文。
- 如果要大量查法務部法規或命令，先執行 `twlaw moj sync --dataset <id> --json`。
- 內建資料與快取查詢可以並行執行；即時連到政府網站的查詢要控制併發量。
- 查詢結果是法律研究參考資料，不是法律意見。

## 範例

```bash
twlaw sources status --json
twlaw agent guide --json
twlaw regulation query --law "民法" --article "184" --json
twlaw moj sync --dataset ch-law --json
twlaw moj search --dataset ch-order --keyword "勞動" --include-articles --limit 20 --json
twlaw moj updates --kind order --limit 10 --json
twlaw moj agreements --kind treaty --keyword "CEDAW" --json
twlaw mojlaw search --kind admin-interpretation --keyword "個資" --limit 10 --json
twlaw judgment search --keyword "預售屋 遲延交屋" --case-type "民事" --max-results 10 --json
twlaw interpretation current --limit 10 --json
twlaw open-data legal-catalog --keyword "判決" --limit 10 --json
```

## 支援的 agent

| 介面 | 狀態 | 檔案 |
| --- | --- | --- |
| Codex | 原生 plugin skill | `.codex-plugin/plugin.json`、`skills/agentic-tw-legal-db/SKILL.md` |
| Claude Code | 已支援 | `CLAUDE.md`、`.claude/commands/*.md` |
| 能執行 shell 指令的通用 agent | 已支援 | `AGENTS.md` |
| MCP client | 未內建 | 如有需要，可另外做一層 wrapper。 |

## 測試與發布

```bash
scripts/test.sh
scripts/release-check.sh --marketplace
```

本專案不是台灣政府機關、OpenAI、Anthropic 或任何第三方市集的官方專案。發布前請閱讀 `docs/PRIVACY.md`、`docs/TERMS.md` 與 `docs/PUBLISHING.md`。
