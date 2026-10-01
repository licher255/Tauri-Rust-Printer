$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$job = Get-Content -LiteralPath $env:AIRPRINTER_MANIFEST -Raw -Encoding UTF8 | ConvertFrom-Json
Add-Type -AssemblyName System.Drawing

# All request values are read from JSON, never interpolated into executable code.
if ($job.format -eq 'application/pdf') {
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType=WindowsRuntime]
    $null = [Windows.Data.Pdf.PdfDocument, Windows.Data.Pdf, ContentType=WindowsRuntime]
    $null = [Windows.Storage.Streams.InMemoryRandomAccessStream, Windows.Storage.Streams, ContentType=WindowsRuntime]
    $null = [Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType=WindowsRuntime]
    $null = [Windows.Data.Pdf.PdfPageRenderOptions, Windows.Data.Pdf, ContentType=WindowsRuntime]
    function Await-Result($operation, [Type] $resultType) {
        $method = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and $_.IsGenericMethod -and $_.GetGenericArguments().Count -eq 1 -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
        } | Select-Object -First 1
        $task = $method.MakeGenericMethod($resultType).Invoke($null, @($operation))
        $task.GetAwaiter().GetResult()
    }
    function Await-Action($operation) {
        $method = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and -not $_.IsGenericMethod -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncAction'
        } | Select-Object -First 1
        $task = $method.Invoke($null, @($operation)); $task.GetAwaiter().GetResult()
    }
    $file = Await-Result ([Windows.Storage.StorageFile]::GetFileFromPathAsync($job.input)) ([Windows.Storage.StorageFile])
    $pdf = Await-Result ([Windows.Data.Pdf.PdfDocument]::LoadFromFileAsync($file)) ([Windows.Data.Pdf.PdfDocument])
    if ($pdf.PageCount -lt 1 -or $pdf.PageCount -gt 500) { throw 'Invalid PDF page count' }
    $pages = @()
    $totalPixels = 0L
    for ($index = 0; $index -lt $pdf.PageCount; $index++) {
        if (Test-Path -LiteralPath $job.cancel) { throw 'Print job canceled' }
        $page = $pdf.GetPage($index)
        $stream = New-Object Windows.Storage.Streams.InMemoryRandomAccessStream
        try {
            $options = New-Object Windows.Data.Pdf.PdfPageRenderOptions
            $options.DestinationWidth = [uint32][Math]::Ceiling($page.Size.Width * 300 / 96)
            $options.DestinationHeight = [uint32][Math]::Ceiling($page.Size.Height * 300 / 96)
            $totalPixels += [long]$options.DestinationWidth * $options.DestinationHeight
            if ($options.DestinationWidth -gt 20000 -or $options.DestinationHeight -gt 20000 -or $totalPixels -gt 130000000) { throw 'PDF rendering exceeds size limit' }
            Await-Action ($page.RenderToStreamAsync($stream, $options))
            $reader = New-Object Windows.Storage.Streams.DataReader ($stream.GetInputStreamAt(0))
            try {
                $size = [uint32]$stream.Size
                $loaded = Await-Result ($reader.LoadAsync($size)) ([uint32])
                if ($loaded -ne $size) { throw 'Incomplete PDF page render' }
                $bytes = New-Object byte[] $size
                $reader.ReadBytes($bytes)
                $path = Join-Path $job.directory ("pdf-page-{0}.png" -f $index)
                [IO.File]::WriteAllBytes($path, $bytes)
                $pages += $path
            } finally { $reader.Dispose() }
        } finally {
            $page.Dispose()
            $stream.Dispose()
        }
    }
}
else { $pages = @($job.pages) }
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Printing;
using System.Runtime.InteropServices;
using System.IO;
using System.ComponentModel;

