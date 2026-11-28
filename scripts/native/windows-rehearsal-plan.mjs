// Pure proposal: no registry, installer, process, shell or remote access.
export function windowsRehearsalPlan({ sandboxRoot, snapshotId, originalVersion, candidateVersion }) {
  if (!/^[a-zA-Z0-9-]{1,60}$/.test(snapshotId) || !/^[A-Z]:\\[^\0]+/.test(sandboxRoot) || /(?:^|\\)\.\.(?:\\|$)/.test(sandboxRoot)) throw new Error('Use a disposable VM snapshot and an explicit sandbox root')
  const identity = { product:'Tezbar', executable:'tezbar.exe', manufacturer:'tezbar', installMode:'currentUser', allowDowngrades:false }
  return { kind:'PROPOSAL_ONLY', requires:'Disposable Windows VM/user with empty hive, no installed Tezbar and network disabled except captured fixtures', sandboxRoot,snapshotId,originalVersion,candidateVersion,identity,
    expected: { install:'$LOCALAPPDATA\\Tezbar', manufacturerKey:'HKCU\\Software\\tezbar\\Tezbar', uninstallKey:'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Tezbar', executable:'tezbar.exe', startMenu:'$SMPROGRAMS\\Tezbar.lnk' },
    cases:[
      {name:'original-to-native passive',argv:['/P','/R','/UPDATE','/ARGS'],assert:['same install directory','same registry keys and main binary','one relaunch','no WebView runtime probe']},
      {name:'empty relaunch args',currentArgs:[],expectedFragments:['/P','/R','/UPDATE','/ARGS']},
      {name:'separate escaping',currentArgs:['--profile','C:\\fixture with spaces\\','','/slash','--quote="x"','қазақша 😀'],oracle:'tezbar_update::native::StagedWindows::arguments (locked updater escaping port)'},
      {name:'quiet',argv:['/S','/R','/UPDATE','/ARGS'],assert:['one relaunch']},
      {name:'downgrade',assert:['lower version refused','old installation bytes intact']},
      {name:'uninstall',assert:['owned shortcuts and registry removed','profile data unchanged unless explicit installer UI selection']},
    ], evidence:['before/after VM hive exports','install tree hashes','dummy executable argv capture as UTF-16','same-context Tauri and native installer generated NSIS diff','signature and updater receipts'], executableCommands:[] }
}
