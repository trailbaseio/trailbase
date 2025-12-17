# Install script for: https://github.com/trailbaseio/trailbase originally
# generated using https://instl.sh. License note at the end.
#
# To use this install script, run the following command:
#
# Linux:
# curl -sSL instl.sh/trailbaseio/trailbase/linux | bash
#
# macOS:
# curl -sSL instl.sh/trailbaseio/trailbase/macos | bash
#
# Windows:
# iwr instl.sh/trailbaseio/trailbase/windows | iex

# Import libraries
# --- Sourced from file: ./lib/colors.ps1 ---

# Foreground Colors

function fRed
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkRed
}

function fRedLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Red
}

function fGreen
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkGreen
}

function fGreenLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Green
}

function fYellow
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkYellow
}

function fYellowLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Yellow
}

function fBlue
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkBlue
}

function fBlueLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Blue
}

function fMagenta
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkMagenta
}

function fMagentaLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Magenta
}

function fCyan
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkCyan
}

function fCyanLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Cyan
}

function fWhite
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor White
}

function fBlack
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Black
}

function fGray
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor DarkGray
}

function fGrayLight
{
    param (
        $Msg
    )
    Write-Host $Msg -NoNewline -ForegroundColor Gray
}

# Special Colors

function resetColor
{
    Write-Host -NoNewline -Reset
}

# Theme

function info
{
    param (
        $Msg
    )
    fBlueLight " i "
    fBlue $Msg
    echo ""
}

function warning
{
    param (
        $Msg
    )
    fYellowLight " ! "
    fYellow $Msg
    echo ""
}

function error
{
    param (
        $Msg
    )
    fRedLight " X "
    fRed $Msg
    echo ""
    exit 1
}

function success
{
    param (
        $Msg
    )
    fGreenLight " + "
    fGreen $Msg
    echo ""
}

function verbose
{
    param (
        $Msg
    )
    if ($verbose)
    {
        fGrayLight " > "
        fGray $Msg
        echo ""
    }
}


# --- End of ./lib/colors.ps1 ---



# Setup variables
$verboseString = "false"
$verbose = $false
if ($verboseString -eq "true")
{
    $verbose = $true
}

verbose "Setting up variables"
$owner = "trailbaseio"
$repo = "trailbase"

$tmpDir = "$env:TEMP\trailbase-cache"
verbose "Temporary directory: $tmpDir"
# Remove previous cache, if it exists
if (test-path $tmpDir)
{
    rm -r -fo $tmpDir
}
New-Item -Path $tmpDir -ItemType Directory > $null

$installLocation = "$HOME\$repo"
verbose "Install location: $installLocation"
# Remove previous installation, if it exists
if (test-path $installLocation)
{
    Remove-Item -r -fo $installLocation
}
New-Item -Path $installLocation -ItemType Directory > $null


# --- End of ../shared/intro.ps1 ---


# Installation
$headers = @{}

# Check for GH_TOKEN in environment variables
if ($env:GH_TOKEN) {
    verbose "Using authentication with GH_TOKEN"
    $Headers['Authorization'] = "Bearer $($env:GH_TOKEN)"
}
# Check for GITHUB_TOKEN in environment variables
elseif ($env:GITHUB_TOKEN) {
    verbose "Using authentication with GITHUB_TOKEN"
    $Headers['Authorization'] = "Bearer $($env:GITHUB_TOKEN)"
}
# No authentication tokens found
else {
    verbose "No authentication"
}

# GitHub public API
$latestReleaseURL = "https://api.github.com/repos/$owner/$repo/releases/latest"
$latestRelease = Invoke-WebRequest -Method Get -Uri $latestReleaseURL -Headers $Headers | ConvertFrom-Json
$tagName = $latestRelease.tag_name
info "Found latest release of $repo (version: $tagName)"

# Get list of assets
verbose "Getting list of assets"
$assetsRaw = $latestRelease.assets
# Map to array
$assets = $assetsRaw | ForEach-Object { $_.browser_download_url }
$assetCount = $assets.Count
info "Found $assetCount assets in '$tagName' - searching for one that fits your system..."

# Get host architecture
$arch = $env:PROCESSOR_ARCHITECTURE
# arch to lowercase
$arch = $arch.ToLower()
verbose "Host architecture: $arch"

# Set aliases for architecture
$amd64 = @("amd64", "x86_64", "x86-64", "x64")
$amd32 = @("386", "i386", "i686", "x86")
$windows = @("windows", "win")

$currentArchAliases = @()
if ($arch -eq "amd64")
{
    $currentArchAliases = $amd64
}
elseif ($arch -eq "x86")
{
    $currentArchAliases = $amd32
}
else
{
    error "Unsupported architecture: $arch"
}
verbose "Current architecture aliases: $currentArchAliases"

