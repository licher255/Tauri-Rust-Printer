# Tauri + Vanilla TS

This template should help get you started developing with Tauri in vanilla HTML, CSS and Typescript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## dev and build
```
pnpm tauri dev
pnpm tauri build

```

| 命令                         | 用途    | 说明          |
| -------------------------- | ----- | ----------- |
| `pnpm tauri dev`           | 开发运行  | 带调试功能，热重载   |
| `pnpm tauri build`         | 打包发布  | 生成安装包       |
| `pnpm tauri build --debug` | 调试打包  | 打包但不优化，用于测试 |
| `pnpm dev`                 | 仅前端开发 | 在浏览器中调试前端   |
