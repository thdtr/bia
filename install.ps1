# Installs BIA on Windows.
#
#   irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
#
# To uninstall:
#   $env:BIA_UNINSTALL = 1; irm https://raw.githubusercontent.com/thdtr/bia/main/install.ps1 | iex
#
# Installs for the current user only. No administrator rights needed.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'thdtr/bia'
$target = 'x86_64-pc-windows-msvc'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\bia'
$shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\BIA.lnk'
$version = if ($env:BIA_VERSION) { $env:BIA_VERSION } else { 'latest' }

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }
function Set-UserPath($value) { [Environment]::SetEnvironmentVariable('Path', $value, 'User') }

if ($env:BIA_UNINSTALL) {
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    Remove-Item -Force $shortcut -ErrorAction SilentlyContinue
    $rest = (Get-UserPath) -split ';' | Where-Object { $_ -and $_ -ne $dir }
    Set-UserPath ($rest -join ';')
    Write-Host 'BIA was removed. Your settings are still in %APPDATA%\bia.'
    return
}

$base = if ($version -eq 'latest') {
    "https://github.com/$repo/releases/latest/download"
} else {
    "https://github.com/$repo/releases/download/$version"
}
$archive = "bia-$target.zip"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("bia-" + [guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null

try {
    Write-Host "Downloading BIA ($target)..."
    Invoke-WebRequest "$base/$archive" -OutFile "$tmp\$archive" -UseBasicParsing
    Invoke-WebRequest "$base/$archive.sha256" -OutFile "$tmp\$archive.sha256" -UseBasicParsing

    $expected = ((Get-Content "$tmp\$archive.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash "$tmp\$archive" -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) {
        throw "Checksum mismatch, refusing to install (expected $expected, got $actual)"
    }

    Expand-Archive "$tmp\$archive" -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item "$tmp\bia-$target\*" $dir -Recurse -Force

    if (((Get-UserPath) -split ';') -notcontains $dir) {
        Set-UserPath (((Get-UserPath), $dir | Where-Object { $_ }) -join ';')
    }

    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($shortcut)
    $link.TargetPath = Join-Path $dir 'bia.exe'
    $link.IconLocation = Join-Path $dir 'bia.ico'
    $link.Description = 'BIA'
    $link.Save()

    Write-Host "Installed BIA in $dir and added it to the Start menu."
    Write-Host 'Open a new terminal and run: bia <folder>'
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
