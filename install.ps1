# codel00p installer for Windows.
#
#   irm https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.ps1 | iex
#
# Environment overrides:
#   $env:CODEL00P_INSTALL_DIR   install location (default: %LOCALAPPDATA%\codel00p\bin)
#   $env:CODEL00P_VERSION       release tag to install (default: latest)

$ErrorActionPreference = "Stop"

$repo = "in-th3-l00p/codel00p"
$target = "x86_64-pc-windows-msvc"
$asset = "codel00p-$target.zip"

$installDir = if ($env:CODEL00P_INSTALL_DIR) { $env:CODEL00P_INSTALL_DIR } else { "$env:LOCALAPPDATA\codel00p\bin" }
$version = if ($env:CODEL00P_VERSION) { $env:CODEL00P_VERSION } else { "latest" }

$url = if ($version -eq "latest") {
  "https://github.com/$repo/releases/latest/download/$asset"
} else {
  "https://github.com/$repo/releases/download/$version/$asset"
}

$tmp = Join-Path $env:TEMP ("codel00p-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
  Write-Host "Downloading codel00p ($target)..."
  $zip = Join-Path $tmp $asset
  Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
  Expand-Archive -Path $zip -DestinationPath $tmp -Force

  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  Copy-Item -Path (Join-Path $tmp "codel00p.exe") -Destination (Join-Path $installDir "codel00p.exe") -Force
  Write-Host "Installed codel00p to $installDir\codel00p.exe"

  # Add to the user PATH if it is not already there.
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (($userPath -split ";") -notcontains $installDir) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Write-Host "Added $installDir to your user PATH. Restart your terminal to use it."
  }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
