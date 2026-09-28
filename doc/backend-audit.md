# 后端审核与本地模拟记录

日期：2026-09-23。目标：将 PC 已安装驱动的实体队列发布到局域网，供 iPhone/iPad/Mac 通过 IPP/AirPrint 直接打印。

## 审核结果与修复

| 原问题 | 当前实现 |
| --- | --- |
| 接收后弹出 PrintDialog，失败时调用默认程序打开文件 | Windows.Data.Pdf / 图像或光栅解码 → GDI → 指定 Windows 队列；没有浏览器或打印对话框降级 |
| 全局只有一个打印机名称，所有广播共用 `/ipp/print` | 每队列独立、稳定且有长度限制的资源路径；路由和作业都绑定实际 Windows 队列 |
| mDNS 基础服务和子类型重复注册，SRV 主机名混用服务名 | 同一个服务实例关联 `_universal` PTR；SRV 指向正常 `.local.` 主机名；自动更新 IPv4 地址 |
| 每分钟注销再注册，停止共享不完整 | 由 mDNS daemon 维护记录；按队列注销并等待确认，最后关闭监听与 daemon |
| 广播不存在的 AirPrinter255、PDF/OneNote 等交互队列 | 移除虚构队列入口；正式列表过滤已知交互目的地 |
| 假称支持 URF/JPEG，所有数据都保存为 `.pdf` | 按 MIME 和签名校验；真正解码 URF、PWG Raster、JPEG、PDF |
| 尚未打印就返回 job-state=completed | 先 pending，再 processing；查询 Windows spooler，驱动失败时 aborted |
| 作业 ID 复用 request-id，取消和查询只是空响应 | 独立递增 ID，Create/Send/Get/Cancel 与队列绑定，取消投递到 Windows |
| 彩色/双面/份数等能力硬编码 | 查询 Windows 驱动能力；不支持的选项返回错误；纸张按驱动尺寸映射 |
| 中文名称按字节切片可能 panic | UTF-8 边界截断；队列原名保存，URI 使用 UUID v5 |
| 诊断脚本对合法 DNS RR 解包失败 | 重写边界检查、压缩指针、PTR/SRV/TXT/A/AAAA 解析，覆盖循环指针和截断包 |
| README 宣称完整 IPP Everywhere、建议关闭防火墙 | 明确实际实现范围；提供按 EXE、专用网络、本地子网放行的脚本 |

## 已获得的证据

- 后端测试二进制 `airprinter-6d9768ab92ef346f.exe --include-ignored --nocapture`：12 passed，0 failed，0 ignored。
- 默认单元/协议测试覆盖 UTF-8、队列状态、不同打印机的 URI/名称、撤销后 404、监听端口释放、自定义纸型、Create/Cancel/Validate、发送文档边界、驱动失败不报完成、光栅像素及截断数据。
- 本机 mDNS 真实网络测试：两个明确命名的模拟实例分别解析到对应资源/端口；停止 A 后收到 goodbye，B 保留；最终均清理。
- 四种文档格式都通过 Microsoft Print to PDF 静默输出。PDF 测试使用首次生成的 PDF 再渲染打印，验证 Windows PDF 渲染器实际被调用。
- 完整本地 HTTP 测试：发送 URF Print-Job，返回 pending，通过 Get-Job-Attributes 等待 Windows 队列完成，生成 `src-tauri/target/airprint-simulation/http-urf-13664.pdf`。
- 最终测试生成的 `http-urf-13664.pdf` 经 Poppler 检查为 A4、1 页、PDF 1.7；渲染后人工检查红、绿、蓝、白四个区域的位置和颜色正确。
- `python scripts/diagnose_mdns.py --self-test` 通过。
- `pnpm --config.verify-deps-before-run=false run build` 通过 TypeScript 与 Vite 构建。
- `cargo build --manifest-path src-tauri/Cargo.toml --features custom-protocol --locked --offline` 通过；生成内嵌前端的 `src-tauri/target/debug/airprinter.exe`。
- Windows 当前检测结果为 `[]`：只有 PDF/XPS/OneNote/Fax 等虚拟目的地，没有连接实体打印机，与用户提供的状态一致。

模拟输出是测试制品；正式网络请求不能指定输出文件，也不能启用测试后端。测试使用 Microsoft 虚拟队列，无实体打印。

## 尚未证明的最终条件

1. 另一台实际 iPhone/iPad/Mac 在系统打印面板中发现此 PC 的共享队列。当前 mDNS 测试是在本机运行，不能证明跨 Wi-Fi/网线、路由器或防火墙可达。
2. 实体打印机正确出纸，包括多页、双面、份数、纸型及断纸/离线/取消行为。Windows 队列完成或作业消失本身不证明纸张已输出。
3. 全部 Apple 系统版本和应用的互操作性，以及 IPP Everywhere 1.1 的完整一致性/认证。项目没有将这些声明为已完成。

## 连接硬件后的验收

