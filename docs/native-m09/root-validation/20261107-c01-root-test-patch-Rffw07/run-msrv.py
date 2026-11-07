import pathlib,sys
sys.path.insert(0,str(pathlib.Path(__file__).parent));from gate import run,OUT
base=['cargo','+1.85.0'];extra={'CARGO_TARGET_DIR':str(OUT/'target-msrv')}
for n,c in [
 ('msrv-check',base+['check','--locked','--offline','-j2','-p','tezbar-core','-p','tezbar-host','-p','tezbar-update','-p','tezbar-ui-model','--all-features']),
 ('models-185',base+['test','--locked','--offline','-j2','-p','tezbar-ui-model','--all-features','--lib','--','--test-threads=2'])]:
 r=run(n,c,extra)
 if r:sys.exit(r)
