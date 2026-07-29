Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$drawingAssembly = [System.Drawing.Bitmap].Assembly.Location
$formsAssembly = [System.Windows.Forms.Form].Assembly.Location

Add-Type -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public static class TezbarColorPickerDpi {
    [StructLayout(LayoutKind.Sequential)]
    private struct NativePoint {
        public int X;
        public int Y;
    }

    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out NativePoint point);

    public static Point GetCursorPosition() {
        NativePoint point;
        return GetCursorPos(out point) ? new Point(point.X, point.Y) : Point.Empty;
    }
}

public sealed class TezbarColorPickerMagnifier : Form {
    public Bitmap Source { get; set; }
    public Point SamplePoint { get; set; }
    public string ColorLabel { get; set; }

    public TezbarColorPickerMagnifier() {
        FormBorderStyle = FormBorderStyle.None;
        StartPosition = FormStartPosition.Manual;
        ShowInTaskbar = false;
        TopMost = true;
        Width = 184;
        Height = 218;
        DoubleBuffered = true;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
        ColorLabel = string.Empty;
    }

    protected override CreateParams CreateParams {
        get {
            CreateParams parameters = base.CreateParams;
            // Do not steal focus or mouse clicks from the picker underneath.
            parameters.ExStyle |= 0x08000000 | 0x00000080 | 0x00000020; // NOACTIVATE | TOOLWINDOW | TRANSPARENT
            return parameters;
        }
    }

    protected override void WndProc(ref Message message) {
        if (message.Msg == 0x84) { // WM_NCHITTEST
            message.Result = (IntPtr)(-1); // HTTRANSPARENT
            return;
        }
        base.WndProc(ref message);
    }

    protected override void OnPaint(PaintEventArgs eventArgs) {
        Graphics graphics = eventArgs.Graphics;
        graphics.Clear(Color.FromArgb(248, 24, 25, 29));

        if (Source != null && Source.Width > 0 && Source.Height > 0) {
            const int sourceSize = 11;
            int sourceLeft = Math.Max(0, Math.Min(Source.Width - sourceSize, SamplePoint.X - sourceSize / 2));
            int sourceTop = Math.Max(0, Math.Min(Source.Height - sourceSize, SamplePoint.Y - sourceSize / 2));
            Rectangle source = new Rectangle(sourceLeft, sourceTop, sourceSize, sourceSize);
            Rectangle destination = new Rectangle(10, 10, Width - 20, Width - 58);

            graphics.InterpolationMode = InterpolationMode.NearestNeighbor;
            graphics.PixelOffsetMode = PixelOffsetMode.Half;
            graphics.DrawImage(Source, destination, source, GraphicsUnit.Pixel);

            using (Pen borderPen = new Pen(Color.White, 2)) {
                graphics.DrawRectangle(borderPen, destination);
            }
            using (Pen guidePen = new Pen(Color.FromArgb(190, 255, 255, 255), 1)) {
                int centerX = destination.Left + destination.Width / 2;
                int centerY = destination.Top + destination.Height / 2;
                graphics.DrawLine(guidePen, centerX - 12, centerY, centerX + 12, centerY);
                graphics.DrawLine(guidePen, centerX, centerY - 12, centerX, centerY + 12);
            }
        }

        using (SolidBrush labelBrush = new SolidBrush(Color.White)) {
            graphics.DrawString(ColorLabel, SystemFonts.MessageBoxFont, labelBrush, 10, Height - 40);
        }
        base.OnPaint(eventArgs);
    }
}
'@ -ReferencedAssemblies @($drawingAssembly, $formsAssembly)

try {
    # Per-monitor-aware V2 keeps the overlay and captured bitmap in the same
    # physical coordinate system on mixed-DPI, multi-monitor desktops.
    [void][TezbarColorPickerDpi]::SetProcessDpiAwarenessContext([IntPtr](-4))
} catch {
    # Windows versions without the V2 API still work with the process default.
}

$virtualScreen = [System.Windows.Forms.SystemInformation]::VirtualScreen
if ($virtualScreen.Width -le 0 -or $virtualScreen.Height -le 0) {
    [Console]::Out.Write('null')
    exit 1
}

