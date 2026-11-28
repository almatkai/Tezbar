import pathlib,sys
sys.path.insert(0,str(pathlib.Path(__file__).parent));from gate import run
base=['cargo','+1.98.1'];gates=[('ui',base+['test','--locked','--offline','-j2','-p','tezbar-ui','--lib','--','--test-threads=2']),
('models-198',base+['test','--locked','--offline','-j2','-p','tezbar-ui-model','--all-features','--lib','--','--test-threads=2']),
('core-worker',base+['test','--locked','--offline','-j2','-p','tezbar-core','--lib','extension_worker::tests','--','--test-threads=2']),
('core-host',base+['test','--locked','--offline','-j2','-p','tezbar-core','--lib','platform::tests::host_','--','--test-threads=2']),
('core-status',base+['test','--locked','--offline','-j2','-p','tezbar-core','--lib','scoped_status_item_limits_update_callbacks_and_cleanup_cross_host_port','--','--test-threads=2']),
('core-launch',base+['test','--locked','--offline','-j2','-p','tezbar-core','--lib','explicit_worker_launch_validates_owned_paths_without_launching','--','--test-threads=2']),
('core-worker-integration',base+['test','--locked','--offline','-j2','-p','tezbar-core','--lib','extension_worker::integration_tests::real_rust_host_bun_roundtrip_and_termination','--','--ignored','--exact','--test-threads=2','--nocapture']),
('host',base+['test','--locked','--offline','-j2','-p','tezbar-host','--lib','--','--test-threads=2']),
('native-check',base+['check','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders']),
('native-build',base+['build','--locked','--offline','-j2','-p','tezbar-native','--features','runtime-shaders']),
]
for n,c in gates:
 code=run(n,c)
 if code: print('STOPPED AT '+n,flush=True);sys.exit(code)
