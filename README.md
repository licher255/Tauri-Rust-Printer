# Tauri + Vanilla TS

This template should help get you started developing with Tauri in vanilla HTML, CSS and Typescript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## dev and build
```
cargo clean
cargo check

pnpm tauri dev
pnpm tauri build

```

| 命令                         | 用途    | 说明          |
| -------------------------- | ----- | ----------- |
| `pnpm tauri dev`           | 开发运行  | 带调试功能，热重载   |
| `pnpm tauri build`         | 打包发布  | 生成安装包       |
| `pnpm tauri build --debug` | 调试打包  | 打包但不优化，用于测试 |
| `pnpm dev`                 | 仅前端开发 | 在浏览器中调试前端   |

## Structure
```
airprinter/
├── src/                             ← 前端
│   ├── main.ts                      ← 入口（只负责初始化）
│   ├── styles.css                   ← 样式
│   ├── index.html                   ← HTML
│   │
│   ├── components/                  ← UI组件（新增文件夹）
│   │   ├── PrinterList.ts         ← 打印机列表组件
│   │   ├── LogPanel.ts            ← 日志面板组件
│   │   └── Header.ts              ← 头部组件
│   │
│   ├── services/                    ← 业务逻辑（新增文件夹）
│   │   ├── printerService.ts      ← 打印机相关API调用
│   │   └── logService.ts          ← 日志服务
│   │
│   ├── utils/                       ← 工具函数（新增文件夹）
│   │   └── helpers.ts             ← 通用工具
│   │
│   └── i18n/
│       └── index.ts
│
├── src-tauri/
│   ├── src/
│   │   ├── main.rs                  ← 入口（只注册命令）
│   │   ├── lib.rs                   ← 库入口（新增）
│   │   │
│   │   ├── commands/                ← 命令处理（新增文件夹）
│   │   │   ├── mod.rs             ← 命令汇总
│   │   │   ├── printer.rs         ← 打印机命令
│   │   │   └── system.rs          ← 系统命令
│   │   │
│   │   ├── services/                ← 业务逻辑（新增文件夹）
│   │   │   ├── mod.rs
│   │   │   ├── printer_detector.rs ← 打印机检测
│   │   │   ├── airprint_server.rs  ← AirPrint服务
│   │   │   └── ipp_handler.rs      ← IPP协议处理
│   │   │
│   │   └── models/                  ← 数据结构（新增文件夹）
│   │       ├── mod.rs
│   │       └── printer.rs          ← 打印机模型
│   │
│   └── Cargo.toml
```