# 揭棋连线分析助手 🚩

## 项目简介

"揭棋连线分析助手"（jieqilink）是一款揭棋辅助工具，基于开源项目 [chessboard](https://github.com/atopx/chessboard)（象棋版）改造，识别模型与暗子素材参考了 [JieqiBox](https://github.com/Velithia/JieqiBox)。通过实时识别屏幕中的揭棋棋盘（含暗子），结合皮卡鱼揭棋版引擎分析，为在线对弈平台的玩家提供精准走法建议与策略指导。

## ✨ 功能亮点

- ⚡ **轻量极速**
  一键启动，无需繁琐配置，毫秒级别识别与分析，畅享零等待。
- 🔒 **安全可靠**
  核心模块采用 Rust 开发，确保内存安全与高并发性能。
- 🪟 **Windows 优先**
  提供 CPU / GPU 两种安装包；代码层面跨平台，其他平台需自行放置对应引擎二进制。
- 🚀 **GPU 智能加速**
  支持 CUDA、DirectML（Windows）/ CoreML（macOS），全力释放显卡性能。

## 🎯 核心功能

- 📷 **实时棋盘识别**
  基于 YOLOv8 与 ONNX Runtime，34 类棋子精准定位与分类（含暗子 X/x），自动跟随对局状态。
- 🤖 **智能走法分析**
  集成皮卡鱼揭棋版引擎（Pikafish jieqi 分支 + NNUE），支持分析深度、时间、线程数与哈希自定义，MultiPV 候选招法与偏离提示。
- 🀄️ **中文招法展示**
  分析结果转为易读的中文走法描述；暗子按起始位名义兵种记谱，翻开后以括号后缀标注实际兵种（如 `车九进一(翻车)`）。
- 📚 **开局库支持**
  接入云库开局建议，建议来源（云库/引擎）与深度随招法一并展示。
- 🎯 **局势推演**
  演练盘从当前局面自由试招，合法落点自动高亮（含蹩马腿、将帅对脸、送将等规则校验），每步给出提示与亏损提示。

## 🏗 技术架构

### 前端
- **框架**：Vue 3 + TypeScript
- **UI 组件**：Naive UI
- **桌面发行版**：Tauri（轻量化跨平台打包）

### 后端
- **编程语言**：Rust
- **棋盘识别**：YOLOv8 + ONNX Runtime（`libs/jieqi.onnx`，编译期内嵌）
- **揭棋引擎**：皮卡鱼揭棋版（`libs/pikajieqi/`，二进制 `pikajieqi-windows.exe` + 权重 `pikajieqi.nnue`）
- **应用通信**：Tauri API

## 🚀 快速上手

### 使用（打包版）

1. 启动程序，在弹窗中选择目标对弈窗口
2. 工具自动识别棋盘并启动引擎分析
3. 右侧面板实时展示最佳走法、评分与胜率
4. 可在"设置"中调整提示强度、深度、时间、线程数等参数

### 从源码构建（开发者）

```powershell
cd JieqiBox\jieqilink
pnpm install
pnpm tauri dev        # 首次会 panic，按 product.md 建立 junction 并复制 DLL 后重试
pnpm build:cpu        # 打包 CPU 版
pnpm build:gpu        # 打包 GPU 版（需先按 product.md 下载 GPU DLL）
```

> 详细的 dev 调试、GPU 切换、打包注意事项见 [product.md](./product.md)；功能优化待办见 [project.md](./project.md)。

## 📜 许可声明

1. 本项目基于 Apache-2.0 许可证（[LICENSE](./LICENSE)），永久免费开源，仅供学习与研究使用。
2. 严禁任何商业化或非法用途。
3. 使用本工具时，请遵守各平台用户协议，本工具不涉及任何破解或自动化操作。

## 🙏 致谢

- [chessboard](https://github.com/atopx/chessboard)：本项目的前身（中国象棋版），架构与识别方案源自该项目
- [JieqiBox](https://github.com/Velithia/JieqiBox)：揭棋识别模型（`jieqi.onnx`）与暗子图片素材取自该项目
- [Pikafish](https://github.com/official-pikafish/Pikafish)：皮卡鱼引擎（jieqi 分支支持揭棋）
