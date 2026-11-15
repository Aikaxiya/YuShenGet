param(
    [string]$Uid = '114709792'
)

$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Drawing
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$types = @'
using System;
using System.Runtime.InteropServices;

public static class W32j
{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
'@
Add-Type -TypeDefinition $types
[W32j]::SetProcessDPIAware() | Out-Null

$env:TEYVAT_START_ROUTE = "/analysis/$Uid"

$exe = 'D:\java\RustProject\target\release\teyvat-gacha-desktop.exe'
$proc = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 12

$handle = $proc.MainWindowHandle
$rect = New-Object 'W32j+RECT'
[W32j]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
Write-Output "window=${width}x${height}"

if ($width -gt 0 -and $height -gt 0) {
    $path = 'D:\java\RustProject\tools\ui-analysis.png'
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    [W32j]::PrintWindow($handle, $hdc, 2) | Out-Null
    $graphics.ReleaseHdc($hdc)
    $graphics.Dispose()
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    Write-Output "shot=$((Get-Item $path).Length)"
}

Stop-Process -Id $proc.Id -Force
Write-Output '--- avatars ---'
Get-ChildItem 'D:\java\RustProject\target\release\data\avatars' -ErrorAction SilentlyContinue |
    Select-Object Name, Length
