# Installs or updates Shepit from the latest GitHub release:
#
#   irm https://raw.githubusercontent.com/themarchenko/shepit/main/install.ps1 | iex
#
# The installer is not code-signed, so a copy downloaded in a browser makes SmartScreen show "Windows
# protected your PC". A file PowerShell downloads carries no Mark of the Web, so the app this script
# installs starts without that prompt. No administrator rights are needed: Shepit installs for the
# current user.
#
# SHEPIT_URL takes another installer, as a URL or a local path, for example a local build. The
# checksum is always read from "<SHEPIT_URL>.sha256" and always compared; there is no way to skip it.
#
# Everything runs inside a function and failures throw instead of calling exit, because
# "irm | iex" runs in the caller's session and exit would close their window.

function Install-Shepit {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'   # the progress bar makes Invoke-WebRequest very slow in 5.1
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $url = $env:SHEPIT_URL
  if (-not $url) { $url = 'https://github.com/themarchenko/shepit/releases/latest/download/Shepit-setup.exe' }
  $dir = Join-Path $env:LOCALAPPDATA 'Shepit'
  $exe = Join-Path $dir 'shepit-app.exe'

  if (-not [Environment]::Is64BitOperatingSystem) { throw 'Shepit needs 64-bit Windows.' }
  if ([Environment]::OSVersion.Version.Major -lt 10) { throw 'Shepit needs Windows 10 or later.' }

  $tmp = Join-Path ([IO.Path]::GetTempPath()) ('shepit-' + [Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    $setup = Join-Path $tmp 'Shepit-setup.exe'
    $sumFile = Join-Path $tmp 'Shepit-setup.exe.sha256'

    Write-Host 'Downloading Shepit...'
    Get-ShepitFile $url $setup
    Get-ShepitFile "$url.sha256" $sumFile

    # The file holds "<hex>  <name>", like sha256sum prints.
    $expected = ((Get-Content -Raw $sumFile).Trim() -split '\s+')[0]
    $actual = (Get-FileHash -Algorithm SHA256 -Path $setup).Hash
    if ($actual -ne $expected) { throw 'The downloaded installer is damaged. Try again.' }
    Unblock-File -Path $setup

    # Stop only the copy being replaced. A build running from somewhere else keeps running.
    $running = @(Get-Process -Name 'shepit-app' -ErrorAction SilentlyContinue |
      Where-Object { $_.Path -and $_.Path.StartsWith($dir + '\', [StringComparison]::OrdinalIgnoreCase) })
    if ($running.Count -gt 0) {
      Write-Host 'Quitting the running Shepit...'
      $running | Stop-Process -Force
      $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
    }

    Write-Host 'Installing...'
    $proc = Start-Process -FilePath $setup -ArgumentList '/S' -Wait -PassThru
    if ($proc.ExitCode -ne 0) { throw "The installer failed with exit code $($proc.ExitCode)." }
    if (-not (Test-Path $exe)) { throw "The installer finished but $exe is missing." }

    $version = (Get-Item $exe).VersionInfo.ProductVersion
    Write-Host "Installed Shepit $version in $dir."
    Start-Process -FilePath $exe
    Write-Host 'Shepit is in the notification area (the arrow next to the clock). Allow the microphone when Windows asks.'
  }
  finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
}

# Fetches a URL, or copies a local path, to a file.
function Get-ShepitFile($source, $target) {
  if ($source -match '^https://') {
    try {
      Invoke-WebRequest -UseBasicParsing -Uri $source -OutFile $target
    }
    catch {
      if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) {
        throw 'The latest release has no Windows installer yet. Try again in a few minutes.'
      }
      throw "Download failed: $source"
    }
  }
  elseif (Test-Path -LiteralPath $source) {
    Copy-Item -LiteralPath $source -Destination $target
  }
  else {
    throw "Not an https URL or an existing file: $source"
  }
}

Install-Shepit
