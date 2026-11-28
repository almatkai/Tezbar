import pathlib,subprocess,json,re
root=pathlib.Path.cwd();out=pathlib.Path(__file__).parent;owned=json.loads((root/'docs/native-m09/root-repairs/owned-files.json').read_text());owned['apps/tezbar/src/surfaces/command_bar/extension_integration_tests.rs']={};items=[];logs=[];copies=out/'patched-format-baseline';copies.mkdir()
for p in owned:
 if not p.endswith('.rs'):continue
 edition='2024' if p.startswith('apps/') else '2021';args=['rustfmt','+1.98.1','--check','--edition',edition,'--config','skip_children=true']
 r=subprocess.run([*args,p],capture_output=True);logs.append(b'FILE '+p.encode()+b'\n'+r.stdout+r.stderr)
 old=subprocess.run(['git','show','9230bcc:'+p],capture_output=True);baseline=None
 if old.returncode==0:
  temp=copies/(p.replace('/','_'));temp.write_bytes(old.stdout);b=subprocess.run([*args,str(temp)],capture_output=True);baseline=b.returncode
 items.append({'file':p,'exit_code':r.returncode,'baseline_9230bcc_exit_code':baseline})
prior=root/'docs/native-m09/gates/68-final-global-fmt.log';baseline_files=sorted({x.split(':')[0].removeprefix(str(root)+'/') for x in re.findall(r'^Diff in (.*):\d+:$',prior.read_text(),re.M)})
(out/'patched-format-details.log').write_bytes(b''.join(logs));(out/'patched-format-results.json').write_text(json.dumps({'owned':items,'prior_global_baseline_count':len(baseline_files),'prior_baseline_files':baseline_files,'actual_owned_overlap':sorted({i['file'] for i in items if i['exit_code']} & set(baseline_files)),'no_source_changes':True},indent=2));print(json.dumps({'checked':len(items),'failed':[i['file'] for i in items if i['exit_code']],'prior_global_baseline':len(baseline_files)}))
# Return 0 for diagnostic inventory, retain actual individual nonzero format results honestly.
