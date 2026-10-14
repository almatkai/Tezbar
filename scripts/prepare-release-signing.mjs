// Credentials stay in runner-private temporary files, never in the checkout.
import { execFileSync } from 'node:child_process'
import { writeFileSync, unlinkSync, appendFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

if (process.env.GITHUB_ACTIONS !== 'true' || !process.env.RUNNER_TEMP)
  throw new Error('Signing setup is CI-only')
const appleKey = join(process.env.RUNNER_TEMP, 'tezbar-notarization.p8')
const pfx = join(process.env.RUNNER_TEMP, 'tezbar-signing.pfx')
if (process.argv.includes('--cleanup')) {
  for (const file of [appleKey, pfx]) if (existsSync(file)) unlinkSync(file)
} else if (process.platform === 'win32') {
  if (
    !process.env.WINDOWS_CERTIFICATE ||
    !process.env.WINDOWS_CERTIFICATE_PASSWORD ||
    !process.env.WINDOWS_CERTIFICATE_THUMBPRINT
  )
    throw new Error('Windows signing secrets are missing')
  writeFileSync(pfx, Buffer.from(process.env.WINDOWS_CERTIFICATE, 'base64'), { mode: 0o600 })
  try {
    execFileSync(
      'powershell.exe',
      [
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        "$ErrorActionPreference='Stop'; $password=ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force; $cert=Import-PfxCertificate -FilePath $env:TEZBAR_PFX_PATH -CertStoreLocation Cert:\\CurrentUser\\My -Password $password; if($env:WINDOWS_CERTIFICATE_THUMBPRINT -notin @($cert.Thumbprint)){throw 'Certificate thumbprint mismatch'}",
      ],
      {
        windowsHide: true,
        env: { ...process.env, TEZBAR_PFX_PATH: pfx },
        stdio: ['ignore', 'ignore', 'inherit'],
      }
    )
  } finally {
    unlinkSync(pfx)
  }
} else if (process.platform === 'darwin') {
  if (
    !process.env.APPLE_API_PRIVATE_KEY ||
    !process.env.APPLE_CERTIFICATE ||
    !process.env.APPLE_CERTIFICATE_PASSWORD
  )
    throw new Error('Apple signing secrets are missing')
  writeFileSync(appleKey, process.env.APPLE_API_PRIVATE_KEY, { mode: 0o600 })
  appendFileSync(process.env.GITHUB_ENV, `APPLE_API_KEY_PATH=${appleKey}\n`)
}
