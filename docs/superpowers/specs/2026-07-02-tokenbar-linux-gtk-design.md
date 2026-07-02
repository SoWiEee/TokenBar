# TokenBar Linux（GTK4）版本 — 設計文件

- **日期**：2026-07-02
- **狀態**：已核准設計，待實作計畫
- **目標平台**：Ubuntu 24.04（GNOME 46），Rust + GTK4
- **範圍決策**：完整功能對等、扁平設計（不要 Liquid Glass）、真 3D 可繞行貢獻圖、托盤優先 + 視窗 fallback、`.deb` + apt repo 散佈

---

## 1. 背景與目標

TokenBar 目前是 macOS 原生 App（Swift + AppKit + SwiftUI + Sparkle），資料層是 Rust（`vendor/tokscale-core` + `crates/tb_core_ffi`），兩者以 C-ABI 橋接。本專案要在 **不影響 macOS 版** 的前提下，新增一個功能對等的 Linux 版本。

**關鍵發現（決定了整體工作量分佈）：**

- **Rust 資料層與網路層在 Linux 上幾乎原封不動就能運作。**
  - Claude 憑證讀取是三層 fallback：環境變數 →（macOS）Keychain → 檔案 `~/.claude/.credentials.json`（`agent_usage.rs:1258`）。Linux 上第二層回傳 `None`，自動落到第三層——正是 Claude Code 在 Linux 存 OAuth 憑證之處。
  - Codex 讀 `~/.codex/auth.json`，本來就跨平台。
  - `paths.rs` 已 XDG-aware（`XDG_CONFIG_HOME` / `~/.config/tokscale`）。
  - `reqwest` 用 rustls，非 macOS Security framework。
- **UI 層（整個 `Sources/TokenBar`、大部分 `TokenBarCore`）等於從零重寫**：`SwiftUI ×27`、`AppKit ×18`、`Sparkle`、`ServiceManagement`、`SceneKit` 在 Linux 上皆不存在。

因此真正的工作集中在：**（a）抽取共用 Rust 邏輯、（b）用 GTK4 重寫 UI、（c）打包 `.deb`**。

---

## 2. 整體架構

目前 report/agent 邏輯被困在 `crates/tb_core_ffi` 這個 staticlib 內（`mod agent_usage` 等私有模組），只能經 C-ABI 給 Swift。Linux 版不需要 C-ABI，所以先抽取共用邏輯成獨立 lib crate：

```
crates/
├── tb_reports/      【新增 lib】從 tb_core_ffi 抽出的純邏輯：
│                       model_report / hourly_report / agents_report、
│                       usage_graph、usage_tail、agent_usage、agent_history、
│                       agent_antigravity、agent_copilot、opencode_integrations、
│                       graph 快取（GRAPH_CACHE）、tail-tick single-flight。
│                       各模組維持 pub fn run()，回傳型別化結果 / serde_json::Value。
│                       抽成 lib 後可正常撰寫 #[cfg(test)] 單元測試。
├── tb_core_ffi/     【瘦身】變成 tb_reports 之上的薄 C-ABI shim。
│                       envelope / guarded / into_raw_json / tb_free 留在此。
│                       RUNTIME / RAYON_INIT 等進程級 static 視共用程度決定去留。
│                       macOS ↔ Swift 的 ctb.h 契約與 JSON shape 完全不變。
└── tokenbar-gtk/    【新增 bin】Linux App：GTK4 + libadwaita，
                        直接依賴 tb_reports + tokscale-core。無 FFI、單一進程。
vendor/tokscale-core/  不動（維持與上游可 diff）
```

**不變式**：macOS 建置路徑（`make` / `swift build`）與 Swift 端行為完全不受影響——`tb_core_ffi` 僅內部改為呼叫 `tb_reports`，對外 C-ABI 一致。

**抽取範圍界定**：
- 純 report `run()` 函式、graph 快取、tail-tick single-flight → 移入 `tb_reports`（兩個前端都受益）。
- `tokio` `RUNTIME`（`agent_usage` 用）建議也移入 `tb_reports`，兩前端共用。
- FFI 專屬的 `envelope` / `guarded` / `CString` / `tb_free` → 留在 `tb_core_ffi`。
- 每個進程只跑一個前端，故進程級 static 共存無虞。

---

## 3. GTK App 結構（`tokenbar-gtk`）

### 3.1 托盤

- `ksni` crate（純 Rust 的 StatusNotifierItem，無 C 依賴）。
- 動畫圖示（貓的影格，資源從 macOS 的 `anim-cat2` 等移植為 PNG 影格序列）。
- 標題文字：今日 tokens / 花費 / live rate / 剩餘 quota（沿用 macOS 的 TrayMode 概念）。
- 左鍵切換主視窗；右鍵選單切換 quota source 與離開。

### 3.2 主視窗（同時扮演 popover 與 fallback 視窗）

SNI 沒有 macOS 那種錨定托盤的 popover，因此主視窗一物二用：托盤優先時作為點擊展開的儀表板，沒裝 AppIndicator 擴充時作為一般視窗。

- libadwaita `ApplicationWindow`。
- Header：app 分頁（Overview/Claude/Codex…）過濾**哪些** agent。
- View switcher：六個 lens（Overview / Models / Daily / Hourly / Stats / Agents）切換**如何**拆解。兩者組合，對齊 macOS `AppView` + client tab 語意。
- 各 lens 用 GTK4 `ListBox` / `Grid` + `DrawingArea` + Cairo 繪製 2D 圖表與卡片。
- Overview 含 quota 卡（pace projection）、live session trace、streaks。

### 3.3 3D 貢獻圖（最高工作量元件）

