# 优化与功能待办

概率推演：估算对方暗子是车 / 马 / 炮 / 士象的概率分布

比起象棋更加占用内存为什么

## 高优先级（低成本高收益）

- [ ] **帧差跳帧**：棋盘画面无变化时跳过截图后的 YOLO 推理（先做廉价像素 diff），预计监听基础 CPU 再降 50% 以上，且不损失反应速度
- [ ] **线程/哈希设置立即生效**：`config.rs` 的 `set_engine_threads`/`set_engine_hash` 直接调用 `engine.set_threads/set_hash`（或前端接入现成的 `reload_engine` 命令），改完删除 product.md 中"需重启应用"的说明
- [ ] **打包防污染**：`build:cpu`/`build:gpu` 前自动清理 `server\target\release\onnxruntime*.dll`，打包完成后自动给 MSI 加 `-CPU`/`-GPU` 后缀
- [x] **监听健壮性**：截图改为可失败（`listen.rs` capture 返回 Option），目标窗口关闭时连续 3 次截图失败自动结束监听并发 `listen_stopped` 事件，前端弹窗提示并复位按钮状态
- [ ] **配置项补全**：`timer_interval`/`confirm_interval` 增加前端设置入口
- [x] **接通 `show_wdl`**：引擎 info 解析 wdl 千分比胜率随 `analyse` 事件下发；设置抽屉新增"胜率显示"开关，改完立即生效（`set_engine_show_wdl` 直接下发 setoption，无需重启）

## 中优先级（体验提升）

- [ ] **对局记录与复盘**：逐步保存 FEN + 引擎评分，对局结束后可复盘"哪步走亏了多少分"、导出棋谱
- [ ] **胜率曲线**：`show_wdl` 已打通（千分比胜率已随 `analyse` 事件下发），前端绘制局面胜率随步数变化的曲线
- [x] **建议来源标注**：UI 显示 `QueryResult.source`（云库/引擎）与 `depth`，让用户知道每招的分量
- [x] **对手缓招提醒**：非预期走子（与 `expect_board` 不符）后，用下次分析的评分与上次预期评分计算亏损，`QueryResult.deviation` 随 `analyse` 事件下发，面板显示"我方/对方偏离提示 亏 N 分"

## 低优先级（大功能/工程化）

- [ ] 悬浮窗提示（游戏全屏时无需切窗口）、识别置信度展示、多显示器/DPI 适配
- [ ] 目标窗口自动识别：现在需在弹窗中手动选择目标窗口（`listen.rs` 按 window id 匹配），可增加按标题/进程名的自动匹配规则（适配其他对弈平台/模拟器投屏）
- [ ] GitHub Actions 自动构建并发布 MSI
- [x] `chess.rs`/`attack.rs` 纯逻辑单元测试（16 个用例: FEN/走法/翻子 diff/中文记谱）
- [ ] **日志体系整改**：接通 `config.loglevel`（字段已定义但 tracer 写死 DEBUG，`lib.rs:47`，日常用 INFO 可大幅减少日志量）；启动时自动清理 N 天前的旧日志；修复日志目录拼接问题（`logger.rs` 把应用数据目录误当用户主目录又拼了一层 `AppData\Local`）。日志实际位置：`C:\Users\<用户>\AppData\Roaming\top.itmeng.jieqilink\AppData\Local\jieqilink\logs\`，手动清理＝删除旧日期的 `runtime.log.*` 文件（应用运行中当日文件被占用，删不掉属正常）
