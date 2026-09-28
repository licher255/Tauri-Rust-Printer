#Requires -RunAsAdministrator
param([Parameter(Mandatory=$true)][string]$Program)
$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Program).ProviderPath
if ([IO.Path]::GetExtension($resolved) -ne '.exe' -or -not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
    throw 'Program must be an existing AirPrinter executable.'
}
foreach ($entry in @(@{ Name='AirPrinter-IPP-Private'; Protocol='TCP'; Port=@('631','8631-8699') }, @{ Name='AirPrinter-mDNS-Private'; Protocol='UDP'; Port=@('5353') })) {
    $existing = Get-NetFirewallRule -Name $entry.Name -ErrorAction SilentlyContinue
    if ($existing) { Remove-NetFirewallRule -Name $entry.Name }
    New-NetFirewallRule -Name $entry.Name -DisplayName $entry.Name -Group 'AirPrinter' -Direction Inbound -Action Allow -Enabled True -Profile Private,Public -Program $resolved -Protocol $entry.Protocol -LocalPort $entry.Port -RemoteAddress LocalSubnet | Out-Null
}
Write-Host 'AirPrinter is allowed from the local subnet on Private and Public networks.'
