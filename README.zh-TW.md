# agentic tw legal db

`agentic-tw-legal-db` 是一套給 agent 使用的台灣法律資料查詢工具。安裝後，本機會有 `twlaw` 指令；Codex、Claude Code、Cursor、Windsurf、Gemini CLI 等能執行 shell 指令的 agent，都可以透過它查詢台灣官方公開法律資料，並以 JSON 取得結果。

本專案以 CLI 為核心。agent 一律執行 `twlaw ... --json`，取得結構化資料後，在回覆中保留官方來源網址與查詢時間。使用者不需要申請政府 API key，也不需要設定 MCP。

英文版文件：[README.md](README.md)

## 這是什麼

- 讓 agent 查詢台灣法規、命令、裁判書、憲法法庭資料、法務部法律參考資料，以及政府資料開放平臺上的法律相關資料集。
- 查詢成功或失敗都回傳 JSON，方便 agent 穩定解析。
- 內建資料來源盤點指令，讓 agent 在開始研究前先說明涵蓋範圍、已知缺口、資料新舊，以及是否需要帳號或 API key。
- 常用資料盡量走快取或內建資料快照；即時連到政府網站的查詢則限制查詢量，避免對官方網站造成不必要壓力。
- 查詢結果是法律研究參考資料，不是法律意見。

## 使用的資料來源

| 範圍 | 官方來源 | 取用方式 | 目前涵蓋內容 |
| --- | --- | --- | --- |
| 法規與命令 | 法務部 `law.moj.gov.tw` | 公開網頁 + 不需 API key 的 ZIP 檔 | 法規名稱搜尋、pcode 查詢、條文與全文取得、沿革資料、中文與英文法規/命令資料同步與搜尋。 |
| 法規異動與條約協定 | 法務部 `law.moj.gov.tw` | 公開網頁 | 最新法規訊息、命令、行政規則、地方法規、草案預告、法務部頁面提供的公報連結、條約協定與兩岸協議列表。 |
| 法務部法律參考資料 | 法務部 `mojlaw.moj.gov.tw` | 公開網頁 | 行政函釋、法規諮詢意見、法律問題座談、聲明異議決定書，以及其他法務部主管法規查詢系統中的法律參考資料。 |
| 司法院裁判書 | 司法院 `judgment.judicial.gov.tw` | 公開網頁查詢，並限制查詢量 | 裁判書搜尋、依查詢結果 id 或網址取得全文、簡易案件、除權判決、公示催告裁定等特殊查詢。 |
| 憲法法庭與大法官資料 | 司法院憲法法庭 `cons.judicial.gov.tw` | 內建資料快照 + 公開網頁/AJAX 查詢，並限制查詢量 | 搜尋內建的大法官解釋與憲法法庭裁判、查最新判決列表、終結案件查詢、引用關係與理由書片段。 |
| 政府資料開放平臺目錄 | `data.gov.tw` | 不需 API key 的公開目錄匯出 | 查找法律相關資料集，保留機關、授權、來源網址與目錄資訊。 |
| 司法院 JList/JDoc API | `data.judicial.gov.tw/jdg/api` | 需申請 API key 的官方 API | 已盤點但不作為預設資料來源；本工具的基本流程必須不用申請帳號或 API key。 |

## 用提示詞安裝

把下面這一句貼給你正在使用的 agent：

```text
Install the public Taiwan legal research CLI plugin from https://github.com/yu2001-s/agentic-tw-legal-db.
```

agent 應會自行下載 repo、進入 plugin 目錄、執行安裝腳本，並用 `twlaw --version`、`twlaw sources status --json`、`twlaw agent guide --json` 確認安裝結果。這個 plugin 不需要使用者申請政府 API key，也不需要設定 MCP。

## 從這裡開始

- [Plugin 說明](plugins/agentic-tw-legal-db/README.zh-TW.md)
- [通用 agent 指引](plugins/agentic-tw-legal-db/AGENTS.md)
- [Claude Code 指引](plugins/agentic-tw-legal-db/CLAUDE.md)
- [發布檢查清單](plugins/agentic-tw-legal-db/docs/PUBLISHING.md)

預設工作流程使用台灣官方公開法律資料來源；需要帳號或 API key 的來源只列為延伸參考，不是基本查詢流程的必要條件。

## 支援哪些 agent

目前支援：

- Codex：透過 `plugins/agentic-tw-legal-db/skills/` 內的原生 plugin skills。
- Claude Code：透過 `CLAUDE.md` 與 `.claude/commands/`。
- 能執行 shell 指令的通用 agent：透過 `AGENTS.md`。
- 其他 agent：直接用 shell 執行 `twlaw`。

公開安裝流程刻意保持簡單：使用者只要貼一句提示詞，agent 負責下載 repo、安裝與驗證。
