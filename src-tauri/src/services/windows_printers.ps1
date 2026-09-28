$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Drawing
$standard = @{ 9 = @('iso_a4_210x297mm',21000,29700); 11 = @('iso_a5_148x210mm',14800,21000); 1 = @('na_letter_8.5x11in',21590,27940); 5 = @('na_legal_8.5x14in',21590,35560) }
function Get-MediaSize($paper) {
    if ($paper.Width -le 0 -or $paper.Height -le 0) { return }
    $kind = [int]$paper.RawKind
    if ($standard.ContainsKey($kind)) {
        $name = $standard[$kind][0]; $width = $standard[$kind][1]; $height = $standard[$kind][2]
    } else {
        $width = [int][Math]::Round($paper.Width * 25.4)
        $height = [int][Math]::Round($paper.Height * 25.4)
        $w = ($paper.Width / 100.0).ToString('0.##',[Globalization.CultureInfo]::InvariantCulture)
        $h = ($paper.Height / 100.0).ToString('0.##',[Globalization.CultureInfo]::InvariantCulture)
        $name = "custom_win${kind}_${w}x${h}in"
    }
    @{ name=$name; width=$width; height=$height; windows_kind=$kind }
}
$queues = @(foreach ($printer in Get-Printer) {
    # Interactive destinations are not unattended physical queues.
    if ($printer.PortName -match '^(PORTPROMPT:|FILE:|nul:|SHRFAX:)$' -or $printer.DriverName -match 'Microsoft.*(PDF|XPS|OneNote|Fax|Virtual Print)') { continue }
    $settings = New-Object System.Drawing.Printing.PrinterSettings
    $settings.PrinterName = $printer.Name
    if (-not $settings.IsValid) { continue }
    $sizes = @($settings.PaperSizes | ForEach-Object { Get-MediaSize $_ })
    if ($sizes.Count -eq 0) { continue }
    $media = @($sizes | ForEach-Object { $_.name } | Select-Object -Unique)
    $defaultMedia = (Get-MediaSize $settings.DefaultPageSettings.PaperSize).name
    if (-not ($media -contains $defaultMedia)) { $defaultMedia = $media[0] }
    @{
        name = $printer.Name
        status = [uint32]$printer.PrinterStatus
        capabilities = @{
            color = [bool]$settings.SupportsColor
            duplex = [bool]$settings.CanDuplex
            max_copies = [Math]::Max(1, [Math]::Min(99, [int]$settings.MaximumCopies))
            media = $media
            media_sizes = $sizes
            default_media = $defaultMedia
        }
    }
})
ConvertTo-Json -InputObject $queues -Depth 6 -Compress