- `GLArea` 內嵌 OpenGL，用 `three-d` 或 `glow` crate 重現可繞行的 3D 貢獻圖。
- 繞行 / 縮放用 GTK gesture controller（`GestureDrag` / `EventControllerScroll`）。
- **建議最早做技術原型**，驗證 GLArea 的 GL context 與 3D crate 的整合可行性後再往下。

### 3.4 執行緒模型與資料流

- GTK 主迴圈單執行緒。重工作（`agent_usage` 網路請求、log 解析）丟到 worker thread。
- worker → UI 透過 `glib` channel（`glib::MainContext::channel` / async）回傳型別化結果，UI 執行緒更新。
- 沿用 macOS「失敗時保留上一筆好值」的韌性：每次 poll 包在 `Result`，`Err` 時保留前值，不清空畫面。

### 3.5 輪詢節奏

用 `glib::timeout_add_seconds` 對齊 macOS：
- 標題刷新：預設 30 分（`tokenbar.refresh.intervalMin` 對應設定），含強制刷新時鐘。
- graph poll：60s。
- trace / agent_usage：各自 poller，fetch-first 覆寫。

---

## 4. 平台差異處理

| 項目 | 處理方式 |
|---|---|
| Claude / Codex 憑證 | ✅ 已支援（檔案 fallback）——無需改動 |
| Session log 路徑 | ✅ `paths.rs` 已 XDG-aware |
| 資料目錄 | `dirs::data_dir()` 在 Linux → `~/.local/share/com.nyanako.tokenbar/`（如 codex-weekly-history.json），自洽 |
| 開機自啟 | macOS 用 ServiceManagement；Linux 改用 XDG autostart（`~/.config/autostart/*.desktop`）+ 設定開關 |
| Swift 端純邏輯 | ModelColors 色階數學、Format 數字格式、UsagePace pace 投影、QuotaResolver 需**移植成 Rust**（部分對應已存在的 `provider_identity` / `pricing`）；輸出須與 macOS 一致 |
| macOS frameworks | 不涉及（Linux 不建置 Swift package，`Package.swift` 的 Security/SystemConfiguration/CoreFoundation/resolv 連結與 Linux 無關） |

---

## 5. 建置與打包

- Cargo workspace 新增 `crates/tb_reports`、`crates/tokenbar-gtk` 兩個 member。
- Ubuntu 24.04 系統依賴：`libgtk-4-dev`、`libadwaita-1-dev`、GL 相關開發套件；`ksni` 需要環境提供 StatusNotifierWatcher（即 AppIndicator 擴充或提供 SNI 的桌面環境）。
- `.deb` 用 `cargo-deb` 產生。
- apt repo：以 GPG 簽章託管於 GitHub Pages（reprepro / aptly）；**起步可先只在 GitHub Releases 放 `.deb`**，apt 自動更新後補，以降低初期維運成本。
- CI：新增 `ubuntu-24.04` job：裝 GTK 依賴 → `cargo build -p tokenbar-gtk --release` → 跑測試 → 打 `.deb`。既有 macOS job（`ci.yml`）不動。
- Makefile 新增 `linux` / `deb` targets。

---

## 6. 測試策略

- `tb_reports` 抽成 lib 後可正常單元測試，補強現有 Rust 測試（原本困在 staticlib，難以覆蓋）。
- `tokenbar-gtk` 加 `--selftest` / `--smoke` CLI 模式（對齊 macOS `main.swift` 的做法），讓 CI 無顯示器也能驗證核心邏輯與 FFI-free 的 report 呼叫。
- 移植自 Swift 的純邏輯（ModelColors / Format / UsagePace / QuotaResolver）在 `--selftest` 中以斷言比對，確保與 macOS 輸出一致。
- GTK UI 層以 `xvfb` 做選配的 headless 煙霧測試。

---

## 7. 風險與未知

1. **3D 圖整進 GLArea**（最高工作量）：`three-d` / `glow` 與 GLArea 的 GL context 整合、繞行控制。→ 最早做原型驗證。
2. **GNOME 托盤**：`ksni` 需 StatusNotifierWatcher（AppIndicator 擴充）；視窗 fallback 已涵蓋未安裝者。SNI 動畫圖示更新可能有延遲，需驗證影格節奏是否可接受。
3. **兩套 UI 程式庫**：macOS Swift + Linux GTK 只共用 Rust 核心；新功能有分歧成本，這是原生化的代價。
4. **apt repo 簽章維運**：有 GPG 維護成本；已決定可先用 Releases 起步。
5. **Swift 純邏輯移植對齊**：色階 / 格式 / pace 投影需與 macOS 輸出逐一比對。

---

## 8. 實作階段（概要，細節留待實作計畫）

1. **Phase 0 — 抽取共用核心**：建立 `tb_reports` lib，把 report/agent 模組從 `tb_core_ffi` 移入；`tb_core_ffi` 改為薄 shim；驗證 macOS `make` + `--selftest` + `--smoke` 全綠（不得回歸）。
2. **Phase 1 — 3D 原型**：GTK4 `GLArea` + 3D crate 渲染可繞行貢獻圖的最小原型，排除最大技術風險。
3. **Phase 2 — GTK 骨架**：libadwaita 視窗 + ksni 托盤 + 六個 lens 的資料綁定（先 2D 圖表 / 卡片）。
4. **Phase 3 — 平台整合**：Swift 純邏輯移植成 Rust、autostart、quota source 設定、韌性 poll。
5. **Phase 4 — 打包與 CI**：`cargo-deb`、`ubuntu-24.04` CI job、Makefile targets、Releases 上架。
