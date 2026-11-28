// Native GPUI distribution tooling. Importing this module never builds, signs or reads keys.
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { readFile, writeFile, mkdir, mkdtemp, realpath, lstat, copyFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import path from 'node:path'
import { tmpdir, homedir } from 'node:os'
import { fileURLToPath } from 'node:url'
import { assertRelease, resolveBuildMode, APP_IDS, PRODUCT_NAMES } from '../lib/release-policy.mjs'
import { macOSSigningIdentity, signMacOSResources, verifyMacOSSigning } from '../lib/macos-signing.mjs'
import { assertEmbeddingRuntimeDistribution } from '../lib/embedding-runtime-policy.mjs'
import { verifyUpdaterSignature } from '../verify-updater-signature.mjs'
import { inventoryResources, RESOURCE_ROOTS, planMacos, assembleMacos } from './package-macos.mjs'
import { captureBuildMetadata, GPUI_REV, RUST_VERSION } from './build-metadata.mjs'
import { runCargo } from './cargo-artifact.mjs'
import { sourceSnapshot, assertSameSource } from './build.mjs'

const ROOT = fileURLToPath(new URL('../../', import.meta.url))
export const HELP = `Usage: pnpm release:build --channel <alpha|beta|stable> --resources <reviewed-stage> --output <fresh-directory> [options]
Build native GPUI only: macOS arm64 DMG/updater archive or Windows x64 NSIS.

  --version <version>   Must match committed package/Cargo/release policy; never bumps it
  --notes <path>        Release notes (default: docs/releases/<version>.md)
  --allow-unsigned     Alpha/Beta testing exception; macOS pinned certificate still required
  --local-preview      Explicit private ad-hoc macOS preview, never publishable
  --nsis-tools <dir>   Reviewed offline Windows NSIS kit with toolchain.json inventory
  --dry-run            Validate inputs and print plan; no build, signing, keys or output
  --help               Show this help without inspecting source, resources or credentials

Public builds require clean source, pinned resources/signing and updater signatures.
Run on the target host. No Tauri app build, WebView fallback, install or publication.
`
const digest = bytes => createHash('sha256').update(bytes).digest('hex')
const json = async file => JSON.parse(await readFile(file, 'utf8'))
const inside = (a, b) => { const r = path.relative(a, b); return r === '' || (!r.startsWith(`..${path.sep}`) && r !== '..' && !path.isAbsolute(r)) }
const pins = files => files.map(({ source, ...pin }) => pin)
const inventoryHash = files => digest(JSON.stringify(pins(files)))

export function parseRelease(args) {
  const options = { channel:'stable', allowUnsigned:false, localPreview:false, dryRun:false }
  const seen = new Set()
  for (let i=0; i<args.length; i++) {
    const flag=args[i]
    if (flag === '--') continue
    if (seen.has(flag)) throw Error(`Duplicate release argument ${flag}`)
    seen.add(flag)
    const boolean = { '--allow-unsigned':'allowUnsigned', '--local-preview':'localPreview', '--dry-run':'dryRun' }[flag]
    if (boolean) options[boolean]=true
    else if (['--channel','--version','--resources','--output','--notes','--nsis-tools','--app'].includes(flag) && args[i+1] && !args[i+1].startsWith('--')) {
      const value=args[++i]
      if (flag === '--app') { if (value !== 'native') throw Error('Only the native GPUI app is supported') }
      else options[flag.slice(2).replace(/-([a-z])/g, (_,c)=>c.toUpperCase())]=value
    } else throw Error(`Unknown or incomplete native release argument ${flag}`)
  }
  if (!options.resources || !options.output) throw Error('Native release requires explicit reviewed --resources and fresh --output directories')
  resolveBuildMode(options.channel, options)
  return options
}

export function platformPlan(platform, arch) {
  if (platform === 'darwin' && arch === 'arm64') return { platform:'darwin-aarch64', target:'aarch64-apple-darwin', binary:'tezbar', installer:'dmg + app.tar.gz' }
  if (platform === 'win32' && arch === 'x64') return { platform:'windows-x86_64-nsis', target:'x86_64-pc-windows-msvc', binary:'tezbar.exe', installer:'NSIS (currentUser, no WebView)' }
  throw Error('Native releases support macOS arm64 and Windows x64 only; cross-compilation is not release acceptance')
}

export function cargoPlan(target) {
  return [`+${RUST_VERSION}`, 'build', '--release', '--locked', '--offline', '-j2', '-p', 'tezbar-native', '--bin', 'tezbar', '--target', target, '--message-format=json']
}

export function assertMetadata(metadata, { version, sourceCommit, sourceDirty }) {
  if (metadata.schema !== 2 || metadata.version !== version || metadata.sourceCommit !== sourceCommit || metadata.dirty !== sourceDirty || metadata.benchControl !== false || metadata.runtimeShaders !== false || metadata.gpuiRev !== GPUI_REV || metadata.rustVersion !== RUST_VERSION)
    throw Error('Native executable schema-2 metadata differs from the release source/version/build policy')
}

// Windows has no macOS helper/runtime ZIP. The worker is the ONLY JS entrypoint.
export const WINDOWS_ROOTS = Object.freeze(RESOURCE_ROOTS.filter(([,to]) => !['dist-backend/libsqlite3.dylib','embedding-runtime','icon.icns'].includes(to) && !to.startsWith('native/axhelper/') && !to.startsWith('native/screenocr/') && !to.endsWith('-helper')).concat([['packaging/icons/icon.ico','icon.ico']]))

async function canonicalDirectory(directory) {
  const absolute=path.resolve(directory), canonical=await realpath(absolute)
  if (absolute !== canonical || !(await lstat(absolute)).isDirectory()) throw Error('Use canonical non-symlink input directories')
  return canonical
}
async function freshOutput(directory, root, resources) {
  const output=path.resolve(directory), parent=await canonicalDirectory(path.dirname(output))
  if (existsSync(output) || inside(resources, output) || inside(output, resources) || inside(output, root)) throw Error('Output must be fresh and disjoint from reviewed resources/source')
  return path.join(parent, path.basename(output))
}

/** Read-only preflight. No output creation, key discovery, tool execution or native build. */
export async function prepareRelease(options, { root=ROOT, platform=process.platform, arch=process.arch, snapshot=sourceSnapshot } = {}) {
  root=await canonicalDirectory(root)
  const host=platformPlan(platform, arch), mode=resolveBuildMode(options.channel, options)
  const base=await json(path.join(root,'packaging/legacy-app-policy.json'))
  const version=options.version ?? base.version
  assertRelease(options.channel, version)
  const packageVersion=(await json(path.join(root,'package.json'))).version
  const cargoManifest=path.join(root,'apps/tezbar/Cargo.toml')
  const cargoVersion=/^version = "([^"]+)"/m.exec(await readFile(cargoManifest,'utf8'))?.[1]
  if (base.identifier !== APP_IDS[options.channel] || base.productName !== PRODUCT_NAMES[options.channel] || packageVersion !== version || cargoVersion !== version || base.version !== version)
    throw Error('Release identity/version must match committed package, native Cargo and release policy; this tool never bumps versions')
  if (!base.plugins?.updater?.pubkey) throw Error('Committed updater public key missing')
  const notes=path.resolve(root, options.notes ?? `docs/releases/${version}.md`)
  const notesBytes=await readFile(notes)
  if (!notesBytes.toString('utf8').trim() || notesBytes.length > 1024*1024) throw Error('Release notes must be nonempty and at most 1 MiB')
  const resources=await canonicalDirectory(path.resolve(root,options.resources))
  if (inside(resources,root)) throw Error('Review a resource staging directory, not an ancestor of the source tree')
  const output=await freshOutput(path.resolve(root,options.output),root,resources)
  const before=snapshot(root)
  const roots=platform === 'darwin' ? RESOURCE_ROOTS : WINDOWS_ROOTS
  const files=await inventoryResources(resources,roots)
  for (const [,to] of roots) if (!files.some(file=>file.destination===to || file.destination.startsWith(to+'/'))) throw Error(`Missing declared resource ${to}`)
  if (platform==='win32') {
    if (files.some(file=>file.kind==='mach-o' || file.destination.endsWith('.dylib'))) throw Error('macOS native resources cannot be packaged in a Windows release')
    const vector=files.find(file=>file.destination==='dist-backend/vec0')
    if (!(await readFile(vector.source)).subarray(0,2).equals(Buffer.from('MZ'))) throw Error('Windows vec0 must be a reviewed native PE resource')
  }
  let policy, catalog, pinnedIdentity
  if (platform === 'darwin') {
    policy=await json(path.join(root,'packaging/macos-signing.json'))
    catalog=await json(path.join(root,'packaging/runtime/embedding-macos-arm64.json'))
    // Validate the full existing packaging contract even before Cargo provides the binary.
    planMacos({version,channel:options.channel,localPreview:mode.localPreview,allowUnsigned:options.allowUnsigned,stage:path.join(path.resolve(tmpdir()),'tezbar-release-validation', 'stage'),osTemp:path.resolve(tmpdir()),executable:{source:path.join(root,'validation-only'),destination:'tezbar',bytes:1,sha256:'0'.repeat(64),kind:'mach-o'},files,policy,catalog,payload:{bytes:catalog.archiveBytes,sha256:catalog.archiveSha256},...before,gpuiRev:GPUI_REV,rustVersion:RUST_VERSION,benchControl:false,runtimeShaders:false})
    pinnedIdentity=/certificate leaf\[subject\.CN\] = "([^"]+)"/.exec(policy.designatedRequirement)?.[1]
    if (policy.identifier !== base.identifier || !pinnedIdentity || (base.bundle?.macOS?.signingIdentity && base.bundle.macOS.signingIdentity!==pinnedIdentity)) throw Error('Certificate selection does not match the committed requirement pin')
  }
  assertSameSource(before,snapshot(root))
  return { options,root,host,mode,base,version,notes,notesSha256:digest(notesBytes),resources,output,before,files,roots,policy,catalog,pinnedIdentity,cargoManifest,cargoArgs:cargoPlan(host.target) }
}