- Windows 本地测试页正常后，在 AirPrinter 列表选择真实队列共享。
- 从另一台局域网电脑运行 `python scripts/diagnose_mdns.py`，核对 `_universal` PTR、SRV 地址/端口、TXT `rp`。
- 分别从 iPhone/iPad/Mac 系统打印面板发送 PDF、照片和多页文件，核对内容、页数、颜色、方向、份数、双面和纸型。
- 两台队列并行提交，确认不会串队列；停止一台后新作业被拒绝且另一个仍可用。
- 测试离线、缺纸、取消、重新连接、PC 地址变化。跨设备无法发现时先检查专用网络放行规则与路由器隔离，不关闭整个防火墙。

相关实现：`src-tauri/src/services/{mdns_broadcaster.rs,ipp/server.rs,print_job.rs,raster.rs,windows_print.ps1,windows_printers.ps1}`。

## 续测：记住共享选择与隔离 JS 客户端

- 程序将已选择的 Windows 队列名原子写入用户配置目录；后台每 30 秒检测一次队列。离线或暂时消失时撤回 IPP/mDNS，队列恢复后按已保存的选择重新发布；手动“停止共享”会删除记忆。
- Windows 短时间保留 TCP 631，导致立即重启时绑定失败。服务改为在 631 已被占用时选择 8631–8699 中的空闲端口，并把实际端口公布在 DNS-SD SRV 记录中。防火墙脚本覆盖这些端口。
- 更新后的本地后端测试：14 passed，0 failed。新增测试覆盖选择持久化、模拟离线与重连、驱动能力刷新以及 631 被占用时选择可访问的备用端口。
- `node scripts/simulate_airprint.mjs 172.22.48.1` 通过。Rust 模拟服务限定在主机专用虚拟网卡；独立 Node.js 进程验证真实 IPP 属性、拒绝无效打印份数及错误 Host。该虚拟交换机不会把多播回送给本机，因此 JS 的 DNS-SD 响应由隔离 UDP 回环模拟器提供；这项结果不证明真实 mDNS 跨设备可见。
- 仍没有实体打印机，也没有实际 Apple 设备的系统打印发现及出纸证据；最终目标保持待验收。

## 续查：多网卡地址和 Windows Public 网络

- 检查 `mdns-sd` v0.11.5 源码：`ServiceInfo::get_addrs_on_intf` 按本机接口掩码筛选同网段地址。真实 WLAN 上的 192.168.3.249/24 不会将 Hyper-V 的 172.22.48.1 发布为该接口的 A 记录。
- 本机 WLAN 当前为 Windows `Public`，且未发现 AirPrinter 专属防火墙规则；即使 mDNS 守护进程在本机宣布成功，也不能据此推断其他设备可发现。
- 放行脚本现覆盖 Private/Public，两种配置都仅允许指定 AirPrinter EXE 接收本地子网发来的 TCP 631（及 8631–8699 备用端口）和 UDP 5353。应用增加“允许局域网访问”按钮，只有用户点击并接受 Windows 提权提示才会应用规则。本轮仅验证脚本语法及构建，**没有修改本机防火墙**。
- 仍需实体打印机、Apple 设备以及跨设备的网络验证，不能将本机模拟视为最终完成。

## 规范属性复核

对照 `cs-ippeve11-20200515-5100.14.pdf` 第 29–31、36、52 页：

- 修正 `multiple-operation-timeout` 名称；增加与实际查询实现一致的 `which-jobs-supported` 和打印机状态消息。
- 彩色队列使用 `print-color-mode-default=auto`；Windows 后端按实际 `SupportsColor` 决定默认输出。
- DNS-SD 增加型号、文档类别、优先级和按驱动计算的份数标志，保留 `rp` 在前；长中文队列名仍保持 TXT 总长 ≤ 400 字节。
- 文档第 52 页明确说明 `printer-device-id` 已不再必需，因此没有虚构实体打印机厂商及型号。
- 当前实现仍不满足表 4、表 5 的所有 IPP Everywhere 条目，没有宣称完整认证，也未发布规范要求的 `_print` 子类型。Apple 的 `_universal` 发现路径和跨设备打印仍需实机验证。
- 变更后本地全量模拟测试：16 passed，0 failed。

## 续查：IPP URI 的默认端口

- `ipp` crate 的序列化已保证响应操作属性从 `attributes-charset`、`attributes-natural-language` 开始，排除了属性顺序这一怀疑。
- 按 RFC 3510 与 RFC 8010，IPP URI 可省略默认 631；RFC 8010 的直接请求示例仍以 `Host: printer.example.com:631` 传输。原校验把两者用同一规则处理，合法的无端口 `printer-uri` 会被误判为 404。
- 现按 URI 默认端口 631 校验 `printer-uri` 和 `job-uri`，HTTP `Host` 仍按连接的实际端口检查；备用端口 8631–8699 的 URI 必须显式包含端口。
- 覆盖能力查询、Create-Job、按 Job URI 查询，以及 Bonjour `.local` 名称。更新后本地模拟测试 17 passed，0 failed。跨 Apple 设备和实体出纸仍待验收。
