from pathlib import Path
import json,subprocess,sys,time
b=Path(__file__).resolve().parent;host=sys.argv[1]
while True:
 try:
  rows=json.loads((b/(host+'-wnf-epoch4-gates.json')).read_text())
  if any(r['exit-code'] for r in rows):raise RuntimeError('corrected focus failed')
  if len(rows)==4:break
 except (FileNotFoundError,json.JSONDecodeError):pass
 time.sleep(5)
raise SystemExit(subprocess.run([sys.executable,'-B',str(b/'run-owning4-gates.py'),host]).returncode)