export function describeRelease(plan) {
  return {
    kind:'native-gpui-release-plan', state:'NOT_BUILT', ...plan.host,
    version:plan.version, channel:plan.options.channel, appIdentifier:plan.mode.localPreview?'com.tezbar.preview.native':plan.base.identifier,
    sourceCommit:plan.before.sourceCommit, sourceDirty:plan.before.sourceDirty,
    resources:plan.resources, resourceCount:plan.files.length, inventorySha256:inventoryHash(plan.files), output:plan.output,
    cargo:{command:'cargo',args:plan.cargoArgs}, distribution:plan.mode.localPreview ? 'local-preview' : plan.mode.unsigned ? 'testing-preview' : 'signed-release',
    required:plan.mode.localPreview
      ? ['fresh OS-temp GPUI assembly','explicit local-preview ad-hoc app seal only; not publishable','DMG + tarball without updater signature; no signing-key discovery']
      : plan.host.platform.startsWith('darwin')
      ? ['fresh OS-temp app assembly','sign every Mach-O resource with pinned certificate','recapture signed resource inventory before outer seal','exact pinned requirement + Team ID + deep strict verification',...(!plan.mode.unsigned ? ['Developer ID + notarytool + stapler + Gatekeeper'] : []),'DMG + updater tarball','updater signature verified against committed public key']
      : ['reviewed offline --nsis-tools kit with toolchain.json SHA256 inventory, NSIS 3.11 + nsis_tauri_utils 0.5.3 + English.nsh','native PE binary','locked packaging/windows/installer.nsi; no WebView bootstrap','Authenticode binary + installer + uninstaller for signed-release','updater signature verified against committed public key'],
    acceptance:'Planning is not signing, installer, runtime or updater acceptance; dual-platform release publication is separate.',
  }
}

