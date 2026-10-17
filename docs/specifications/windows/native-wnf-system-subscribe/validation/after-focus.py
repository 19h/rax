from pathlib import Path
import json,subprocess,sys,time
b=Path(__file__).resolve().parent;host,kind=sys.argv[1:]
while True:
 try:
  rows=json.loads((b/(host+'-wnf-system-epoch2-gates.json')).read_text())
  if any(r['exit-code'] for r in rows):raise RuntimeError('epoch2 focused gate failed')
  if len(rows)==3:break
 except (FileNotFoundError,json.JSONDecodeError):pass
 time.sleep(5)
script='run-system-final-unix.py'if kind=='full'else'run-system-owning-gates.py'
raise SystemExit(subprocess.run([sys.executable,'-B',str(b/script),host]).returncode)
