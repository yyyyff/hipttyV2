#Requires -Version 5.1
<#
.SYNOPSIS
  hiptty 一键安装 / 卸载脚本（Windows PowerShell）

.DESCRIPTION
  从 GitHub Releases 下载预编译包并安装 hiptty / hiptty-cli。
  已安装时可选择升级或卸载；卸载需二次确认。

.EXAMPLE
  # 安装 / 升级
  irm https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.ps1 | iex

.EXAMPLE
  # 卸载
  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/yyyyff/hipttyV2/main/install.ps1))) -Uninstall

.EXAMPLE
  # 本地运行
  .\install.ps1
  .\install.ps1 -Uninstall
  .\install.ps1 -Force
#>

[CmdletBinding()]
param(
    [switch]$Uninstall,
    [switch]$Force,
    [string]$InstallDir = "",
    [string]$Version = "latest",
    [string]$Repo = "yyyyff/hipttyV2"
)

$ErrorActionPreference = "Stop"

function Write-Info  { param([string]$Msg) Write-Host $Msg }
function Write-Ok    { param([string]$Msg) Write-Host $Msg -ForegroundColor Green }
function Write-Warn  { param([string]$Msg) Write-Host $Msg -ForegroundColor Yellow }
function Write-Err   { param([string]$Msg) Write-Host "错误：$Msg" -ForegroundColor Red }

function Get-DefaultInstallDir {
    if ($env:HIPTTY_INSTALL_DIR) { return $env:HIPTTY_INSTALL_DIR }
    if ($InstallDir) { return $InstallDir }
    $base = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $env:USERPROFILE "AppData\Local" }
    return (Join-Path $base "hiptty")
}

function Get-ConfigDir {
    if ($env:HIPTTY_CONFIG) { return $env:HIPTTY_CONFIG }
    if ($env:XDG_CONFIG_HOME) { return (Join-Path $env:XDG_CONFIG_HOME "hiptty") }
    return (Join-Path $env:USERPROFILE ".config\hiptty")
}

function Get-BinPaths {
    param([string]$Dir)
    return @(
        (Join-Path $Dir "hiptty.exe"),
        (Join-Path $Dir "hiptty-cli.exe")
    )
}

function Test-IsInstalled {
    param([string]$Dir)
    foreach ($p in (Get-BinPaths $Dir)) {
        if (Test-Path -LiteralPath $p) { return $true }
    }
    return $false
}

function Get-InstalledPaths {
    param([string]$Dir)
    $found = @()
    foreach ($p in (Get-BinPaths $Dir)) {
        if (Test-Path -LiteralPath $p) { $found += $p }
    }
    return $found
}

function Get-InstalledVersion {
    param([string]$Dir)
    $bin = Join-Path $Dir "hiptty.exe"
    if (Test-Path -LiteralPath $bin) {
        try {
            $out = & $bin --version 2>$null
            if ($out) { return ($out | Out-String).Trim() }
        } catch {}
    }
    return "未知（位于 $Dir）"
}

function Confirm-Yes {
    param([string]$Prompt)
    $reply = Read-Host $Prompt
    return ($reply -match '^(?i:y|yes|是)$')
}

function Get-AssetUrl {
    param(
        [string]$Target,
        [string]$Ver,
        [string]$Repository
    )
    $asset = "hiptty-$Target.zip"
    if ($Ver -eq "latest") {
        return "https://github.com/$Repository/releases/latest/download/$asset"
    }
    $tag = if ($Ver.StartsWith("v")) { $Ver } else { "v$Ver" }
    return "https://github.com/$Repository/releases/download/$tag/$asset"
}

function Add-UserPathIfMissing {
    param([string]$Dir)
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $userPath) { $userPath = "" }
    $parts = $userPath -split ';' | Where-Object { $_ -ne "" }
    $exists = $parts | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') }
    if ($exists) {
        return $false
    }
    $newPath = if ($userPath.TrimEnd(';')) { "$userPath;$Dir" } else { $Dir }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    # 当前会话也生效
    if ($env:Path -notlike "*$Dir*") {
        $env:Path = "$env:Path;$Dir"
    }
    return $true
}

