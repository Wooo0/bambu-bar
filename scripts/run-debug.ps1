# Bambu Bar 排错脚本：带调试日志重启应用，退出后把日志留在桌面。
# 用法：右键「使用 PowerShell 运行」，或
#       powershell -ExecutionPolicy Bypass -File run-debug.ps1
# 复现问题后关闭应用，把桌面上的 bambu-bar-debug.log 发回来即可。

$ErrorActionPreference = 'Continue'

$exe = Join-Path $env:LOCALAPPDATA 'Bambu Bar\bambu-bar.exe'
if (-not (Test-Path $exe)) {
    # 非默认路径安装时兜底找一下
    $exe = Get-ChildItem "$env:LOCALAPPDATA\Bambu Bar*\bambu-bar.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $exe) {
    Write-Host '未找到已安装的 Bambu Bar，请先安装或把脚本放到安装目录旁运行。'
    Read-Host '按回车退出'
    exit 1
}

# 1) 采集环境信息
$log = Join-Path ([Environment]::GetFolderPath('Desktop')) 'bambu-bar-debug.log'
"===== Bambu Bar 排错信息 $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') =====" | Out-File $log -Encoding utf8

"--- 进程 ---" | Out-File $log -Append utf8
Get-Process bambu-bar -ErrorAction SilentlyContinue |
    Select-Object Id, ProcessName, Path, StartTime | Format-List | Out-File $log -Append utf8

"--- WebView2 Runtime ---" | Out-File $log -Append utf8
$wv2 = Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
if (-not $wv2) { $wv2 = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue }
"WebView2 版本: $($wv2.pv)" | Out-File $log -Append utf8

# 2) 杀掉已在运行的实例（含提权残留时本脚本需以管理员运行）
$running = Get-Process bambu-bar -ErrorAction SilentlyContinue
if ($running) {
    "发现 $($running.Count) 个运行中的实例，尝试结束..." | Out-File $log -Append utf8
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
}

# 3) 带调试开关重启，stderr 全量落盘
$env:BAMBU_BAR_DEBUG_TRAY = '1'
"--- 应用输出（BAMBU_BAR_DEBUG_TRAY=1）---" | Out-File $log -Append utf8
$proc = Start-Process -FilePath $exe -PassThru -RedirectStandardError $log -RedirectStandardOutput "$log.out"
Wait-Process -Id $proc.Id -ErrorAction SilentlyContinue

Write-Host ''
Write-Host "应用已退出，日志保存在: $log"
Write-Host '请把该文件发给开发者。'
Read-Host '按回车关闭'
