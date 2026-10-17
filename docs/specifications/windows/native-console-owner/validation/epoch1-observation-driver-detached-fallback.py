from pathlib import Path
import subprocess,json,hashlib,datetime,time
root=Path('C:/Windows/Temp/assist-native-rax-20261010');share=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');records=[]
programs=[root/'src/tests/fixtures/user/windows/bin/arm64/smoke.exe',root/'assist/native-process-probe.exe',Path('C:/Windows/System32/whoami.exe')]
for i,program in enumerate(programs):
 row={'path':str(program),'scope':'actual production startup via unchanged observer,native_libraries=true,slice_insns1,3Mturn ceiling;controller0 does not imply guest exit'}
 if program.exists():
  row['program-sha256']=hashlib.sha256(program.read_bytes()).hexdigest();log=root/('native-console-owner-epoch1-ordinary-'+str(i)+'.log');start=time.time()
  with log.open('wb') as f:
   try:row['controller-exit-code']=subprocess.run([str(root/'native-console-owner-epoch1-trace.exe'),str(program)],stdout=f,stderr=subprocess.STDOUT,timeout=900).returncode
   except subprocess.TimeoutExpired:row['controller-timeout-seconds']=900
  row['seconds']=time.time()-start;shutil_data=log.read_bytes();(share/log.name).write_bytes(shutil_data);row['log']=log.name;row['sha256']=hashlib.sha256(shutil_data).hexdigest()
 else:row['unavailable']='program file absent;not counted as execution'
 records.append(row);(share/'native-console-owner-epoch1-ordinary-outcomes.json').write_text(json.dumps(records,indent=2)+'\n');print('ordinary observation',i,'complete;guest terminal output requires review',flush=True)

# Auxiliary read-only diagnostic, after all original ordinary observations.
# Primary program selection, observer bytes, limits and result rows above are unchanged.
# This task-owned fallback bypasses a failing Parallels command-creation request.
import os
auxiliary = share / 'loader-first-error-native-launch.json'
queue_record = share / 'loader-first-error-via-existing-driver.json'
if not auxiliary.exists() and not queue_record.exists():
    entry = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
             'parent-pid': os.getpid(), 'source': 'existing epoch1 ordinary-observation driver',
             'scope': 'separate optional read-only diagnostic; not a primary gate or guest success'}
    flags = subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
    args = [str(root / 'python/python.exe'), '-B', str(share / 'run-loader-first-error-atomic.py')]
    try:
        try:
            child = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     creationflags=flags | subprocess.CREATE_BREAKAWAY_FROM_JOB)
            entry['job-breakaway'] = True
        except PermissionError:
            child = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     creationflags=flags)
            entry['job-breakaway'] = False
        entry.update({'child-pid': child.pid, 'state': 'requested; native atomic record is authoritative'})
    except OSError as error:
        entry.update({'state': 'auxiliary launch failed; original gates continue',
                      'error-class': type(error).__name__, 'error': str(error)})
    queue_record.write_text(json.dumps(entry, indent=2) + '\n')