function Install-Hiptty {
    param(
        [string]$Dir,
        [string]$Ver,
        [string]$Repository
    )

    $target = "x86_64-pc-windows-msvc"
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -match 'ARM64') {
        throw "暂未发布 Windows ARM64 预编译包"
    }

    $url = Get-AssetUrl -Target $target -Ver $Ver -Repository $Repository
    $zipName = "hiptty-$target.zip"

    Write-Info "正在安装 hiptty"
    Write-Info "  仓库：  $Repository"
    Write-Info "  版本：  $Ver"
    Write-Info "  目标：  $target"
    Write-Info "  目录：  $Dir"
    Write-Info "  地址：  $url"
    Write-Info ""

    if (-not (Test-Path -LiteralPath $Dir)) {
        New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    }

    $tmpdir = Join-Path ([System.IO.Path]::GetTempPath()) ("hiptty-install-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $tmpdir -Force | Out-Null
    try {
        $zipPath = Join-Path $tmpdir $zipName
        Write-Info "正在下载…"
        # TLS 1.2 for older Windows PowerShell
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        } catch {}

        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

        Write-Info "正在解压…"
        Expand-Archive -LiteralPath $zipPath -DestinationPath $tmpdir -Force

        $stagingBin = Get-ChildItem -Path $tmpdir -Recurse -Filter "hiptty.exe" -File | Select-Object -First 1
        if (-not $stagingBin) {
            throw "压缩包中未找到 hiptty.exe"
        }
        $staging = $stagingBin.DirectoryName
        $cli = Join-Path $staging "hiptty-cli.exe"
        if (-not (Test-Path -LiteralPath $cli)) {
            throw "压缩包中未找到 hiptty-cli.exe"
        }

        Write-Info "正在安装到 $Dir…"
        Copy-Item -LiteralPath (Join-Path $staging "hiptty.exe") -Destination (Join-Path $Dir "hiptty.exe") -Force
        Copy-Item -LiteralPath $cli -Destination (Join-Path $Dir "hiptty-cli.exe") -Force

        Write-Ok "已安装："
        Write-Info "  $(Join-Path $Dir 'hiptty.exe')"
        Write-Info "  $(Join-Path $Dir 'hiptty-cli.exe')"

        if (Add-UserPathIfMissing -Dir $Dir) {
            Write-Ok "已将安装目录加入用户 PATH（新开终端后生效，当前会话已临时加入）。"
        } else {
            Write-Info "安装目录已在用户 PATH 中。"
        }

        Write-Info ""
        Write-Warn "未做代码签名时，SmartScreen 可能提示未知发布者，属预期现象。"
        Write-Info ""
        Write-Ok "完成。运行：hiptty"
    }
    finally {
        if (Test-Path -LiteralPath $tmpdir) {
            Remove-Item -LiteralPath $tmpdir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Uninstall-Hiptty {
    param([string]$Dir)

    $paths = @(Get-InstalledPaths $Dir)
    if ($paths.Count -eq 0) {
        Write-Warn "在 $Dir 下没有可卸载的文件"
        Write-Info "  查找：$(Join-Path $Dir 'hiptty.exe')"
        Write-Info "        $(Join-Path $Dir 'hiptty-cli.exe')"
        return
    }

    $configDir = Get-ConfigDir

    Write-Host "卸载 hiptty" -ForegroundColor White
    Write-Info ""
    Write-Info "将删除以下文件："
    foreach ($p in $paths) {
        Write-Info "  - $p"
    }
    Write-Info ""
    Write-Info "默认不会删除用户数据："
    Write-Info "  - $configDir\   （设置、登录凭证、会话）"
    Write-Info ""

    if (-not (Confirm-Yes "确认继续卸载？[y/N]")) {
        Write-Warn "已取消。"
        return
    }

    Write-Info ""
    Write-Warn "需要二次确认。"
    Write-Info "请输入 yes 以永久删除上方列出的文件。"
    $reply = Read-Host "请输入 yes 确认"
    if ($reply -ne "yes") {
        Write-Warn "已取消（需精确输入：yes）。"
        return
    }

    foreach ($p in $paths) {
        Remove-Item -LiteralPath $p -Force
        Write-Ok "已删除 $p"
    }

    if (Test-Path -LiteralPath $configDir) {
        Write-Info ""
        if (Confirm-Yes "是否同时删除配置目录 $configDir ？[y/N]") {
            Write-Info "请再输入 yes 确认删除配置（含凭证与会话）。"
            $reply2 = Read-Host "请输入 yes 确认删除配置"
            if ($reply2 -eq "yes") {
                Remove-Item -LiteralPath $configDir -Recurse -Force
                Write-Ok "已删除 $configDir"
            } else {
                Write-Warn "已保留配置目录。"
            }
        } else {
            Write-Info "已保留配置目录。"
        }
    }

    Write-Info ""
    Write-Ok "卸载完成。"
    Write-Info "提示：用户 PATH 中的安装目录条目未自动移除，可按需手动清理。"
}

function Show-InstalledMenu {
    param(
        [string]$Dir,
        [string]$Ver,
        [string]$Repository
    )
    Write-Info "检测到 hiptty 已安装于 $Dir"
    Write-Info "  当前版本：$(Get-InstalledVersion $Dir)"
    Write-Info ""
    Write-Info "请选择操作："
    Write-Info "  1) 重新安装 / 升级  （默认）"
    Write-Info "  2) 卸载"
    Write-Info "  3) 取消"
    $choice = Read-Host "请输入 [1/2/3]"
    if (-not $choice) { $choice = "1" }
    switch ($choice) {
        "2" { Uninstall-Hiptty -Dir $Dir }
        { $_ -in @("3", "q", "Q", "n", "N") } {
            Write-Warn "已取消。"
        }
        "1" { Install-Hiptty -Dir $Dir -Ver $Ver -Repository $Repository }
        default { throw "无效选项：$choice" }
    }
}

# --- main ---
$ver = if ($env:HIPTTY_VERSION) { $env:HIPTTY_VERSION } else { $Version }
$repo = if ($env:HIPTTY_REPO) { $env:HIPTTY_REPO } else { $Repo }
$forceEnv = ($env:HIPTTY_FORCE -eq "1") -or $Force
$dir = Get-DefaultInstallDir

if ($Uninstall) {
    Uninstall-Hiptty -Dir $dir
    return
}

if ((Test-IsInstalled $dir) -and -not $forceEnv) {
    Show-InstalledMenu -Dir $dir -Ver $ver -Repository $repo
} else {
    Install-Hiptty -Dir $dir -Ver $ver -Repository $repo
}
