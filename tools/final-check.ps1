$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Drawing
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$types = @'
using System;
using System.Runtime.InteropServices;

public static class W32g
{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
'@
Add-Type -TypeDefinition $types

function Save-WindowShot {
    param([IntPtr]$Handle, [string]$Path)
    $rect = New-Object 'W32g+RECT'
    [W32g]::GetWindowRect($Handle, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) {
        return $false
    }
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    [W32g]::PrintWindow($Handle, $hdc, 2) | Out-Null
    $graphics.ReleaseHdc($hdc)
    $graphics.Dispose()
    $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    return $true
}

$appDir = 'D:\java\RustProject\target\release'
$dataDir = Join-Path $appDir 'data'
$marker = Join-Path $dataDir 'frontend-ready.log'
Remove-Item $marker -ErrorAction SilentlyContinue

$proc = Start-Process -FilePath (Join-Path $appDir 'teyvat-gacha-desktop.exe') -PassThru
Start-Sleep -Seconds 4
$shell = New-Object -ComObject WScript.Shell
$shell.AppActivate($proc.Id) | Out-Null
Start-Sleep -Seconds 4

Write-Output '--- frontend-ready.log ---'
if (Test-Path $marker) {
    Get-Content $marker -Encoding utf8
} else {
    Write-Output 'NO_MARKER'
}

$handle = $proc.MainWindowHandle
Save-WindowShot -Handle $handle -Path 'D:\java\RustProject\tools\page-query.png' | Out-Null
Write-Output "query shot bytes=$((Get-Item 'D:\java\RustProject\tools\page-query.png').Length)"

# 点历史记录条目，进入分析页（坐标取自上一张截图的布局）
$rect = New-Object 'W32g+RECT'
[W32g]::GetWindowRect($handle, [ref]$rect) | Out-Null
$clickX = $rect.Left + 248
$clickY = $rect.Top + 612
[W32g]::SetCursorPos($clickX, $clickY) | Out-Null
Start-Sleep -Milliseconds 200
[W32g]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
[W32g]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Seconds 3

Save-WindowShot -Handle $handle -Path 'D:\java\RustProject\tools\page-analysis.png' | Out-Null
Write-Output "analysis shot bytes=$((Get-Item 'D:\java\RustProject\tools\page-analysis.png').Length)"

Write-Output '--- log after click ---'
if (Test-Path $marker) {
    Get-Content $marker -Encoding utf8 | Select-Object -Last 5
}

if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
}
