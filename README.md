# AirPrinter - AirPrint 打印服务器

将 Windows 上的 USB 打印机共享为 AirPrint 打印机，让 iPhone/iPad/macOS 可以直接发现并打印。

## 功能特性

- 🔍 自动检测系统打印机
- 📡 mDNS/Bonjour 服务广播（AirPrint 兼容）
- 📄 支持 PDF、JPEG、URF 格式
- 🖨️ 支持双面打印、多份打印
- 🌐 支持 IPP Everywhere™ 协议

## 系统要求

- Windows 10/11
- 同一局域网内的 iOS/macOS 设备
- USB 打印机（已安装 Windows 驱动）

## 开发环境

### 推荐 IDE

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

### 构建命令

```bash
# 开发运行（带调试功能、热重载）
pnpm tauri dev

# 打包发布（生成安装包）
pnpm tauri build

# 调试打包（不优化，用于测试）
pnpm tauri build --debug

# 仅前端开发（浏览器调试）
pnpm dev
```

### 常用命令

| 命令                         | 用途    | 说明          |
| -------------------------- | ----- | ----------- |
| `pnpm tauri dev`           | 开发运行  | 带调试功能，热重载   |
| `pnpm tauri build`         | 打包发布  | 生成安装包       |
| `pnpm tauri build --debug` | 调试打包  | 打包但不优化，用于测试 |
| `pnpm dev`                 | 仅前端开发 | 在浏览器中调试前端   |

## 项目结构

```
airprinter/
├── src/                             ← 前端 (TypeScript)
│   ├── main.ts                      ← 入口
│   ├── components/                  ← UI组件
│   │   ├── PrinterList.ts         ← 打印机列表
│   │   ├── LogPanel.ts            ← 日志面板
│   │   └── Header.ts              ← 头部组件
│   ├── services/                    ← 业务逻辑
│   │   ├── printerService.ts      ← 打印机API
│   │   └── logService.ts          ← 日志服务
│   └── i18n/                        ← 国际化
│
├── src-tauri/                       ← 后端 (Rust)
│   ├── src/
│   │   ├── main.rs                  ← 入口
│   │   ├── commands/                ← 命令处理
│   │   ├── services/                ← 业务逻辑
│   │   │   ├── mdns_broadcaster.rs ← mDNS广播
│   │   │   ├── ipp/server.rs       ← IPP协议服务器
│   │   │   └── airprint_server.rs  ← AirPrint服务
│   │   └── models/                  ← 数据结构
│   └── Cargo.toml
│
└── scripts/                         ← 诊断脚本
    ├── diagnose_mdns.py            ← mDNS诊断
    └── diagnose_network.ps1        ← 网络诊断
```

## ⚠️ 使用注意事项

### 1. 网络环境要求

- **同一局域网**：手机和电脑必须连接到**同一个 Wi-Fi 路由器**
- **关闭 AP 隔离**：路由器不能开启"客户端隔离"或"AP Isolation"
- **避免 169.254.x.x**：如果电脑获取到链路本地地址，mDNS 广播可能无法正常工作

### 2. 防火墙设置

Windows 防火墙需要放行以下端口：

```powershell
# 以管理员身份运行 PowerShell
ipconfig /flushdns

# 允许 mDNS 入站 (UDP 5353)
netsh advfirewall firewall add rule name="mDNS AirPrint" dir=in action=allow protocol=udp localport=5353

# 允许 IPP 端口 (TCP 631)
netsh advfirewall firewall add rule name="IPP Server" dir=in action=allow protocol=tcp localport=631
```

**临时测试可关闭防火墙**：
```powershell
netsh advfirewall set allprofiles state off
# 测试完后记得开启：netsh advfirewall set allprofiles state on
```

### 3. AirPrint 服务发现机制

本应用实现了完整的 **IPP Everywhere™ v1.1** 规范，注册以下 mDNS 服务：

| 服务类型 | 端口 | 说明 |
|---------|------|------|
| `_ipp._tcp` | 631 | 基础 IPP 服务 |
| `_printer._tcp` | 0 | RFC 6763 Flagship Naming（端口0表示不支持LPD）|
| `_print._sub._ipp._tcp` | 631 | IPP Everywhere™ 子类型（iOS系统打印必需）|

**注意**：3 个服务必须使用**相同的实例名称**，例如：
- `air-PrinterName._ipp._tcp.local.`
- `air-PrinterName._printer._tcp.local.`
- `air-PrinterName._print._sub._ipp._tcp.local.`

### 4. IPP 必需属性

为确保 iOS 系统打印能发现打印机，IPP `Get-Printer-Attributes` 响应必须包含：

```
ipp-features-supported = ipp-everywhere
ipp-versions-supported = 1.1, 2.0
printer-device-id      = MFG:...;MDL:...;CMD:...
media-supported        = iso_a4_210x297mm, ...
sides-supported        = one-sided, two-sided-long-edge, ...
urf-supported          = V1.4, CP1, DM1, ...
```

### 5. 故障排除

#### 问题：Discovery App 能发现，但 iOS 系统打印无法发现

**解决方案**：
1. 检查是否注册了 `_print._sub._ipp._tcp` 子类型
2. 检查 IPP 响应是否包含 `ipp-features-supported = ipp-everywhere`
3. 确认所有 mDNS 服务使用相同的实例名称

#### 问题：手机完全无法发现打印机

**排查步骤**：
1. 确认手机和电脑在同一 Wi-Fi 下
2. 检查 Windows 防火墙是否放行 UDP 5353 和 TCP 631
3. 检查路由器是否开启 AP 隔离
4. 运行诊断脚本：`scripts/diagnose_network.ps1`
5. 使用 Discovery App 检查是否能发现 `_ipp._tcp` 服务

#### 问题：能发现但无法打印

**排查步骤**：
1. 检查 IPP 服务器是否监听在 `0.0.0.0:631`
2. 检查 Windows 打印机驱动是否正常工作
3. 查看应用日志中的 IPP 请求处理情况
4. 尝试用浏览器访问 `http://<电脑IP>:631/ipp/print`

### 6. 参考资料

- [IPP Everywhere™ v1.1 规范 (PWG 5100.14-2020)](https://ftp.pwg.org/pub/pwg/fsg/jobticket/IPP%20Everywhere%20v1.1.pdf)
- [RFC 6763 - DNS-Based Service Discovery](https://tools.ietf.org/html/rfc6763)
- [AirPrint 官方文档](https://support.apple.com/zh-cn/HT201311)

## 许可证

[LICENSE](LICENSE)
