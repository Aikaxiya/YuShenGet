$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# 当前进程环境变量里还没有新的用户 PATH，显式补齐
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path

Set-Location 'D:\java\RustProject\apps\desktop'
& npx tauri build --no-bundle 2>&1 | Select-Object -Last 25
