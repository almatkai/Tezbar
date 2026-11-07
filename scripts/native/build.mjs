// Explicit native opt-in; never reads signing secrets or changes the default Tauri product.
import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync, realpathSync, existsSync, lstatSync, readlinkSync } from 'node:fs'
import { resolve, dirname, join, basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createHash } from 'node:crypto'
import { tmpdir } from 'node:os'
import { inventoryResources, planMacos, assembleMacos, GPUI_REV } from './package-macos.mjs'
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..')
export function parseNative(args) {
  const options={check:false,release:false,localPreview:false,resources:null,output:null}
  for(let i=0;i<args.length;i++) {
    const a=args[i]
    if(a==='--check') options.check=true
    else if(a==='--release') options.release=true
    else if(a==='--local-preview') options.localPreview=true
    else if(['--resources','--output'].includes(a) && args[i+1]) options[a.slice(2)]=resolve(args[++i])
    else throw Error(`Unknown native argument: ${a}`)
  }
  if((options.resources||options.output)&&(!options.localPreview||!options.resources||!options.output)) throw Error('Packaging requires --local-preview, --resources and --output together')
  if(options.localPreview && (!options.resources || !options.output || options.check)) throw Error('Preview assembly requires --resources and --output, and cannot use --check')
  return options
}
export function binaryPath(target, options, platform=process.platform) {
  return join(target, options.release ? 'release' : 'debug', platform==='win32' ? 'tezbar.exe' : 'tezbar')
}
export function sourceSnapshot(directory=root) {
  const git=args=>{const r=spawnSync('git',args,{cwd:directory,encoding:'utf8'});if(r.status!==0) throw Error('Cannot attest native source: Git unavailable');return r.stdout}
  const sourceCommit=git(['rev-parse','HEAD']).trim()
  if(!/^[0-9a-f]{40}$/.test(sourceCommit)) throw Error('Invalid native source commit')
  const sourceDirty=Boolean(git(['status','--porcelain','--untracked-files=all']).trim())
  const hash=createHash('sha256')
  const names=[...new Set(git(['ls-files','-co','--exclude-standard','-z']).split('\0').filter(Boolean))].sort()
  for(const name of names) {
    const path=join(directory,name)
    hash.update(name+'\0')
    let info
    try {info=lstatSync(path)} catch(error) {if(error.code!=='ENOENT') throw error;hash.update('missing\0');continue}
    hash.update(info.isSymbolicLink() ? 'link:'+readlinkSync(path) : readFileSync(path))
    hash.update('\0')
  }
  return {sourceCommit,sourceDirty,sourceDigest:hash.digest('hex')}
}
export function assertSameSource(before,after) {
  if(JSON.stringify(before)!==JSON.stringify(after)) throw Error('Native source changed during build; artifact was not packaged')
}
export async function nativeBuild(args) {
  const options=parseNative(args)
  if(options.resources && process.platform!=='darwin') throw Error('macOS assembly requires macOS; Windows installer rehearsal remains blocked')
  if(options.output) {
    const temp=realpathSync(tmpdir()),parent=realpathSync(dirname(options.output))
    options.output=join(parent,basename(options.output))
    if(!parent.startsWith(temp+'/') || existsSync(options.output)) throw Error('Use a fresh child of OS temp for local preview output')
  }
  const before=sourceSnapshot()
  const command=['+1.98.1',options.check?'check':'build','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders']
  if(options.release) command.push('--release')
  const result=spawnSync('cargo',command,{cwd:root,stdio:'inherit'})
  if(result.status!==0) throw result.error??Error(`Native Cargo exit ${result.status}`)
  assertSameSource(before,sourceSnapshot())
  if(!options.resources) return
  const {sourceCommit,sourceDirty}=before
  const metadata=spawnSync('cargo',['+1.98.1','metadata','--locked','--offline','--format-version','1','--no-deps'],{cwd:root,encoding:'utf8'})
  if(metadata.status!==0) throw Error('Cannot locate Cargo target directory')
  const executable=binaryPath(JSON.parse(metadata.stdout).target_directory,options)
  const observedVersion=spawnSync(executable,['--version'],{cwd:root,encoding:'utf8'})
  if(observedVersion.status!==0) throw Error('Cannot attest built native executable')
  const embedded=JSON.parse(observedVersion.stdout)
  if(embedded.source!==sourceCommit || String(embedded.dirty)!==String(sourceDirty) || embedded.bench_control || embedded.gpui_rev!==GPUI_REV) throw Error('Built native executable provenance does not match source')
  const files=await inventoryResources(options.resources)
  const [binary]=await inventoryResources(dirname(executable),[[basename(executable),'tezbar']])
  const policy=JSON.parse(readFileSync(join(root,'src-tauri/macos-signing.json')))
  const catalog=JSON.parse(readFileSync(join(root,'src-tauri/runtime/embedding-macos-arm64.json')))
  const version=JSON.parse(readFileSync(join(root,'package.json'))).version
  // Existing version only. Channel is derived for policy validation, never promoted or published.
  const channel=version.includes('-beta.')?'beta':version.includes('-alpha.')?'alpha':'stable'
  const plan=planMacos({version,channel,localPreview:true,stage:options.output,osTemp:realpathSync(tmpdir()),executable:binary,files,policy,catalog,payload:{bytes:catalog.archiveBytes,sha256:catalog.archiveSha256},sourceCommit,sourceDirty,gpuiRev:GPUI_REV,rustVersion:'1.98.1',benchControl:false})
  assertSameSource(before,sourceSnapshot())
  const receipt=await assembleMacos(plan,{template:readFileSync(join(root,'packaging/macos/Info.plist.in'),'utf8'),permissions:readFileSync(join(root,'crates/tezbar-core/src/system/macos-permissions.plist'),'utf8'),osTemp:realpathSync(tmpdir())})
  const signed=spawnSync('/usr/bin/codesign',['--force','--sign','-',plan.bundle],{stdio:'inherit'})
  if(signed.status!==0) throw Error('Local preview ad-hoc seal failed; unsealed stage retained')
  writeFileSync(join(options.output,'local-preview-receipt.json'),JSON.stringify({...receipt,sourceDigest:before.sourceDigest,profile:options.release?'release':'debug',binaryMetadata:embedded,state:'LOCAL_PREVIEW_AD_HOC',sourceDirty,publishable:false,platformVerified:false,identityContinuityVerified:false},null,2)+'\n',{flag:'wx'})
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) await nativeBuild(process.argv.slice(2))
