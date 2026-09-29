# 揭棋连线分析助手

## 🚀 快速上手

### 从源码运行/构建（开发者）

环境要求：Node.js、pnpm、Rust 工具链、WebView2 Runtime（Windows 11 自带，Windows 10 需 [手动安装](https://developer.microsoft.com/microsoft-edge/webview2/)）。

```powershell
cd JieqiBox\jieqilink
pnpm install
pnpm tauri dev        # 首次会 panic 在引擎路径，没关系
New-Item -ItemType Directory -Force server\target\debug\_up_ | Out-Null
New-Item -ItemType Junction -Path server\target\debug\_up_\libs -Target libs
Copy-Item libs\windows-cpu\*.dll server\target\debug\ -Force
pnpm tauri dev        # 第二次成功
```

> dev 模式首次运行 panic 是因为 Tauri 把资源路径中的 `..` 映射为 `_up_` 目录，需要在 `target\debug\` 下建立 junction 指向 `libs`。正式打包不受影响。
>
> `libs\` 已就位揭棋识别模型 `jieqi.onnx`（34 类，编译期 `include_bytes!` 内嵌）与揭棋版皮卡鱼引擎（`libs\pikajieqi\`，二进制 `pikajieqi-windows.exe` + 权重 `pikajieqi.nnue`），无需额外下载。
>
> `libs\windows-cpu\` 与 `libs\windows-gpu\` 是微软 ONNX Runtime 运行库（供识别模型推理加载 `jieqi.onnx` 用），并非棋类引擎文件，勿与 `libs\pikajieqi\` 引擎混淆。
>
> 复制 CPU 版 DLL 是因为 ort 以 `load-dynamic` 方式按 exe 目录加载 `onnxruntime.dll`；dev 模式不会自动放置 DLL，若 exe 目录没有，Windows 会继续搜索 system32，可能命中其他软件遗留的旧版 DLL（如 1.10），导致 ort 版本校验 panic（要求 ≥1.20）。

dev 模式启用 GPU 推理（覆盖 DLL 法）：

```powershell
# 一次性：把 GPU 版 DLL 复制到 target\debug\（覆盖 CPU 版同名文件）
Copy-Item F:\w\template\JieqiBox\jieqilink\libs\windows-gpu\* F:\w\template\JieqiBox\jieqilink\server\target\debug\ -Force
# 之后每次启动都用 --features gpu
pnpm tauri dev --features gpu
# 切回 CPU
Copy-Item F:\w\template\JieqiBox\jieqilink\libs\windows-cpu\*.dll F:\w\template\JieqiBox\jieqilink\server\target\debug\ -Force
pnpm tauri dev
```

> GPU 版需要先从 [chessboard Release](https://github.com/zengsheng-git/chessboard/releases/tag/v0.1.2-gpu-dlls) 下载 windows-gpu.zip（196MB）**解压**到 `libs/windows-gpu/`（得到 4 个 `onnxruntime*.dll`，zip 本身无法被复制使用）；`--features gpu` 首次会重新链接 ort，较慢。无 N 卡驱动时按注册顺序自动落到 DirectML 或 CPU。

打包安装包：

```powershell
pnpm build:cpu    # CPU 版（仓库自带运行时 DLL）
pnpm build:gpu    # GPU 版（GPU DLL 取自 chessboard 仓库 Release 的 windows-gpu.zip，解压到 libs\windows-gpu\）
```

产物位置：`server\target\release\bundle\msi\jieqilink_0.1.2_x64_en-US.msi`。

## ⚙️ 引擎配置说明

界面"引擎配置"的 8 个设置（括号内为默认值）：

| 设置 | 默认值 | 作用 | 生效时机 |
|---|---|---|---|
| 提示强度 | 最强(不限) | 档位：最强=不限棋力；新手/入门/初级/中级/高级改用固定浅深度搜索（深度 2/4/6/8/12），每一手都是一致的中低水平，而非强引擎概率放水 | 立即 |
| 深度 | 20 | 搜索层数上限，越大棋力越强、耗时越长 | 立即 |
| 时间 | 5 秒 | 单次思考的时间上限（界面单位为秒，保存时转毫秒） | 立即 |
| 线程 | 4 | 引擎搜索线程数，越多则同样时间搜得越深 | **需重启应用** |
| 哈希 | 64MB | 置换表（搜索缓存）大小，减少重复计算 | **需重启应用** |
| 候选招数 | 3 | MultiPV 数量，决定展示几条次优候选（1~5） | 立即 |
| 次优分差 | 300 | 次优候选与最优分差不超过该值（厘分）才展示 | 立即 |
| 胜率显示 | 开启 | 评价行显示行棋方胜率（引擎 WDL） | 立即 |

机制要点：

- **提示强度非"最强"时**用固定浅深度覆盖用户深度：每一手都出自同一水平的浅思考，招法强度整体一致、可预期；深度与时间仍同时下发，档位低时通常深度先到。
- 深度与时间同时下发（`go depth N movetime M`），**谁先到谁生效**：最多思考设定的时间，提前搜满深度则立即出招。
- **生效时机不同**：深度 / 时间 / 胜率显示改完立即生效（每次分析实时读取，胜率显示直接向引擎下发 setoption）；**线程 / 哈希改完仅保存配置，需重启应用才生效**（当前实现下界面不会触发引擎重载，Threads/Hash 只在应用启动时下发给引擎进程）。
- **胜率来源**：需开启"胜率显示"（`UCI_ShowWDL`），引擎 WDL 千分比换算为百分比。杀棋局面（几步杀）不显示胜率。
- **偏离提示**：实际走子与预测不符时，用上次预期评分与本次分析评分（均为行棋方视角）计算亏损，面板提示"我方/对方偏离提示 亏 N 分"。

调参建议：

- **出招更快**：调小时间（如 2 秒）或降低深度。
- **棋力更强**：调大线程（不超过物理核数）和哈希（如 256MB），改完重启应用生效；引擎思考期间 CPU 会短时升高，属正常现象。

## 📖 分析面板解读

分析面板围绕"为什么要走这里"展示四层信息：

- **最佳招法 + 评分**：`我方 车九进一(翻车)`、`+120 较优`（正=行棋方占优，按均势/略优/较优/大优/胜势分档，几步杀直接显示步数），右上角深度，以及行棋方胜率。
- **次优招法**：与最优分差不超过"次优分差"的其他候选首着，`-40` 表示不走提示招要亏 40 分。
- **后续主线**：完整推演着法，每步带 我方/对方 标注；鼠标悬停某一步可在棋盘上预演到该步的局面，移开还原。
- **偏离提示**：实际走子与预测不符时提示 `对方偏离提示, 亏 80 分`（红色=利好我方），分差很小则显示"与预期相当"。

## ♟️ 局势推演

顶栏模式切换到"局势推演"进入演练盘，面板信息与连线分析一致（提示/评分/胜率/次优分差/完整主线悬停预演），另增加：

- 进入推演模式窗口自动加宽：左侧观看页（镜像棋盘+分析）保持可见并持续跟随真实对局，右侧为推演盘与推演面板，两边并排；
- 一键从连线分析的当前局面开始，鼠标点选棋子自由替双方试招，合法落点自动高亮（含蹩马腿、将帅对脸、送将等规则校验，明士/明象不受九宫与过河限制）；
- 每走一步自动给出当前行棋方的提示；自己走亏了会提示"这步亏了 N 分"（正分为行棋方视角）；
- 支持悔棋与重置，着法以中文记谱记录；主线预演基于演练盘当前局面。

## 🔍 界面缩放

工具栏提供"界面缩放"选择（80% ~ 125%）：整体等比缩放所有内容（棋盘/面板/文字），窗口尺寸随之调整，选择持久化在本地，重启后保留。

## ⚠️ 注意事项

- **CPU 与 GPU 版本互斥**：MSI 文件名完全相同，后打的会覆盖前者，打包后请立即按内容加 `-CPU` / `-GPU` 后缀（GPU 版约 350MB+，CPU 版约 75MB）。
- **GPU DLL 仓库不含**：从 [v0.1.2-gpu-dlls Release](https://github.com/zengsheng-git/chessboard/releases/tag/v0.1.2-gpu-dlls) 下载 `windows-gpu.zip`（196MB），解压到 `libs/windows-gpu/` 后才能 `pnpm build:gpu`（`onnxruntime_providers_cuda.dll` 320MB 超过 GitHub 单文件 100MB 限制）。
- **dev 模式 `_up_` junction 与 DLL**：`cargo clean` 或删除 `target/` 后需要重新建立 junction 并重新复制 `libs\windows-cpu\*.dll` 到 `target\debug\`，否则引擎启动会 panic。
- **交替打包 CPU/GPU 前先清理 `target\release\`**：`tauri build` 会把 `bundle.resources` 声明的 DLL 复制到 `server\target\release\`（exe 旁），打包时还会把该目录里**所有** DLL 扫进安装包。GPU 打包留下的 `onnxruntime_providers_cuda.dll`、`onnxruntime_providers_tensorrt.dll` 会让之后打出的 CPU 包也带上 300MB+ 的 CUDA 文件（表现为 CPU 包从 75MB 变成 266MB）。打 CPU 包前先删除 `server\target\release\onnxruntime*.dll`（打包过程会自动放入需要的）。
- **WebView2 与沙箱**：WebView2 需正常访问 `AppData\Local\<identifier>\EBWebView\` 目录，在某些受限终端（如 sandbox）下 webview 会启动失败导致窗口白屏，需在普通终端运行。
- **揭棋规则差异**：暗子（棋盘上的 X/x）不能移动，需先翻开；翻开的士/象不受九宫与过河限制；中文记谱中暗子按起始位名义兵种记录，翻开后实际兵种以括号后缀补充（如 `车九进一(翻车)`）。