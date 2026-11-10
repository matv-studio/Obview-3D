# Installe Inno Setup sans fenêtre dans un dossier choisi (construction de l'installateur Windows, D-32).
#
#   tools/inno.ps1 <version, ex. 6.7.4> <dossier>
param([Parameter(Mandatory)][string]$Version, [Parameter(Mandatory)][string]$Dir)
$ErrorActionPreference = "Stop"
$tag = "is-" + ($Version -replace '\.', '_')
$urls = @(
  "https://files.jrsoftware.org/is/6/innosetup-$Version.exe",
  "https://github.com/jrsoftware/issrc/releases/download/$tag/innosetup-$Version.exe"
)
$ok = $false
foreach ($u in $urls) {
  try { Invoke-WebRequest -Uri $u -OutFile innosetup.exe; $ok = $true; break } catch { Write-Host "échec : $u" }
}
if (-not $ok) { throw "Inno Setup $Version introuvable" }
# Signature Authenticode de l'éditeur vérifiée avant exécution.
$sig = Get-AuthenticodeSignature innosetup.exe
if ($sig.Status -ne "Valid") { throw "signature d'Inno Setup invalide : $($sig.Status)" }
Write-Host "signé par : $($sig.SignerCertificate.Subject)"
Start-Process -FilePath .\innosetup.exe -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /CURRENTUSER /DIR=`"$Dir`"" -Wait -NoNewWindow
if (-not (Test-Path "$Dir\ISCC.exe")) { throw "ISCC.exe absent de $Dir" }
