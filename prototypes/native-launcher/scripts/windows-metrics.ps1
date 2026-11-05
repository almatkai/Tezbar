param([Parameter(Mandatory=$true)][int]$RootPid,[int]$Seconds=60,[Parameter(Mandatory=$true)][string]$Output)
# PrivateUsage (commit) primary; WorkingSetSize secondary. No command lines.
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class NativeTree {
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Entry {
  public uint size,usage,pid; public UIntPtr heap; public uint module,threads,parent;public int priority;public uint flags;
  [MarshalAs(UnmanagedType.ByValTStr,SizeConst=260)] public string exe;
 }
 [DllImport("kernel32.dll")] static extern IntPtr CreateToolhelp32Snapshot(uint flags,uint pid);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool Process32FirstW(IntPtr h,ref Entry e);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool Process32NextW(IntPtr h,ref Entry e);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 public static uint[] Owned(uint root) {
  var entries=new List<Entry>();var h=CreateToolhelp32Snapshot(2,0);
  try {var e=new Entry();e.size=(uint)Marshal.SizeOf(e);if(Process32FirstW(h,ref e)){do{entries.Add(e);}while(Process32NextW(h,ref e));}}
  finally{CloseHandle(h);}
  var owned=new HashSet<uint>();owned.Add(root);bool change;
  do{change=false;foreach(var e in entries)if(owned.Contains(e.parent)&&owned.Add(e.pid))change=true;}while(change);
  var result=new uint[owned.Count];owned.CopyTo(result);return result;
 }
}
'@
$samples=@();$clock=[Diagnostics.Stopwatch]::StartNew()
while($clock.Elapsed.TotalSeconds -lt $Seconds){
 $rows=@();foreach($owned in [NativeTree]::Owned($RootPid)){
  try{$p=[Diagnostics.Process]::GetProcessById($owned);$rows+=@{pid=$owned;name=$p.ProcessName;start_ticks=$p.StartTime.ToUniversalTime().Ticks;private_usage=$p.PrivateMemorySize64;working_set=$p.WorkingSet64;cpu_seconds=$p.TotalProcessorTime.TotalSeconds};$p.Dispose()}catch{}
 }
 $samples+=@{t=$clock.Elapsed.TotalSeconds;processes=$rows};Start-Sleep -Milliseconds 1000
}
@{metric='sum PrivateUsage (commit)';secondary='sum WorkingSetSize';private_working_set='not collected';cpu='user+kernel seconds from GetProcessTimes via Process.TotalProcessorTime';ownership='Toolhelp32 parent tree';samples=$samples}|ConvertTo-Json -Depth 8|Set-Content -Encoding UTF8 $Output
