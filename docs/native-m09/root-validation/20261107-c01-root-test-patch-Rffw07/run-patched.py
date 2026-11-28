import pathlib,sys
sys.path.insert(0,str(pathlib.Path(__file__).parent));from gate import run
base=['cargo','+1.98.1']
for n,c in [
('patched-native-normal',base+['test','--locked','--offline','-j2','-p','tezbar-native','--lib','--features','runtime-shaders','--','--test-threads=2','--nocapture']),
('patched-native-bench',base+['test','--locked','--offline','-j2','-p','tezbar-native','--lib','--features','runtime-shaders,bench-control','--','--test-threads=2','--nocapture']),
('patched-native-check',base+['check','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders']),
('patched-native-build',base+['build','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders'])]:
 r=run(n,c)
 if r:sys.exit(r)