# Create hastable of assets and a score
$assetMap = @{ }
# Loop through assets
foreach ($asset in $assets)
{
    if ( $asset.ToLower().Contains("darwin"))
    {
        continue
    }
    if ( $asset.ToLower().EndsWith(".sbom"))
    {
        continue
    }
    $assetMap[$asset] = 0
    # Loop through windows aliases
    $windows | %{
        $windowsAlias = $_
        # If asset contains architecture alias, increase score
        if ( $asset.ToLower().Contains($windowsAlias))
        {
            verbose "Asset $asset contains windows alias $windowsAlias"
            $assetMap[$asset] = $assetMap[$asset] + 1
        }
    }

    # Loop through architecture aliases
    $currentArchAliases | %{
        $archAlias = $_
        # If asset contains architecture alias, increase score
        if ( $asset.ToLower().Contains($archAlias))
        {
            verbose "Asset $asset contains architecture alias $archAlias"
            $assetMap[$asset] = $assetMap[$asset] + 1
        }
    }
}

# Get highest score
$highestScore = 0
$highestScoreAsset = ""
foreach ($Key in $assetMap.Keys)
{
    $asset = $Key
    $score = $assetMap[$Key]
    verbose "Asset: $asset, score: $score"
    if ($score -gt $highestScore)
    {
        $highestScore = $score
        $highestScoreAsset = $asset
    }
}

# Check if no asset is found
if ($highestScore -eq 0)
{
    error "Could not find any assets that fit your system"
}

$assetURL = $highestScoreAsset
$assetName = $assetURL.Split('/')[-1]
info "Found asset with highest match score: $assetName"

# Downoad asset
info "Downloading asset..."
$assetPath = "$tmpDir\$assetName"
Invoke-WebRequest -Uri $assetURL -OutFile $assetPath -Headers $Headers
verbose "Asset downloaded to $assetPath"

info "Installing $repo"
# Extract asset if it is a zip file
if ( $assetName.EndsWith(".zip"))
{
    Expand-Archive -Path $assetPath -Destination $installLocation
    verbose "Asset extracted to $installLocation"
}
elseif ( $assetName.EndsWith(".tar.gz"))
{
    tar -xzf $assetPath -C $installLocation
    verbose "Asset extracted to $installLocation"
}
elseif ( $assetName.EndsWith(".tar"))
{
    tar -xf $assetPath -C $installLocation
    verbose "Asset extracted to $installLocation"
}
else
{
    error "Asset is not a zip, tar or tar.gz file"
}

# If it was unpacked to a single directory, move the files to the root of the tmpDir
# Also check that there are not other non directory files in the tmpDir
verbose "Checking if asset was unpacked to a single directory"
$files = Get-ChildItem -Path $installLocation
if ($files.Count -eq 1 -and $files[0].PSIsContainer)
{
    verbose "Moving files to root of tmpDir"
    $subDir = $files[0]
    $subDirPath = $subDir.FullName
    $subDirFiles = Get-ChildItem -Path $subDirPath
    foreach ($file in $subDirFiles)
    {
        $filePath = $file.FullName
        $fileName = $file.Name
        verbose "Moving $fileName to root of tmpDir"
        Move-Item -Path $filePath -Destination $installLocation
    }
}
else
{
    verbose "Asset was not unpacked to a single directory"
}

# Find binary file in install path
$binaryFile = (Get-ChildItem -Path $installLocation -Filter "*.exe")[0]
$binaryFile = $installLocation + "\" + $binaryFile.Name
$binaryName = $binaryFile.Split('\')[-1]
$command = $binaryName.Split('.')[0]
verbose "Binary file: $binaryFile"

# Change PATH to include install location
verbose "Changing PATH to include install location"
$oldPath = (Get-ItemProperty -Path 'Registry::HKEY_CURRENT_USER\Environment' -Name PATH).path
# if oldPath does not contain install location, add it
if (!$oldPath.Contains($installLocation))
{
    verbose "PATH does not contain install location, adding it"
    $newPath = $oldPath + ";" + $installLocation
    Set-ItemProperty -Path 'Registry::HKEY_CURRENT_USER\Environment' -Name PATH -Value $newPath
}

info "Running clean up..."
if (test-path $tmpDir)
{
    verbose "Removing temporary directory"
    rm -r -fo $tmpDir
}

# Test if binary exists
if (test-path $binaryFile)
{
    verbose "Binary file exists"
}
else
{
    error "Binary file does not exist"
}

Write-Host ""
success "You can now run '$command' in your terminal!"
info "You might have to restart your terminal session for the changes to take effect"


##################################################################################
# MIT License                                                                    #
#                                                                                #
# Copyright (c) 2022 Marvin Wendt (Instl | https://instl.sh)                     #
#                                                                                #
# Permission is hereby granted, free of charge, to any person obtaining a copy   #
# of this software and associated documentation files (the "Software"), to deal  #
# in the Software without restriction, including without limitation the rights   #
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell      #
# copies of the Software, and to permit persons to whom the Software is          #
# furnished to do so, subject to the following conditions:                       #
#                                                                                #
# The above copyright notice and this permission notice shall be included in all #
# copies or substantial portions of the Software.                                #
#                                                                                #
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR     #
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,       #
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE    #
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER         #
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,  #
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE  #
# SOFTWARE.                                                                      #
##################################################################################

# --------------------------------- Metadata ----------------------------------- #

# Script generated for https://github.com/trailbaseio/trailbase
# Script generated at 2025-08-06 13:53:57.89808815 +0000 UTC m=+12582025.680520342
# Script generated for version "latest"
# Script generated with instl version "dev"