$script:capturedDesktop = New-Object System.Drawing.Bitmap(
    $virtualScreen.Width,
    $virtualScreen.Height,
    [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
)
$captureGraphics = [System.Drawing.Graphics]::FromImage($script:capturedDesktop)
try {
    $captureGraphics.CopyFromScreen(
        $virtualScreen.Location,
        [System.Drawing.Point]::Empty,
        $virtualScreen.Size,
        [System.Drawing.CopyPixelOperation]::SourceCopy
    )
} finally {
    $captureGraphics.Dispose()
}

$script:selectedColor = $null
$script:pickerForm = New-Object System.Windows.Forms.Form
$script:pickerForm.Text = 'Tezbar Color Picker'
$script:pickerForm.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
$script:pickerForm.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$script:pickerForm.Bounds = $virtualScreen
$script:pickerForm.AutoScaleMode = [System.Windows.Forms.AutoScaleMode]::None
$script:pickerForm.TopMost = $true
$script:pickerForm.ShowInTaskbar = $false
$script:pickerForm.KeyPreview = $true
# Keep the normal Windows pointer. The magnifier provides the picker affordance
# without replacing it with the generic '+' crosshair.
$script:pickerForm.Cursor = [System.Windows.Forms.Cursors]::Default
$script:pickerForm.BackgroundImage = $script:capturedDesktop
$script:pickerForm.BackgroundImageLayout = [System.Windows.Forms.ImageLayout]::None

$script:magnifier = New-Object TezbarColorPickerMagnifier
$script:magnifier.Source = $script:capturedDesktop
$script:cursorTimer = New-Object System.Windows.Forms.Timer
$script:cursorTimer.Interval = 16
$script:cursorTimer.Add_Tick({
    $screenPoint = [TezbarColorPickerDpi]::GetCursorPosition()
    $x = [Math]::Max(0, [Math]::Min($script:capturedDesktop.Width - 1, $screenPoint.X - $virtualScreen.Left))
    $y = [Math]::Max(0, [Math]::Min($script:capturedDesktop.Height - 1, $screenPoint.Y - $virtualScreen.Top))
    $pixel = $script:capturedDesktop.GetPixel($x, $y)
    $script:magnifier.SamplePoint = New-Object System.Drawing.Point($x, $y)
    $script:magnifier.ColorLabel = ('#{0:X2}{1:X2}{2:X2}  RGB({0}, {1}, {2})' -f $pixel.R, $pixel.G, $pixel.B)

    $tooltipGap = 24
    $tipX = $screenPoint.X + $tooltipGap
    $tipY = $screenPoint.Y + $tooltipGap
    if ($tipX + $script:magnifier.Width -gt $virtualScreen.Right) {
        $tipX = $screenPoint.X - $script:magnifier.Width - $tooltipGap
    }
    if ($tipY + $script:magnifier.Height -gt $virtualScreen.Bottom) {
        $tipY = $screenPoint.Y - $script:magnifier.Height - $tooltipGap
    }
    $tipX = [Math]::Max($virtualScreen.Left, [Math]::Min($virtualScreen.Right - $script:magnifier.Width, $tipX))
    $tipY = [Math]::Max($virtualScreen.Top, [Math]::Min($virtualScreen.Bottom - $script:magnifier.Height, $tipY))
    $script:magnifier.Location = New-Object System.Drawing.Point([int]$tipX, [int]$tipY)
    $script:magnifier.Invalidate()
})

$script:pickerForm.Add_MouseClick({
    param($sender, $event)
    if ($event.Button -ne [System.Windows.Forms.MouseButtons]::Left) {
        return
    }

    $x = [Math]::Max(0, [Math]::Min($script:capturedDesktop.Width - 1, $event.X))
    $y = [Math]::Max(0, [Math]::Min($script:capturedDesktop.Height - 1, $event.Y))
    $pixel = $script:capturedDesktop.GetPixel($x, $y)
    $script:selectedColor = @{
        red = $pixel.R
        green = $pixel.G
        blue = $pixel.B
        alpha = $pixel.A
        colorSpace = 'srgb'
    }
    $script:cursorTimer.Stop()
    $script:magnifier.Close()
    $script:pickerForm.Close()
})

$script:pickerForm.Add_KeyDown({
    param($sender, $event)
    if ($event.KeyCode -eq [System.Windows.Forms.Keys]::Escape) {
        $script:pickerForm.Close()
    }
})

$script:pickerForm.Add_Shown({
    $script:pickerForm.Activate()
    $script:pickerForm.Focus()
    $script:magnifier.Show()
    $script:cursorTimer.Start()
})

try {
    [void]$script:pickerForm.ShowDialog()
    if ($null -eq $script:selectedColor) {
        [Console]::Out.Write('null')
    } else {
        [Console]::Out.Write(($script:selectedColor | ConvertTo-Json -Compress))
    }
} finally {
    $script:cursorTimer.Stop()
    $script:cursorTimer.Dispose()
    $script:magnifier.Close()
    $script:magnifier.Dispose()
    $script:pickerForm.Dispose()
    $script:capturedDesktop.Dispose()
}
