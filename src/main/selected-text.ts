import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { runPowerShellScript } from './powershell-script'

const execFileAsync = promisify(execFile)

export const macSelectedTextScript = `tell application "System Events"
  set frontmostProcess to first application process whose frontmost is true
  tell frontmostProcess
    set focusedElement to value of attribute "AXFocusedUIElement"
    set selectedValue to value of attribute "AXSelectedText" of focusedElement
    if selectedValue is missing value then return ""
    return selectedValue as text
  end tell
end tell`

export const windowsSelectedTextScript = `
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$focused = [System.Windows.Automation.AutomationElement]::FocusedElement
if ($null -eq $focused) { throw 'No focused text element' }
$pattern = $null
if (-not $focused.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
  throw 'The focused application does not expose text selection through UI Automation'
}
$selection = $pattern.GetSelection()
$text = (@($selection | ForEach-Object { $_.GetText(-1) }) -join '')
[Console]::Write([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)))
`

// Read accessibility selection, never simulate Ctrl+C or replace the clipboard.
export async function readSelectedText(): Promise<string> {
  let selected: string
  if (process.platform === 'win32') {
    const encoded = await runPowerShellScript(windowsSelectedTextScript, { timeout: 3000 })
    // Preserve selection whitespace even though the PowerShell utility trims
    // its CLI's final line separator.
    selected = Buffer.from(encoded, 'base64').toString('utf8')
  } else if (process.platform === 'darwin') {
    const { stdout } = await execFileAsync('/usr/bin/osascript', ['-e', macSelectedTextScript], {
      encoding: 'utf8',
      timeout: 3000,
      maxBuffer: 1024 * 1024,
    })
    selected = stdout.replace(/\r?\n$/, '')
  } else {
    throw new Error('Reading selected text is only supported on macOS and Windows')
  }
  if (!selected) throw new Error('No text is selected in the focused application')
  return selected
}
