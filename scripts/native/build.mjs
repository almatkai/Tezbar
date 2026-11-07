// Explicit native opt-in; never reads signing secrets or changes the default Tauri product.
import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync, realpathSync, existsSync } from 'node:fs'
import { resolve, dirname, join, basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'
import { inventoryResources, planMacos, assembleMacos, GPUI_REV } from './package-macos.mjs'
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..')
export function parseNative(args) {
  const options={check:false,localPreview:false,resources:null,output:null}
  for(let i=0;i<args.length;i++) {
    const a=args[i]
    if(a==='--check') options.check=true
    else if(a==='--local-preview') options.localPreview=true
    else if(['--resources','--output'].includes(a) && args[i+1]) options[a.slice(2)]=resolve(args[++i])
    else throw Error(`Unknown native argument: ${a}`)
  }
  if((options.resources||options.output)&&(!options.localPreview||!options.resources||!options.output)) throw Error('Packaging requires --local-preview, --resources and --output together')
  return options
}
export async function nativeBuild(args) {
  const options=parseNative(args)
  if(options.resources && process.platform!=='darwin') throw Error('macOS assembly requires macOS; Windows installer rehearsal remains blocked')
  if(options.output) {
    const temp=realpathSync(tmpdir()),parent=realpathSync(dirname(options.output))
    options.output=join(parent,basename(options.output))
    if(!parent.startsWith(temp+'/') || existsSync(options.output)) throw Error('Use a fresh child of OS temp for local preview output')
  }
  const command=['+1.98.1',options.check?'check':'build','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders']
  const result=spawnSync('cargo',command,{cwd:root,stdio:'inherit'})
  if(result.status!==0) throw result.error??Error(`Native Cargo exit ${result.status}`)
  if(!options.resources) return
  const get=arg=>spawnSync('git',arg,{cwd:root,encoding:'utf8'})
  const sourceCommit=get(['rev-parse','HEAD']).stdout.trim()
  const sourceDirty=Boolean(get(['status','--porcelain']).stdout.trim())
  const files=await inventoryResources(options.resources)
  const [binary]=await inventoryResources(root,[['target/debug/tezbar','tezbar']])
  const policy=JSON.parse(readFileSync(join(root,'src-tauri/macos-signing.json')))
  const catalog=JSON.parse(readFileSync(join(root,'src-tauri/runtime/embedding-macos-arm64.json')))
  const version=JSON.parse(readFileSync(join(root,'package.json'))).version
  // Existing version only. Channel is derived for policy validation, never promoted or published.
  const channel=version.includes('-beta.')?'beta':version.includes('-alpha.')?'alpha':'stable'
  const plan=planMacos({version,channel,localPreview:true,stage:options.output,osTemp:realpathSync(tmpdir()),executable:binary,files,policy,catalog,payload:{bytes:catalog.archiveBytes,sha256:catalog.archiveSha256},sourceCommit,sourceDirty,gpuiRev:GPUI_REV,rustVersion:'1.98.1',benchControl:false})
  const receipt=await assembleMacos(plan,{template:readFileSync(join(root,'packaging/macos/Info.plist.in'),'utf8'),permissions:readFileSync(join(root,'crates/tezbar-core/src/system/macos-permissions.plist'),'utf8'),osTemp:realpathSync(tmpdir())})
  const signed=spawnSync('/usr/bin/codesign',['--force','--sign','-',plan.bundle],{stdio:'inherit'})
  if(signed.status!==0) throw Error('Local preview ad-hoc seal failed; unsealed stage retained')
  writeFileSync(join(options.output,'local-preview-receipt.json'),JSON.stringify({...receipt,state:'LOCAL_PREVIEW_AD_HOC',sourceDirty,publishable:false,platformVerified:false,identityContinuityVerified:false},null,2)+'\n',{flag:'wx'})
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) await nativeBuild(process.argv.slice(2))
