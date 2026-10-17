"""Continue final validation within the existing native job, without new RPC."""
from pathlib import Path
import ctypes
from ctypes import wintypes
import datetime
import hashlib
import json
import os
import subprocess
import sys
import time

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
RECORD = SHARE / 'console-owner-native-handoff.json'
LOCK = BASE / 'console-owner-native-handoff.lock'


def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def save(row):
    row['updated-utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    RECORD.write_text(json.dumps(row, indent=2) + '\n')


def alive(pid):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        if ctypes.get_last_error() == 87:
            return False
        raise OSError(ctypes.get_last_error(), 'cannot verify existing task process')
    try:
        code = wintypes.DWORD()
        if not kernel.GetExitCodeProcess(handle, ctypes.byref(code)):
            raise OSError(ctypes.get_last_error(), 'cannot query existing task process')
        return code.value == 259
    finally:
        kernel.CloseHandle(handle)


def completed_or_wait(path):
    while True:
        row = load(path)
        if 'exit-code' in row:
            return row['exit-code']
        if not alive(row['pid']):
            raise RuntimeError('Task process ended without a terminal record: ' + path.name)
        time.sleep(5)


def run(name):
    return subprocess.run([str(BASE / 'python/python.exe'), '-B', str(SHARE / name)]).returncode


def ensure_run(path, name):
    if path.exists():
        return completed_or_wait(path)
    code = run(name)
    # A concurrent host watcher can win the exclusive native launch. Its
    # actual process/terminal record remains authoritative, not our RPC status.
    return completed_or_wait(path) if path.exists() else code


def main():
    if RECORD.exists():
        raise RuntimeError('Native handoff already requested; inspect existing record')
    descriptor = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    os.write(descriptor, str(os.getpid()).encode())
    os.close(descriptor)
    initial = load(SHARE / 'console-owner-epoch1-launch.json')
    initial_focused = SHARE / 'native-console-owner-epoch1-focused.log'
    initial_ran = initial.get('exit-code') == 101 and initial_focused.exists()
    row = {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'state': 'verifying-original-ordinary-observations',
           'epoch1-native-focused': ('ran; initial native focused result retained' if initial_ran else
                                    'not run; superseded source; initial SP failures retained on both Unix hosts'),
           'scope': 'same corrected Windows core and owning gates; no test assertion, selection or production parameter change',
           'final-source-epoch': 2}
    save(row)
    diagnostic = None
    trace_probe = None
    try:
        ordinary = load(SHARE / 'native-console-owner-epoch1-ordinary-outcomes.json')
        if len(ordinary) != 3:
            raise ValueError('Original ordinary observation selection incomplete')
        for observation in ordinary:
            if 'log' in observation:
                data = (SHARE / observation['log']).read_bytes()
                if hashlib.sha256(data).hexdigest() != observation['sha256']:
                    raise ValueError('Original ordinary log changed')
        row['ordinary-outcomes-sha256'] = hashlib.sha256(
            (SHARE / 'native-console-owner-epoch1-ordinary-outcomes.json').read_bytes()).hexdigest()
        row['state'] = 'running-final-epoch2'
        save(row)
        auxiliary = SHARE / 'loader-first-error-native-launch.json'
        if not auxiliary.exists():
            diagnostic = subprocess.Popen([str(BASE / 'python/python.exe'), '-B',
                                           str(SHARE / 'run-loader-first-error-atomic.py')])
            row['diagnostic-child-pid'] = diagnostic.pid
            save(row)
        trace_record = SHARE / 'trace-event-native-launch.json'
        if not trace_record.exists():
            trace_probe = subprocess.Popen([str(BASE / 'python/python.exe'), '-B',
                                            str(SHARE / 'run-trace-event-atomic.py')])
            row['trace-producer-child-pid'] = trace_probe.pid
            save(row)
        final_record = SHARE / 'console-owner-epoch2-launch.json'
        code = ensure_run(final_record, 'run-console-owner-epoch2-atomic.py')
        if code != 0:
            raise RuntimeError('Final corrected native core gates did not complete: ' + str(code))
        row['final-core-exit-code'] = code
        row['state'] = 'waiting-for-independent-diagnostic'
        save(row)
        if diagnostic is not None:
            row['diagnostic-exit-code'] = diagnostic.wait()
            save(row)
        elif auxiliary.exists():
            row['diagnostic-exit-code'] = completed_or_wait(auxiliary)
            save(row)
        if trace_probe is not None:
            row['trace-producer-exit-code'] = trace_probe.wait()
            save(row)
        elif trace_record.exists():
            row['trace-producer-exit-code'] = completed_or_wait(trace_record)
            save(row)
        row['state'] = 'running-final-owning-gates'
        save(row)
        owning = SHARE / 'console-owner-owning-epoch2-atomic-v2-launch.json'
        code = ensure_run(owning, 'run-console-owner-owning-epoch2-atomic.py')
        if code != 0:
            raise RuntimeError('Final native owning gates did not complete: ' + str(code))
        row.update({'state': 'finished', 'final-owning-exit-code': code, 'exit-code': 0})
        save(row)
    except BaseException as error:
        row.update({'state': 'failed', 'exit-code': 1, 'error': repr(error)})
        save(row)
        raise
    finally:
        LOCK.unlink()
    return 0


if __name__ == '__main__':
    sys.exit(main())