function runTool(command,args,options={}) {
  return execFileSync(command,args,{encoding:'utf8',stdio:['ignore','pipe','pipe'],windowsHide:true,...options})
}

async function signingEnvironment(plan, env, run) {
  if (plan.mode.localPreview) return {...env}
  if (plan.before.sourceDirty) throw Error('Every distributed native release requires a clean source checkout (including untracked files)')
  if (env.CI || env.GITHUB_ACTIONS) throw Error('Application release builds must run on local hosts, not CI')
  const result={...env}
  if (plan.policy) {
    const identity=macOSSigningIdentity({identity:env.APPLE_SIGNING_IDENTITY ?? plan.pinnedIdentity})
    const available=String(await run('security',['find-identity','-v','-p','codesigning']))
    const entries=[...available.matchAll(/\b([A-Fa-f0-9]{40}) "([^"]+)"/g)]
    if (!entries.some(([,fingerprint,name]) => name === plan.pinnedIdentity && (identity === name || identity.toUpperCase() === fingerprint.toUpperCase()))) throw Error('Pinned macOS signing certificate is unavailable; no identity fallback is permitted')
    result.APPLE_SIGNING_IDENTITY=identity
    if (!plan.mode.unsigned && (!plan.pinnedIdentity.startsWith('Developer ID Application:') || !env.APPLE_API_KEY || !env.APPLE_API_ISSUER || !env.APPLE_API_KEY_PATH)) throw Error('Signed macOS releases require the pinned Developer ID certificate and notarization credentials')
  } else if (!plan.mode.unsigned) {
    if (!/^[0-9A-Fa-f]{40}$/.test(env.WINDOWS_CERTIFICATE_THUMBPRINT ?? '')) throw Error('A pinned Windows code-signing certificate thumbprint is required')
    await run('powershell.exe',['-NoLogo','-NoProfile','-NonInteractive','-Command',"$c=Get-Item -LiteralPath ('Cert:\\CurrentUser\\My\\'+$env:WINDOWS_CERTIFICATE_THUMBPRINT); if(!$c.HasPrivateKey -or $c.NotAfter -lt (Get-Date)){throw 'Windows signing certificate unavailable'}"],{env:result})
  }
  // Key discovery occurs only after public-source/platform/certificate/resource guards.
  if (!result.TAURI_SIGNING_PRIVATE_KEY) {
    const key=path.join(env.USERPROFILE || env.HOME || homedir(),'.tauri','tezbar.key')
    if (existsSync(key)) result.TAURI_SIGNING_PRIVATE_KEY=await readFile(key,'utf8')
  }
  if (!result.TAURI_SIGNING_PRIVATE_KEY) throw Error('Every distributed build requires the existing updater signing key')
  result.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= ''
  return result
}

