# AirPrinter 网络诊断脚本
param()

Write-Host "==============================================" -ForegroundColor Cyan
Write-Host "AirPrinter 网络诊断工具" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan
Write-Host ""

# 检查管理员权限
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")
if (-not $isAdmin) {
    Write-Host "⚠️  警告: 没有以管理员身份运行" -ForegroundColor Yellow
    Write-Host "    某些诊断可能需要管理员权限" -ForegroundColor Yellow
    Write-Host ""
}

# 1. 检查网络接口
Write-Host "[1/5] 检查网络接口..." -ForegroundColor Green
try {
    $interfaces = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { 
        $_.IPAddress -notlike "127.*" -and 
        $_.IPAddress -notlike "169.254.*"
    }

    if ($interfaces) {
        Write-Host "✅ 找到以下活动网络接口:" -ForegroundColor Green
        foreach ($iface in $interfaces) {
            Write-Host "    接口: $($iface.InterfaceAlias)" -ForegroundColor White
            Write-Host "    IP: $($iface.IPAddress)/$($iface.PrefixLength)" -ForegroundColor White
        }
    } else {
        Write-Host "❌ 没有找到有效的 IPv4 网络接口" -ForegroundColor Red
        Write-Host "    请检查网络连接" -ForegroundColor Yellow
    }
} catch {
    Write-Host "❌ 获取网络接口失败: $($_.Exception.Message)" -ForegroundColor Red
}
Write-Host ""

# 2. 检查端口 631 占用
Write-Host "[2/5] 检查端口 631 占用情况..." -ForegroundColor Green
try {
    $port631 = Get-NetTCPConnection -LocalPort 631 -ErrorAction SilentlyContinue
    if ($port631) {
        Write-Host "⚠️  端口 631 已被占用:" -ForegroundColor Yellow
        foreach ($conn in $port631) {
            $process = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
            Write-Host "    PID: $($conn.OwningProcess), 进程: $($process.ProcessName)" -ForegroundColor White
        }
    } else {
        Write-Host "✅ 端口 631 可用" -ForegroundColor Green
    }
} catch {
    Write-Host "✅ 端口 631 可用 (未被占用)" -ForegroundColor Green
}
Write-Host ""

# 3. 检查端口 5353 (mDNS) 占用
Write-Host "[3/5] 检查 mDNS 端口 5353..." -ForegroundColor Green
try {
    $port5353 = Get-NetUDPEndpoint -LocalPort 5353 -ErrorAction SilentlyContinue
    if ($port5353) {
        Write-Host "ℹ️  端口 5353 使用情况:" -ForegroundColor Cyan
        foreach ($conn in $port5353) {
            $process = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
            Write-Host "    PID: $($conn.OwningProcess), 进程: $($process.ProcessName)" -ForegroundColor White
        }
    } else {
        Write-Host "ℹ️  端口 5353 当前未被使用" -ForegroundColor Cyan
    }
} catch {
    Write-Host "ℹ️  端口 5353 当前未被使用" -ForegroundColor Cyan
}
Write-Host ""

# 4. 检查防火墙规则
Write-Host "[4/5] 检查 Windows 防火墙..." -ForegroundColor Green
try {
    $firewallRules = Get-NetFirewallRule -Direction Inbound -Enabled True -ErrorAction SilentlyContinue | 
        Where-Object { $_.DisplayName -like "*AirPrint*" -or $_.DisplayName -like "*631*" -or $_.DisplayName -like "*mDNS*" }

    if ($firewallRules) {
        Write-Host "ℹ️  找到相关防火墙规则:" -ForegroundColor Cyan
        foreach ($rule in $firewallRules) {
            Write-Host "    $($rule.DisplayName) - $($rule.Action)" -ForegroundColor White
        }
    } else {
        Write-Host "⚠️  没有找到 AirPrint 相关的防火墙规则" -ForegroundColor Yellow
    }
} catch {
    Write-Host "⚠️  无法检查防火墙规则" -ForegroundColor Yellow
}
Write-Host ""

# 5. 检查 AirPrint 应用
Write-Host "[5/5] 检查 AirPrinter 进程..." -ForegroundColor Green
try {
    $airprintProcess = Get-Process | Where-Object { 
        $_.ProcessName -like "*airprint*" -or 
        $_.ProcessName -like "*AirPrint*" 
    }

    if ($airprintProcess) {
        Write-Host "ℹ️  找到 AirPrinter 进程:" -ForegroundColor Cyan
        foreach ($proc in $airprintProcess) {
            Write-Host "    进程名: $($proc.ProcessName)" -ForegroundColor White
            Write-Host "    PID: $($proc.Id)" -ForegroundColor White
        }
    } else {
        Write-Host "⚠️  没有找到 AirPrinter 进程" -ForegroundColor Yellow
        Write-Host "    请先启动 AirPrinter 应用程序" -ForegroundColor Yellow
    }
} catch {
    Write-Host "⚠️  无法检查进程" -ForegroundColor Yellow
}
Write-Host ""

# 总结
Write-Host "==============================================" -ForegroundColor Cyan
Write-Host "诊断完成" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "如果 AirPrint 仍然无法工作，请尝试:" -ForegroundColor White
Write-Host "1. 以管理员身份运行 AirPrinter" -ForegroundColor Yellow
Write-Host "2. 暂时关闭 Windows Defender 防火墙进行测试" -ForegroundColor Yellow
Write-Host "3. 检查路由器是否阻止了多播流量" -ForegroundColor Yellow
Write-Host "4. 确保 iOS 设备和电脑在同一网络" -ForegroundColor Yellow
Write-Host ""

Pause
