param(
    [string]$OutFile = ""
)

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$source = @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;

public class WinCap {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    public static Bitmap CaptureWindow(IntPtr hWnd) {
        ShowWindow(hWnd, 9); // SW_RESTORE
        SetForegroundWindow(hWnd);
        System.Threading.Thread.Sleep(300);

        RECT rect;
        GetWindowRect(hWnd, out rect);
        int width = rect.Right - rect.Left;
        int height = rect.Bottom - rect.Top;

        if (width <= 0 || height <= 0) return null;

        Bitmap bmp = new Bitmap(width, height);
        using (Graphics g = Graphics.FromImage(bmp)) {
            g.CopyFromScreen(rect.Left, rect.Top, 0, 0, new Size(width, height), CopyPixelOperation.SourceCopy);
        }
        return bmp;
    }
}
"@

Add-Type -TypeDefinition $source -ReferencedAssemblies System.Drawing

$p = Get-Process conflux-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p -or $p.MainWindowHandle -eq [IntPtr]::Zero) {
    Write-Error "conflux-desktop is not running or has no main window handle."
    exit 1
}

$bmp = [WinCap]::CaptureWindow($p.MainWindowHandle)
if ($bmp) {
    $dest = if ($OutFile -ne "") { $OutFile } else {
        [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\..\docs\images\app-main.png"))
    }
    $destDir = [System.IO.Path]::GetDirectoryName($dest)
    if (-not [System.IO.Directory]::Exists($destDir)) {
        [System.IO.Directory]::CreateDirectory($destDir) | Out-Null
    }
    $bmp.Save($dest, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "Window screenshot saved to $dest"
} else {
    Write-Error "Failed to capture bitmap."
    exit 1
}