async function writeProductionManifest(directory, executable, files) {
  const captured=await captureBuildMetadata(executable)
  await writeFile(path.join(directory,'.tezbar-native-source'),captured.metadata.sourceCommit+'\n')
  await writeFile(path.join(directory,'.tezbar-native-production.json'),JSON.stringify({...captured.metadata,binaryMetadataSha256:captured.sha256,files:pins(files)},null,2)+'\n')
}

async function notarize(file, plan, environment, run) {
  const response=JSON.parse(await run('xcrun',['notarytool','submit',file,'--key',environment.APPLE_API_KEY_PATH,'--key-id',environment.APPLE_API_KEY,'--issuer',environment.APPLE_API_ISSUER,'--wait','--output-format','json'],{env:environment}))
  if (response.status !== 'Accepted') throw Error('Apple notarization did not accept the native artifact')
}

async function packageMac(plan, executable, environment, stageRoot, deps) {
  const {run,signResources,verifyMac}=deps
  const [binary]=await inventoryResources(path.dirname(executable),[[path.basename(executable),'tezbar']])
  const assembly=planMacos({version:plan.version,channel:plan.options.channel,localPreview:plan.mode.localPreview,allowUnsigned:plan.options.allowUnsigned,stage:path.join(stageRoot,'assembly'),osTemp:await realpath(tmpdir()),executable:binary,files:plan.files,policy:plan.policy,catalog:plan.catalog,payload:{bytes:plan.catalog.archiveBytes,sha256:plan.catalog.archiveSha256},...plan.before,gpuiRev:GPUI_REV,rustVersion:RUST_VERSION,benchControl:false,runtimeShaders:false})
  await assembleMacos(assembly,{template:await readFile(path.join(plan.root,'packaging/macos/Info.plist.in'),'utf8'),permissions:await readFile(path.join(plan.root,'crates/tezbar-core/src/system/macos-permissions.plist'),'utf8'),osTemp:await realpath(tmpdir())})
  const app=assembly.bundle, resources=path.join(app,'Contents/Resources'), identity=plan.mode.localPreview ? '-' : environment.APPLE_SIGNING_IDENTITY
  if (!plan.mode.localPreview) await signResources(resources,identity,plan.policy)
  // Signing mutates helpers/dylibs: seal their POST-sign hashes, never stale pre-sign hashes.
  const signedFiles=await inventoryResources(resources,plan.roots.map(([,to])=>[to,to]))
  const reviewedPins=new Map(plan.files.map(file=>[file.destination,file]))
  if (signedFiles.length!==plan.files.length || signedFiles.some(file=>{
    const reviewed=reviewedPins.get(file.destination)
    return !reviewed || reviewed.kind!==file.kind || (reviewed.kind!=='mach-o' && (reviewed.bytes!==file.bytes || reviewed.sha256!==file.sha256))
  })) throw Error('Signing changed the declared resource inventory or non-native reviewed data')
  await writeProductionManifest(resources,path.join(app,'Contents/MacOS/tezbar'),signedFiles)
  await run('codesign',['--force','--sign',identity,...(plan.mode.localPreview ? [] : ['--options','runtime','--timestamp','--identifier',plan.policy.identifier,'--requirements',`=designated => ${plan.policy.designatedRequirement}`]),app],{env:environment})
  const signing=plan.mode.localPreview ? undefined : await verifyMac(app,plan.policy)
  if (!plan.mode.localPreview) await run(path.join(app,'Contents/MacOS/tezbar'),['--verify-installation'],{env:environment})
  if (!plan.mode.unsigned) {
    const submission=path.join(stageRoot,'notarize.zip')
    await run('ditto',['-c','-k','--keepParent',app,submission])
    await notarize(submission,plan,environment,run)
    await run('xcrun',['stapler','staple',app])
    await run('xcrun',['stapler','validate',app])
    await run('spctl',['--assess','--type','execute',app])
  }
  const stem=`Tezbar_${plan.version}_aarch64`, dmg=path.join(plan.output,`${stem}.dmg`), archive=path.join(plan.output,`${stem}.app.tar.gz`)
  const imageRoot=path.join(stageRoot,'dmg-root'); await mkdir(imageRoot)
  await run('ditto',[app,path.join(imageRoot,'Tezbar.app')])
  if (!plan.mode.localPreview) await verifyMac(path.join(imageRoot,'Tezbar.app'),plan.policy)
  await run('hdiutil',['create','-volname','Tezbar','-srcfolder',imageRoot,'-format','UDZO',dmg])
  if (!plan.mode.unsigned) {
    await run('codesign',['--sign',identity,'--timestamp',dmg],{env:environment})
    await notarize(dmg,plan,environment,run)
    await run('xcrun',['stapler','staple',dmg]); await run('xcrun',['stapler','validate',dmg])
  }
  await run('tar',['--exclude=._*','-czf',archive,'-C',path.dirname(app),'Tezbar.app'],{env:{...environment,COPYFILE_DISABLE:'1'}})
  if (!plan.mode.localPreview) await verifyMac(app,plan.policy)
  return { artifacts:[{file:dmg,updater:false},{file:archive,updater:true}], macOSSigning:signing, packagedInventorySha256:inventoryHash(signedFiles) }
}

