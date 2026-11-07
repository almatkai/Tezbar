"""Retain original cases; attach exact passing filters without claiming parity acceptance."""
import hashlib, json, pathlib, re
root=pathlib.Path(__file__).resolve().parent
repo=root.parents[1]
old=json.loads((root/'source-case-map.json').read_text())
def tests(log):return re.findall(r'^test (\S+) \.\.\. ok$',(root/'gates'/log).read_text(),re.M)
native=tests('frozen-06-normal-native.log');model=tests('frozen-04-model-all.log')
data=dict(schema_version=2,lane='M9',phase='linked and executed headlessly; independent/platform acceptance pending',acceptance=False,original_map='docs/native-m09/source-case-map.json',expanded_ts_cases_retained=115,final_source_freeze='docs/native-m09/final-source-freeze-06.json',native_suite_passed=len(native),cases=[],ledger=[],real_sdk_fixtures=[],scope_note='Exact tests below supply evidence only for their assertions. An expanded TS case does not gain complete GPUI, visual, accessibility or platform parity merely because its old model filter passed. Original evidence remains unchanged.')
for case in old['cases']:
 c=case.copy();f=c.get('rust_test_filter');c['executed_model_tests']=[n for n in model if f and f in n];c['executed_native_tests']=[n for n in native if f and f in n]
 c['m09_native_status']='selected native assertions executed; one-to-one parity unaccepted' if c['executed_native_tests'] else 'retained mapping; one-to-one native parity unaccepted'
 c['m09_native_evidence']=['docs/native-m09/gates/frozen-06-normal-native.log']if c['executed_native_tests']else[]
 c['model_evidence']=['docs/native-m09/gates/frozen-04-model-all.log']if c['executed_model_tests']else[]
 c['source_hash_current']=hashlib.sha256((repo/c['source']).read_bytes()).hexdigest()
 c['note']='Original expanded case/filter retained. Only the exact passing tests listed here are execution evidence; complete native interaction and platform closure are not inferred.'
 c['coverage']='mapped assertions executed'if c['executed_model_tests']or c['executed_native_tests']else'retained case; no matching filter in selected passing logs'
 data['cases'].append(c)
fixtures={}
for f in old['real_sdk_fixtures']:
 case=f['case'];name=f'surfaces::extension_runtime::real_sdk::sdk_{case}';assert name in native
 data['real_sdk_fixtures'].append(dict(case=case,source=f['source'],source_sha256=hashlib.sha256((repo/f['source']).read_bytes()).hexdigest(),runner=name,registered_native_tests=1,status='passed actual Bun production worker, initialized NativeCore/TestHost and RuntimeView',evidence='docs/native-m09/gates/frozen-06-normal-native.log',acceptance=False));fixtures[case]=name
for row in old['ledger']:
 n=int(row['id'][2:]);r=row.copy();r.update(acceptance=False,status='linked implementation; selected headless evidence; platform/design acceptance pending',fixture=[],executed_native_tests=[],evidence=[])
 names=[];cases=[]
 if n<=3:
  names=[s for s in native if 'extensions_store::tests::'in s]
  r['limits']='Native catalog/install/remove surface and finite transport; tests use scripted replies. No live store, real installation or platform authorization was run.'
 elif n==4:
  names=[s for s in native if 'retained_store_runtime_ack_source_fence_and_settings_route'in s];r['status']='M7-owned Settings writer; M9 route exercised'
  r['limits']='M9 route reaches existing Extensions Settings; M7 retains durable settings/hotkey/alias/toggle ownership and acceptance.'
 elif n==22:
  names=[s for s in native if 'color_detail_formats_'in s]
  r['limits']='Canonical Rust nine-row formatting, wheel change and finite copy request exercised. Physical pointer/AX/paint acceptance remains pending.'
 else:
  if n in [5,23,24,25,26,27,28,29,30,31,32,34]:cases=['form']
  elif n==6:cases=['invalidation','menubar']
  elif n in [7,8,9]:cases=['actions']
  elif n==10:cases=['actions','feedback','prompts']
  elif n in range(11,17):cases=['list']
  elif n in range(17,20):cases=['grid']
  elif n in [20,21]:cases=['detail']
  elif n==35:cases=['menubar']
  elif n==36:cases=['detail','grid']
  elif n==41:cases=['invalidation','menubar']
  names=[fixtures[c]for c in cases]
  if n in [31,33]:names +=[s for s in native if 'native_wheel_and_file_results_'in s]
  if n in [37,38,39,40,42,43,44,45]:r['status']='shared implementation retained; M9 does not claim full row closure'
  r['limits']='Only listed scenario assertions are evidence. Not every behavior of this ledger row is separately exercised; no automatic visual/platform/live-service acceptance.'
 r['fixture']=cases;r['executed_native_tests']=names;r['evidence']=['docs/native-m09/gates/frozen-06-normal-native.log']if names else[];data['ledger'].append(r)
assert len(data['cases'])==115 and len(data['ledger'])==45 and len(data['real_sdk_fixtures'])==9
(root/'final-source-case-map.json').write_text(json.dumps(data,indent=2)+'\n')
print(json.dumps(dict(retained_cases=115,mapped_model_filters_found=sum(bool(c['executed_model_tests'])for c in data['cases']),native_filters_found=sum(bool(c['executed_native_tests'])for c in data['cases']),retained_without_matching_filter=sum(not(c['executed_model_tests']or c['executed_native_tests'])for c in data['cases']),unaccepted_ledger_rows=45,registered_real_sdk=9,passed_real_sdk=9)))