public static class AirPrinterNative {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    struct DocInfo {
        public int size;
        [MarshalAs(UnmanagedType.LPWStr)] public string name;
        [MarshalAs(UnmanagedType.LPWStr)] public string output;
        [MarshalAs(UnmanagedType.LPWStr)] public string datatype;
        public int flags;
    }
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr CreateDC(string driver, string device, string output, IntPtr mode);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern int StartDoc(IntPtr dc, ref DocInfo info);
    [DllImport("gdi32.dll")] static extern int StartPage(IntPtr dc);
    [DllImport("gdi32.dll")] static extern int EndPage(IntPtr dc);
    [DllImport("gdi32.dll")] static extern int EndDoc(IntPtr dc);
    [DllImport("gdi32.dll")] static extern int AbortDoc(IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
    [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr dc, int index);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr handle);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr handle);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalFree(IntPtr handle);

    public static int Print(string queue, string[] pages, int copies, int quality, string sides, string color, int paperKind, int paperWidth, int paperHeight, int orientation, string name, string cancel, string idFile, string output) {
        var settings = new PrinterSettings();
        settings.PrinterName = queue;
        if (!settings.IsValid) throw new InvalidOperationException("Windows printer is unavailable: " + queue);
        if (copies < 1 || copies > settings.MaximumCopies) throw new InvalidOperationException("Unsupported copies");
        settings.Copies = (short)copies;
        settings.Collate = true;
        if (sides != "one-sided" && !settings.CanDuplex) throw new InvalidOperationException("Printer cannot duplex");
        settings.Duplex = sides == "two-sided-long-edge" ? Duplex.Vertical : sides == "two-sided-short-edge" ? Duplex.Horizontal : Duplex.Simplex;
        var pageSettings = settings.DefaultPageSettings;
        if (quality != 4) {
            var preferred = quality == 5 ? PrinterResolutionKind.High : PrinterResolutionKind.Draft;
            foreach (PrinterResolution resolution in settings.PrinterResolutions) {
                if (resolution.Kind == preferred) { pageSettings.PrinterResolution = resolution; break; }
            }
        }
        pageSettings.Landscape = orientation == 4 || orientation == 5;
        if (color == "color" && !settings.SupportsColor) throw new InvalidOperationException("Printer cannot print color");
        pageSettings.Color = color != "monochrome" && settings.SupportsColor;
        bool found = false;
        foreach (PaperSize size in settings.PaperSizes) {
            if (size.RawKind == paperKind && Math.Abs(size.Width - paperWidth) <= 1 && Math.Abs(size.Height - paperHeight) <= 1) {
                pageSettings.PaperSize = size; found = true; break;
            }
        }
        if (!found) throw new InvalidOperationException("Selected paper is no longer available in the Windows driver");
        var hmode = settings.GetHdevmode(pageSettings);
        IntPtr dc = IntPtr.Zero;
        bool started = false;
        try {
            var mode = GlobalLock(hmode);
            if (mode == IntPtr.Zero) throw new Win32Exception();
            try { dc = CreateDC("WINSPOOL", queue, null, mode); }
            finally { GlobalUnlock(hmode); }
            if (dc == IntPtr.Zero) throw new Win32Exception();
            var info = new DocInfo { size = Marshal.SizeOf(typeof(DocInfo)), name = name, output = String.IsNullOrEmpty(output) ? null : output };
            int id = StartDoc(dc, ref info);
            if (id <= 0) throw new Win32Exception();
            started = true;
            File.WriteAllText(idFile, id.ToString());
            foreach (string path in pages) {
                if (File.Exists(cancel)) throw new OperationCanceledException();
                using (var image = Image.FromFile(path)) {
                    if ((long)image.Width * image.Height > 130000000) throw new InvalidOperationException("Image size limit exceeded");
                    if (Array.IndexOf(image.PropertyIdList, 0x0112) >= 0) {
                        int exif = image.GetPropertyItem(0x0112).Value[0];
                        var rotations = new RotateFlipType[] { RotateFlipType.RotateNoneFlipNone, RotateFlipType.RotateNoneFlipNone, RotateFlipType.RotateNoneFlipX, RotateFlipType.Rotate180FlipNone, RotateFlipType.Rotate180FlipX, RotateFlipType.Rotate90FlipX, RotateFlipType.Rotate90FlipNone, RotateFlipType.Rotate270FlipX, RotateFlipType.Rotate270FlipNone };
                        if (exif >= 1 && exif <= 8) image.RotateFlip(rotations[exif]);
                    }
                    if (orientation == 5 || orientation == 6) image.RotateFlip(RotateFlipType.Rotate180FlipNone);
                    if (StartPage(dc) <= 0) throw new Win32Exception();
                    using (var graphics = Graphics.FromHdc(dc)) {
                        graphics.PageUnit = GraphicsUnit.Pixel;
                        float width = GetDeviceCaps(dc, 8), height = GetDeviceCaps(dc, 10);
                        float scale = Math.Min(width / image.Width, height / image.Height);
                        var bounds = new RectangleF((width - image.Width * scale) / 2, (height - image.Height * scale) / 2, image.Width * scale, image.Height * scale);
                        graphics.DrawImage(image, bounds);
                    }
                    if (EndPage(dc) <= 0) throw new Win32Exception();
                }
            }
            if (File.Exists(cancel)) throw new OperationCanceledException();
            if (EndDoc(dc) <= 0) throw new Win32Exception();
            started = false;
            return id;
        } finally {
            if (started) AbortDoc(dc);
            if (dc != IntPtr.Zero) DeleteDC(dc);
            GlobalFree(hmode);
        }
    }
}
'@
$id = [AirPrinterNative]::Print($job.printer, [string[]]$pages, $job.copies, $job.quality, $job.sides, $job.color_mode, $job.paper_kind, $job.paper_width, $job.paper_height, $job.orientation, $job.name, $job.cancel, $job.spool_id, $job.output)
@{ spool_id = $id } | ConvertTo-Json -Compress