// The checked-in Handlebars template is deliberately NOT a general template engine.
// Only its reviewed native context is accepted; changed/unknown directives fail closed.
export function renderWindowsInstaller(template, context, files) {
  const safe=value=>{ const s=String(value); if (/["$\r\n\0]/.test(s)) throw Error('Unsafe NSIS literal'); return s }
  const windows=name=>safe(name.replaceAll('/','\\'))
  const dirs=[...new Set(files.flatMap(f=>{let d=path.posix.dirname(f.destination),out=[];while(d!=='.'){out.push(d);d=path.posix.dirname(d)}return out}))].sort((a,b)=>a.split('/').length-b.split('/').length || a.localeCompare(b))
  template=template.replace(/{{#if installer_hooks}}[\s\S]*?{{\/if}}/g,'')
    .replace(/{{#each file_associations as \|association\| ~}}[\s\S]*?{{\/each}}\s*{{\/each}}/g,'')
    .replace(/{{#each deep_link_protocols as \|protocol\| ~}}[\s\S]*?{{\/each}}/g,'')
  const blocks={
    languages:'!insertmacro MUI_LANGUAGE "English"',
    language_files:`!include "${safe(context.language_file)}"`,
    resources_dirs:dirs.map(d=>`CreateDirectory "$INSTDIR\\${windows(d)}"`).join('\n'),
    resources:body=>files.map(f=>body.includes('File /a') ? `File /a "/oname=${windows(f.destination)}" "${safe(f.source)}"` : `Delete "$INSTDIR\\${windows(f.destination)}"`).join('\n'),
    resources_ancestors:[...dirs].reverse().map(d=>`RMDir /REBOOTOK "$INSTDIR\\${windows(d)}"`).join('\n'), binaries:'',
  }
  template=template.replace(/{{#each (\w+)}}([\s\S]*?){{\/each}}/g,(_,name,body)=>{if (!(name in blocks)) throw Error(`Unknown NSIS block ${name}`);return typeof blocks[name]==='function' ? blocks[name](body) : blocks[name]})
  template=template.replace(/{{(\w+)}}/g,(_,name)=>{
    if (!(name in context)) throw Error(`Unknown NSIS context ${name}`)
    if (name!=='uninstaller_sign_cmd') return safe(context[name])
    if (/[\r\n\0']/.test(context[name])) throw Error('Unsafe NSIS signing command')
    return context[name].replaceAll('$',()=> '$$').replaceAll('"',()=> '$\\"')
  })
  if (/{{|WEBVIEW2|Section WebView/i.test(template.split('\n').filter(line=>!line.trim().startsWith(';')).join('\n'))) throw Error('Unresolved or non-native NSIS template')
  return template
}

/** Offline NSIS kit: toolchain.json = {schema:1,nsisVersion:"3.11",pluginVersion:"0.5.3",files:[{destination,bytes,sha256}]}.
 * The caller reviews/stages this complete symlink-free kit. Nothing is downloaded. */
async function windowsKit(plan) {
  if (!plan.options.nsisTools) throw Error('Windows installer requires a reviewed offline --nsis-tools kit (NSIS 3.11, plugin 0.5.3, English.nsh); see describeRelease plan')
  const root=await canonicalDirectory(path.resolve(plan.root,plan.options.nsisTools)), manifest=await json(path.join(root,'toolchain.json'))
  const files=(await inventoryResources(root,[['.','kit']])).filter(f=>f.destination!=='kit/toolchain.json').map(f=>({...f,destination:f.destination.slice(4)}))
  const normalized=entries=>entries.map(({destination,bytes,sha256})=>({destination,bytes,sha256})).sort((a,b)=>a.destination.localeCompare(b.destination,'en'))
  if (manifest.schema!==1 || manifest.nsisVersion!=='3.11' || manifest.pluginVersion!=='0.5.3' || !Array.isArray(manifest.files) || JSON.stringify(normalized(files))!==JSON.stringify(normalized(manifest.files))) throw Error('Reviewed NSIS toolkit inventory mismatch')
  for (const required of ['makensis.exe','Bin/makensis.exe','Include/MUI2.nsh','Plugins/x86-unicode/additional/nsis_tauri_utils.dll','English.nsh']) if (!files.some(f=>f.destination===required)) throw Error(`Missing NSIS toolkit file ${required}`)
  const plugin=await readFile(path.join(root,'Plugins/x86-unicode/additional/nsis_tauri_utils.dll'))
  if (createHash('sha1').update(plugin).digest('hex')!=='75197fee3c6a814fe035788d1c34ead39349b860') throw Error('NSIS plugin differs from locked upstream 0.5.3 pin')
  return {root,inventorySha256:inventoryHash(files)}
}

async function packageWindows(plan, executable, environment, stageRoot, {run}, kit) {
  if (!(await readFile(executable)).subarray(0,2).equals(Buffer.from('MZ'))) throw Error('Cargo native Windows executable must be PE')
  const staged=path.join(stageRoot,'windows'); await mkdir(staged)
  const files=[]
  for (const pin of plan.files) {
    const target=path.join(staged,...pin.destination.split('/')); await mkdir(path.dirname(target),{recursive:true})
    const bytes=await readFile(pin.source)
    if (bytes.length!==pin.bytes || digest(bytes)!==pin.sha256) throw Error('Reviewed Windows resource changed before copy')
    await writeFile(target,bytes,{flag:'wx'}); files.push({...pin,source:target})
  }
  const [binaryPin]=await inventoryResources(path.dirname(executable),[[path.basename(executable),'tezbar.exe']])
  const binary=path.join(staged,'tezbar.exe'); await copyFile(executable,binary)
  const copied=await readFile(binary)
  if (copied.length!==binaryPin.bytes || digest(copied)!==binaryPin.sha256) throw Error('Native Windows executable changed before copy')
  assertMetadata((await captureBuildMetadata(binary)).metadata,{version:plan.version,...plan.before})
  await writeProductionManifest(staged,binary,files)
  const sign=async file=>run('signtool',['sign','/sha1',environment.WINDOWS_CERTIFICATE_THUMBPRINT,'/fd','sha256','/tr','http://timestamp.digicert.com','/td','sha256',file],{env:environment})
  const verify=async file=>run('powershell.exe',['-NoLogo','-NoProfile','-NonInteractive','-Command',"$s=Get-AuthenticodeSignature -LiteralPath $env:TEZBAR_RELEASE_ARTIFACT; if($s.Status -ne 'Valid' -or $s.SignerCertificate.Thumbprint -ne $env:WINDOWS_CERTIFICATE_THUMBPRINT){throw 'Invalid Windows code signature'}"],{env:{...environment,TEZBAR_RELEASE_ARTIFACT:file}})
  if (!plan.mode.unsigned) {await sign(binary);await verify(binary)}
  const updater=path.join(plan.output,`Tezbar_${plan.version}_x64-setup.exe`)
  const context={manufacturer:'tezbar',product_name:'Tezbar',version:plan.version,version_with_build:plan.version.split('-')[0]+'.0',homepage:'',install_mode:'currentUser',license:'',installer_icon:files.find(f=>f.destination==='icon.ico').source,sidebar_image:'',header_image:'',uninstaller_icon:'',uninstaller_header_image:'',main_binary_name:'tezbar',main_binary_path:binary,bundle_id:plan.base.identifier,copyright:'',out_file:updater,arch:'x64',additional_plugins_path:path.join(kit,'Plugins/x86-unicode/additional'),allow_downgrades:'false',display_language_selector:'false',estimated_size:String(Math.ceil((await readFile(binary)).length/1024)+files.reduce((n,f)=>n+Math.ceil(f.bytes/1024),0)),start_menu_folder:'',compression:'lzma',language_file:path.join(kit,'English.nsh'),uninstaller_sign_cmd:plan.mode.unsigned?'':`signtool sign /sha1 ${environment.WINDOWS_CERTIFICATE_THUMBPRINT} /fd sha256 /tr http://timestamp.digicert.com /td sha256 "%1"`}
  // Include generated provenance in installation/uninstallation inventory too.
  for (const name of ['.tezbar-native-production.json','.tezbar-native-source']) files.push({destination:name,source:path.join(staged,name)})
  const nsi=renderWindowsInstaller(await readFile(path.join(plan.root,'packaging/windows/installer.nsi'),'utf8'),context,files)
  await writeFile(path.join(stageRoot,'installer.nsi'),'\ufeff'+nsi)
  for (const name of ['utils.nsh','FileAssociation.nsh']) await copyFile(path.join(plan.root,'packaging/windows',name),path.join(stageRoot,name))
  await run(path.join(kit,'makensis.exe'),['-INPUTCHARSET','UTF8','installer.nsi'],{cwd:stageRoot,env:{...environment,NSISDIR:kit}})
  if (!plan.mode.unsigned) {await sign(updater);await verify(updater)}
  return {artifacts:[{file:updater,updater:true}],packagedInventorySha256:inventoryHash(plan.files)}
}

export async function nativeRelease(args, adapters={}) {
  const deps={root:ROOT,platform:process.platform,arch:process.arch,env:process.env,snapshot:sourceSnapshot,run:runTool,cargo:runCargo,signResources:signMacOSResources,verifyMac:verifyMacOSSigning,verifyUpdater:verifyUpdaterSignature,kit:windowsKit,log:console.log,...adapters}
  if (args.length === 1 && args[0] === '--help') { deps.log(HELP); return {state:'HELP'} }
  const options=parseRelease(args), plan=await prepareRelease(options,deps)
  if (options.dryRun) {const description=describeRelease(plan);deps.log(JSON.stringify(description,null,2));return description}
  // Every blocker is evaluated before building or creating output.
  const kit=deps.platform==='win32' ? await deps.kit(plan) : null
  if (options.localPreview && deps.platform==='win32') throw Error('Windows local preview is a development binary; public installer paths require updater signatures')
  const environment=await signingEnvironment(plan,deps.env,deps.run)
  if (plan.catalog) assertEmbeddingRuntimeDistribution({platform:deps.platform,arch:deps.arch,unsigned:plan.mode.unsigned,catalog:plan.catalog})
  assertSameSource(plan.before,deps.snapshot(plan.root))
  const executable=await deps.cargo('cargo',plan.cargoArgs,{cwd:plan.root,manifest:plan.cargoManifest,env:{...environment,TEZBAR_RELEASE_NOTES_PATH:plan.notes}})
  assertSameSource(plan.before,deps.snapshot(plan.root))
  const {metadata}=await captureBuildMetadata(executable)
  assertMetadata(metadata,{version:plan.version,...plan.before})
  if (digest(await readFile(plan.notes))!==plan.notesSha256) throw Error('Release notes changed during build')
  const recaptured=await inventoryResources(plan.resources,plan.roots)
  if (inventoryHash(recaptured)!==inventoryHash(plan.files)) throw Error('Reviewed resource inventory changed during build')
  if (kit && JSON.stringify(await deps.kit(plan))!==JSON.stringify(kit)) throw Error('Reviewed NSIS toolkit changed during build')
  await mkdir(plan.output,{mode:0o700})
  const stageRoot=await mkdtemp(path.join(await realpath(tmpdir()),'tezbar-native-release-'))
  // Retain failed stages for inspection; never sweep unrelated build/cache files.
  const packaged=deps.platform==='darwin' ? await packageMac(plan,executable,environment,stageRoot,deps) : await packageWindows(plan,executable,environment,stageRoot,deps,kit.root)
  const artifacts=[]
  for (const artifact of packaged.artifacts) {
    const stat=await lstat(artifact.file)
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size===0) throw Error('Native release artifact missing or empty')
    const bytes=await readFile(artifact.file)
    if (artifact.updater && !plan.mode.localPreview) {
      // Tauri's signer is used ONLY as the established minisign utility. No Tauri app build/config/bundler is invoked.
      await deps.run(process.execPath,[path.join(plan.root,'node_modules/@tauri-apps/cli/tauri.js'),'signer','sign',artifact.file],{env:environment})
      await deps.verifyUpdater(bytes,await readFile(artifact.file+'.sig','utf8'),plan.base.plugins.updater.pubkey)
      if (digest(await readFile(artifact.file))!==digest(bytes)) throw Error('Updater artifact changed while signing')
    }
    artifacts.push({filename:path.basename(artifact.file),sha256:digest(bytes),updater:artifact.updater})
  }
  assertSameSource(plan.before,deps.snapshot(plan.root))
  const receipt={channel:options.channel,version:plan.version,productName:'Tezbar',appIdentifier:plan.mode.localPreview?'com.tezbar.preview.native':plan.base.identifier,platform:plan.host.platform,sourceCommit:plan.before.sourceCommit,sourceDirty:plan.before.sourceDirty,sourceDigest:plan.before.sourceDigest,...(plan.mode.localPreview?{publishable:false}:{}),distribution:plan.mode.localPreview?'local-preview':plan.mode.unsigned?'testing-preview':'signed-release',platformVerified:!plan.mode.unsigned,binaryMetadata:metadata,inventorySha256:inventoryHash(plan.files),packagedInventorySha256:packaged.packagedInventorySha256,...(packaged.macOSSigning?{macOSSigning:packaged.macOSSigning}:{}),artifacts}
  await writeFile(path.join(plan.output,`receipt-${plan.host.platform}.json`),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'})
  deps.log(`Native GPUI artifacts: ${plan.output}. No publication, installation or dual-platform acceptance performed. Assembly retained: ${stageRoot}`)
  return receipt
}

if (process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) await nativeRelease(process.argv.slice(2))
